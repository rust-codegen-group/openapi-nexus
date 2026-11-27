//! Restore nulls elided by ordinary Rust payload codecs at one union use site.

use std::collections::{HashMap, HashSet};

use sigil_stitch::error::SigilStitchError;
use sigil_stitch::prelude::{CodeBlock, TypeName, sigil_quote};
use sigil_stitch::spec::{fun_spec::FunSpec, parameter_spec::ParameterSpec};

use crate::ir::types::{IrSchemaKind, IrSpec, IrTypeExpr};

use super::{TaggedWirePlans, value_type};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum DeepNulls {
    None,
    Restore,
    // The ordinary serializer does not identify a nested union branch in its
    // JSON output. Nulls cannot be restored safely from those fields alone.
    Project,
}

pub(super) fn plan<'ir>(
    expr: &'ir IrTypeExpr,
    ir: &'ir IrSpec,
    wire_plans: &TaggedWirePlans<'_>,
) -> DeepNulls {
    requirement(expr, false, ir, wire_plans, &mut HashSet::new())
}

fn requirement<'ir>(
    expr: &'ir IrTypeExpr,
    nested: bool,
    ir: &'ir IrSpec,
    wire_plans: &TaggedWirePlans<'_>,
    visiting: &mut HashSet<(&'ir str, bool)>,
) -> DeepNulls {
    match expr.representation() {
        IrTypeExpr::Array(inner) | IrTypeExpr::Map(inner) => {
            requirement(inner, true, ir, wire_plans, visiting)
        }
        IrTypeExpr::Nullable(inner) => requirement(inner, nested, ir, wire_plans, visiting),
        IrTypeExpr::Named(name)
            if wire_plans.get(name).is_none() && visiting.insert((name, nested)) =>
        {
            let mut child = |expr, nested| requirement(expr, nested, ir, wire_plans, visiting);
            let result = match ir.schemas.get(name).map(|schema| &schema.kind) {
                Some(IrSchemaKind::Object(object)) => {
                    let children = object
                        .properties
                        .values()
                        .map(|prop| {
                            let own = if nested && prop.required && prop.nullable {
                                DeepNulls::Restore
                            } else {
                                DeepNulls::None
                            };
                            own.max(child(&prop.type_expr, true))
                        })
                        .max()
                        .unwrap_or(DeepNulls::None);
                    children.max(
                        object
                            .additional_properties
                            .as_ref()
                            .map_or(DeepNulls::None, |value| child(value, true)),
                    )
                }
                Some(IrSchemaKind::Intersection(composed)) => composed
                    .members
                    .iter()
                    .map(|member| child(member, nested))
                    .max()
                    .unwrap_or(DeepNulls::None),
                Some(IrSchemaKind::Union(union))
                    if union
                        .members
                        .iter()
                        .any(|member| child(member, true) != DeepNulls::None) =>
                {
                    DeepNulls::Project
                }
                Some(IrSchemaKind::TaggedUnion(union))
                    if union
                        .variants
                        .iter()
                        .any(|case| child(&case.content_type, true) != DeepNulls::None) =>
                {
                    DeepNulls::Project
                }
                Some(IrSchemaKind::Alias(inner)) => child(inner, nested),
                _ => DeepNulls::None,
            };
            visiting.remove(&(name, nested));
            result
        }
        // Inline unions use Value; adopted nested codecs handle their own nulls.
        _ => DeepNulls::None,
    }
}

/// One private writer per relevant native model. Record the name before
/// expanding its children so recursive containers do not expand infinitely.
pub(super) struct Restoration<'ir, 'plans> {
    ir: &'ir IrSpec,
    wire_plans: &'plans TaggedWirePlans<'ir>,
    names: Vec<&'ir str>,
    indices: HashMap<&'ir str, usize>,
}

impl<'ir, 'plans> Restoration<'ir, 'plans> {
    pub(super) fn new(ir: &'ir IrSpec, wire_plans: &'plans TaggedWirePlans<'ir>) -> Self {
        Self {
            ir,
            wire_plans,
            names: Vec::new(),
            indices: HashMap::new(),
        }
    }

    pub(super) fn root(
        &mut self,
        expr: &'ir IrTypeExpr,
        tag: &str,
    ) -> Result<Option<CodeBlock>, SigilStitchError> {
        self.statement(
            expr,
            sigil_quote!(RustLang { &mut value })?,
            sigil_quote!(RustLang { Some($S(tag)) })?,
        )
    }

