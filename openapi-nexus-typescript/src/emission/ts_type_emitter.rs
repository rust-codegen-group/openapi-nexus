//! TypeScript type expression emitter

use pretty::RcDoc;

use crate::ast::{TsExpression, TsPrimitive};
use openapi_nexus_core::traits::ToRcDoc;

/// Helper struct for emitting TypeScript type expressions
pub struct TsTypeEmitter;

impl TsTypeEmitter {
    /// Emit a TypeExpression as a pretty-printed RcDoc
    pub fn emit_type_expression_doc(&self, type_expr: &TsExpression) -> RcDoc<'static, ()> {
        self.emit_type_expression_doc_with_indent(type_expr, 0)
    }

    /// Emit a TypeExpression as a pretty-printed RcDoc with specified indentation level
    pub fn emit_type_expression_doc_with_indent(
        &self,
        type_expr: &TsExpression,
        indent_level: usize,
    ) -> RcDoc<'static, ()> {
        match type_expr {
            TsExpression::Primitive(primitive) => {
                let type_name = match primitive {
                    TsPrimitive::String => "string",
                    TsPrimitive::Number => "number",
                    TsPrimitive::Boolean => "boolean",
                    TsPrimitive::Null => "null",
                    TsPrimitive::Undefined => "undefined",
                    TsPrimitive::Any => "any",
                    TsPrimitive::Unknown => "unknown",
                    TsPrimitive::Void => "void",
                    TsPrimitive::Never => "never",
                };
                RcDoc::text(type_name.to_string())
            }
            TsExpression::Array(item_type) => {
                let item_doc =
                    self.emit_type_expression_doc_with_indent(item_type, indent_level + 1);
                RcDoc::text("Array<".to_string())
                    .append(item_doc)
                    .append(RcDoc::text(">".to_string()))
            }
            TsExpression::Union(types) => {
                let docs: Vec<RcDoc<'static, ()>> = types
                    .iter()
                    .map(|t| self.emit_type_expression_doc_with_indent(t, indent_level + 1))
                    .collect();
                if docs.len() == 1 {
                    docs[0].clone()
                } else {
                    let separator = RcDoc::text(" | ");
                    RcDoc::intersperse(docs, separator)
                }
            }
            TsExpression::Intersection(types) => {
                let docs: Vec<RcDoc<'static, ()>> = types
                    .iter()
                    .map(|t| self.emit_type_expression_doc_with_indent(t, indent_level))
                    .collect();
                if docs.len() == 1 {
                    docs[0].clone()
                } else {
                    let separator = RcDoc::text(" & ");
                    RcDoc::intersperse(docs, separator)
                }
            }
            TsExpression::Reference(name) => RcDoc::text(name.clone()),
            TsExpression::Literal(value) => RcDoc::text(value.clone()),
            TsExpression::Object(properties) => {
                if properties.is_empty() {
                    RcDoc::text("{}")
                } else {
                    // Check if this object should be formatted inline or multiline
                    let should_multiline = self.should_format_object_multiline(properties);
                    if should_multiline {
                        // Multi-line format with proper indentation
                        let mut result = RcDoc::text("{");
                        result = result.append(RcDoc::line());

                        let current_indent = "  ".repeat(indent_level + 1);
                        for (i, (name, type_expr)) in properties.iter().enumerate() {
                            let type_doc = self
                                .emit_type_expression_doc_with_indent(type_expr, indent_level + 1);
                            let prop_doc = RcDoc::text(current_indent.clone())
                                .append(RcDoc::text(name.clone()))
                                .append(RcDoc::text(": "))
                                .append(type_doc)
                                .append(RcDoc::text(";"));

                            result = result.append(prop_doc);
                            if i < properties.len() - 1 {
                                result = result.append(RcDoc::line());
                            }
                        }

                        result = result.append(RcDoc::line());
                        let closing_indent = "  ".repeat(indent_level);
                        result = result.append(RcDoc::text(closing_indent));
                        result = result.append(RcDoc::text("}"));
                        result
                    } else {
                        // Inline format for simple objects
                        let props: Vec<RcDoc<'_, ()>> = properties
                            .iter()
                            .map(|(name, type_expr)| {
                                let type_doc = self
                                    .emit_type_expression_doc_with_indent(type_expr, indent_level);
                                RcDoc::text(name.clone())
                                    .append(RcDoc::text(": "))
                                    .append(type_doc)
                            })
                            .collect();

                        let separator = RcDoc::text("; ");
                        RcDoc::text("{ ")
                            .append(RcDoc::intersperse(props, separator))
                            .append(RcDoc::text(" }"))
                            .group()
                    }
                }
            }
            TsExpression::Function {
                parameters,
                return_type,
            } => {
                let param_docs: Vec<RcDoc<'_, ()>> =
                    parameters.iter().map(|p| p.to_rcdoc()).collect();

                let params = if param_docs.is_empty() {
                    RcDoc::text("()")
                } else {
                    RcDoc::text("(")
                        .append(RcDoc::intersperse(param_docs, RcDoc::text(", ")))
                        .append(RcDoc::text(")"))
                };

                let mut func_doc = params;
                if let Some(return_type) = return_type {
                    let return_doc =
                        self.emit_type_expression_doc_with_indent(return_type, indent_level);
                    func_doc = func_doc.append(RcDoc::text(" => ")).append(return_doc);
                }

                func_doc
            }
            TsExpression::Tuple(types) => {
                let docs: Vec<RcDoc<'static, ()>> = types
                    .iter()
                    .map(|t| self.emit_type_expression_doc_with_indent(t, indent_level))
                    .collect();
                RcDoc::text("[")
                    .append(RcDoc::intersperse(docs, RcDoc::text(", ")))
                    .append(RcDoc::text("]"))
            }
            TsExpression::Generic(name) => RcDoc::text(name.clone()),
            TsExpression::IndexSignature(key_type, value_type) => {
                let value_doc = self.emit_type_expression_doc_with_indent(value_type, indent_level);
                RcDoc::text("[key: ")
                    .append(RcDoc::text(key_type.clone()))
                    .append(RcDoc::text("]: "))
                    .append(value_doc)
            }
        }
    }

    /// Determine if an object should be formatted multiline based on complexity
    pub fn should_format_object_multiline(
        &self,
        properties: &std::collections::BTreeMap<String, TsExpression>,
    ) -> bool {
        // Format multiline if:
        // 1. More than 2 properties
        // 2. Any property has a complex nested type
        if properties.len() > 2 {
            return true;
        }

        for type_expr in properties.values() {
            if Self::is_complex_type(type_expr) {
                return true;
            }
        }

        false
    }

    /// Check if a type expression is complex (nested objects, arrays, unions, etc.)
    pub fn is_complex_type(type_expr: &TsExpression) -> bool {
        match type_expr {
            TsExpression::Object(properties) => {
                // Only consider objects complex if they have more than 2 properties
                // or contain nested complex types
                if properties.len() > 2 {
                    return true;
                }
                for prop_type in properties.values() {
                    if Self::is_complex_type(prop_type) {
                        return true;
                    }
                }
                false
            }
            TsExpression::Array(_) => true,
            TsExpression::Union(types) => types.len() > 1,
            TsExpression::Intersection(types) => types.len() > 1,
            TsExpression::Function { .. } => true,
            TsExpression::Tuple(types) => types.len() > 1,
            _ => false,
        }
    }
}
