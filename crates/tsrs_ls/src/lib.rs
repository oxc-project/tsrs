// Go internal/ls (+ lsconv, lsutil, change), astnav, format, sourcemap (docs/LSP.md).

mod api;
pub mod astnav;
pub mod autoimport;
mod autoinsert;
mod callhierarchy;
pub mod change;
mod codeactions;
mod codeactions_fixclassincorrectlyimplementsinterface;
mod codeactions_fixmissingtypeannotation;
mod codeactions_importfixes;
mod codeactions_missingmemberfixer;
mod codelens;
mod completions;
mod completions_2;
mod completions_3;
mod completions_4;
#[cfg(test)]
mod completions_smoke_test;
mod crossproject;
mod definition;
mod diagnostics;
mod displaypartswriter;
mod documenthighlights;
mod file_rename;
mod findallreferences;
#[cfg(test)]
mod findallreferences_test;
mod folding;
pub mod format;
mod host;
mod hover;
mod hovericon;
mod import_tracker;
mod inlay_hints;
mod jsdoc;
mod jsdoc_snippet;
mod languageservice;
mod linkedediting;
#[cfg(test)]
mod ls_smoke_test;
pub mod lsconv;
mod lsformat;
pub mod lsutil;
mod organizeimports;
#[cfg(test)]
mod refs_smoke_test;
mod rename;
pub use tsrs_sourcemap as sourcemap;
mod selectionranges;
mod semantictokens;
mod signaturehelp;
mod source_map;
mod sourcedefinition;
pub mod spanmap;
mod string_completions;
mod symbols;
mod utilities;

pub use api::{ERR_NO_SOURCE_FILE, ERR_NO_TOKEN_AT_POSITION};
pub use codeactions::{CodeAction, CodeFixContext, CodeFixProvider, CombinedCodeActions};
pub use completions::{
    COMPLETION_TRIGGER_CHARACTERS, CompletionItem, CompletionKind, CompletionList,
    ERR_NEEDS_AUTO_IMPORTS, KeywordCompletionFilters, SORT_TEXT_AUTO_IMPORT_SUGGESTIONS,
    SORT_TEXT_CLASS_MEMBER_SNIPPETS, SORT_TEXT_GLOBALS_OR_KEYWORDS,
    SORT_TEXT_JAVASCRIPT_IDENTIFIERS, SORT_TEXT_LOCAL_DECLARATION_PRIORITY,
    SORT_TEXT_LOCATION_PRIORITY, SORT_TEXT_MEMBER_DECLARED_BY_SPREAD_ASSIGNMENT,
    SORT_TEXT_OPTIONAL_MEMBER, SORT_TEXT_SUGGESTED_CLASS_MEMBERS, SortText, deprecate_sort_text,
    err_needs_auto_imports, is_err_needs_auto_imports, object_literal_property_sort_text,
    sort_below,
};
pub use completions_3::{
    SOURCE_CLASS_MEMBER_SNIPPET, SOURCE_OBJECT_LITERAL_MEMBER_WITH_COMMA,
    SOURCE_OBJECT_LITERAL_METHOD_SNIPPET, SOURCE_SWITCH_CASES, SOURCE_THIS_PROPERTY,
    SOURCE_TYPE_ONLY_ALIAS, compare_completion_entries,
};
pub use crossproject::*;
pub use findallreferences::{
    Definition, DefinitionKind, ReferenceEntry, SignatureUsage, SymbolAndEntries,
    SymbolAndEntriesData,
};
pub use host::*;
pub use jsdoc::{JSDocTagInfo, get_symbol_documentation_comment, get_symbol_jsdoc_tags};
pub use languageservice::*;
pub use rename::{
    RenameInfo, client_supports_document_changes, client_supports_rename_resource_operations,
    client_supports_will_rename_files,
};
pub use semantictokens::semantic_tokens_legend;
pub use signaturehelp::{SIGNATURE_HELP_RETRIGGER_CHARACTERS, SIGNATURE_HELP_TRIGGER_CHARACTERS};
pub use symbols::provide_workspace_symbols;
pub use utilities::{is_in_string, range_contains_range};
