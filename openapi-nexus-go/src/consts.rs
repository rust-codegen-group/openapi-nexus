//! Configuration constants for Go code generation

/// Maximum line width for pretty printing Go code
pub const MAX_LINE_WIDTH: usize = 100;

/// Go reserved keywords that cannot be used as identifiers
pub const GO_RESERVED_KEYWORDS: &[&str] = &[
    "break", "default", "func", "interface", "select", "case", "defer", "go", "map", "struct",
    "chan", "else", "goto", "package", "switch", "const", "fallthrough", "if", "range", "type",
    "continue", "for", "import", "return", "var",
];

/// Escape a Go identifier if it's a reserved keyword by appending an underscore
pub fn escape_go_keyword(identifier: &str) -> String {
    if GO_RESERVED_KEYWORDS.contains(&identifier) {
        format!("{}_", identifier)
    } else {
        identifier.to_string()
    }
}
