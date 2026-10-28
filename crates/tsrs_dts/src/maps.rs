use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use rustc_hash::FxHashMap;
use tsrs_ast::{Node, SourceFile};
use tsrs_compiler::SourceMapEmitResult;
use tsrs_core::{P, tspath};
use tsrs_printer::Printer;
use tsrs_sourcemap::{Generator, Mapping, SourceIndex};

use crate::Files;

struct Fragment {
    buffer: usize,
    line: u32,
    mappings: String,
}

/// One output file's text. Fragments are attributed to the buffer by its id.
pub struct OutputBuffer {
    id: usize,
    pub text: String,
    /// Generated line at which the next appended fragment starts.
    line: u32,
}

pub struct OutputPrinter<'a> {
    printer: Printer,
    /// Captured declaration maps, keyed by declaration path.
    maps: &'a BTreeMap<String, SourceMapEmitResult>,
    /// Mappings of the declaration maps that composition has used so far.
    decoded: FxHashMap<&'a str, Vec<Mapping>>,
    buffers: usize,
    enabled: bool,
    directory: String,
    fragments: Vec<Fragment>,
    /// Fragment source table, in the order fragment generators assigned indices.
    sources: Vec<String>,
    /// `Printer::write` caches source indices across writes, so every fragment
    /// generator starts from this one, seeded with `sources`.
    /// Map parity tests cover repeated fragments and multiple output buffers.
    ///
    /// TODO: Use an independent-generator printer API when available, then
    /// remove this table seeding. See docs/DTS.md.
    template: Generator,
}

/// Where one fragment source index composes to in the final map.
enum Origin<'a> {
    /// A captured declaration whose own map leads back to original sources.
    Declaration(&'a SourceMapEmitResult, &'a [Mapping]),
    /// Any other file, mapped as is.
    File(String),
}

impl<'a> OutputPrinter<'a> {
    pub fn new(
        printer: Printer,
        maps: &'a BTreeMap<String, SourceMapEmitResult>,
        enabled: bool,
        directory: String,
    ) -> Self {
        let template = tsrs_sourcemap::new_generator(
            "fragment.d.ts",
            "",
            &directory,
            tspath::ComparePathsOptions {
                current_directory: directory.clone(),
                use_case_sensitive_file_names: true,
            },
        );
        Self {
            printer,
            maps,
            decoded: FxHashMap::default(),
            enabled,
            directory,
            fragments: Vec::new(),
            buffers: 0,
            sources: Vec::new(),
            template,
        }
    }

    pub fn buffer(&mut self) -> OutputBuffer {
        self.buffers += 1;
        OutputBuffer { id: self.buffers, text: String::new(), line: 0 }
    }

    pub fn append(&mut self, buffer: &mut OutputBuffer, node: P<Node>, source: P<SourceFile>) {
        let text = if self.enabled {
            let mut writer = tsrs_printer::new_text_writer("\n", 0);
            let mut generator = self.template.clone();
            self.printer.write(node, Some(source), &mut *writer, Some(&mut generator));
            for source in &generator.sources()[self.sources.len()..] {
                self.template.add_source(source);
                self.sources.push(source.clone());
            }
            self.fragments.push(Fragment {
                buffer: buffer.id,
                line: buffer.line,
                mappings: generator.raw_source_map().mappings,
            });
            writer.string()
        } else {
            self.printer.emit(node, Some(source))
        };
        buffer.line += text.bytes().filter(|&byte| byte == b'\n').count() as u32
            + u32::from(!text.ends_with('\n'));
        buffer.text.push_str(&text);
        if !text.ends_with('\n') {
            buffer.text.push('\n');
        }
    }

    /// Return the buffer's final text, adding its composed map to `files` when maps are enabled.
    pub fn finish(
        &mut self,
        buffer: OutputBuffer,
        file: &str,
        files: &mut Files,
        prefix_lines: u32,
    ) -> String {
        let OutputBuffer { id: buffer, text: mut code, .. } = buffer;
        if !self.enabled {
            return code;
        }
        let fragments: Vec<_> =
            self.fragments.extract_if(.., |fragment| fragment.buffer == buffer).collect();
        let mut generator = tsrs_sourcemap::new_generator(
            file,
            "file:///",
            "/",
            tspath::ComparePathsOptions {
                current_directory: self.directory.clone(),
                use_case_sensitive_file_names: true,
            },
        );
        let origins = self.origins();
        let mut indices = FxHashMap::<&str, SourceIndex>::default();
        let mut content_sources = BTreeSet::new();
        for fragment in &fragments {
            let mut decoder = tsrs_sourcemap::decode_mappings(&fragment.mappings);
            while let Some(mapping) = decoder.next() {
                let line = prefix_lines + fragment.line + mapping.generated_line;
                let origin =
                    mapping.source_index.get().and_then(|index| origins.get(index as usize));
                let mapped = origin.and_then(|origin| match origin {
                    Origin::Declaration(map, mappings) => {
                        let target = (mapping.source_line?, mapping.source_character?);
                        let index = mappings.partition_point(|candidate| {
                            (candidate.generated_line, candidate.generated_character) <= target
                        });
                        let original = *mappings.get(index.checked_sub(1)?)?;
                        if original.generated_line != mapping.source_line? {
                            return None;
                        }
                        let index = original.source_index.get()? as usize;
                        Some((
                            map.input_source_file_names.get(index)?.as_str(),
                            original.source_line?,
                            original.source_character?,
                            map.source_map
                                .sources_content
                                .as_ref()
                                .and_then(|contents| contents.get(index))
                                .and_then(|content| content.as_deref()),
                        ))
                    }
                    Origin::File(source) => Some((
                        source.as_str(),
                        mapping.source_line?,
                        mapping.source_character?,
                        None,
                    )),
                });
                if let Some((source, original_line, original_column, content)) = mapped {
                    let index =
                        *indices.entry(source).or_insert_with(|| generator.add_source(source));
                    if let Some(content) = content.filter(|_| content_sources.insert(index.get())) {
                        generator.set_source_content(index, content).unwrap();
                    }
                    generator
                        .add_source_mapping(
                            line,
                            mapping.generated_character,
                            index,
                            original_line,
                            original_column,
                        )
                        .unwrap();
                } else {
                    generator.add_generated_mapping(line, mapping.generated_character).unwrap();
                }
            }
        }
        let name = format!("{file}.map");
        files.insert(name.clone(), generator.string());
        writeln!(code, "//# sourceMappingURL={}", tsrs_core::stringutil::encode_uri(&name))
            .unwrap();
        code
    }

    /// Resolve every fragment source index once, decoding declaration maps on first use.
    fn origins(&mut self) -> Vec<Origin<'_>> {
        let keys: Vec<_> = self
            .sources
            .iter()
            .map(|source| {
                let source = tspath::normalize_path(source);
                let map = self.maps.get_key_value(source.as_str());
                if let Some((key, map)) = map {
                    self.decoded.entry(key.as_str()).or_insert_with(|| {
                        tsrs_sourcemap::decode_mappings(&map.source_map.mappings).values().collect()
                    });
                }
                (source, map)
            })
            .collect();
        keys.into_iter()
            .map(|(source, map)| match map {
                Some((key, map)) => Origin::Declaration(map, &self.decoded[key.as_str()]),
                None => Origin::File(source),
            })
            .collect()
    }
}
