use std::sync::Arc;
use tsrs_ast::MappedDiagnosticDirectivePolicy;
use tsrs_ast::SourceFileParseOptions;
use tsrs_ast::storage::*;
use tsrs_ast::storage::{AstStoreBuilder, BindingAttempt, FileAstOwner, SourceFileInfoDraft};
use tsrs_ast::{Kind, NodeFlags};
use tsrs_core::TextRange;
use tsrs_core::tspath::Path;
use tsrs_spanmap::SpanMap;
use tsrs_spanmap::{Feature, Fidelity, MappingErrorKind, Segment};

fn options(name: &str) -> SourceFileParseOptions {
    SourceFileParseOptions {
        file_name: name.to_owned(),
        path: Path::from(name),
        ..SourceFileParseOptions::default()
    }
}

fn info(name: &str, text: &str) -> Arc<SourceFileInfo> {
    Arc::new(SourceFileInfoDraft::new(options(name), Arc::from(text)).unwrap().freeze())
}

fn output(source: Arc<SourceFileInfo>, map: Option<Arc<SpanMap>>) -> MappedSourceOutput {
    MappedSourceOutput::new(source, String::new(), map, Vec::new())
}

fn alias_map(start: u32, end: u32) -> Arc<SpanMap> {
    Arc::new(SpanMap::new_owned(&[Segment {
        virtual_start: start,
        virtual_end: end,
        original_start: 0,
        original_end: 2,
        kind: tsrs_spanmap::Kind::ALIAS,
        features: Feature::All,
    }]))
}

fn directive() -> OwnedMappedDiagnosticDirective {
    OwnedMappedDiagnosticDirective {
        original_range: TextRange::new(0, 2),
        virtual_range: TextRange::new(0, 7),
        policy: MappedDiagnosticDirectivePolicy::Expect,
        unused_code: 2578,
        unused_message_text: "unused expectation".to_owned(),
        source: "mapper".to_owned(),
    }
}

#[test]
fn supplemental_results_keep_metadata_and_mappings_after_all_bodies_retire() {
    let canonical = info("/input.vue", "__alias\n");
    let supplemental = info("/input.vue.0.ts", "let s = __alias;\n");
    let canonical_id = canonical.id();
    let supplemental_id = supplemental.id();
    let weak_canonical = Arc::downgrade(&canonical);
    let weak_supplemental = Arc::downgrade(&supplemental);
    let original: Arc<str> = Arc::from("é\n");
    let weak_original = Arc::downgrade(&original);
    let map = alias_map(8, 15);
    let weak_map = Arc::downgrade(&map);
    let bundle = MappedSourceBundle::new(
        "mapper".to_owned(),
        "transform-v1".to_owned(),
        options("/input.vue"),
        original,
        MappedSourceOutput::new(
            canonical,
            "/input.vue.ts".to_owned(),
            Some(alias_map(0, 7)),
            Vec::new(),
        ),
        vec![MappedSourceOutput::new(
            supplemental,
            "/input.vue.0.ts".to_owned(),
            Some(map),
            vec![directive()],
        )],
    )
    .unwrap();
    let weak_bundle = Arc::downgrade(&bundle);
    let mut owners = Vec::new();
    let mut weak_bodies = Vec::new();
    for source in bundle.source_infos() {
        let mut ast = AstStoreBuilder::new(Arc::clone(source.source())).unwrap();
        let end = u32::try_from(source.text().len()).unwrap();
        ast.new_token(Kind::EndOfFile, NodeFlags::None, TextRange::new(end, end)).unwrap();
        let owner = FileAstOwner::parsed(ast, Arc::default()).unwrap();
        let BindingAttempt::Started(binding) = owner.try_begin_binding().unwrap() else {
            panic!("each file is initially unbound");
        };
        let body = binding.publish().unwrap();
        weak_bodies.push(body.downgrade());
        drop(body);
        owners.push(owner);
    }
    let result = bundle.file(supplemental_id).unwrap();
    drop(bundle);
    for owner in owners {
        owner.retire().unwrap();
    }
    assert!(weak_bodies.iter().all(|body| body.upgrade().is_none()));

    let view = result.view();
    let canonical = view.canonical_source_file().unwrap();
    assert_eq!(canonical.source_info().id(), canonical_id);
    assert_eq!(view.original_file_name(), "/input.vue");
    assert_eq!(view.original_text(), "é\n");
    assert_eq!(view.source_info().id(), supplemental_id);
    assert_eq!(view.source_info().ecma_line_map(), [0, 17]);
    assert_eq!(view.diagnostic_directives(), [directive()]);
    assert_eq!(view.content_mapper_parse_options().path.as_str(), "/input.vue");
    assert_eq!(view.transform_identity(), "transform-v1");
    let map = view.span_map().unwrap();
    assert_eq!(
        map.virtual_to_original_span(TextRange::new(8, 15)),
        (TextRange::new(0, 2), Fidelity::Atom),
    );
    assert_eq!(map.virtual_to_original_span(TextRange::new(0, 3)).1, Fidelity::None);
    assert_eq!(map.original_to_virtual_positions(0, Feature::Hover)[0].position, 8);
    let canonical_result = canonical.retain();
    drop(result);
    assert!(weak_bundle.upgrade().is_some());
    assert_eq!(canonical_result.view().supplemental_source_files().count(), 1);
    drop(canonical_result);
    assert!(weak_bundle.upgrade().is_none());
    assert!(weak_canonical.upgrade().is_none());
    assert!(weak_supplemental.upgrade().is_none());
    assert!(weak_original.upgrade().is_none());
    assert!(weak_map.upgrade().is_none());
}

