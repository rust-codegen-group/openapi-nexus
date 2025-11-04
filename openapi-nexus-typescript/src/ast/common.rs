//! Common TypeScript AST types

pub mod ts_doc_comment;
pub mod ts_enum_variant;
pub mod ts_generic;
pub mod ts_parameter;
pub mod ts_property;
pub mod ts_visibility;

pub use ts_doc_comment::TsDocComment;
pub use ts_enum_variant::{TsEnumValue, TsEnumVariant};
pub use ts_generic::TsGeneric;
pub use ts_parameter::TsParameter;
pub use ts_property::TsProperty;
pub use ts_visibility::TsVisibility;