    fn statement(
        &mut self,
        expr: &'ir IrTypeExpr,
        access: CodeBlock,
        excluded: CodeBlock,
    ) -> Result<Option<CodeBlock>, SigilStitchError> {
        Ok(match expr.representation() {
            IrTypeExpr::Named(name)
                if requirement(expr, true, self.ir, self.wire_plans, &mut HashSet::new())
                    == DeepNulls::Restore =>
            {
                let index = *self.indices.entry(name).or_insert_with(|| {
                    let index = self.names.len();
                    self.names.push(name);
                    index
                });
                Some(sigil_quote!(RustLang { $N(symbol(index))($C(access), $C(excluded)); })?)
            }
            IrTypeExpr::Nullable(inner) => self.statement(inner, access, excluded)?,
            IrTypeExpr::Array(inner) | IrTypeExpr::Map(inner) => {
                let (method, iter) = if matches!(expr.representation(), IrTypeExpr::Array(_)) {
                    ("as_array_mut", "iter_mut")
                } else {
                    ("as_object_mut", "values_mut")
                };
                self.statement(
                    inner,
                    sigil_quote!(RustLang { item })?,
                    sigil_quote!(RustLang { None })?,
                )?
                .map(|child| {
                    sigil_quote!(RustLang {
                        if let Some(items) = ($C(access)).$N(method)() {
                            for item in items.$N(iter)() { $C(child) }
                        }
                    })
                })
                .transpose()?
            }
            _ => None,
        })
    }

    fn model(&mut self, name: &'ir str) -> Result<CodeBlock, SigilStitchError> {
        let schema = &self.ir.schemas[name];
        match &schema.kind {
            IrSchemaKind::Object(object) => {
                let mut fields = Vec::new();
                for (key, property) in &object.properties {
                    if let Some(child) = self.statement(
                        &property.type_expr,
                        sigil_quote!(RustLang { item })?,
                        sigil_quote!(RustLang { None })?,
                    )? {
                        fields.push(sigil_quote!(RustLang {
                            if let Some(item) = object.get_mut($S(key)) { $C(child) }
                        })?);
                    }
                }
                let keys = object.properties.keys().collect::<Vec<_>>();
                let additional = object
                    .additional_properties
                    .as_ref()
                    .map(|expr| {
                        self.statement(
                            expr,
                            sigil_quote!(RustLang { item })?,
                            sigil_quote!(RustLang { None })?,
                        )
                    })
                    .transpose()?
                    .flatten();
                let required = object
                    .properties
                    .iter()
                    .filter_map(|(key, property)| {
                        (property.required && property.nullable).then_some(key)
                    })
                    .collect::<Vec<_>>();
                sigil_quote!(RustLang {
                    $if(required.is_empty()) { let _ = excluded; }
                    if let Some(object) = value.as_object_mut() {
                        $for(key in &required) {
                            if excluded != Some($S(*key)) { object.entry($S(*key)).or_insert(serde_json::Value::Null); }
                        }
                        $C_each(fields)
                        $if(additional.is_some()) {
                            $if(keys.is_empty()) {
                                for item in object.values_mut() { $C(additional.unwrap()) }
                            } $else {
                                let known = [$for(key in &keys; separator = ", ") { $S(*key) }];
                                for (key, item) in object {
                                    if !known.contains(&key.as_str()) { $C(additional.unwrap()) }
                                }
                            }
                        }
                    }
                })
            }
            IrSchemaKind::Intersection(intersection) => {
                let mut members = Vec::new();
                for member in &intersection.members {
                    if let Some(child) = self.statement(
                        member,
                        sigil_quote!(RustLang { value })?,
                        sigil_quote!(RustLang { excluded })?,
                    )? {
                        members.push(child);
                    }
                }
                sigil_quote!(RustLang { let _ = excluded; $C_each(members) })
            }
            IrSchemaKind::Alias(expr) => {
                let child = self
                    .statement(
                        expr,
                        sigil_quote!(RustLang { value })?,
                        sigil_quote!(RustLang { excluded })?,
                    )?
                    .expect("registered alias needs null restoration");
                sigil_quote!(RustLang { let _ = excluded; $C(child) })
            }
            _ => unreachable!("only native models with definite fields can be restored"),
        }
    }

    pub(super) fn functions(&mut self) -> Result<Vec<FunSpec>, SigilStitchError> {
        let mut functions = Vec::new();
        while let Some(name) = self.names.get(functions.len()).copied() {
            let body = self.model(name)?;
            functions.push(
                FunSpec::builder(&symbol(functions.len()))
                    .add_param(ParameterSpec::of(
                        "value",
                        TypeName::reference_mut(value_type()),
                    ))
                    .add_param(ParameterSpec::of(
                        "excluded",
                        TypeName::optional(TypeName::reference(TypeName::primitive("str"))),
                    ))
                    .body(body)
                    .build()?,
            );
        }
        Ok(functions)
    }
}

fn symbol(index: usize) -> String {
    format!("__nexus_restore_required_null_{index}")
}
