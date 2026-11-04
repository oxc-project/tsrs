// Port of project/contentmapper_test.go: content-mapped files in the project system, through the in-process test
// mappers of tsrs_contentmappertest. TestContentMapperInferredProjectSurvivesTypingsInstall is not ported: it needs
// automatic type acquisition, which tsrs does not port.

use std::io::Write;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Once};

use tsrs_contentmapper::{Definition, Manifest, Mapper, Spawner};
use tsrs_contentmappertest as contentmappertest;
use tsrs_core::Locale;
use tsrs_core::context::{Context, with_locale};
use tsrs_core::tspath::Path;
use tsrs_ipc::{Closer, ReadWriteCloser};
use tsrs_lsproto as lsproto;
use tsrs_project::session::{ContentMapperContributions, Session, SessionOptions, new_session};
use tsrs_project::{Kind, ProgramUpdateKind};
use tsrs_vfs::bundled;

use crate::projecttestutil::{SessionUtils, TEST_TYPINGS_LOCATION, get_session_init_options};

// contentmapper_test.go:27
struct RecordingContentMapperSpawner {
    inner: Arc<dyn Spawner>,
    spawns: AtomicI32,
    closes: Arc<AtomicI32>,
}

fn recording_spawner() -> Arc<RecordingContentMapperSpawner> {
    Arc::new(RecordingContentMapperSpawner {
        inner: contentmappertest::new_spawner(),
        spawns: AtomicI32::new(0),
        closes: Arc::new(AtomicI32::new(0)),
    })
}

impl Spawner for RecordingContentMapperSpawner {
    // contentmapper_test.go:33
    fn spawn(
        &self,
        command: &[String],
        dir: &str,
        stderr: Option<Box<dyn Write + Send>>,
    ) -> Result<ReadWriteCloser, String> {
        let process = self.inner.spawn(command, dir, stderr)?;
        self.spawns.fetch_add(1, Ordering::SeqCst);
        Ok(ReadWriteCloser {
            reader: process.reader,
            writer: process.writer,
            closer: Arc::new(RecordingContentMapperProcess {
                inner: process.closer,
                closes: Arc::clone(&self.closes),
                once: Once::new(),
            }),
        })
    }
}

// contentmapper_test.go:42
struct RecordingContentMapperProcess {
    inner: Arc<dyn Closer>,
    closes: Arc<AtomicI32>,
    once: Once,
}

impl Closer for RecordingContentMapperProcess {
    // contentmapper_test.go:48
    fn close(&self) -> std::io::Result<()> {
        self.once.call_once(|| {
            self.closes.fetch_add(1, Ordering::SeqCst);
        });
        self.inner.close()
    }

    fn exit_code(&self) -> Option<i32> {
        self.inner.exit_code()
    }
}

fn ctx() -> Context {
    Context::background()
}

fn uri(s: &str) -> lsproto::DocumentUri {
    lsproto::DocumentUri(s.to_string())
}

fn session_options(current_directory: &str, run_external_code: bool) -> SessionOptions {
    SessionOptions {
        current_directory: current_directory.to_string(),
        default_library_path: bundled::lib_path(),
        typings_location: TEST_TYPINGS_LOCATION.to_string(),
        position_encoding: lsproto::PositionEncodingKind::UTF8,
        run_external_code,
        ..Default::default()
    }
}

// A session over `files` whose content mappers are served by `spawner` (Go sets `init.Spawner`).
fn new_mapper_session(
    files: &[(&str, &str)],
    options: SessionOptions,
    spawner: Arc<dyn Spawner>,
) -> (Arc<Session>, SessionUtils) {
    let (mut init, utils) = get_session_init_options(files, Some(options));
    init.spawner = Some(spawner);
    (new_session(&init), utils)
}

fn file<'a>(files: &[(&str, &'a str)], name: &str) -> &'a str {
    files.iter().find(|(k, _)| *k == name).unwrap().1
}

fn whole(text: &str) -> Vec<lsproto::TextDocumentContentChangePartialOrWholeDocument> {
    vec![lsproto::TextDocumentContentChangePartialOrWholeDocument {
        whole_document: Some(lsproto::TextDocumentContentChangeWholeDocument {
            text: text.to_string(),
        }),
        ..Default::default()
    }]
}

fn changed(name: &str) -> lsproto::FileEvent {
    lsproto::FileEvent {
        uri: uri(&format!("file://{name}")),
        type_: lsproto::FileChangeType::CHANGED,
    }
}

fn extension_contribution(package_directory: &str) -> ContentMapperContributions {
    ContentMapperContributions {
        mappers: vec![Mapper {
            definition: Definition {
                package: "test.extension".to_string(),
                extensions: vec![".box".to_string()],
                ..Default::default()
            },
            manifest: Manifest {
                name: "mapper".to_string(),
                version: "1.0.0".to_string(),
                exec: vec![contentmappertest::TRANSFORMING_MAPPER.to_string()],
                compiler_options: contentmappertest::DECLARED_OPTIONS
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
                ..Default::default()
            },
            package_directory: package_directory.to_string(),
            contribution_id: "test.extension[0]".to_string(),
        }],
        extensions: vec![".box".to_string()],
    }
}

// contentmapper_test.go:53
#[test]
fn content_mapper_project_without_mapped_files() {
    for has_mapper in [false, true] {
        let mut config = r#"{"compilerOptions": {"noLib": true}}"#.to_string();
        if has_mapper {
            config = r#"{
					"compilerOptions": { "noLib": true },
					"contentMappers": [ { "package": "mapper", "extensions": [".box"] } ]
				}"#
            .to_string();
        }
        let manifest = contentmappertest::package_json(contentmappertest::TRANSFORMING_MAPPER);
        let files = [
            ("/home/project/tsconfig.json", config.as_str()),
            ("/home/project/node_modules/mapper/package.json", manifest.as_str()),
            ("/home/project/main.ts", "export {};"),
        ];
        let spawner = recording_spawner();
        let (session, _) = new_mapper_session(
            &files,
            SessionOptions {
                typings_location: String::new(),
                ..session_options("/home/project", true)
            },
            Arc::clone(&spawner) as Arc<dyn Spawner>,
        );

        let main = uri("file:///home/project/main.ts");
        session.did_open_file(
            &ctx(),
            main.clone(),
            1,
            file(&files, "/home/project/main.ts").to_string(),
            lsproto::LanguageKind::TYPE_SCRIPT,
        );
        let language_service = session.get_language_service(&ctx(), &main).unwrap();
        // Access after freezing must not try to initialize using the cleared builder.
        let program = language_service.get_program();
        let mapper_project = program.content_mapper_project();
        assert_eq!(mapper_project.is_some(), has_mapper, "hasMapper={has_mapper}");
        assert_eq!(
            program.content_mapper_project().map(|p| Arc::as_ptr(&p).cast::<()>()),
            mapper_project.map(|p| Arc::as_ptr(&p).cast::<()>())
        );
        assert_eq!(spawner.spawns.load(Ordering::SeqCst), 0);
        session.close();
    }
}

