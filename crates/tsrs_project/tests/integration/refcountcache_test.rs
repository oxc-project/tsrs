// Port of the content-mapped parse cache cases of project/refcountcache_test.go.

use tsrs_ast::{ContentMapperSourceFileInfo, SourceFileParseOptions};
use tsrs_compiler::DuplicateSourceFile;
use tsrs_contentmapper::SourceFiles;
use tsrs_core::ScriptKind;
use tsrs_core::tspath::Path;
use tsrs_project::parsecache::{
    ContentMappedParseCacheKey, content_mapped_parse_cache_key_for_duplicate,
    content_mapped_parse_cache_key_for_file, new_content_mapped_parse_cache,
};
use tsrs_project::refcountcache::RefCountCacheOptions;

fn parse_options(file_name: &str) -> SourceFileParseOptions {
    SourceFileParseOptions {
        file_name: file_name.to_string(),
        path: Path::new(file_name.to_string()),
        ..Default::default()
    }
}

// refcountcache_test.go:22
#[test]
fn content_mapped_parse_cache_bundle_lifetime() {
    let cache = new_content_mapped_parse_cache(RefCountCacheOptions::default());
    let key = ContentMappedParseCacheKey {
        source_file_parse_options: parse_options("/component.vue"),
        hash: 0,
    };

    // The cache owns the complete transform result as one value, so reuse preserves every file's identity.
    let acquired = cache
        .acquire_or_error(&key, 0, || {
            let canonical =
                tsrs_parser::parse_source_file(parse_options("/component.vue"), "", ScriptKind::TS);
            let supplemental = tsrs_parser::parse_source_file(
                parse_options("/component.vue.0.ts"),
                "",
                ScriptKind::TS,
            );
            Ok::<_, ()>(SourceFiles {
                canonical: Some(canonical),
                supplemental: vec![supplemental],
            })
        })
        .unwrap();
    let reused = cache
        .acquire_or_error(&key, 0, || -> Result<SourceFiles, ()> {
            panic!("cached bundle should be reused")
        })
        .unwrap();
    assert_eq!(reused.canonical, acquired.canonical);
    assert_eq!(reused.supplemental[0], acquired.supplemental[0]);

    // Canonical and supplemental files share the bundle's refcount and disappear after its final release.
    cache.deref(&key);
    assert!(cache.has(&key));
    cache.deref(&key);
    assert!(!cache.has(&key));
}

// refcountcache_test.go:49
#[test]
fn content_mapped_parse_cache_key_reconstruction() {
    let acquire_options = parse_options("/component.box");
    let mut mapped_options = acquire_options.clone();
    mapped_options.external_module_indicator_options.force = true;
    let hash = xxhash_rust::xxh3::xxh3_128(b"cache key");
    let file = tsrs_parser::parse_source_file(mapped_options.clone(), "export {};", ScriptKind::TS);
    file.hash.set(hash);
    file.set_content_mapper_info(ContentMapperSourceFileInfo {
        content_mapper: "mapper",
        parse_options: acquire_options.clone(),
        ..Default::default()
    });
    let expected =
        ContentMappedParseCacheKey { source_file_parse_options: acquire_options.clone(), hash };
    assert_eq!(content_mapped_parse_cache_key_for_file(file), expected);

    let duplicate = DuplicateSourceFile {
        parse_options: mapped_options,
        content_mapper_parse_options: acquire_options,
        hash,
        script_kind: ScriptKind::Unknown,
        content_mapper: "mapper",
        is_content_mapper_failure_stub: false,
    };
    assert_eq!(content_mapped_parse_cache_key_for_duplicate(&duplicate), expected);
}
