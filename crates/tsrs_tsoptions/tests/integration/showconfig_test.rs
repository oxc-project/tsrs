// Port of Go's tsoptions/showconfig_test.go: the hand-written serializeCompilerOptions checked against a serializer
// that walks the options like Go's reflection-based one. Go also tries the out-of-range Tristate(255) and -1 for every
// int32 enum; Rust cannot hold either for Tristate, ModuleDetectionKind or NewLineKind.

use std::sync::Arc;

use tsrs_core::collections::OrderedMap;
use tsrs_core::json::{JsonMap, Value};
use tsrs_core::tspath;
use tsrs_core::{
    CompilerOptions, JsxEmit, ModuleDetectionKind, ModuleKind, ModuleResolutionKind, NewLineKind,
    PluginImport, ScriptTarget, Tristate,
};
use tsrs_diagnostics::{COMMAND_LINE_OPTIONS, OUTPUT_FORMATTING};
use tsrs_tsoptions::{
    COMMAND_LINE_COMPILER_OPTIONS_MAP, CommandLineOption, CommandLineOptionKind,
    CompilerOptionsValue, OPTIONS_DECLARATIONS, for_each_compiler_option_value,
    for_each_compiler_options_field, serialize_compiler_option_enum,
    serialize_compiler_option_path, serialize_compiler_options,
};

const CONFIG_FILE_PATH: &str = "/project/tsconfig.json";
const CASE_SENSITIVE: bool = true;

fn option_value_to_json(value: &CompilerOptionsValue) -> Value {
    match value {
        CompilerOptionsValue::Null => Value::Null,
        CompilerOptionsValue::Bool(value) => Value::Bool(*value),
        CompilerOptionsValue::Int(value) => Value::Number(*value as f64),
        CompilerOptionsValue::Float(value) => Value::Number(*value),
        CompilerOptionsValue::String(value) => Value::String(value.clone()),
        CompilerOptionsValue::StringArray(values) => {
            Value::Array(values.iter().map(|value| Value::String(value.clone())).collect())
        }
        CompilerOptionsValue::Array(values) => {
            Value::Array(values.iter().map(option_value_to_json).collect())
        }
        CompilerOptionsValue::Object(entries) => Value::Object(
            entries.iter().map(|(key, value)| (key.clone(), option_value_to_json(value))).collect(),
        ),
        value => panic!("unexpected compiler option value {value:?}"),
    }
}