// contentmapper_test.go:95
#[test]
fn content_mapper_parallel_file_loading() {
    // Parallel parsing reads the mapper project identity while another file initializes it.
    const FILE_COUNT: usize = 32;
    let manifest = contentmappertest::package_json(contentmappertest::TRANSFORMING_MAPPER);
    let mut files: Vec<(String, String)> = vec![
        (
            "/home/project/tsconfig.json".to_string(),
            r#"{
			"compilerOptions": { "target": "es2020", "noLib": true },
			"contentMappers": [ { "package": "mapper", "extensions": [".box"] } ]
		}"#
            .to_string(),
        ),
        ("/home/project/node_modules/mapper/package.json".to_string(), manifest),
        ("/home/project/main.ts".to_string(), "export {};".to_string()),
    ];
    for i in 0..FILE_COUNT {
        files.push((
            format!("/home/project/file{i}.box"),
            "export const version = #{target};\n".to_string(),
        ));
    }
    let files: Vec<(&str, &str)> = files.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let (session, _) = new_mapper_session(
        &files,
        SessionOptions {
            typings_location: String::new(),
            ..session_options("/home/project", true)
        },
        contentmappertest::new_spawner(),
    );

    let main = uri("file:///home/project/main.ts");
    session.did_open_file(
        &ctx(),
        main.clone(),
        1,
        "export {};".to_string(),
        lsproto::LanguageKind::TYPE_SCRIPT,
    );
    let language_service = session.get_language_service(&ctx(), &main).unwrap();
    let mapper_project = language_service.get_program().content_mapper_project();
    assert!(mapper_project.is_some());
    for i in 0..FILE_COUNT {
        let file_name = format!("/home/project/file{i}.box");
        let file = language_service.get_program().get_source_file(&file_name);
        assert!(file.is_some(), "expected {file_name} to be loaded");
        assert_eq!(
            file.unwrap().text(),
            "const __VERSION = \"1.0.0\";\nexport const version = 7;\n"
        );
    }
    session.close();
}

const IN_PROJECT_CONFIG: &str = r#"{
			"compilerOptions": { "target": "es2020", "module": "esnext", "moduleResolution": "bundler", "strict": true },
			"contentMappers": [ { "package": "mapper", "extensions": [".box"] } ]
		}"#;
const IN_PROJECT_APP: &str = "export const version = #{target};\n";
const IN_PROJECT_MAIN: &str =
    "import { version } from \"./app.box\";\nexport const twice: number = version * 2;\n";

// contentmapper_test.go:148 (newSession)
fn in_project_session(trusted: bool) -> (Arc<Session>, SessionUtils) {
    let manifest = contentmappertest::package_json(contentmappertest::TRANSFORMING_MAPPER);
    let files = [
        ("/home/project/tsconfig.json", IN_PROJECT_CONFIG),
        ("/home/project/node_modules/mapper/package.json", manifest.as_str()),
        ("/home/project/app.box", IN_PROJECT_APP),
        ("/home/project/main.ts", IN_PROJECT_MAIN),
    ];
    new_mapper_session(
        &files,
        SessionOptions { logging_enabled: true, ..session_options("/home/project", trusted) },
        contentmappertest::new_spawner(),
    )
}

// contentmapper_test.go:160
#[test]
fn content_mapper_in_project_trusted_workspace_transforms_the_content_mapped_file() {
    let (session, utils) = in_project_session(true);
    let main = uri("file:///home/project/main.ts");
    session.did_open_file(
        &ctx(),
        main.clone(),
        1,
        IN_PROJECT_MAIN.to_string(),
        lsproto::LanguageKind::TYPE_SCRIPT,
    );
    let ls = session.get_language_service(&ctx(), &main).unwrap();

    let box_file = ls.get_program().get_source_file("/home/project/app.box");
    assert!(box_file.is_some(), "expected app.box to be loaded into the program");
    // The #{target} token was substituted with the es2020 target value (7) by the content mapper.
    let text = box_file.unwrap().text();
    assert!(text.contains("export const version = 7;"), "app.box was not transformed: {text:?}");

    // The config's .box mapper should have been registered for text document synchronization.
    session.wait_for_background_tasks();
    let calls = utils.client().register_content_mapper_extensions_calls();
    assert!(!calls.is_empty(), "expected RegisterContentMapperExtensions to be called");
    assert_eq!(calls.last().unwrap(), &vec![".box".to_string()]);
    session.close();
}

// contentmapper_test.go:181
#[test]
fn content_mapper_in_project_untrusted_workspace_does_not_run_the_content_mapper() {
    let (session, utils) = in_project_session(false);
    let main = uri("file:///home/project/main.ts");
    session.did_open_file(
        &ctx(),
        main.clone(),
        1,
        IN_PROJECT_MAIN.to_string(),
        lsproto::LanguageKind::TYPE_SCRIPT,
    );
    let ls = session.get_language_service(&ctx(), &main).unwrap();

    // Without workspace trust, the content mapper gate drops the mappers, so .box is not a recognized
    // extension and app.box never enters the program.
    assert!(
        ls.get_program().get_source_file("/home/project/app.box").is_none(),
        "app.box should not be loaded without trust"
    );

    // No content mapper extensions should be registered without trust.
    session.wait_for_background_tasks();
    for call in utils.client().register_content_mapper_extensions_calls() {
        assert!(
            call.is_empty(),
            "expected no content mapper extensions to be registered without trust"
        );
    }
    session.close();
}