#[test]
fn mapped_updates_preserve_original_links_and_share_mapping_storage() {
    let canonical = info("/original.vue", "__alias");
    let supplemental = info("/original.vue.0.ts", "__alias");
    let canonical_id = canonical.id();
    let supplemental_id = supplemental.id();
    let map = alias_map(0, 7);
    let bundle = MappedSourceBundle::new(
        "mapper".to_owned(),
        "v1".to_owned(),
        options("/original.vue"),
        Arc::from("é"),
        MappedSourceOutput::new(
            Arc::clone(&canonical),
            "/original.vue.ts".to_owned(),
            Some(Arc::clone(&map)),
            vec![directive()],
        ),
        vec![output(Arc::clone(&supplemental), Some(map))],
    )
    .unwrap();
    let canonical_file = bundle.file(canonical_id).unwrap();
    let supplemental_file = bundle.file(supplemental_id).unwrap();
    let replacement = Arc::new(
        SourceFileInfoDraft::new(options("/updated.vue"), Arc::clone(canonical.source()))
            .unwrap()
            .freeze(),
    );
    let replacement_id = replacement.id();
    let updated = canonical_file.copy_for(replacement).unwrap();
    assert_ne!(replacement_id, canonical_id);
    assert!(updated.view().canonical_source_file().is_none());
    assert_eq!(updated.view().original_file_name(), "/updated.vue");
    assert_eq!(updated.view().virtual_file_name(), "/original.vue.ts");
    assert_eq!(
        updated.view().supplemental_source_files().next().unwrap().source_info().id(),
        supplemental_id,
    );
    assert!(std::ptr::eq(
        updated.view().span_map().unwrap(),
        canonical_file.view().span_map().unwrap(),
    ));
    assert!(std::ptr::eq(
        updated.view().diagnostic_directives(),
        canonical_file.view().diagnostic_directives(),
    ));
    let updated_supplemental =
        supplemental_file.copy_for(info("/updated.vue.0.ts", "__alias")).unwrap();
    assert_eq!(
        updated_supplemental.view().canonical_source_file().unwrap().source_info().id(),
        canonical_id,
    );
    assert_eq!(updated_supplemental.view().original_file_name(), "/original.vue");
    assert_eq!(updated_supplemental.view().supplemental_source_files().count(), 0);
    assert!(
        matches!(bundle.file(replacement_id), Err(SourceFileInfoError::UnknownSource(id)) if id == replacement_id)
    );
    let changed = info("/changed.vue", "different");
    assert!(matches!(
        canonical_file.copy_for(Arc::clone(&changed)),
        Err(SourceFileInfoError::DifferentSourceText { original, replacement })
            if original == canonical_id && replacement == changed.id()
    ));
}