fn reflected_serialize_compiler_options(
    options: &CompilerOptions,
    config_file_path: &str,
    use_case_sensitive_file_names: bool,
) -> JsonMap {
    let mut result = JsonMap::with_capacity(32);
    let config_dir = tspath::get_directory_path(config_file_path);
    let path = |value: &str| {
        serialize_compiler_option_path(
            &tspath::get_normalized_absolute_path(value, &config_dir),
            config_file_path,
            use_case_sensitive_file_names,
        )
    };
    for_each_compiler_option_value(
        options,
        |_| true,
        |option, field, _| {
            if option.category.is_some_and(|category| {
                std::ptr::eq(category, &raw const COMMAND_LINE_OPTIONS)
                    || std::ptr::eq(category, &raw const OUTPUT_FORMATTING)
            }) || field.is_zero()
            {
                return false;
            }
            let value = field.interface();
            if let Some(enum_map) = option.enum_map() {
                if let Some((key, _)) =
                    enum_map.iter().find(|(_, enum_value)| **enum_value == value)
                {
                    result.insert(option.name, Value::String((*key).to_string()));
                }
                return false;
            }
            let serialized = match option.kind {
                CommandLineOptionKind::ListOrElement => {
                    panic!("listOrElement option should not reach serialization")
                }
                CommandLineOptionKind::List => match (option.elements(), &value) {
                    (Some(element), CompilerOptionsValue::StringArray(values))
                        if element.is_file_path =>
                    {
                        Value::Array(
                            values
                                .iter()
                                .map(|value| {
                                    Value::String(if value.is_empty() {
                                        String::new()
                                    } else {
                                        path(value)
                                    })
                                })
                                .collect(),
                        )
                    }
                    (Some(element), CompilerOptionsValue::StringArray(values))
                        if element.enum_map().is_some() =>
                    {
                        let enum_map = element.enum_map().unwrap();
                        Value::Array(
                            values
                                .iter()
                                .map(|item| {
                                    let name = enum_map.iter().find(|(_, enum_value)| {
                                        matches!(enum_value, CompilerOptionsValue::String(enum_value) if enum_value == item)
                                    });
                                    Value::String(name.map_or_else(|| item.clone(), |(name, _)| (*name).to_string()))
                                })
                                .collect(),
                        )
                    }
                    _ => option_value_to_json(&value),
                },
                CommandLineOptionKind::String => match &value {
                    CompilerOptionsValue::String(value) if option.is_file_path => {
                        Value::String(path(value))
                    }
                    value => option_value_to_json(value),
                },
                CommandLineOptionKind::Boolean => {
                    let CompilerOptionsValue::Tristate(tristate) = value else {
                        panic!("boolean option {} holds {value:?}", option.name)
                    };
                    if tristate.is_true() {
                        Value::Bool(true)
                    } else if tristate.is_false() {
                        Value::Bool(false)
                    } else {
                        return false;
                    }
                }
                _ => option_value_to_json(&value),
            };
            result.insert(option.name, serialized);
            false
        },
    );
    result
}

fn check(options: &CompilerOptions) -> Result<(), String> {
    let got = serialize_compiler_options(options, CONFIG_FILE_PATH, CASE_SENSITIVE);
    let mut want = reflected_serialize_compiler_options(options, CONFIG_FILE_PATH, CASE_SENSITIVE);
    want.shift_remove("listFiles");
    want.shift_remove("listEmittedFiles");
    if got.iter().eq(want.iter()) {
        return Ok(());
    }
    Err(format!("Serialization differs:\ngot:  {got:?}\nwant: {want:?}"))
}

// compilerOptionTestValues plus the values TestShowConfigSerialization appends for each field type.
trait TestValues: Clone {
    fn test_values(option: Option<&CommandLineOption>) -> Vec<Self>;
}

impl TestValues for Tristate {
    fn test_values(_: Option<&CommandLineOption>) -> Vec<Self> {
        vec![Tristate::Unknown, Tristate::False, Tristate::True]
    }
}

impl TestValues for String {
    fn test_values(_: Option<&CommandLineOption>) -> Vec<Self> {
        ["", "b", "c", "/project/src", "../other", "./local"].map(str::to_string).to_vec()
    }
}

impl TestValues for Option<Vec<String>> {
    fn test_values(_: Option<&CommandLineOption>) -> Vec<Self> {
        let items = ["lib.es2015.d.ts", "lib.es2016.d.ts", "unknown", "/project/types"];
        vec![
            None,
            Some(Vec::new()),
            Some(vec![String::new()]),
            Some(items.map(str::to_string).to_vec()),
        ]
    }
}

impl TestValues for Option<i64> {
    fn test_values(_: Option<&CommandLineOption>) -> Vec<Self> {
        vec![None, Some(0), Some(0)]
    }
}

impl TestValues for Option<Arc<OrderedMap<String, Option<Vec<String>>>>> {
    fn test_values(_: Option<&CommandLineOption>) -> Vec<Self> {
        vec![None, Some(Arc::default()), Some(Arc::default())]
    }
}

impl TestValues for Option<Vec<PluginImport>> {
    fn test_values(_: Option<&CommandLineOption>) -> Vec<Self> {
        vec![None, Some(Vec::new()), Some(vec![PluginImport::default()])]
    }
}