// contentmapper_test.go:202
#[test]
fn content_mapper_in_project_editing_an_open_content_mapped_file_reparses_it_through_the_mapper() {
    let (session, _) = in_project_session(true);
    let main = uri("file:///home/project/main.ts");
    let app = uri("file:///home/project/app.box");
    session.did_open_file(
        &ctx(),
        main.clone(),
        1,
        IN_PROJECT_MAIN.to_string(),
        lsproto::LanguageKind::TYPE_SCRIPT,
    );
    // Open the .box with its content-mapped language id so its overlay script kind is Unknown, matching how an
    // editor opens a content-mapped file. This is what made the incremental reparse panic.
    session.did_open_file(
        &ctx(),
        app.clone(),
        1,
        IN_PROJECT_APP.to_string(),
        lsproto::LanguageKind("box"),
    );
    session.get_language_service(&ctx(), &main).unwrap();

    // Editing the open .box file drives the single-file incremental reparse path
    // (Program.UpdateProgram), which must re-run the content mapper transform rather than parse the
    // raw source text.
    session.did_change_file(
        &ctx(),
        app,
        2,
        whole("export const version = #{target};\nexport const extra = 1;\n"),
    );
    let ls = session.get_language_service(&ctx(), &main).unwrap();

    let box_file = ls.get_program().get_source_file("/home/project/app.box");
    assert!(box_file.is_some(), "expected app.box to be loaded");
    let text = box_file.unwrap().text();
    assert!(
        text.contains("export const version = 7;"),
        "reparsed app.box was not transformed: {text:?}"
    );
    assert!(
        text.contains("export const extra = 1;"),
        "reparsed app.box missing the edit: {text:?}"
    );
    session.close();
}

// contentmapper_test.go:230
#[test]
fn content_mapper_in_project_watch_change_to_a_content_mapped_file_updates_the_program() {
    let (session, utils) = in_project_session(true);
    let main = uri("file:///home/project/main.ts");
    session.did_open_file(
        &ctx(),
        main.clone(),
        1,
        IN_PROJECT_MAIN.to_string(),
        lsproto::LanguageKind::TYPE_SCRIPT,
    );
    let language_service = session.get_language_service(&ctx(), &main).unwrap();
    let original = language_service.get_program().get_source_file("/home/project/app.box");
    assert!(original.is_some(), "expected app.box to be loaded");

    // Wait until the configured extension set has been published; watch filtering uses the set captured
    // when the snapshot change is created.
    session.wait_for_background_tasks();
    let updated_content = "export const version = #{target};\nexport const watched = true;\n";
    utils.fs().write_file("/home/project/app.box", updated_content).unwrap();
    session.did_change_watched_files(&ctx(), &[changed("/home/project/app.box")]);

    let language_service = session.get_language_service(&ctx(), &main).unwrap();
    let updated_snapshot = session.snapshot();
    let configured_project = updated_snapshot.get_default_project(&main);
    assert!(configured_project.is_some(), "expected configured project");
    let configured_project = configured_project.unwrap();
    assert_eq!(configured_project.program_update_kind, ProgramUpdateKind::Cloned);
    assert_eq!(configured_project.program_last_update, updated_snapshot.id());
    let updated = language_service.get_program().get_source_file("/home/project/app.box");
    assert!(updated.is_some(), "expected app.box to remain loaded");
    assert!(updated != original, "expected the watched content-mapped file to be reparsed");
    let text = updated.unwrap().text();
    assert!(
        text.contains("export const version = 7;"),
        "updated app.box was not transformed: {text:?}"
    );
    assert!(
        text.contains("export const watched = true;"),
        "updated app.box missing watched change: {text:?}"
    );
    session.close();
}

// contentmapper_test.go:267
#[test]
fn content_mapper_in_project_unchanged_content_mapped_file_is_reused_from_the_cache_across_a_full_rebuild()
 {
    let (session, utils) = in_project_session(true);
    let main = uri("file:///home/project/main.ts");
    session.did_open_file(
        &ctx(),
        main.clone(),
        1,
        IN_PROJECT_MAIN.to_string(),
        lsproto::LanguageKind::TYPE_SCRIPT,
    );
    let ls = session.get_language_service(&ctx(), &main).unwrap();
    let box_file = ls.get_program().get_source_file("/home/project/app.box");
    assert!(box_file.is_some(), "expected app.box to be loaded");
    let text = box_file.unwrap().text();
    assert!(text.contains("export const version = 7;"), "app.box was not transformed: {text:?}");

    // Changing a compiler option the mapper does not depend on (strict) forces a full program
    // rebuild while leaving app.box's content and the mapper's transform identity unchanged, so the
    // transformed file must be served from the parse cache rather than re-transformed.
    utils
        .fs()
        .write_file(
            "/home/project/tsconfig.json",
            r#"{
			"compilerOptions": { "target": "es2020", "module": "esnext", "moduleResolution": "bundler", "strict": false },
			"contentMappers": [ { "package": "mapper", "extensions": [".box"] } ]
		}"#,
        )
        .unwrap();
    session.did_change_watched_files(&ctx(), &[changed("/home/project/tsconfig.json")]);

    let ls = session.get_language_service(&ctx(), &main).unwrap();
    let rebuilt_box = ls.get_program().get_source_file("/home/project/app.box");
    assert!(
        rebuilt_box == box_file,
        "expected the unchanged content-mapped file to be reused from the parse cache, not re-transformed"
    );
    session.close();
}

