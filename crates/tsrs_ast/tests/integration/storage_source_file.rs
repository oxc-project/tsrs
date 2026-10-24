use std::sync::Arc;
use std::sync::Barrier;
use tsrs_ast::{
    CheckJsDirective, CommentDirective, FileReference, Pragma, SourceFileLike,
    SourceFileParseOptions,
};
use tsrs_core::tspath::Path;
use tsrs_core::{LanguageVariant, ScriptKind, Tristate};

use tsrs_core::TextRange;

use tsrs_ast::storage::*;
use tsrs_ast::storage::{
    AstDirectory, AstStoreBuilder, BindingAttempt, FileAstOwner, FileAstPhase,
};
use tsrs_ast::{CommentDirectiveKind, CommentRange, Kind, NodeFlags, PragmaArgument};

fn draft(file_name: &str, text: Arc<str>) -> SourceFileInfoDraft {
    SourceFileInfoDraft::new(
        SourceFileParseOptions {
            file_name: file_name.to_owned(),
            path: Path::from(file_name),
            ..SourceFileParseOptions::default()
        },
        text,
    )
    .unwrap()
}

fn reference(name: &str) -> FileReference {
    FileReference {
        text_range: TextRange::new(0, 1),
        file_name: name.to_owned(),
        resolution_mode: tsrs_core::ModuleKind::COMMON_JS,
        preserve: true,
    }
}

fn check_js(enabled: bool) -> CheckJsDirective {
    CheckJsDirective {
        enabled,
        range: CommentRange {
            text_range: TextRange::new(0, 1),
            kind: Kind::SingleLineCommentTrivia,
            has_trailing_new_line: true,
        },
    }
}

#[test]
fn source_version_ids_survive_freezing_without_reuse() {
    let original = draft("/original.ts", Arc::from("same"));
    let id = original.info().id();
    let original = original.freeze();
    assert_eq!(original.id(), id);
    let mut updated = draft("/original.ts", Arc::clone(original.source()));
    updated.copy_parse_metadata_from(&original);
    assert_ne!(updated.info().id(), id);
}

#[test]
fn durable_source_info_survives_body_retirement_without_retaining_syntax() {
    let source: Arc<str> = Arc::from("é\r\n😀\u{2028}end");
    let weak_source = Arc::downgrade(&source);
    let mut metadata = draft("/source.ts", Arc::clone(&source));
    metadata.parse_results_mut().script_kind = ScriptKind::TS;
    metadata.directives_mut().referenced_files.push(reference("dependency.ts"));
    metadata.set_hash(123);
    let info = Arc::new(metadata.freeze());
    assert!(Arc::ptr_eq(&source, info.source()));

    let mut ast = AstStoreBuilder::new(Arc::clone(info.source())).unwrap();
    let end = u32::try_from(source.len()).unwrap();
    let statements = ast.new_node_list(TextRange::new(0, end), &[]).unwrap();
    let eof = ast.new_token(Kind::EndOfFile, NodeFlags::None, TextRange::new(end, end)).unwrap();
    let root =
        ast.new_source_file(NodeFlags::None, TextRange::new(0, end), statements, eof).unwrap();
    let directory = Arc::new(AstDirectory::default());
    let owner = FileAstOwner::parsed(ast, Arc::clone(&directory)).unwrap();
    let weak_owner = Arc::downgrade(&owner);
    let BindingAttempt::Started(binding) = owner.try_begin_binding().unwrap() else {
        panic!("first binding takes the draft");
    };
    let lease = binding.publish().unwrap();
    let weak_body = lease.downgrade();
    owner.retire().unwrap();
    assert_eq!(owner.phase(), Ok(FileAstPhase::Retired));
    assert!(lease.node(root).unwrap().as_source_file().is_ok());
    drop(lease);
    assert!(weak_body.upgrade().is_none());
    drop(owner);
    drop(source);
    assert!(weak_owner.upgrade().is_none());

    // Both maps are first initialized after syntax, bindings and entry died.
    assert_eq!(info.file_name(), "/source.ts");
    assert_eq!(info.hash(), 123);
    assert_eq!(info.directives().referenced_files[0].file_name, "dependency.ts");
    assert_eq!(info.ecma_line_count(), 3);
    assert_eq!(info.memory().text_maps.allocations, 0);
    let source_like: &dyn SourceFileLike = info.as_ref();
    assert_eq!(source_like.ecma_line_map(), [0, 4, 11]);
    assert_eq!(source_like.text(), "é\r\n😀\u{2028}end");
    assert_eq!(info.get_position_map().utf8_to_utf16(11), 6);
    assert!(weak_source.upgrade().is_some());
    drop(info);
    assert!(weak_source.upgrade().is_none());
    directory.prune_expired().unwrap();
}

