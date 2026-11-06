use std::sync::Arc;

use tsrs_core::context::Context;
use tsrs_ls::{FileText, Host};
use tsrs_lsproto as lsproto;
use tsrs_project::session::{Session, new_session};
use tsrs_vfs::{Entries, FS, bundled, vfsmatch, vfstest};

use crate::projecttestutil;

const INDEX: &str = "/project/index.ts";
const INDEX_TEXT: &str = "export {};";

fn setup(files: &[(&str, &str)], case_sensitive: bool) -> (Arc<Session>, Arc<dyn FS>) {
    let options = tsrs_project::session::SessionOptions {
        watch_enabled: false,
        logging_enabled: false,
        push_diagnostics_enabled: false,
        ..projecttestutil::default_session_options()
    };
    let (mut init, _) = projecttestutil::get_session_init_options(files, Some(options));
    // The shared helper fixes case sensitivity to false; retain its session setup for both modes.
    if case_sensitive {
        init.fs = Arc::new(bundled::wrap_fs(Arc::new(vfstest::from_map(
            files.iter().map(|(path, content)| {
                let file = match content.strip_prefix("symlink:") {
                    Some(target) => vfstest::symlink(target),
                    None => vfstest::MapFile::from(*content),
                };
                (*path, file)
            }),
            true,
        ))));
    }
    let disk = Arc::clone(&init.fs);
    (new_session(&init), disk)
}

fn uri(file_name: &str) -> lsproto::DocumentUri {
    lsproto::DocumentUri(format!("file://{file_name}"))
}

fn open(session: &Session, file_name: &str, text: &str) {
    session.did_open_file(
        &Context::background(),
        uri(file_name),
        1,
        text.to_string(),
        lsproto::LanguageKind::TYPE_SCRIPT,
    );
}

fn load_project(session: &Session, cached_files: &[&str]) {
    let ls = session.get_language_service(&Context::background(), &uri(INDEX)).unwrap();
    for file_name in cached_files {
        assert!(ls.get_program().get_source_file(file_name).is_some(), "{file_name}");
    }
    drop(ls);
    session.wait_for_background_tasks();
}

fn assert_entries(mut entries: Entries, files: &[&str], directories: &[&str], symlinks: &[&str]) {
    let mut files: Vec<_> = files.iter().map(ToString::to_string).collect();
    let mut directories: Vec<_> = directories.iter().map(ToString::to_string).collect();
    entries.files.sort_unstable();
    entries.directories.sort_unstable();
    files.sort_unstable();
    directories.sort_unstable();
    assert_eq!(entries.files, files);
    assert_eq!(entries.directories, directories);
    assert_eq!(entries.symlinks, Some(symlinks.iter().map(ToString::to_string).collect()));
}

fn cached_entries_shadow_disk_changes(case_sensitive: bool) {
    let files = &[
        (
            "/project/tsconfig.json",
            r#"{
                "compilerOptions": { "noLib": true, "types": [] },
                "files": [
                    "index.ts", "entries/file.ts", "entries/Cased.ts",
                    "entries/folder/member.ts", "entries/cased-folder/member.ts",
                    "entries/retyped/member.ts"
                ]
            }"#,
        ),
        (INDEX, INDEX_TEXT),
        ("/project/entries/file.ts", "export const original = 1;"),
        ("/project/entries/Cased.ts", "export const original = 2;"),
        ("/project/entries/folder/member.ts", "export {};"),
        ("/project/entries/cased-folder/member.ts", "export {};"),
        ("/project/entries/retyped/member.ts", "export {};"),
    ];
    let (session, disk) = setup(files, case_sensitive);
    open(&session, INDEX, INDEX_TEXT);
    load_project(
        &session,
        &[
            "/project/entries/file.ts",
            "/project/entries/Cased.ts",
            "/project/entries/folder/member.ts",
            "/project/entries/cased-folder/member.ts",
            "/project/entries/retyped/member.ts",
        ],
    );
    let snapshot = session.snapshot();
    let fs = snapshot.fs();
    assert_eq!(fs.use_case_sensitive_file_names(), case_sensitive);

    // Cache these negative file probes before the on-disk directories become files. Go's merge also
    // consults the lower FileExists cache when deciding whether a cached child is a directory.
    assert!(!fs.file_exists("/project/entries/folder"));
    assert!(!fs.file_exists("/project/entries/cased-folder"));

    // Model disk changes before watcher delivery. Explicit roots leave this directory's listing unread.
    disk.remove("/project/entries/file.ts").unwrap();
    disk.write_file("/project/entries/file.ts/new.txt", "new child").unwrap();
    disk.remove("/project/entries/Cased.ts").unwrap();
    disk.write_file("/project/entries/cased.ts/new.txt", "new child").unwrap();
    disk.remove("/project/entries/folder").unwrap();
    disk.write_file("/project/entries/folder", "replacement file").unwrap();
    disk.remove("/project/entries/cased-folder").unwrap();
    disk.write_file("/project/entries/CASED-FOLDER", "replacement file").unwrap();
    disk.remove("/project/entries/retyped").unwrap();
    disk.write_file("/project/entries/retyped", "replacement file").unwrap();
    disk.write_file("/project/entries/loose.txt", "uncached file").unwrap();
    disk.write_file("/project/entries/loose-dir/child.txt", "uncached directory").unwrap();

    assert_entries(
        disk.get_accessible_entries("/project/entries"),
        &["folder", "CASED-FOLDER", "retyped", "loose.txt"],
        &["file.ts", "cased.ts", "loose-dir"],
        &[],
    );
    // Without an earlier negative file probe, the cached ancestor takes its current lower-layer type.
    assert!(fs.file_exists("/project/entries/retyped"));
    let (expected_files, expected_directories): (&[&str], &[&str]) = if case_sensitive {
        (
            &["file.ts", "Cased.ts", "CASED-FOLDER", "retyped", "loose.txt"],
            &["folder", "cased-folder", "cased.ts", "loose-dir"],
        )
    } else {
        (&["file.ts", "Cased.ts", "retyped", "loose.txt"], &["folder", "cased-folder", "loose-dir"])
    };
    assert_entries(
        fs.get_accessible_entries("/project/entries"),
        expected_files,
        expected_directories,
        &[],
    );
    assert_eq!(fs.read_file("/project/entries/file.ts").unwrap(), "export const original = 1;");
    assert_eq!(fs.read_file("/project/entries/Cased.ts").unwrap(), "export const original = 2;");
    assert_eq!(fs.read_file("/project/entries/folder/member.ts").unwrap(), "export {};");
    session.close();
}