// contentmapper_test.go:297
#[test]
fn content_mapper_package_manifest_change_reloads_config() {
    const PACKAGE_JSON_PATH: &str = "/home/mapper/package.json";
    let files = [
        (
            "/home/project/tsconfig.json",
            r#"{
			"compilerOptions": { "target": "es2020", "module": "esnext", "moduleResolution": "bundler" },
			"contentMappers": [{ "package": "mapper", "extensions": [".box"] }]
		}"#,
        ),
        ("/home/project/node_modules/mapper", "symlink:/home/mapper"),
        (
            PACKAGE_JSON_PATH,
            r#"{
			"name": "mapper",
			"version": "1.0.0",
			"typescript": { "contentMapper": { "exec": ["compiler-test-mapper"] } }
		}"#,
        ),
        ("/home/project/app.box", "export const version = #{target};\n"),
        ("/home/project/main.ts", "import { version } from \"./app.box\";\n"),
    ];
    let mut caps = lsproto::ResolvedClientCapabilities::default();
    caps.workspace.did_change_watched_files.relative_pattern_support = true;
    let ctx = lsproto::with_client_capabilities(&ctx(), Arc::new(caps));
    let (mut init, utils) = get_session_init_options(
        &files,
        Some(SessionOptions { watch_enabled: true, ..session_options("/home/project", true) }),
    );
    init.background_ctx = ctx.clone();
    init.spawner = Some(contentmappertest::new_spawner());
    let session = new_session(&init);

    let main = uri("file:///home/project/main.ts");
    session.did_open_file(
        &ctx,
        main.clone(),
        1,
        file(&files, "/home/project/main.ts").to_string(),
        lsproto::LanguageKind::TYPE_SCRIPT,
    );
    session.get_language_service(&ctx, &main).unwrap();
    let configured_project = session.snapshot().get_default_project(&main);
    assert!(configured_project.is_some());
    let mappers = configured_project.unwrap().command_line.unwrap().get().content_mappers();
    assert_eq!(mappers.len(), 1);
    assert_eq!(mappers[0].package_directory, "/home/mapper");
    session.wait_for_background_tasks();
    assert!(
        utils.watches_file(PACKAGE_JSON_PATH),
        "expected the invalid mapper package manifest to be watched"
    );
    assert!(
        utils.client().watch_files_calls().iter().any(|call| {
            call.watchers.iter().any(|watcher| {
                watcher.glob_pattern.relative_pattern.as_ref().is_some_and(|relative| {
                    relative
                        .base_uri
                        .uri
                        .as_ref()
                        .is_some_and(|base| base.0 == "file:///home/mapper")
                        && relative.pattern == "**/*"
                })
            })
        }),
        "expected an external relative-pattern watcher for the mapper package"
    );

    let fixed_manifest = contentmappertest::package_json(contentmappertest::TRANSFORMING_MAPPER)
        .replacen(r#""version": "1.0.0""#, r#""version": "2.0.0""#, 1);
    assert!(fixed_manifest.contains(r#""version": "2.0.0""#));
    utils.fs().write_file(PACKAGE_JSON_PATH, &fixed_manifest).unwrap();
    session.did_change_watched_files(&ctx, &[changed(PACKAGE_JSON_PATH)]);

    let language_service = session.get_language_service(&ctx, &main).unwrap();
    let box_file = language_service.get_program().get_source_file("/home/project/app.box");
    assert!(box_file.is_some(), "expected app.box in the rebuilt program");
    let text = box_file.unwrap().text();
    assert!(
        text.contains("export const version = 7;"),
        "expected fixed mapper manifest to be reloaded: {text:?}"
    );
    session.close();
}

// contentmapper_test.go:368
#[test]
fn content_mapper_supplemental_file_cloned_on_edit() {
    let manifest = contentmappertest::package_json(contentmappertest::SUPPLEMENTAL_MAPPER);
    let files = [
        (
            "/home/project/tsconfig.json",
            r#"{ "compilerOptions": { "strict": true }, "contentMappers": [{ "package": "mapper", "extensions": [".box"] }] }"#,
        ),
        ("/home/project/node_modules/mapper/package.json", manifest.as_str()),
        ("/home/project/app.box", "declare const supplementalValue: number;\n"),
        ("/home/project/extra.d.ts", "interface Extra {}\n"),
        ("/home/project/main.ts", "const value: number = supplementalValue;\n"),
    ];
    let (session, utils) = new_mapper_session(
        &files,
        session_options("/home/project", true),
        contentmappertest::new_spawner(),
    );

    let main = uri("file:///home/project/main.ts");
    session.did_open_file(
        &ctx(),
        main.clone(),
        1,
        file(&files, "/home/project/main.ts").to_string(),
        lsproto::LanguageKind::TYPE_SCRIPT,
    );
    let language_service = session.get_language_service(&ctx(), &main).unwrap();
    let old_program = language_service.get_program();
    let old_canonical = old_program.get_source_file("/home/project/app.box").unwrap();
    let old_supplemental = old_canonical.supplemental_source_files();
    assert_eq!(old_supplemental.len(), 1);
    assert_eq!(old_supplemental[0].file_name(), "/home/project/app.box.0.ts");
    assert_eq!(old_supplemental[0].path(), &Path::new("/home/project/app.box.0.ts".to_string()));
    assert_eq!(old_supplemental[0].hash.get(), old_canonical.hash.get());
    assert_eq!(
        old_program.get_source_file_by_path(old_supplemental[0].path()),
        Some(old_supplemental[0])
    );
    assert_eq!(
        old_program.files_by_path().get(old_supplemental[0].path()),
        Some(&old_supplemental[0])
    );

    utils
        .fs()
        .write_file("/home/project/app.box", "declare const supplementalValue: string;\n")
        .unwrap();
    session.did_change_watched_files(&ctx(), &[changed("/home/project/app.box")]);
    let language_service = session.get_language_service(&ctx(), &main).unwrap();
    let updated_snapshot = session.snapshot();
    let configured_project = updated_snapshot.get_default_project(&main).unwrap();
    assert_eq!(configured_project.program_update_kind, ProgramUpdateKind::Cloned);

    let new_program = language_service.get_program();
    let new_canonical = new_program.get_source_file("/home/project/app.box").unwrap();
    let new_supplemental = new_canonical.supplemental_source_files();
    assert_eq!(new_supplemental.len(), 1);
    assert_eq!(new_supplemental[0].path(), old_supplemental[0].path());
    assert_ne!(new_canonical, old_canonical);
    assert_ne!(new_supplemental[0], old_supplemental[0]);
    assert_eq!(new_supplemental[0].hash.get(), new_canonical.hash.get());
    assert_ne!(new_supplemental[0].hash.get(), old_supplemental[0].hash.get());
    assert_eq!(
        new_program.files_by_path().get(new_supplemental[0].path()),
        Some(&new_supplemental[0])
    );
    assert!(new_supplemental[0].text().contains("supplementalValue: string"));
    let main_file = new_program.get_source_file("/home/project/main.ts");
    let diagnostics = new_program.get_semantic_diagnostics(&ctx(), main_file);
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.code() == 2322));

    // Changing the supplemental file's reference graph must fall back from cloning to a full rebuild.
    utils
        .fs()
        .write_file(
            "/home/project/app.box",
            "/// <reference path=\"./extra.d.ts\" />\ndeclare const supplementalValue: string;\n",
        )
        .unwrap();
    session.did_change_watched_files(&ctx(), &[changed("/home/project/app.box")]);
    session.get_language_service(&ctx(), &main).unwrap();
    let configured_project = session.snapshot().get_default_project(&main).unwrap();
    assert_eq!(configured_project.program_update_kind, ProgramUpdateKind::SameFileNames);
    session.close();
}