#[test]
fn source_maps_publish_once_across_readers_and_borrow_the_retaining_info() {
    let info = draft("/unicode.ts", Arc::from("\u{feff}a\r\n😀é\u{2028}z\u{2029}")).freeze();
    assert_eq!(info.ecma_line_count(), 4);
    assert_eq!(info.memory().text_maps.allocations, 0);
    let barrier = Barrier::new(4);
    std::thread::scope(|scope| {
        let readers: Vec<_> = std::iter::repeat_with(|| {
            scope.spawn(|| {
                barrier.wait();
                let lines = info.ecma_line_map();
                let positions = info.get_position_map();
                let mut utf16 = 0;
                for (byte, ch) in info.text().char_indices() {
                    let byte = u32::try_from(byte).unwrap();
                    assert_eq!(positions.utf8_to_utf16(byte), utf16);
                    assert_eq!(positions.utf16_to_utf8(utf16), byte);
                    utf16 += u32::try_from(ch.len_utf16()).unwrap();
                }
                assert_eq!(positions.utf16_to_utf8(utf16), info.text().len() as u32);
                (lines, positions)
            })
        })
        .take(4)
        .collect();
        for reader in readers {
            let (lines, positions) = reader.join().unwrap();
            assert_eq!(lines, [0, 6, 15, 19]);
            assert!(std::ptr::eq(lines, info.ecma_line_map()));
            assert!(std::ptr::eq(positions, info.get_position_map()));
        }
    });
    assert_eq!(info.memory().text_maps.allocations, 2);
}

#[test]
fn source_file_updates_copy_only_the_existing_metadata_subset() {
    let mut original = draft("/original.ts", Arc::from("old\ntext"));
    *original.parse_results_mut() = SourceFileParseResults {
        language_variant: LanguageVariant::JSX,
        script_kind: ScriptKind::TSX,
        is_declaration_file: true,
        uses_uri_style_node_core_modules: Tristate::True,
        identifier_count: 5,
        node_count: 10,
        text_count: 15,
    };
    original.set_hash(123);
    let directives = original.directives_mut();
    directives.ambient_module_names.push("ambient".to_owned());
    directives
        .comment_directives
        .push(CommentDirective { loc: TextRange::new(0, 1), kind: CommentDirectiveKind::Ignore });
    directives.pragmas.push(Pragma {
        comment_range: check_js(true).range,
        name: "reference".to_owned(),
        args: std::iter::once((
            "path".to_owned(),
            PragmaArgument {
                text_range: TextRange::new(0, 1),
                name: "path".to_owned(),
                value: "dependency.ts".to_owned(),
            },
        ))
        .collect(),
    });
    directives.referenced_files.push(reference("dependency.ts"));
    directives.type_reference_directives.push(reference("types"));
    directives.lib_reference_directives.push(reference("esnext"));
    original.set_check_js_directive(Some(check_js(true)));
    let original = original.freeze();
    original.ecma_line_map();
    original.get_position_map();

    let mut updated = draft("/updated.ts", Arc::from("é"));
    updated.copy_parse_metadata_from(&original);
    assert!(std::ptr::eq(updated.info().directives(), original.directives()));
    assert_eq!(updated.info().parse_results().node_count, 0);
    assert_eq!(updated.info().parse_results().identifier_count, 0);
    assert_eq!(updated.info().parse_results().text_count, 0);
    assert_eq!(updated.info().hash(), 0);
    assert_eq!(updated.info().check_js_directive(), None);
    assert_eq!(updated.info().memory().text_maps.allocations, 0);
    drop(original);
    let info = updated.info();
    assert_eq!(info.file_name(), "/updated.ts");
    assert_eq!(info.path().as_str(), "/updated.ts");
    assert_eq!(info.text(), "é");
    assert_eq!(info.parse_results().script_kind, ScriptKind::TSX);
    assert!(info.parse_results().is_declaration_file);
    let directives = info.directives();
    assert_eq!(directives.ambient_module_names, ["ambient"]);
    assert_eq!(directives.comment_directives[0].kind, CommentDirectiveKind::Ignore);
    assert_eq!(directives.pragmas[0].args["path"].value, "dependency.ts");
    assert_eq!(directives.referenced_files, [reference("dependency.ts")]);
    assert_eq!(directives.type_reference_directives, [reference("types")]);
    assert_eq!(directives.lib_reference_directives, [reference("esnext")]);
    assert_eq!(info.ecma_line_map(), [0]);
    assert_eq!(info.get_position_map().utf8_to_utf16(2), 1);

    // Copy omissions preserve a target's values rather than resetting them.
    updated.set_hash(456);
    updated.parse_results_mut().node_count = 20;
    updated.set_check_js_directive(Some(check_js(false)));
    let empty = draft("/empty.ts", Arc::from("\n")).freeze();
    updated.copy_parse_metadata_from(&empty);
    let updated = updated.freeze();
    assert_eq!(updated.hash(), 456);
    assert_eq!(updated.parse_results().node_count, 20);
    assert_eq!(updated.check_js_directive(), Some(&check_js(false)));
    assert!(updated.directives().is_empty());
    assert_eq!(updated.ecma_line_map(), [0]);
    assert_eq!(updated.get_position_map().utf8_to_utf16(2), 1);
}

