use bitflags::bitflags;

bitflags! {
    #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
    pub struct EmitFlags: u32 {
        const SINGLE_LINE = 1 << 0; // The contents of this node should be emitted on a single line.
        const MULTI_LINE = 1 << 1; // The contents of this node should be emitted on multiple lines.
        const NO_LEADING_SOURCE_MAP = 1 << 2; // Do not emit a leading source map location for this node.
        const NO_TRAILING_SOURCE_MAP = 1 << 3; // Do not emit a trailing source map location for this node.
        const NO_NESTED_SOURCE_MAPS = 1 << 4; // Do not emit source map locations for children of this node.
        const NO_TOKEN_LEADING_SOURCE_MAPS = 1 << 5; // Do not emit leading source map location for token nodes.
        const NO_TOKEN_TRAILING_SOURCE_MAPS = 1 << 6; // Do not emit trailing source map location for token nodes.
        const NO_LEADING_COMMENTS = 1 << 7; // Do not emit leading comments for this node.
        const NO_TRAILING_COMMENTS = 1 << 8; // Do not emit trailing comments for this node.
        const NO_NESTED_COMMENTS = 1 << 9; // Do not emit nested comments for children of this node.
        const HELPER_NAME = 1 << 10; // The Identifier refers to an *unscoped* emit helper (one that is emitted at the top of the file)
        const EXPORT_NAME = 1 << 11; // Ensure an export prefix is added for an identifier that points to an exported declaration with a local name (see SymbolFlags.ExportHasLocal).
        const LOCAL_NAME = 1 << 12; // Ensure an export prefix is not added for an identifier that points to an exported declaration.
        const INDENTED = 1 << 13; // Adds an explicit extra indentation level for class and function bodies when printing (used to match old emitter).
        const NO_INDENTATION = 1 << 14; // Do not indent the node.
        const REUSE_TEMP_VARIABLE_SCOPE = 1 << 15; // Reuse the existing temp variable scope during emit.
        const CUSTOM_PROLOGUE = 1 << 16; // Treat the statement as if it were a prologue directive (NOTE: Prologue directives are *not* transformed).
        const NO_ASCII_ESCAPING = 1 << 17; // When synthesizing nodes that lack an original node or textSourceNode, we want to write the text on the node with ASCII escaping substitutions.
        const EXTERNAL_HELPERS = 1 << 18; // This source file has external helpers
        const START_ON_NEW_LINE = 1 << 19; // Start this node on a new line
        const INDIRECT_CALL = 1 << 20; // Emit CallExpression as an indirect call: `(0, f)()`
        const ASYNC_FUNCTION_BODY = 1 << 21; // The node was originally an async function body.
        const NO_LEXICAL_ARGUMENTS = 1 << 22; // Do not capture `arguments` for this arrow function. Set on arrows lowered from class static blocks, where `arguments` is an error; preserves Strada's emit behavior.
        const TRANSFORM_PRIVATE_STATIC_ELEMENTS = 1 << 23; // Indicates static private elements in a file or class should be transformed regardless of --target (used by esDecorators transform).
        const NO_LEXICAL_THIS = 1 << 24; // Do not capture `this` for this node's subtree. Set on relocated static initializers, where `this` is handled by the class fields transform.
        const INTERNAL_SYMBOL_NAME = 1 << 25; // Port-only: this string literal's text is a symbol name whose leading U+007F is the port's stand-in for Go's internal-name prefix byte 0xFE.

        const NONE = 0;
        const NO_SOURCE_MAP = Self::NO_LEADING_SOURCE_MAP.bits() | Self::NO_TRAILING_SOURCE_MAP.bits(); // Do not emit a source map location for this node.
        const NO_TOKEN_SOURCE_MAPS = Self::NO_TOKEN_LEADING_SOURCE_MAPS.bits() | Self::NO_TOKEN_TRAILING_SOURCE_MAPS.bits(); // Do not emit source map locations for tokens of this node.
        const NO_COMMENTS = Self::NO_LEADING_COMMENTS.bits() | Self::NO_TRAILING_COMMENTS.bits(); // Do not emit comments for this node.
    }
}