// contentmapper_test.go:441
#[test]
fn content_mapper_module_extension_cloned_on_unrelated_edit() {
    let manifest = contentmappertest::package_json(contentmappertest::MODULE_VERBATIM_MAPPER);
    let files = [
        (
            "/home/project/tsconfig.json",
            r#"{ "contentMappers": [{ "package": "mapper", "extensions": [".box"] }] }"#,
        ),
        ("/home/project/node_modules/mapper/package.json", manifest.as_str()),
        ("/home/project/app.box", "export const value = 1;\n"),
        ("/home/project/main.ts", r#"import { value } from "./app.box"; value;"#),
    ];
    let (session, _) = new_mapper_session(
        &files,
        session_options("/home/project", true),
        contentmappertest::new_spawner(),
    );

    let main = uri("file:///home/project/main.ts");
    session.did_open_file(
        &ctx(),
        main.clone(),
        1,
        file(&files, "/home/project/main.ts").to_string(),
        lsproto::LanguageKind::TYPE_SCRIPT,
    );
    let language_service = session.get_language_service(&ctx(), &main).unwrap();
    let mapped_file = language_service.get_program().get_source_file("/home/project/app.box");
    assert!(mapped_file.is_some());
    assert_eq!(mapped_file.unwrap().virtual_file_name(), "/home/project/app.box.mts");
    assert!(mapped_file.unwrap().parse_options().external_module_indicator_options.force);

    session.did_change_file(
        &ctx(),
        main.clone(),
        2,
        whole(r#"import { value } from "./app.box"; value + 1;"#),
    );
    let language_service = session.get_language_service(&ctx(), &main).unwrap();
    let configured_project = session.snapshot().get_default_project(&main).unwrap();
    assert_eq!(configured_project.program_update_kind, ProgramUpdateKind::Cloned);
    assert_eq!(
        language_service.get_program().get_source_file("/home/project/app.box"),
        mapped_file
    );
    session.close();
}

// contentmapper_test.go:480
#[test]
fn content_mapper_locale_change() {
    let manifest = contentmappertest::package_json(contentmappertest::VERBATIM_MAPPER);
    let files = [
        (
            "/home/project/tsconfig.json",
            r#"{ "contentMappers": [ { "package": "mapper", "extensions": [".box"] } ] }"#,
        ),
        ("/home/project/node_modules/mapper/package.json", manifest.as_str()),
        ("/home/project/app.box", "export const value = 1;\n"),
        ("/home/project/main.ts", r#"import { value } from "./app.box"; value;"#),
    ];
    let spawner = recording_spawner();
    let (session, utils) = new_mapper_session(
        &files,
        session_options("/home/project", true),
        Arc::clone(&spawner) as Arc<dyn Spawner>,
    );
    utils.client().set_locale_updates_locale.store(true, Ordering::SeqCst);

    let ctx = with_locale(&ctx(), Locale::DEFAULT);
    let main = uri("file:///home/project/main.ts");
    let locale_reads = utils.client().get_locale_calls();
    session.did_open_file(
        &ctx,
        main.clone(),
        1,
        file(&files, "/home/project/main.ts").to_string(),
        lsproto::LanguageKind::TYPE_SCRIPT,
    );
    session.get_language_service(&ctx, &main).unwrap();
    // Snapshot adoption reads the current locale for its background work; project construction should not.
    assert_eq!(utils.client().get_locale_calls(), locale_reads + 1);
    assert_eq!(spawner.spawns.load(Ordering::SeqCst), 1);

    let mut preferences = session.config();
    preferences.locale = "fr".to_string();
    session.configure(&preferences);
    assert_eq!(spawner.closes.load(Ordering::SeqCst), 1);

    session.get_language_service(&Context::background(), &main).unwrap();
    assert_eq!(spawner.spawns.load(Ordering::SeqCst), 2);
    session.close();
}

// contentmapper_test.go:534
#[test]
fn dynamic_content_mapper_in_project() {
    let manifest = contentmappertest::package_json(contentmappertest::DYNAMIC_VERBATIM_MAPPER);
    let files = [
        (
            "/home/project/tsconfig.json",
            r#"{ "contentMappers": [ { "package": "mapper", "extensions": [".box"], "options": { "mode": "project" } } ] }"#,
        ),
        ("/home/project/node_modules/mapper/package.json", manifest.as_str()),
        ("/home/project/mapper.config.json", r#"{ "version": 1 }"#),
        ("/home/project/app.box", "export const value = 1;\n"),
        ("/home/project/main.ts", r#"import { value } from "./app.box"; value;"#),
    ];
    let (session, utils) = new_mapper_session(
        &files,
        session_options("/home/project", true),
        contentmappertest::new_spawner(),
    );

    let main = uri("file:///home/project/main.ts");
    session.did_open_file(
        &ctx(),
        main.clone(),
        1,
        file(&files, "/home/project/main.ts").to_string(),
        lsproto::LanguageKind::TYPE_SCRIPT,
    );
    let language_service = session.get_language_service(&ctx(), &main).unwrap();
    let mapped_file = language_service.get_program().get_source_file("/home/project/app.box");
    assert!(mapped_file.is_some());
    assert!(!mapped_file.unwrap().is_content_mapper_failure_stub());

    let program = language_service.get_program();
    utils.fs().write_file("/home/project/mapper.config.json", r#"{ "version": 2 }"#).unwrap();
    session.did_change_watched_files(&ctx(), &[changed("/home/project/mapper.config.json")]);
    let language_service = session.get_language_service(&ctx(), &main).unwrap();
    assert!(!std::ptr::eq(language_service.get_program(), program));
    let mapped_file = language_service.get_program().get_source_file("/home/project/app.box");
    assert!(mapped_file.is_some());
    assert!(!mapped_file.unwrap().is_content_mapper_failure_stub());
    session.close();
}

// contentmapper_test.go:575
#[test]
fn dynamic_content_mapper_refreshes_for_mixed_watch_batches() {
    let excessive_events = || {
        let mut events: Vec<lsproto::FileEvent> =
            (0..1001).map(|i| changed(&format!("/home/project/noise-{i}.ts"))).collect();
        events[0] = changed("/home/project/mapper.config.json");
        events[1] = changed("/home/project/main.ts");
        events
    };
    let event = |name: &str, type_: lsproto::FileChangeType| lsproto::FileEvent {
        uri: uri(&format!("file://{name}")),
        type_,
    };
    let cases: Vec<(&str, Vec<lsproto::FileEvent>)> = vec![
        ("excessive events", excessive_events()),
        (
            "changed files make project fully dirty before mapper deletion",
            vec![
                changed("/home/project/main.ts"),
                changed("/home/project/app.box"),
                event("/home/project/mapper.config.json", lsproto::FileChangeType::DELETED),
            ],
        ),
        (
            "changed files make project fully dirty before mapper creation",
            vec![
                changed("/home/project/main.ts"),
                changed("/home/project/app.box"),
                event("/home/project/mapper.config.json", lsproto::FileChangeType::CREATED),
            ],
        ),
    ];
    for (name, events) in cases {
        let manifest = contentmappertest::package_json(contentmappertest::DYNAMIC_VERBATIM_MAPPER);
        let files = [
            (
                "/home/project/tsconfig.json",
                r#"{ "contentMappers": [{ "package": "mapper", "extensions": [".box"] }] }"#,
            ),
            ("/home/project/node_modules/mapper/package.json", manifest.as_str()),
            ("/home/project/mapper.config.json", r#"{ "version": 1 }"#),
            ("/home/project/app.box", "export const value = 1;\n"),
            ("/home/project/main.ts", r#"import { value } from "./app.box"; value;"#),
        ];
        let lifecycle = Arc::new(contentmappertest::ProjectLifecycle::default());
        let (session, _) = new_mapper_session(
            &files,
            session_options("/home/project", true),
            contentmappertest::new_spawner_with_project_lifecycle(Arc::clone(&lifecycle)),
        );

        let main = uri("file:///home/project/main.ts");
        session.did_open_file(
            &ctx(),
            main.clone(),
            1,
            file(&files, "/home/project/main.ts").to_string(),
            lsproto::LanguageKind::TYPE_SCRIPT,
        );
        session.get_language_service(&ctx(), &main).unwrap();
        assert_eq!(lifecycle.opens.load(Ordering::SeqCst), 1, "{name}");
        assert_eq!(lifecycle.closes.load(Ordering::SeqCst), 0, "{name}");

        session.did_change_watched_files(&ctx(), &events);
        session.wait_for_background_tasks();
        session.get_language_service(&ctx(), &main).unwrap();
        assert_eq!(lifecycle.closes.load(Ordering::SeqCst), 1, "{name}");
        assert_eq!(lifecycle.opens.load(Ordering::SeqCst), 2, "{name}");
        session.close();
    }
}

// contentmapper_test.go:653
#[test]
fn unused_dynamic_content_mapper_is_not_opened() {
    let manifest = contentmappertest::package_json(contentmappertest::DYNAMIC_VERBATIM_MAPPER);
    let files = [
        (
            "/home/project/tsconfig.json",
            r#"{ "contentMappers": [{ "package": "mapper", "extensions": [".box"] }] }"#,
        ),
        ("/home/project/node_modules/mapper/package.json", manifest.as_str()),
        ("/home/project/main.ts", "export const value = 1;"),
    ];
    let lifecycle = Arc::new(contentmappertest::ProjectLifecycle::default());
    let (session, _) = new_mapper_session(
        &files,
        session_options("/home/project", true),
        contentmappertest::new_spawner_with_project_lifecycle(Arc::clone(&lifecycle)),
    );

    let main = uri("file:///home/project/main.ts");
    session.did_open_file(
        &ctx(),
        main.clone(),
        1,
        file(&files, "/home/project/main.ts").to_string(),
        lsproto::LanguageKind::TYPE_SCRIPT,
    );
    session.get_language_service(&ctx(), &main).unwrap();
    assert_eq!(lifecycle.opens.load(Ordering::SeqCst), 0);
    session.close();
}

// contentmapper_test.go:679
#[test]
fn content_mappers_in_parallel_project_references() {
    let manifest = contentmappertest::package_json(contentmappertest::TRANSFORMING_MAPPER);
    let files = [
        (
            "/home/project/tsconfig.json",
            r#"{
			"files": ["src/index.d.ts"],
			"references": [{ "path": "./a" }, { "path": "./b" }]
		}"#,
        ),
        ("/home/project/src/index.d.ts", "export {};"),
        (
            "/home/project/a/tsconfig.json",
            r#"{
			"compilerOptions": { "composite": true },
			"files": ["../src/index.d.ts"],
			"contentMappers": [{ "package": "mapper", "extensions": [".vue"] }]
		}"#,
        ),
        (
            "/home/project/b/tsconfig.json",
            r#"{
			"compilerOptions": { "composite": true },
			"files": ["../src/index.d.ts"],
			"contentMappers": [{ "package": "mapper", "extensions": [".svelte"] }]
		}"#,
        ),
        ("/home/project/node_modules/mapper/package.json", manifest.as_str()),
    ];
    let (session, utils) = new_mapper_session(
        &files,
        session_options("/home/project", true),
        contentmappertest::new_spawner(),
    );

    let index = uri("file:///home/project/src/index.d.ts");
    session.did_open_file(
        &ctx(),
        index,
        1,
        "export {};".to_string(),
        lsproto::LanguageKind::TYPE_SCRIPT,
    );
    session.wait_for_background_tasks();
    let calls = utils.client().register_content_mapper_extensions_calls();
    assert!(calls.last().is_some(), "expected content mapper extensions to be registered");
    let mut extensions = calls.last().unwrap().clone();
    extensions.sort();
    assert_eq!(extensions, vec![".svelte".to_string(), ".vue".to_string()]);
    session.close();
}

// contentmapper_test.go:720
#[test]
fn content_mapper_open_file_excluded_by_config_change() {
    let manifest = contentmappertest::package_json(contentmappertest::TRANSFORMING_MAPPER);
    let files = [
        (
            "/home/project/tsconfig.json",
            r#"{
			"compilerOptions": { "target": "es2020", "module": "esnext", "moduleResolution": "bundler", "strict": true },
			"include": ["src"],
			"contentMappers": [ { "package": "mapper", "extensions": [".box"] } ]
		}"#,
        ),
        ("/home/project/node_modules/mapper/package.json", manifest.as_str()),
        ("/home/project/src/app.box", "export const version = #{target};\n"),
        ("/home/project/src/main.ts", "export const main = true;\n"),
    ];
    let (session, utils) = new_mapper_session(
        &files,
        session_options("/home/project", true),
        contentmappertest::new_spawner(),
    );

    let box_uri = uri("file:///home/project/src/app.box");
    session.set_content_mapper_contributions(
        &ctx(),
        extension_contribution("/home/project"),
        Vec::new(),
    );
    session.did_open_file(
        &ctx(),
        box_uri.clone(),
        1,
        file(&files, "/home/project/src/app.box").to_string(),
        lsproto::LanguageKind("box"),
    );
    let language_service = session.get_language_service(&ctx(), &box_uri).unwrap();
    assert!(language_service.get_program().get_source_file("/home/project/src/app.box").is_some());

    utils
        .fs()
        .write_file(
            "/home/project/tsconfig.json",
            r#"{
		"compilerOptions": { "target": "es2020", "module": "esnext", "moduleResolution": "bundler", "strict": true },
		"include": ["src/**/*.ts"],
		"contentMappers": [ { "package": "mapper", "extensions": [".box"] } ]
	}"#,
        )
        .unwrap();
    session.did_change_watched_files(&ctx(), &[changed("/home/project/tsconfig.json")]);
    session.wait_for_background_tasks();

    // The background update removes app.box from the configured project, but inferred
    // project cleanup is deferred until the next file open.
    assert!(session.snapshot().get_default_project(&box_uri).is_none());
    let main = uri("file:///home/project/src/main.ts");
    session.did_open_file(
        &ctx(),
        main,
        1,
        file(&files, "/home/project/src/main.ts").to_string(),
        lsproto::LanguageKind::TYPE_SCRIPT,
    );

    let language_service = session.get_language_service(&ctx(), &box_uri).unwrap();
    let default_project = session.snapshot().get_default_project(&box_uri);
    assert!(default_project.is_some(), "expected a default project for the open app.box");
    assert_eq!(default_project.unwrap().kind, Kind::Inferred);
    let box_file = language_service.get_program().get_source_file("/home/project/src/app.box");
    assert!(box_file.is_some(), "expected the open app.box in the inferred project");
    let box_file = box_file.unwrap();
    assert!(!box_file.content_mapper().is_empty(), "expected app.box to retain its content mapper");
    assert!(
        !box_file.text().contains("#{target}"),
        "expected app.box to be transformed: {:?}",
        box_file.text()
    );
    session.close();
}

// contentmapper_test.go:787
#[test]
fn content_mapper_removal_with_open_file() {
    let manifest = contentmappertest::package_json(contentmappertest::TRANSFORMING_MAPPER);
    let files = [
        (
            "/home/project/tsconfig.json",
            r#"{
			"compilerOptions": { "target": "es2020", "module": "esnext", "moduleResolution": "bundler" },
			"contentMappers": [ { "package": "mapper", "extensions": [".box"] } ]
		}"#,
        ),
        ("/home/project/node_modules/mapper/package.json", manifest.as_str()),
        ("/home/project/app.box", "export const version = #{target};\n"),
    ];
    let spawner = recording_spawner();
    let (session, utils) = new_mapper_session(
        &files,
        session_options("/home/project", true),
        Arc::clone(&spawner) as Arc<dyn Spawner>,
    );

    let box_uri = uri("file:///home/project/app.box");
    session.did_open_file(
        &ctx(),
        box_uri.clone(),
        1,
        file(&files, "/home/project/app.box").to_string(),
        lsproto::LanguageKind("box"),
    );
    let language_service = session.get_language_service(&ctx(), &box_uri).unwrap();
    assert_ne!(
        language_service
            .get_program()
            .get_source_file("/home/project/app.box")
            .unwrap()
            .content_mapper(),
        ""
    );
    assert_eq!(spawner.spawns.load(Ordering::SeqCst), 1);
    assert_eq!(spawner.closes.load(Ordering::SeqCst), 0);
    for version in 2..=4 {
        session.did_change_file(
            &ctx(),
            box_uri.clone(),
            version,
            whole(&format!("export const version = {version};\n")),
        );
        session.get_language_service(&ctx(), &box_uri).unwrap();
        assert_eq!(
            spawner.spawns.load(Ordering::SeqCst),
            1,
            "snapshot clone should reuse the mapper process"
        );
        assert_eq!(
            spawner.closes.load(Ordering::SeqCst),
            0,
            "snapshot clone should preserve overlapping ownership"
        );
    }
    let release_old_snapshot = session
        .with_language_service_and_snapshot(&ctx(), &box_uri, |_, _| Ok(Some(Box::new(|| Ok(())))))
        .unwrap()
        .unwrap();

    utils
        .fs()
        .write_file(
            "/home/project/tsconfig.json",
            r#"{
		"compilerOptions": { "target": "es2020", "module": "esnext", "moduleResolution": "bundler" }
	}"#,
        )
        .unwrap();
    session.did_change_watched_files(&ctx(), &[changed("/home/project/tsconfig.json")]);

    let err = session.get_language_service(&ctx(), &box_uri).err().unwrap();
    assert!(err.message.contains("no project found"), "{}", err.message);
    assert!(
        session.snapshot().get_file("/home/project/app.box").is_some(),
        "overlay should remain until didClose"
    );
    assert!(
        session.snapshot().get_default_project(&box_uri).is_none(),
        "unsupported file should not be in a project"
    );

    session.wait_for_background_tasks();
    assert_eq!(
        spawner.closes.load(Ordering::SeqCst),
        0,
        "live old snapshot should retain the mapper process"
    );
    release_old_snapshot().unwrap();
    assert_eq!(
        spawner.closes.load(Ordering::SeqCst),
        1,
        "process should close after the final live snapshot is released"
    );
    let calls = utils.client().register_content_mapper_extensions_calls();
    assert!(!calls.is_empty(), "expected content mapper registration updates");
    assert!(
        calls.last().unwrap().is_empty(),
        "expected content mapper extensions to be unregistered"
    );

    session.did_close_file(&ctx(), box_uri.clone());
    let err = session.get_language_service(&ctx(), &box_uri).err().unwrap();
    assert!(err.message.contains("no project found"), "{}", err.message);
    session.close();
}

// contentmapper_test.go:857
#[test]
fn content_mapper_process_shared_across_projects() {
    let config = |extension: &str| {
        format!(
            r#"{{
			"compilerOptions": {{ "target": "es2020", "module": "esnext", "moduleResolution": "bundler" }},
			"contentMappers": [ {{ "package": "mapper", "extensions": ["{extension}"] }} ]
		}}"#
        )
    };
    let manifest = contentmappertest::package_json(contentmappertest::TRANSFORMING_MAPPER);
    let (a_config, b_config) = (config(".box"), config(".panel"));
    let files = [
        ("/home/a/tsconfig.json", a_config.as_str()),
        ("/home/a/node_modules/mapper/package.json", manifest.as_str()),
        ("/home/a/app.box", "export const a = 1;\n"),
        ("/home/b/tsconfig.json", b_config.as_str()),
        ("/home/b/node_modules/mapper/package.json", manifest.as_str()),
        ("/home/b/app.panel", "export const b = 1;\n"),
    ];
    let spawner = recording_spawner();
    let (session, utils) = new_mapper_session(
        &files,
        session_options("/home", true),
        Arc::clone(&spawner) as Arc<dyn Spawner>,
    );

    let a_uri = uri("file:///home/a/app.box");
    let b_uri = uri("file:///home/b/app.panel");
    session.did_open_file(
        &ctx(),
        a_uri.clone(),
        1,
        file(&files, "/home/a/app.box").to_string(),
        lsproto::LanguageKind("box"),
    );
    session.get_language_service(&ctx(), &a_uri).unwrap();
    session.did_open_file(
        &ctx(),
        b_uri.clone(),
        1,
        file(&files, "/home/b/app.panel").to_string(),
        lsproto::LanguageKind("panel"),
    );
    session.get_language_service(&ctx(), &b_uri).unwrap();
    assert_eq!(
        spawner.spawns.load(Ordering::SeqCst),
        1,
        "same mapper identity should share one process"
    );

    utils.fs().write_file("/home/a/tsconfig.json", "{}").unwrap();
    session.did_change_watched_files(&ctx(), &[changed("/home/a/tsconfig.json")]);
    let err = session.get_language_service(&ctx(), &a_uri).err().unwrap();
    assert!(err.message.contains("no project found"), "{}", err.message);
    assert_eq!(
        spawner.closes.load(Ordering::SeqCst),
        0,
        "second project still owns the shared process"
    );

    utils.fs().write_file("/home/b/tsconfig.json", "{}").unwrap();
    session.did_change_watched_files(&ctx(), &[changed("/home/b/tsconfig.json")]);
    let err = session.get_language_service(&ctx(), &b_uri).err().unwrap();
    assert!(err.message.contains("no project found"), "{}", err.message);
    assert_eq!(
        spawner.closes.load(Ordering::SeqCst),
        1,
        "final project owner should close the shared process"
    );
    session.close();
}

