//! Direct schema dependency facts for lowering and generator planning.
//!
//! Collection does not resolve aliases, decide which models to retain, or apply
//! support/error policy. Names are borrowed and deduplicated in first-use order.
//! Declaration names and reference-shaped example/default/extension data are not
//! dependencies. Keeping traversal order also keeps consumer diagnostics stable.
//!
//! The native adapters walk the structural schema edges currently consumed by
//! lowering, before composition can project constraints away. They are not a
//! complete OpenAPI reference walker: unsupported fields such as `prefixItems`
//! and non-local references remain outside the native adapter. The IR adapter
//! collects `Named` identifiers verbatim from retained model and operation types
//! after normalization and parameter overrides; it does not parse them as refs.

use indexmap::IndexSet;

use crate::ir::types::{IrSchemaKind, IrSpec, IrTypeExpr};

pub(crate) trait SchemaDependencies {
    fn schema_dependencies(&self) -> IndexSet<&str>;
}

use crate::ir::schema_edges::{SchemaEdgeMode, SchemaEdges};

impl<S> SchemaDependencies for S
where
    S: SchemaEdges,
{
    fn schema_dependencies(&self) -> IndexSet<&str> {
        fn collect<'a, S: SchemaEdges>(schema: &'a S, dependencies: &mut IndexSet<&'a str>) {
            schema
                .visit_edges(SchemaEdgeMode::DependencyFacts, |member| {
                    if let Some(reference) = S::reference(member)
                        && let Some(name) = reference.strip_prefix("#/components/schemas/")
                    {
                        dependencies.insert(name);
                    }
                    if let Some(object) = S::object(member) {
                        collect(object, dependencies);
                    }
                    Ok(())
                })
                .expect("dependency edge traversal is infallible");
        }

        let mut dependencies = IndexSet::new();
        collect(self, &mut dependencies);
        dependencies
    }
}

impl SchemaDependencies for IrSpec {
    fn schema_dependencies(&self) -> IndexSet<&str> {
        let mut dependencies = IndexSet::new();
        for schema in self.schemas.values() {
            collect_schema(&schema.kind, &mut dependencies);
        }
        for operation in &self.operations {
            for parameter in &operation.parameters {
                collect_type(&parameter.type_expr, &mut dependencies);
            }
            if let Some(body) = &operation.request_body {
                for value in body.content.values() {
                    collect_type(value, &mut dependencies);
                }
            }
            for response in &operation.responses {
                for value in response
                    .content
                    .values()
                    .chain(response.item_content.values())
                {
                    collect_type(value, &mut dependencies);
                }
                for header in response.headers.values() {
                    collect_type(&header.type_expr, &mut dependencies);
                }
            }
        }
        dependencies
    }
}

fn collect_schema<'a>(kind: &'a IrSchemaKind, dependencies: &mut IndexSet<&'a str>) {
    match kind {
        IrSchemaKind::Object(object) => {
            for property in object.properties.values() {
                collect_type(&property.type_expr, dependencies);
            }
            if let Some(value) = &object.additional_properties {
                collect_type(value, dependencies);
            }
        }
        IrSchemaKind::TaggedUnion(union) => {
            for variant in &union.variants {
                collect_type(&variant.content_type, dependencies);
                if let Some(value) = &variant.additional_properties {
                    collect_type(value, dependencies);
                }
            }
        }
        IrSchemaKind::Union(union) => {
            for member in &union.members {
                collect_type(member, dependencies);
            }
        }
        IrSchemaKind::Intersection(intersection) => {
            for member in &intersection.members {
                collect_type(member, dependencies);
            }
        }
        IrSchemaKind::Alias(value) => collect_type(value, dependencies),
        IrSchemaKind::Enum(_) => {}
    }
}

fn collect_type<'a>(value: &'a IrTypeExpr, dependencies: &mut IndexSet<&'a str>) {
    match value {
        IrTypeExpr::Named(name) => {
            dependencies.insert(name);
        }
        IrTypeExpr::Array(inner) | IrTypeExpr::Map(inner) | IrTypeExpr::Nullable(inner) => {
            collect_type(inner, dependencies);
        }
        IrTypeExpr::ScalarEnum { representation, .. } => {
            collect_type(representation, dependencies);
        }
        IrTypeExpr::Union(members) => {
            for member in members {
                collect_type(member, dependencies);
            }
        }
        IrTypeExpr::Primitive(_)
        | IrTypeExpr::StringLiteral(_)
        | IrTypeExpr::StringEnum(_)
        | IrTypeExpr::Any => {}
    }
}

#[cfg(test)]
mod tests;
