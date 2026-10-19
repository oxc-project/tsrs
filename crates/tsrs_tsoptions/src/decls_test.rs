use rustc_hash::FxHashMap;
use tsrs_core::{CompilerOptions, Tristate};

use crate::commandlineoption::CommandLineOption;
use crate::declscompiler::{OPTIONS_DECLARATIONS, compiler_option_changes};
use crate::parsinghelpers::for_each_compiler_options_field;

// Go compares the CompilerOptions struct fields (and their json tags) with the declarations; the Rust
// field list with JSON names is `for_each_compiler_options_field`.
#[test]
fn test_compiler_options_declaration() {
    let mut decls: FxHashMap<String, &'static CommandLineOption> = FxHashMap::default();
    decls.extend(OPTIONS_DECLARATIONS.iter().map(|decl| (decl.name.to_lowercase(), *decl)));

    let internal_options = [
        "allowNonTsExtensions",
        "build",
        "configFilePath",
        "noDtsResolution",
        "noEmitForJsFiles",
        "pathsBasePath",
        "suppressOutputPathCheck",
        "build",
    ];
    let internal_options_map: FxHashMap<String, &str> =
        internal_options.iter().map(|o| (o.to_lowercase(), *o)).collect();

    let mut errors: Vec<String> = Vec::new();
    macro_rules! check {
        ($($field:ident: $json:literal,)*) => {
            $(
                let lower_name = $json.to_lowercase();
                match decls.remove(&lower_name) {
                    Some(decl) => {
                        if decl.name != $json {
                            errors.push(format!("Field {} has json name {}, but the option declaration has name {}", stringify!($field), $json, decl.name));
                        }
                    }
                    None => match internal_options_map.get(&lower_name) {
                        Some(name) => {
                            if *name != $json {
                                errors.push(format!("Field {} has json name {}, but the internal option has name {}", stringify!($field), $json, name));
                            }
                        }
                        None => errors.push(format!("CompilerOptions.{} has no options declaration", stringify!($field))),
                    },
                }
            )*
        };
    }
    for_each_compiler_options_field!(check);

    let skipped_options = ["plugins"];
    for opt in skipped_options {
        decls.remove(&opt.to_lowercase());
    }

    errors.extend(
        decls.values().map(|decl| {
            format!("Option declaration {} is not present in CompilerOptions", decl.name)
        }),
    );
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn test_compiler_option_changes() {
    let empty = CompilerOptions::default();
    let changes = compiler_option_changes(Some(&empty), Some(&empty));
    assert!(!changes.affects_semantic_diagnostics());
    assert!(!changes.affects_declaration_path());
    assert!(!changes.affects_emit());

    let changes = compiler_option_changes(None, Some(&empty));
    assert!(changes.affects_semantic_diagnostics());
    assert!(changes.affects_declaration_path());
    assert!(changes.affects_emit());

    let mut options = empty.clone();
    options.check_js = Tristate::True;
    let changes = compiler_option_changes(Some(&empty), Some(&options));
    assert!(changes.affects_semantic_diagnostics());
    assert!(!changes.affects_declaration_path());
    assert!(!changes.affects_emit());

    let mut options = empty.clone();
    options.remove_comments = Tristate::True;
    let changes = compiler_option_changes(Some(&empty), Some(&options));
    assert!(!changes.affects_semantic_diagnostics());
    assert!(!changes.affects_declaration_path());
    assert!(changes.affects_emit());

    let mut options = empty.clone();
    options.out_dir = "dist".to_string();
    let changes = compiler_option_changes(Some(&empty), Some(&options));
    assert!(!changes.affects_semantic_diagnostics());
    assert!(changes.affects_declaration_path());
    assert!(changes.affects_emit());

    let mut strict = empty;
    strict.strict = Tristate::True;
    let mut explicit_strict = strict.clone();
    explicit_strict.no_implicit_any = Tristate::True;
    let changes = compiler_option_changes(Some(&strict), Some(&explicit_strict));
    assert!(!changes.affects_semantic_diagnostics());

    explicit_strict.no_implicit_any = Tristate::False;
    let changes = compiler_option_changes(Some(&strict), Some(&explicit_strict));
    assert!(changes.affects_semantic_diagnostics());
}