// contentmapper_test.go:909
#[test]
fn content_mapper_inferred_project_uses_extension_contributions() {
    let manifest = contentmappertest::package_json(contentmappertest::TRANSFORMING_MAPPER);
    let files = [
        (
            "/home/configured/tsconfig.json",
            r#"{
			"compilerOptions": { "target": "es2020", "module": "esnext", "moduleResolution": "bundler" },
			"contentMappers": [ { "package": "mapper", "extensions": [".box"] } ]
		}"#,
        ),
        ("/home/configured/node_modules/mapper/package.json", manifest.as_str()),
        ("/home/configured/main.ts", "export const main = true;\n"),
        ("/home/loose/app.box", "export const version = #{target};\n"),
    ];
    let (session, _) = new_mapper_session(
        &files,
        session_options("/home", true),
        contentmappertest::new_spawner(),
    );

    let configured = uri("file:///home/configured/main.ts");
    session.did_open_file(
        &ctx(),
        configured.clone(),
        1,
        file(&files, "/home/configured/main.ts").to_string(),
        lsproto::LanguageKind::TYPE_SCRIPT,
    );
    session.get_language_service(&ctx(), &configured).unwrap();

    let box_uri = uri("file:///home/loose/app.box");
    session.did_open_file(
        &ctx(),
        box_uri.clone(),
        1,
        file(&files, "/home/loose/app.box").to_string(),
        lsproto::LanguageKind("box"),
    );
    let err = session.get_language_service(&ctx(), &box_uri).err().unwrap();
    assert!(
        err.message.contains("no project found"),
        "configured mapper must not leak into inferred projects: {}",
        err.message
    );
    session.set_content_mapper_contributions(
        &ctx(),
        extension_contribution("/home"),
        vec![box_uri.clone()],
    );
    let language_service = session.get_language_service(&ctx(), &box_uri).unwrap();
    let default_project = session.snapshot().get_default_project(&box_uri);
    assert!(default_project.is_some(), "expected a default project for the loose app.box");
    assert_eq!(default_project.unwrap().kind, Kind::Inferred);
    let box_file = language_service.get_program().get_source_file("/home/loose/app.box");
    assert!(box_file.is_some(), "expected loose app.box in the inferred project");
    let box_file = box_file.unwrap();
    assert!(
        !box_file.content_mapper().is_empty(),
        "expected loose app.box to use the extension contribution"
    );
    assert!(
        !box_file.text().contains("#{target}"),
        "expected loose app.box to be transformed: {:?}",
        box_file.text()
    );
    session.close();
}

