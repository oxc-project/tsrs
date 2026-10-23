// The `ls` package values the server's capabilities need (ls/completions.go, ls/signaturehelp.go), re-exported from
// tsrs_ls; the semantic tokens legend is `tsrs_ls::semantic_tokens_legend`.

pub use tsrs_ls::{
    COMPLETION_TRIGGER_CHARACTERS, SIGNATURE_HELP_RETRIGGER_CHARACTERS,
    SIGNATURE_HELP_TRIGGER_CHARACTERS,
};

pub fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(std::string::ToString::to_string).collect()
}