macro_rules! enum_test_values {
    ($($ty:ident: [$($value:expr),*], [$($invalid:expr),*];)*) => {
        $(impl TestValues for $ty {
            fn test_values(option: Option<&CommandLineOption>) -> Vec<Self> {
                let mut values = vec![$($value),*];
                values.extend(option.and_then(CommandLineOption::enum_map).into_iter().flat_map(|enum_map| {
                    enum_map.values().map(|value| match value {
                        CompilerOptionsValue::$ty(value) => *value,
                        value => panic!("{} enum map holds {value:?}", stringify!($ty)),
                    })
                }));
                values.extend::<[Self; _]>([$($invalid),*]);
                values
            }
        })*
    };
}

enum_test_values! {
    JsxEmit: [JsxEmit(0), JsxEmit(1), JsxEmit(2)], [JsxEmit(-1)];
    ModuleKind: [ModuleKind(0), ModuleKind(1), ModuleKind(2)], [ModuleKind(-1)];
    ModuleResolutionKind: [ModuleResolutionKind(0), ModuleResolutionKind(1), ModuleResolutionKind(2)],
        [ModuleResolutionKind(-1)];
    ScriptTarget: [ScriptTarget(0), ScriptTarget(1), ScriptTarget(2)], [ScriptTarget(-1)];
    ModuleDetectionKind: [ModuleDetectionKind::None, ModuleDetectionKind::Auto, ModuleDetectionKind::Legacy], [];
    NewLineKind: [NewLineKind::None, NewLineKind::CRLF, NewLineKind::LF], [];
}

fn field_test_values<T: TestValues>(_field: &T, name: &str) -> Vec<T> {
    T::test_values(COMMAND_LINE_COMPILER_OPTIONS_MAP.get(name))
}

#[test]
fn test_show_config_serialization() {
    let mut failures = Vec::new();
    if let Err(error) = check(&CompilerOptions::default()) {
        failures.push(format!("zero options: {error}"));
    }
    let mut all_options = CompilerOptions::default();
    macro_rules! check_fields {
        ($($field:ident: $json:literal,)*) => {
            $({
                let values = field_test_values(&all_options.$field, $json);
                all_options.$field = values.last().unwrap().clone();
                for value in values {
                    let options = CompilerOptions { $field: value, ..Default::default() };
                    if let Err(error) = check(&options) {
                        failures.push(format!("{}: {error}", $json));
                    }
                }
            })*
        };
    }
    for_each_compiler_options_field!(check_fields);
    if let Err(error) = check(&all_options) {
        failures.push(format!("all options: {error}"));
    }
    let mut paths = OrderedMap::default();
    paths.insert("@/*".to_string(), Some(vec!["./src/*".to_string(), "../shared/*".to_string()]));
    paths.insert("empty".to_string(), Some(Vec::new()));
    let options = CompilerOptions {
        paths: Some(Arc::new(paths)),
        plugins: Some(vec![PluginImport { name: "plugin".to_string() }]),
        max_node_module_js_depth: Some(0),
        ..Default::default()
    };
    if let Err(error) = check(&options) {
        failures.push(format!("paths, plugins and maxNodeModuleJsDepth: {error}"));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn test_show_config_enum_serialization() {
    let mut failures = Vec::new();
    for option in OPTIONS_DECLARATIONS.iter() {
        let Some(enum_map) = option.enum_map() else {
            continue;
        };
        let mut seen: Vec<(&CompilerOptionsValue, &str)> = Vec::new();
        for (name, value) in enum_map {
            let first_alias = if let Some((_, first_alias)) =
                seen.iter().find(|(seen_value, _)| *seen_value == value)
            {
                *first_alias
            } else {
                seen.push((value, name));
                name
            };
            let got = serialize_compiler_option_enum(value);
            if got != first_alias {
                failures.push(format!(
                    "{}: {value:?}: got {got:?}, want first alias {first_alias:?}",
                    option.name
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
