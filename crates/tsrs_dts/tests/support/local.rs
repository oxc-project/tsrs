use super::*;
use tsrs_core::json::{self, Value};

pub(super) fn inventory(names: &[&str]) {
    let actual: BTreeSet<_> = fs::read_dir(root())
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_dir(), "unexpected fixture file");
            entry.file_name().into_string().unwrap()
        })
        .collect();
    assert_eq!(actual, names.iter().map(ToString::to_string).collect());
    assert_eq!(actual.len(), names.len(), "duplicate fixture registration");
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn strings(value: Option<&Value>) -> Vec<&str> {
    match value {
        None => Vec::new(),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| match value {
                Value::String(text) => text.as_str(),
                _ => panic!("expected string in fixture array"),
            })
            .collect(),
        _ => panic!("expected fixture array"),
    }
}

pub(super) fn run(name: &str) {
    let directory = root().join(name);
    let path = directory.join("fixture.json");
    let value = if path.exists() {
        json::unmarshal(&fs::read_to_string(path).unwrap()).unwrap()
    } else {
        Value::Object(json::JsonMap::default())
    };
    let Value::Object(config) = &value else { panic!("expected fixture object") };
    for (field, _) in config {
        assert!(
            matches!(
                field,
                "entries"
                    | "contains"
                    | "absent"
                    | "occurrences"
                    | "removed"
                    | "files"
                    | "eachContainsAny"
                    | "repeat"
                    | "consumerStage"
                    | "declarations"
                    | "nodenext"
                    | "shared"
                    | "output"
            ),
            "unknown fixture field {field}"
        );
    }
    let entries = if config.get("entries").is_some() {
        strings(config.get("entries"))
    } else {
        let candidates =
            ["index.ts", "index.d.ts", "index.mts", "index.cts", "index.d.mts", "index.d.cts"];
        let entries: Vec<_> =
            candidates.into_iter().filter(|entry| directory.join(entry).is_file()).collect();
        assert_eq!(
            entries.len(),
            1,
            "{name}: expected one conventional index entry, found {entries:?}; set entries in fixture.json"
        );
        entries
    };
    assert!(!entries.is_empty(), "{name}: empty entries");
    for entry in &entries {
        assert!(
            !entry.contains('\\')
                && !Path::new(entry).is_absolute()
                && entry.split('/').all(|part| !matches!(part, "" | "." | "..")),
            "{name}: unsafe entry {entry}"
        );
        assert!(directory.join(entry).is_file(), "{name}: missing entry {entry}");
    }
    let required = strings(config.get("contains"));
    let forbidden = strings(config.get("absent"));
    let mut occurrences = Vec::new();
    if let Some(value) = config.get("occurrences") {
        let Value::Object(counts) = value else { panic!("expected occurrence object") };
        for (fragment, count) in counts {
            let Value::Number(count) = count else { panic!("expected occurrence count") };
            assert!(*count >= 0.0 && count.fract() == 0.0 && *count <= usize::MAX as f64);
            assert_ne!(fragment, "");
            occurrences.push((fragment, *count as usize));
        }
    }
    let boolean = |key| match config.get(key) {
        None => false,
        Some(Value::Bool(value)) => *value,
        _ => panic!("expected {key} boolean"),
    };
    let count = |key| {
        config.get(key).map(|value| {
            let Value::Number(count) = value else { panic!("expected {key} count") };
            assert!(*count >= 0.0 && count.fract() == 0.0 && *count <= usize::MAX as f64);
            *count as usize
        })
    };
    let generated_consumer_only = match config.get("consumerStage") {
        None => false,
        Some(Value::String(stage)) if stage == "generated" => true,
        _ => panic!("expected consumerStage generated"),
    };
    let output = match config.get("output") {
        None => None,
        Some(Value::String(name)) => {
            assert_eq!(entries.len(), 1, "output requires one entry");
            assert!(
                tsrs_core::tspath::is_declaration_file_name(name)
                    && !name.contains(['/', '\\', ':']),
                "invalid output basename"
            );
            Some(name.as_str())
        }
        _ => panic!("expected output basename"),
    };
    let each_contains_any = strings(config.get("eachContainsAny"));
    fixture::run(
        &Case {
            key: name,
            entries: &entries,
            required_output: &required,
            forbidden_output: &forbidden,
            occurrences: &occurrences,
            removed: boolean("removed"),
            files: count("files"),
            declarations: count("declarations"),
            each_contains_any: &each_contains_any,
            repeat: boolean("repeat"),
            generated_consumer_only,
            shared: boolean("shared"),
            output,
            nodenext: match config.get("nodenext") {
                None => true,
                Some(Value::Bool(value)) => *value,
                _ => panic!("expected nodenext boolean"),
            },
            ..Case::DEFAULT
        },
        &directory,
        directory.join("consumer.ts"),
    );
}
