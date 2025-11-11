use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use crate::ast::{TsDocComment, TsExpression};
use openapi_nexus_core::traits::ToRcDoc;

/// TypeScript property definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsProperty {
    /// The camelCase property name used in the TypeScript interface
    pub ts_name: String,
    /// The original property name from the OpenAPI spec (used in JSON)
    pub original_name: String,
    /// The TypeScript type expression representing the property type
    pub type_expr: TsExpression,
    /// Whether the property is optional in the TypeScript interface
    pub optional: bool,
    /// Whether this property is an index signature (e.g., `[key: string]: ValueType`).
    ///
    /// Index signatures allow additional properties beyond those explicitly defined.
    /// They are used to represent OpenAPI `additionalProperties` in TypeScript interfaces.
    ///
    /// # Example
    ///
    /// ```typescript
    /// interface Example {
    ///   name: string;
    ///   [key: string]: string | number;  // This is an index signature
    /// }
    /// ```
    pub is_index_signature: bool,
    /// Documentation/comment for the property (if any)
    pub documentation: Option<TsDocComment>,
}

impl ToRcDoc for TsProperty {
    fn to_rcdoc(&self) -> RcDoc<'static, ()> {
        let mut doc = RcDoc::text(self.ts_name.clone());

        if self.optional {
            doc = doc.append(RcDoc::text("?"));
        }

        doc.append(RcDoc::text(":"))
            .append(RcDoc::space())
            .append(self.type_expr.to_rcdoc())
    }
}