#[test]
fn cached_entries_shadow_case_insensitive_disk_names() {
    cached_entries_shadow_disk_changes(false);
}

#[test]
fn cached_entries_preserve_distinct_case_sensitive_disk_names() {
    cached_entries_shadow_disk_changes(true);
}

fn cached_entries_preserve_unshadowed_symlinks(case_sensitive: bool) {
    let config = if case_sensitive {
        r#"{
            "compilerOptions": { "noLib": true, "types": [] },
            "files": ["index.ts", "entries/linked.ts", "entries/linked-dir/member.ts"]
        }"#
    } else {
        r#"{
            "compilerOptions": { "noLib": true, "types": [] },
            "files": ["index.ts", "entries/LINKED.ts", "entries/LINKED-DIR/member.ts"]
        }"#
    };
    let files = &[
        ("/project/tsconfig.json", config),
        (INDEX, INDEX_TEXT),
        ("/targets/file.ts", "export const linked = 1;"),
        ("/targets/dir/member.ts", "export const member = 1;"),
        ("/project/entries/linked.ts", "symlink:/targets/file.ts"),
        ("/project/entries/linked-dir", "symlink:/targets/dir"),
        ("/project/entries/unread.ts", "symlink:/targets/file.ts"),
        ("/project/entries/unread-dir", "symlink:/targets/dir"),
        ("/project/entries/cycle", "symlink:/project/entries"),
        ("/project/entries/plain.txt", "unread regular file"),
    ];
    let (session, disk) = setup(files, case_sensitive);
    open(&session, INDEX, INDEX_TEXT);
    let (cached_file, cached_directory) =
        if case_sensitive { ("linked.ts", "linked-dir") } else { ("LINKED.ts", "LINKED-DIR") };
    load_project(
        &session,
        &[
            &format!("/project/entries/{cached_file}"),
            &format!("/project/entries/{cached_directory}/member.ts"),
        ],
    );
    assert_entries(
        disk.get_accessible_entries("/project/entries"),
        &["linked.ts", "unread.ts", "plain.txt"],
        &["linked-dir", "unread-dir", "cycle"],
        &["linked.ts", "linked-dir", "unread.ts", "unread-dir", "cycle"],
    );

    let fs = session.snapshot().fs();
    assert_entries(
        fs.get_accessible_entries("/project/entries"),
        &[cached_file, "unread.ts", "plain.txt"],
        &[cached_directory, "unread-dir", "cycle"],
        &["unread.ts", "unread-dir", "cycle"],
    );
    let mut matches = vfsmatch::read_directory(
        &*fs,
        "/",
        "/project/entries",
        &[".ts"],
        &[] as &[&str],
        &["**/*"],
        4,
    );
    let mut expected_matches = vec![
        format!("/project/entries/{cached_file}"),
        format!("/project/entries/{cached_directory}/member.ts"),
        "/project/entries/unread.ts".to_string(),
        "/project/entries/unread-dir/member.ts".to_string(),
    ];
    matches.sort_unstable();
    expected_matches.sort_unstable();
    assert_eq!(matches, expected_matches);
    assert_entries(
        fs.get_accessible_entries("/project/entries"),
        &[cached_file, "unread.ts", "plain.txt"],
        &[cached_directory, "unread-dir", "cycle"],
        &["unread.ts", "unread-dir", "cycle"],
    );
    assert_entries(
        disk.get_accessible_entries("/project/entries"),
        &["linked.ts", "unread.ts", "plain.txt"],
        &["linked-dir", "unread-dir", "cycle"],
        &["linked.ts", "linked-dir", "unread.ts", "unread-dir", "cycle"],
    );
    assert_eq!(fs.read_file("/project/entries/linked.ts").unwrap(), "export const linked = 1;");
    assert_eq!(fs.realpath("/project/entries/linked.ts"), "/targets/file.ts");
    assert_eq!(fs.realpath("/project/entries/linked-dir/member.ts"), "/targets/dir/member.ts");
    session.close();
}

