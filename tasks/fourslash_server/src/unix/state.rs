// Read-only snapshot views for Go's statebaseline.go. Comparison and formatting stay in Go.
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

use tsrs_core::json::{self, Value};
use tsrs_core::tspath;
use tsrs_project::Snapshot;

use super::strings;

fn object<const N: usize>(fields: [(&str, Value); N]) -> Value {
    Value::Object(fields.into_iter().map(|(key, value)| (key.into(), value)).collect())
}

fn string_array(values: impl Iterator<Item = String>) -> Value {
    Value::Array(values.map(Value::String).collect())
}

fn snapshot_value(snapshot: &Snapshot, open_files: &[String], case_sensitive: bool) -> Value {
    let collection = &snapshot.project_collection;
    let projects = collection
        .projects()
        .iter()
        .map(|project| {
            let program = project.get_program().map_or(Value::Null, |program| {
                let files = program
                    .get_source_files()
                    .iter()
                    .map(|file| {
                        object([
                            ("Handle", Value::String(format!("{:x}", file.key()))),
                            ("Name", Value::String(file.file_name().into())),
                            ("PathName", Value::String(file.path().to_string())),
                        ])
                    })
                    .collect();
                object([
                    ("Handle", Value::String(format!("{program:p}"))),
                    ("Files", Value::Array(files)),
                ])
            });
            object([("Name", Value::String(project.id().0)), ("Program", program)])
        })
        .collect();
    let defaults = open_files
        .iter()
        .map(|name| {
            let path = tspath::to_path(name, "/", case_sensitive);
            let project = collection.get_default_project(&path);
            (path.to_string(), Value::String(project.map_or(String::new(), |p| p.id().0)))
        })
        .collect();
    let registry = collection.config_file_registry();
    let mut configs = Vec::new();
    registry.for_each_test_config_entry(|path, entry| {
        configs.push((
            path.to_string(),
            object([
                ("FileName", Value::String(entry.file_name.clone())),
                (
                    "RetainingProjects",
                    string_array(entry.retaining_projects.iter().map(|id| id.0.clone())),
                ),
                (
                    "RetainingOpenFiles",
                    string_array(entry.retaining_open_files.iter().map(ToString::to_string)),
                ),
                (
                    "RetainingConfigs",
                    string_array(entry.retaining_configs.iter().map(ToString::to_string)),
                ),
            ]),
        ));
    });
    configs.sort_by(|a, b| a.0.cmp(&b.0));
    let mut file_names = Vec::new();
    registry.for_each_test_config_file_names_entry(|path, entry| {
        let mut ancestors: Vec<_> = entry.ancestors.iter().flat_map(|m| m.iter()).collect();
        ancestors.sort_by(|a, b| a.0.cmp(b.0));
        file_names.push((
            path.to_string(),
            object([
                ("NearestConfigFileName", Value::String(entry.nearest_config_file_name.clone())),
                (
                    "Ancestors",
                    Value::Object(
                        ancestors
                            .into_iter()
                            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                            .collect(),
                    ),
                ),
            ]),
        ));
    });
    file_names.sort_by(|a, b| a.0.cmp(&b.0));
    object([(
        "ProjectCollection",
        object([
            ("Items", Value::Array(projects)),
            ("Defaults", Value::Object(defaults)),
            (
                "Registry",
                object([
                    ("Configs", Value::Object(configs.into_iter().collect())),
                    ("FileNames", Value::Object(file_names.into_iter().collect())),
                ]),
            ),
        ]),
    )])
}

pub(super) fn serve(stream: UnixStream, server: &tsrs_lsp::Server, case_sensitive: bool) {
    let mut stream = BufReader::new(stream);
    // Project owners keep program/source-file addresses alive until the next view is captured.
    // Keeping Arc<Snapshot> alone does not retain the program regions after Snapshot::deref.
    let mut previous_projects = Vec::new();
    let mut line = String::new();
    while stream.read_line(&mut line).expect("read Go state request") != 0 {
        let open_files = strings(&json::unmarshal(&line).expect("decode Go state request"));
        let snapshot = server.session().snapshot();
        let projects = snapshot.project_collection.projects();
        let value = snapshot_value(&snapshot, &open_files, case_sensitive);
        drop(std::mem::replace(&mut previous_projects, projects));
        writeln!(stream.get_mut(), "{}", json::marshal(&value).unwrap())
            .expect("write project state to Go");
        line.clear();
    }
}