#[test]
fn source_file_updates_share_directives_until_an_exclusive_draft_edits_them() {
    let mut original = draft("/original.ts", Arc::from("original"));
    original.directives_mut().ambient_module_names.push("ambient".to_owned());
    original.directives_mut().referenced_files.push(reference("reference.ts"));
    original.set_check_js_directive(Some(check_js(true)));
    let original = original.freeze();
    let weak_directives = original.directives_weak().unwrap();

    let mut updated = draft("/updated.ts", Arc::clone(original.source()));
    let mut sibling = draft("/sibling.ts", Arc::clone(original.source()));
    updated.copy_parse_metadata_from(&original);
    sibling.copy_parse_metadata_from(&original);
    assert!(std::ptr::eq(original.directives(), updated.info().directives()));
    assert!(std::ptr::eq(original.directives(), sibling.info().directives()));
    assert_eq!(updated.info().check_js_directive(), None);
    assert_eq!(sibling.info().check_js_directive(), None);

    updated.directives_mut().ambient_module_names[0].push_str("_changed");
    updated.directives_mut().referenced_files.push(reference("added.ts"));
    assert!(!std::ptr::eq(original.directives(), updated.info().directives()));
    assert_eq!(original.directives().ambient_module_names, ["ambient"]);
    assert_eq!(original.directives().referenced_files.len(), 1);
    assert!(std::ptr::eq(original.directives(), sibling.info().directives()));
    drop(original);
    let sibling = sibling.freeze();
    assert!(weak_directives.upgrade().is_some());
    assert_eq!(sibling.directives().referenced_files, [reference("reference.ts")]);
    drop(sibling);
    assert!(weak_directives.upgrade().is_none());
    let updated = updated.freeze();
    assert_eq!(updated.directives().ambient_module_names, ["ambient_changed"]);
    assert_eq!(updated.directives().referenced_files.len(), 2);
}

#[test]
fn metadata_accounting_exposes_optional_headers_owned_strings_maps_and_capacity() {
    let mut info = draft("/empty.ts", Arc::from("ascii"));
    let before = info.info().memory();
    assert_eq!(before.directives, AstBufferMemory::default());
    assert_eq!(before.text_maps, AstBufferMemory::default());
    info.directives_mut().ambient_module_names.reserve(100);
    assert!(info.info().memory().directives.capacity_bytes > 0);
    let info = info.freeze();
    // Touching a draft's empty directive block must not retain spare buffers.
    assert_eq!(info.memory(), before);
    info.get_position_map();
    assert_eq!(info.memory().text_maps, AstBufferMemory::default());

    let mut info = draft("/directives.ts", Arc::from("é\n"));
    let data = info.directives_mut();
    let mut ambient = String::with_capacity(64);
    ambient.push_str("name");
    data.ambient_module_names.push(ambient);
    data.referenced_files.push(reference("ref"));
    data.pragmas.push(Pragma {
        comment_range: check_js(true).range,
        name: "pragma".to_owned(),
        args: std::iter::once((
            "arg".to_owned(),
            PragmaArgument {
                text_range: TextRange::new(0, 1),
                name: "name".to_owned(),
                value: "value".to_owned(),
            },
        ))
        .collect(),
    });
    let expected_live = size_of::<SourceFileDirectives>()
        + size_of::<String>()
        + 4
        + size_of::<FileReference>()
        + 3
        + size_of::<Pragma>()
        + 6
        + 3
        + 4
        + 5;
    let memory = info.info().memory();
    assert_eq!(memory.inline_bytes, size_of::<SourceFileInfoDraft>());
    assert_eq!(memory.directives.live_bytes, expected_live);
    assert!(memory.directives.capacity_bytes >= expected_live + 60);
    assert_eq!(memory.directives.allocations, 10);
    assert_eq!(memory.maps.allocated_maps, 1);
    assert_eq!(memory.maps.live_payload_bytes, size_of::<(String, PragmaArgument)>());
    assert_eq!(memory.source_bytes, 3);
    assert_eq!(memory.path_bytes, "/directives.ts".len());
    info.set_check_js_directive(Some(check_js(true)));
    assert_eq!(
        info.info().memory().check_js_directive,
        AstBufferMemory {
            live_bytes: size_of::<CheckJsDirective>(),
            capacity_bytes: size_of::<CheckJsDirective>(),
            allocations: 1
        },
    );
    info.set_check_js_directive(None);
    assert_eq!(info.info().memory(), memory);
    info.info().ecma_line_map();
    info.info().get_position_map();
    let maps = info.info().memory().text_maps;
    assert_eq!(maps.live_bytes, 2 * size_of::<u32>() + 2 * size_of::<u32>());
    assert!(maps.capacity_bytes >= maps.live_bytes);
    assert_eq!(maps.allocations, 2);
}