#[test]
fn cached_case_insensitive_entries_replace_symlink_names_without_losing_other_links() {
    cached_entries_preserve_unshadowed_symlinks(false);
}

#[test]
fn cached_case_sensitive_entries_replace_symlink_names_without_losing_other_links() {
    cached_entries_preserve_unshadowed_symlinks(true);
}

fn current_text(session: &Session, file_name: &str) -> FileText {
    load_project(session, &[file_name]);
    let snapshot = session.snapshot();
    let text = snapshot.read_file_text(file_name).unwrap();
    assert_eq!(&*text, snapshot.read_file(file_name).unwrap());
    text
}

#[test]
fn file_text_retains_disk_and_overlay_versions_after_edits_close_and_session_drop() {
    let file_name = "/project/value.ts";
    let disk_text = "export const value = 'disk';\r\n";
    let open_text = "export const value = 'before 🦀';\r\n";
    let edited_text = "export const value = 'after résumé';\n";

    let (disk_version, open_version, edited_version, closed_version) = {
        let (session, _) = setup(
            &[
                (
                    "/project/tsconfig.json",
                    r#"{
                        "compilerOptions": { "noLib": true, "types": [] },
                        "files": ["index.ts", "value.ts"]
                    }"#,
                ),
                (INDEX, INDEX_TEXT),
                (file_name, disk_text),
            ],
            false,
        );
        open(&session, INDEX, INDEX_TEXT);
        let disk_version = current_text(&session, file_name);
        assert_eq!(&*disk_version, disk_text);
        assert!(session.snapshot().read_file_text("/project/missing.ts").is_none());

        open(&session, file_name, open_text);
        let open_version = current_text(&session, file_name);
        assert_eq!(&*open_version, open_text);

        session.did_change_file(
            &Context::background(),
            uri(file_name),
            2,
            vec![lsproto::TextDocumentContentChangePartialOrWholeDocument {
                whole_document: Some(lsproto::TextDocumentContentChangeWholeDocument {
                    text: edited_text.to_string(),
                }),
                ..Default::default()
            }],
        );
        let edited_version = current_text(&session, file_name);
        assert_eq!(&*edited_version, edited_text);

        session.did_close_file(&Context::background(), uri(file_name));
        let closed_version = current_text(&session, file_name);
        assert_eq!(&*closed_version, disk_text);
        session.close();
        (disk_version, open_version, edited_version, closed_version)
    };

    let retained_open_version = open_version.clone();
    drop(open_version);
    assert_eq!(&*disk_version, disk_text);
    assert_eq!(&*retained_open_version, open_text);
    assert_eq!(&*edited_version, edited_text);
    assert_eq!(&*closed_version, disk_text);
}

#[test]
fn cached_project_lookup_discovers_pending_custom_configuration() {
    for warm_cache in [false, true] {
        let (session, _) = setup(
            &[
                (
                    "/project/tsconfig.json",
                    r#"{"compilerOptions":{"noLib":true},"files":["index.ts"]}"#,
                ),
                (
                    "/project/alternate.json",
                    r#"{"compilerOptions":{"noLib":true},"files":["index.ts","extra.ts"]}"#,
                ),
                (INDEX, INDEX_TEXT),
                ("/project/extra.ts", "export const extra = 1;"),
            ],
            false,
        );
        open(&session, INDEX, INDEX_TEXT);
        load_project(&session, &[INDEX]);
        let mut cache = tsrs_project::session::ProjectLookupCache::default();
        if warm_cache {
            let projects = session.get_projects_for_file_cached(
                &Context::background(),
                &uri(INDEX),
                &mut cache,
            );
            assert!(projects.iter().any(|p| p.id().ends_with("/tsconfig.json")));
        }
        let mut config = session.config();
        config.custom_config_file_name = "alternate.json".to_string();
        session.configure(&config);
        let projects =
            session.get_projects_for_file_cached(&Context::background(), &uri(INDEX), &mut cache);
        assert!(
            projects.iter().any(|p| p.id().ends_with("/alternate.json")),
            "the URI must participate in configuration invalidation"
        );
        assert_eq!(
            session.snapshot().get_default_project(&uri(INDEX)).unwrap().config_file_name(),
            "/project/alternate.json"
        );
        session.close();
    }
}
