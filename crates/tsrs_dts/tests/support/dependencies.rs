use super::*;
use std::sync::OnceLock;
use tsrs_core::json::{self, Value};

fn object(value: &Value) -> &tsrs_core::json::JsonMap {
    let Value::Object(value) = value else { panic!("expected dependency metadata object") };
    value
}

fn strings(value: &Value) -> impl Iterator<Item = &str> {
    let Value::Array(value) = value else { panic!("expected dependency metadata array") };
    value.iter().map(|value| {
        let Value::String(value) = value else { panic!("expected dependency metadata string") };
        value.as_str()
    })
}

fn policies() -> &'static Value {
    static POLICIES: OnceLock<Value> = OnceLock::new();
    POLICIES.get_or_init(|| {
        json::unmarshal(
            &fs::read_to_string(
                Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixture-environments.json"),
            )
            .unwrap(),
        )
        .unwrap()
    })
}

pub(super) fn configure(key: &str, config: &str) -> String {
    let Some(policy) =
        object(policies()).get(key.strip_prefix("rolldown-plugin-dts/").unwrap_or(key))
    else {
        return config.to_owned();
    };
    let mut config = json::unmarshal(config).unwrap();
    let Value::Object(ref mut root) = config else { unreachable!() };
    let Some(Value::Object(options)) = root.get_mut("compilerOptions") else { unreachable!() };
    for key in ["types", "lib", "module", "moduleResolution"] {
        if let Some(value) = object(policy).get(key) {
            options.insert(key, value.clone());
        }
    }
    json::marshal(&config).unwrap()
}

fn read_package(root: &Path, directory: &Path, destination: &str, files: &mut Files) {
    for entry in fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        assert!(!kind.is_symlink(), "unexpected dependency symlink");
        let path = entry.path();
        if kind.is_dir() {
            if entry.file_name() != "node_modules" {
                read_package(root, &path, destination, files);
            }
        } else if entry.file_name() == "package.json"
            || tsrs_core::tspath::is_declaration_file_name(path.to_str().unwrap())
        {
            files.insert(
                format!(
                    "{destination}/{}",
                    path.strip_prefix(root).unwrap().to_str().unwrap().replace('\\', "/")
                ),
                fs::read_to_string(path).unwrap(),
            );
        }
    }
}

pub(super) fn environment(key: &str) -> Files {
    static ENVIRONMENTS: OnceLock<BTreeMap<String, Files>> = OnceLock::new();
    if object(policies()).get(key.strip_prefix("rolldown-plugin-dts/").unwrap_or(key)).is_none() {
        return Files::new();
    }
    ENVIRONMENTS
        .get_or_init(|| {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/dependencies");
            let manifest =
                json::unmarshal(&fs::read_to_string(root.join("package.json")).unwrap()).unwrap();
            let direct = object(object(&manifest).get("dependencies").unwrap());
            object(policies())
                .iter()
                .map(|(fixture, policy)| {
                    let mut loaded = BTreeMap::new();
                    let mut pending: Vec<_> = strings(object(policy).get("packages").unwrap())
                        .map(|name| {
                            let package_root = root
                                .join("node_modules")
                                .join(name)
                                .canonicalize()
                                .unwrap_or_else(|_| {
                                    panic!(
                                        "Missing test dependencies: run vp run test:dts --install"
                                    )
                                });
                            let directory = format!("node_modules/{name}");
                            loaded.insert(directory.clone(), package_root.clone());
                            (package_root, directory)
                        })
                        .collect();
                    let mut files = Files::new();
                    while let Some((package_root, directory)) = pending.pop() {
                        assert!(
                            package_root.is_dir(),
                            "Missing test dependencies: run vp run test:dts --install"
                        );
                        let destination = format!("/project/{directory}");
                        read_package(&package_root, &package_root, &destination, &mut files);
                        let installed =
                            json::unmarshal(&files[&format!("{destination}/package.json")])
                                .unwrap();
                        let installed = object(&installed);
                        if let Some(version) = directory
                            .strip_prefix("node_modules/")
                            .filter(|name| !name.contains("/node_modules/"))
                            .and_then(|name| direct.get(name))
                        {
                            assert_eq!(
                                installed.get("version"),
                                Some(version),
                                "installed direct dependency differs from manifest"
                            );
                        }
                        if let Some(dependencies) = installed.get("dependencies") {
                            for (name, _) in object(dependencies) {
                                let dependency = package_root
                                    .ancestors()
                                    .map(|parent| parent.join("node_modules").join(name))
                                    .find(|candidate| candidate.join("package.json").is_file())
                                    .unwrap_or_else(|| {
                                        panic!("missing transitive dependency {name}")
                                    });
                                let dependency = dependency.canonicalize().unwrap();
                                let existing =
                                    Path::new(&directory).ancestors().find_map(|parent| {
                                        if parent
                                            .file_name()
                                            .is_some_and(|part| part == "node_modules")
                                        {
                                            return None;
                                        }
                                        let candidate = parent.join("node_modules").join(name);
                                        loaded.get(candidate.to_str().unwrap())
                                    });
                                if existing == Some(&dependency) {
                                    continue;
                                }
                                let destination = if existing.is_some() {
                                    format!("{directory}/node_modules/{name}")
                                } else {
                                    format!("node_modules/{name}")
                                };
                                loaded.insert(destination.clone(), dependency.clone());
                                pending.push((dependency, destination));
                            }
                        }
                    }
                    if let Some(Value::String(kind)) = object(policy).get("packageType") {
                        files.insert(
                            "/project/package.json".into(),
                            format!(r#"{{"type":{}}}"#, json::marshal_string(kind)),
                        );
                    }
                    if let Some(Value::String(directory)) = object(policy).get("files") {
                        assert_eq!(directory, fixture);
                        let base = root.parent().unwrap().join("environments").join(directory);
                        read_package(&base, &base.join("node_modules"), "/project", &mut files);
                    }
                    (format!("rolldown-plugin-dts/{fixture}"), files)
                })
                .collect()
        })
        .get(key)
        .cloned()
        .unwrap_or_default()
}
