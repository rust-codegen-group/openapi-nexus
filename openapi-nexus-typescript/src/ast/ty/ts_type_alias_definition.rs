use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use crate::ast::TsDocComment;
use crate::ast::{TsExpression, TsGeneric};
use crate::emission::ts_type_emitter::TsTypeEmitter;
use openapi_nexus_core::traits::ToRcDoc;

/// Information about a union member type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnionMemberInfo {
    /// The TypeScript name of the member type
    pub ts_name: String,
    /// The TypeScript expression for this member
    pub type_expr: TsExpression,
    /// Whether this member is a primitive type (string, number, etc.)
    pub is_primitive: bool,
    /// Whether this member is an interface (has instanceOf function)
    pub is_interface: bool,
}

/// Represents a TypeScript type alias definition, which may be a plain alias,
/// a union (`oneOf`/`anyOf`), or an intersection (`allOf`). This struct
/// contains all the metadata and type structure required to emit the TypeScript
/// alias as source code, and supports additional schema-based features such as
/// generics and documentation comments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsTypeAliasDefinition {
    /// The TypeScript name of the type alias (e.g., `UserOrPet`)
    pub ts_name: String,
    /// The original name from the OpenAPI/JSON Schema, if different
    pub original_name: String,
    /// The TypeScript type expression this alias refers to. This could be
    /// a union, intersection, primitive, or interface reference.
    pub type_expr: TsExpression,
    /// List of generic type parameters used by this type alias (if any)
    pub generics: Vec<TsGeneric>,
    /// Optional documentation comments to be emitted in the TypeScript file
    pub documentation: Option<TsDocComment>,
    /// For union types (`oneOf`/`anyOf`): describes the constituent members.
    /// `None` if not a union.
    pub union_members: Option<Vec<UnionMemberInfo>>,
}

impl TsTypeAliasDefinition {
    /// Create a new type alias
    pub fn new(ts_name: String, original_name: String, type_expr: TsExpression) -> Self {
        Self {
            ts_name,
            original_name,
            type_expr,
            generics: Vec::new(),
            documentation: None,
            union_members: None,
        }
    }

    /// Add generics
    pub fn with_generics(mut self, generics: Vec<TsGeneric>) -> Self {
        self.generics = generics;
        self
    }

    /// Add documentation
    pub fn with_docs(mut self, documentation: TsDocComment) -> Self {
        self.documentation = Some(documentation);
        self
    }
}

impl ToRcDoc for TsTypeAliasDefinition {
    fn to_rcdoc(&self) -> RcDoc<'static, ()> {
        let type_emitter = TsTypeEmitter;

        let mut doc = RcDoc::text("export ")
            .append(RcDoc::text("type"))
            .append(RcDoc::space())
            .append(RcDoc::text(self.ts_name.clone()));

        // Add generics
        if !self.generics.is_empty() {
            let generic_docs: Vec<_> = self.generics.iter().map(|g| g.to_rcdoc()).collect();
            let generic_strings: Vec<String> = generic_docs
                .iter()
                .map(|doc| format!("{}", doc.pretty(80)))
                .collect();
            doc = doc.append(RcDoc::text(format!("<{}>", generic_strings.join(", "))));
        }

        // Add type expression
        let type_doc = type_emitter.emit_type_expression_doc(&self.type_expr);
        doc = doc.append(RcDoc::text(" = ")).append(type_doc);

        // Add documentation if present
        if let Some(docs) = &self.documentation {
            doc = docs.to_rcdoc().append(RcDoc::line()).append(doc);
        }

        doc
    }
}
