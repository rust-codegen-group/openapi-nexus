//! Emission traits for converting AST nodes to formatted output

use pretty::RcDoc;

/// Trait for converting AST nodes to RcDoc
pub trait ToRcDoc {
    /// Convert to RcDoc
    fn to_rcdoc(&self) -> RcDoc<'static, ()>;
}