// contentmapper_test.go:1037
#[test]
fn content_mapper_created_file_adopted_by_configured_project() {
    // A content-mapped file created while the server is running must be adopted by the
    // configured project: the created-file root matching has to account for the content
    // mapper extensions, otherwise the file falls into the inferred project until a full
    // project reload.
    let manifest = contentmappertest::package_json(contentmappertest::TRANSFORMING_MAPPER);
    let files = [
        (
            "/home/project/tsconfig.json",
            r#"{
			"compilerOptions": { "target": "es2020", "module": "esnext", "moduleResolution": "bundler" },
			"contentMappers": [ { "package": "mapper", "extensions": [".box"] } ]
		}"#,
        ),
        ("/home/project/node_modules/mapper/package.json", manifest.as_str()),
        ("/home/project/main.ts", "export const main = true;\n"),
    ];
    let (session, utils) = new_mapper_session(
        &files,
        session_options("/home/project", true),
        contentmappertest::new_spawner(),
    );

    let main = uri("file:///home/project/main.ts");
    session.did_open_file(
        &ctx(),
        main.clone(),
        1,
        file(&files, "/home/project/main.ts").to_string(),
        lsproto::LanguageKind::TYPE_SCRIPT,
    );
    session.get_language_service(&ctx(), &main).unwrap();

    utils.fs().write_file("/home/project/new.box", "export const version = #{target};\n").unwrap();
    session.did_change_watched_files(
        &ctx(),
        &[lsproto::FileEvent {
            uri: uri("file:///home/project/new.box"),
            type_: lsproto::FileChangeType::CREATED,
        }],
    );
    let box_uri = uri("file:///home/project/new.box");
    session.did_open_file(
        &ctx(),
        box_uri.clone(),
        1,
        "export const version = #{target};\n".to_string(),
        lsproto::LanguageKind("box"),
    );
    let language_service = session.get_language_service(&ctx(), &box_uri).unwrap();
    let default_project = session.snapshot().get_default_project(&box_uri);
    assert!(default_project.is_some(), "expected a default project for the created new.box");
    assert_eq!(default_project.unwrap().kind, Kind::Configured);
    let box_file = language_service.get_program().get_source_file("/home/project/new.box");
    assert!(box_file.is_some(), "expected new.box in the configured project");
    assert!(
        !box_file.unwrap().text().contains("#{target}"),
        "expected new.box to be transformed: {:?}",
        box_file.unwrap().text()
    );
    session.close();
}