#[test]
fn mapped_publication_rejects_wrong_members_and_maps_and_preserves_nil_vs_empty() {
    let source = info("/source.ts", "a");
    let id = source.id();
    let duplicate = MappedSourceBundle::new(
        "mapper".to_owned(),
        String::new(),
        options("/source.ts"),
        Arc::from("a"),
        output(Arc::clone(&source), None),
        vec![output(Arc::clone(&source), None)],
    );
    assert!(matches!(duplicate, Err(SourceFileInfoError::DuplicateSource(found)) if found == id));

    let bad = info("/bad.ts", "__alias");
    let bad_id = bad.id();
    let invalid = MappedSourceBundle::new(
        "mapper".to_owned(),
        String::new(),
        options("/source.ts"),
        Arc::from("a"),
        output(Arc::clone(&source), None),
        vec![output(bad, Some(alias_map(0, 7)))],
    );
    assert!(matches!(invalid, Err(SourceFileInfoError::InvalidMapping { source, error })
        if source == bad_id && error.kind == MappingErrorKind::OutOfBounds));
    let synthesized = info("/synthesized.ts", "generated");
    let synthesized_id = synthesized.id();
    let bundle = MappedSourceBundle::new(
        "mapper".to_owned(),
        String::new(),
        options("/source.ts"),
        Arc::from("a"),
        output(source, None),
        vec![output(synthesized, Some(Arc::new(SpanMap::new_owned(&[]))))],
    )
    .unwrap();
    let stub = bundle.file(id).unwrap();
    assert!(stub.view().is_content_mapper_failure_stub());
    assert_eq!(
        tsrs_spanmap::virtual_to_original_span(stub.view().span_map(), TextRange::new(0, 1)),
        (TextRange::new(0, 1), Fidelity::Exact),
    );
    let empty = bundle.file(synthesized_id).unwrap();
    assert!(!empty.view().is_content_mapper_failure_stub());
    assert_eq!(
        empty.view().span_map().unwrap().virtual_to_original_span(TextRange::new(0, 1)).1,
        Fidelity::None
    );
    // Equal names/text do not identify the same source-file version.
    let unrelated = info("/source.ts", "a");
    assert_ne!(unrelated.id(), id);
    assert!(matches!(bundle.file(unrelated.id()), Err(SourceFileInfoError::UnknownSource(_))));
    let unnamed = MappedSourceBundle::new(
        String::new(),
        String::new(),
        options("/unnamed.ts"),
        Arc::from("original"),
        output(Arc::clone(&unrelated), None),
        Vec::new(),
    )
    .unwrap();
    let unnamed = unnamed.file(unrelated.id()).unwrap();
    assert_eq!(unnamed.view().original_text(), "a");
    assert!(!unnamed.view().is_content_mapper_failure_stub());
}

#[test]
fn mapped_memory_counts_shared_maps_once_including_late_interval_indexes() {
    let first = info("/first.ts", "a");
    let second = info("/second.ts", "a");
    let map = Arc::new(SpanMap::new_owned(&[Segment {
        virtual_start: 0,
        virtual_end: 1,
        original_start: 0,
        original_end: 1,
        kind: tsrs_spanmap::Kind::VERBATIM,
        features: Feature::All,
    }]));
    let bundle = MappedSourceBundle::new(
        "mapper".to_owned(),
        "v1".to_owned(),
        options("/first.ts"),
        Arc::from("a"),
        MappedSourceOutput::new(
            first,
            "first".to_owned(),
            Some(Arc::clone(&map)),
            vec![directive()],
        ),
        vec![MappedSourceOutput::new(
            second,
            "second".to_owned(),
            Some(Arc::clone(&map)),
            Vec::new(),
        )],
    )
    .unwrap();
    let before = bundle.memory();
    let map_memory = map.memory();
    assert_eq!(before.source_infos, 2);
    assert_eq!(before.outputs.live_bytes, 2 * size_of::<MappedSourceOutput>());
    assert_eq!(
        before.output_data.live_bytes,
        "first".len()
            + "second".len()
            + size_of::<OwnedMappedDiagnosticDirective>()
            + "unused expectation".len()
            + "mapper".len()
            + map_memory.inline_bytes
            + map_memory.live_bytes,
    );
    assert_eq!(map.original_to_virtual_positions(0, Feature::Hover)[0].position, 0);
    let after = bundle.memory();
    let initialized = map.memory();
    assert_eq!(
        after.output_data.live_bytes - before.output_data.live_bytes,
        initialized.live_bytes - map_memory.live_bytes
    );
    assert_eq!(after.output_data.allocations - before.output_data.allocations, 2);
}
