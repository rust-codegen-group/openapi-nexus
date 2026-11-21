//! Python union and explicit-extension value conversion, shared by both transports.
//!
//! Matching consumes the target-independent shape plan. Codecs keep original
//! member boundaries: count candidates first, then invoke exactly one delegate.

use std::borrow::Cow;
use std::collections::BTreeSet;

use heck::{ToPascalCase, ToSnakeCase};
use serde::{Deserialize, Serialize};
use sigil_stitch::error::SigilStitchError;
use sigil_stitch::lang::python::Python;
use sigil_stitch::prelude::*;

use super::emit_models::{is_object_schema, python_field_name};
use super::type_names::python_type_name_for_schema;
use crate::codegen::traits::file_writer::FileInfo;
use crate::generators::union_match::{self, JsonShape, MatchScope, UnionMemberMatch};
use crate::generators::{ArtifactKind, GeneratorError, type_application};
use crate::ir::types::{IrSchemaKind, IrSpec, IrTypeExpr};

pub(super) fn optional_type(ty: TypeName) -> TypeName {
    if matches!(ty, TypeName::Optional(_)) {
        ty
    } else {
        TypeName::optional(ty)
    }
}

/// Policy for untagged `oneOf` and `anyOf` member conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UntaggedUnionStrategy {
    /// Prefer concrete members in schema order; preserve unmatched wire values.
    #[default]
    FirstMatch,
    /// Reject multiple concrete matches and values with no matching member.
    Strict,
    /// Leave union subtrees dynamically typed and unconverted.
    Raw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Direction {
    From,
    To,
}

#[derive(Default)]
struct ShapeReferences {
    named: BTreeSet<(String, Direction)>,
    predicates: Vec<(JsonShape, Direction)>,
}

impl ShapeReferences {
    fn predicate(&mut self, shape: &JsonShape, direction: Direction) -> String {
        let index = self
            .predicates
            .iter()
            .position(|(candidate, side)| candidate == shape && *side == direction)
            .unwrap_or_else(|| {
                let index = self.predicates.len();
                self.predicates.push((shape.clone(), direction));
                index
            });
        predicate_name(index)
    }
}

fn predicate_name(index: usize) -> String {
    format!("_matches_shape_{index}")
}

impl Direction {
    fn suffix(self) -> &'static str {
        match self {
            Self::From => "from_dict",
            Self::To => "to_dict",
        }
    }
}

/// A closed, deterministic graph of conversion entrypoints. Internal functions
/// use `object`; public call sites retain their semantic model/alias types.
pub(super) struct ValuePlan<'a> {
    ir: &'a IrSpec,
    public_shapes: Cow<'a, IrSpec>,
    pub strategy: UntaggedUnionStrategy,
    expressions: Vec<IrTypeExpr>,
}

impl<'a> ValuePlan<'a> {
    pub fn new(ir: &'a IrSpec, strategy: UntaggedUnionStrategy) -> Self {
        // Python exposes tagged payloads directly, not their wire wrappers.
        // Reuse the shared shape planner against this selection-only view so
        // aliases, nested properties, containers and productive references all
        // match that public representation. The source IR and codecs stay intact.
        let mut public_shapes = Cow::Borrowed(ir);
        if ir
            .schemas
            .values()
            .any(|schema| matches!(schema.kind, IrSchemaKind::TaggedUnion(_)))
        {
            for schema in public_shapes.to_mut().schemas.values_mut() {
                if let IrSchemaKind::TaggedUnion(union) = &schema.kind {
                    schema.kind = IrSchemaKind::Alias(IrTypeExpr::Union(
                        union
                            .variants
                            .iter()
                            .map(|variant| variant.content_type.clone())
                            .collect(),
                    ));
                }
            }
        }
        let mut plan = Self {
            ir,
            public_shapes,
            strategy,
            expressions: Vec::new(),
        };
        for schema in ir.schemas.values() {
            if let IrSchemaKind::Object(object) = &schema.kind {
                for property in object.properties.values() {
                    if plan.needs_conversion(&property.type_expr) {
                        plan.register(&plan.property_expr(property));
                    }
                }
                if let Some(values) = &object.additional_properties {
                    plan.register(values);
                }
            }
            if plan.needs_conversion(&IrTypeExpr::Named(schema.name.clone())) {
                plan.register(&IrTypeExpr::Named(schema.name.clone()));
            }
            for expr in schema_expressions(&schema.kind) {
                if plan.needs_conversion(expr) {
                    plan.register(expr);
                }
            }
        }
        for operation in &ir.operations {
            let request = operation
                .request_body
                .iter()
                .flat_map(|body| body.content.values());
            let responses = operation
                .responses
                .iter()
                .flat_map(|response| response.content.values());
            for expr in request.chain(responses) {
                if plan.needs_conversion(expr) {
                    plan.register(expr);
                }
            }
        }
        plan
    }

    /// Boundaries requiring planned codecs, through aliases and containers.
    pub fn needs_conversion(&self, expr: &IrTypeExpr) -> bool {
        self.contains_conversion(expr, true)
    }

    /// Preserve union-specific public helpers and field null semantics.
    pub fn has_union(&self, expr: &IrTypeExpr) -> bool {
        self.contains_conversion(expr, false)
    }

    fn contains_conversion(&self, expr: &IrTypeExpr, include_extensions: bool) -> bool {
        fn visit(
            expr: &IrTypeExpr,
            ir: &IrSpec,
            seen: &mut BTreeSet<String>,
            include_extensions: bool,
        ) -> bool {
            match expr {
                IrTypeExpr::Union(_) => true,
                IrTypeExpr::ScalarEnum { representation, .. } => {
                    visit(representation, ir, seen, include_extensions)
                }
                IrTypeExpr::Array(inner) | IrTypeExpr::Map(inner) | IrTypeExpr::Nullable(inner) => {
                    visit(inner, ir, seen, include_extensions)
                }
                IrTypeExpr::Named(name) if seen.insert(name.clone()) => {
                    match ir.schemas.get(name).map(|schema| &schema.kind) {
                        Some(IrSchemaKind::Union(_)) => true,
                        Some(IrSchemaKind::Object(object)) => {
                            include_extensions && object.additional_properties.is_some()
                        }
                        Some(IrSchemaKind::Alias(inner)) => {
                            visit(inner, ir, seen, include_extensions)
                        }
                        Some(IrSchemaKind::TaggedUnion(union)) => {
                            union.variants.iter().any(|variant| {
                                visit(&variant.content_type, ir, seen, include_extensions)
                            })
                        }
                        _ => false,
                    }
                }
                _ => false,
            }
        }
        visit(expr, self.ir, &mut BTreeSet::new(), include_extensions)
    }

    pub fn property_expr(&self, property: &crate::ir::types::IrProperty) -> IrTypeExpr {
        if property.nullable
            && !union_match::accepts_null(&property.type_expr, self.ir)
            && !matches!(
                property.type_expr,
                IrTypeExpr::ScalarEnum { .. }
                    | IrTypeExpr::StringLiteral(_)
                    | IrTypeExpr::StringEnum(_)
            )
        {
            IrTypeExpr::Nullable(Box::new(property.type_expr.clone()))
        } else {
            property.type_expr.clone()
        }
    }

    fn register(&mut self, expr: &IrTypeExpr) {
        if self.expressions.contains(expr) {
            return;
        }
        self.expressions.push(expr.clone());
        match expr {
            IrTypeExpr::ScalarEnum { .. } => self.register(&conversion_representation(expr)),
            IrTypeExpr::Array(inner) | IrTypeExpr::Map(inner) | IrTypeExpr::Nullable(inner) => {
                self.register(inner)
            }
            IrTypeExpr::Union(members) => {
                for member in members {
                    self.register(member);
                }
            }
            IrTypeExpr::Named(name) => match self.ir.schemas.get(name).map(|schema| &schema.kind) {
                Some(IrSchemaKind::Union(union)) => {
                    for member in &union.members {
                        self.register(member);
                    }
                }
                Some(IrSchemaKind::Alias(inner)) => self.register(inner),
                Some(IrSchemaKind::TaggedUnion(union)) => {
                    for variant in &union.variants {
                        self.register(&variant.content_type);
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn function(&self, expr: &IrTypeExpr, direction: Direction) -> String {
        let index = self
            .expressions
            .iter()
            .position(|candidate| candidate == expr)
            .expect("conversion expression belongs to the Python value plan");
        format!("value_{index}_{}", direction.suffix())
    }

    pub fn call(&self, expr: &IrTypeExpr, source: CodeBlock, direction: Direction) -> CodeBlock {
        let function = self.function(expr, direction);
        sigil_quote!(Python { _union.$N(function)($C(source)) })
            .expect("Python union conversion call builds")
    }

    pub fn is_dynamic(&self, expr: &IrTypeExpr) -> bool {
        fn visit(plan: &ValuePlan<'_>, expr: &IrTypeExpr, seen: &mut BTreeSet<String>) -> bool {
            match expr {
                IrTypeExpr::Any => true,
                IrTypeExpr::Union(members) => {
                    plan.strategy == UntaggedUnionStrategy::Raw
                        || members.is_empty()
                        || members.iter().any(|member| visit(plan, member, seen))
                }
                IrTypeExpr::Nullable(inner) => visit(plan, inner, seen),
                IrTypeExpr::ScalarEnum {
                    representation,
                    representation_is_constraint,
                    ..
                } => !representation_is_constraint || visit(plan, representation, seen),
                IrTypeExpr::Named(name) if seen.insert(name.clone()) => {
                    match plan.ir.schemas.get(name).map(|schema| &schema.kind) {
                        Some(IrSchemaKind::Union(union)) => {
                            plan.strategy == UntaggedUnionStrategy::Raw
                                || union.members.is_empty()
                                || union.members.iter().any(|member| visit(plan, member, seen))
                        }
                        Some(IrSchemaKind::Alias(inner)) => visit(plan, inner, seen),
                        Some(IrSchemaKind::Enum(enumeration)) => {
                            enumeration.value_type == crate::ir::types::IrEnumValueType::Mixed
                        }
                        Some(IrSchemaKind::TaggedUnion(union)) => union
                            .variants
                            .iter()
                            .any(|variant| visit(plan, &variant.content_type, seen)),
                        _ => false,
                    }
                }
                _ => false,
            }
        }
        visit(self, expr, &mut BTreeSet::new())
    }

    pub fn coerce(&self, expr: &IrTypeExpr, target: TypeName, source: CodeBlock) -> CodeBlock {
        if self.is_dynamic(expr) {
            return source;
        }
        sigil_quote!(Python { $T(TypeName::importable("typing", "cast"))($T(target), $C(source)) })
            .expect("Python typed union projection builds")
    }

    /// Raw inline unions are explicitly dynamic. Productive cycles reference
    /// modules instead of partially initialized classes; PEP 695 and postponed
    /// annotations defer the member lookup until every model is defined.
    pub fn model_type(&self, expr: &IrTypeExpr, current: &str) -> TypeName {
        self.type_name(expr, Some(current))
    }

    pub fn api_type(&self, expr: &IrTypeExpr) -> TypeName {
        self.type_name(expr, None)
    }

    fn type_name(&self, expr: &IrTypeExpr, current: Option<&str>) -> TypeName {
        match expr {
            // Keep the runtime typing helper distinct from an authored Any model.
            IrTypeExpr::Any => TypeName::importable("typing", "Any").with_alias("_Any"),
            IrTypeExpr::Union(_) if self.strategy == UntaggedUnionStrategy::Raw => {
                TypeName::primitive("object")
            }
            IrTypeExpr::Array(inner) => type_application(
                TypeName::primitive("list"),
                vec![self.type_name(inner, current)],
            ),
            IrTypeExpr::Map(inner) => type_application(
                TypeName::primitive("dict"),
                vec![TypeName::primitive("str"), self.type_name(inner, current)],
            ),
            IrTypeExpr::Nullable(inner) => optional_type(self.type_name(inner, current)),
            IrTypeExpr::ScalarEnum {
                representation,
                representation_is_constraint,
                representation_nullable,
                ..
            } => {
                let ty = if *representation_is_constraint {
                    self.type_name(representation, current)
                } else {
                    TypeName::primitive("object")
                };
                if *representation_nullable {
                    optional_type(ty)
                } else {
                    ty
                }
            }
            IrTypeExpr::Union(members) if !members.is_empty() => TypeName::union(
                members
                    .iter()
                    .map(|member| self.type_name(member, current))
                    .collect(),
            ),
            IrTypeExpr::Named(name) => match current {
                Some(current) => self.model_symbol(name, &name.to_pascal_case(), current),
                None => super::emit_models::api_type_name(expr),
            },
            _ => match current {
                Some(current) => python_type_name_for_schema(expr, current),
                None => super::emit_models::api_type_name(expr),
            },
        }
    }

    /// A recursive dependency must load its module, not a possibly unfinished
    /// class or converter. Use the same symbol policy for annotations and bodies.
    pub fn recursive_reference(&self, name: &str, current: &str) -> bool {
        name != current && self.reaches(name, current, &mut BTreeSet::new())
    }

    fn model_symbol(&self, name: &str, symbol: &str, current: &str) -> TypeName {
        if name == current {
            TypeName::primitive(symbol)
        } else if self.recursive_reference(name, current) {
            TypeName::associated_type(
                TypeName::importable(".", &name.to_snake_case()),
                None,
                symbol,
            )
        } else {
            TypeName::importable(&format!(".{}", name.to_snake_case()), symbol)
        }
    }

    pub fn tagged_helper(&self, name: &str, current: &str, direction: Direction) -> TypeName {
        self.model_symbol(
            name,
            &format!("{}_{}", name.to_snake_case(), direction.suffix()),
            current,
        )
    }

    /// Convert a model field when its nested value needs a Python-specific
    /// module-qualified model or tagged-helper reference. The plan owns the
    /// IR, direction, alias traversal, null policy, and generated local names
    /// so field emitters do not maintain a second conversion graph.
    pub(super) fn model_field_conversion(
        &self,
        expr: &IrTypeExpr,
        value: CodeBlock,
        current: &str,
        direction: Direction,
    ) -> Option<CodeBlock> {
        self.model_field_conversion_seen(expr, value, current, direction, &mut BTreeSet::new())
    }

    fn model_field_conversion_seen(
        &self,
        expr: &IrTypeExpr,
        value: CodeBlock,
        current: &str,
        direction: Direction,
        seen_aliases: &mut BTreeSet<String>,
    ) -> Option<CodeBlock> {
        match expr {
            IrTypeExpr::Named(name) => {
                let schema = self.ir.schemas.get(name)?;
                match &schema.kind {
                    IrSchemaKind::Alias(inner) => {
                        if !seen_aliases.insert(name.clone()) {
                            return None;
                        }
                        let converted = self.model_field_conversion_seen(
                            inner,
                            value,
                            current,
                            direction,
                            seen_aliases,
                        );
                        seen_aliases.remove(name);
                        converted
                    }
                    IrSchemaKind::TaggedUnion(_) => {
                        let helper = self.tagged_helper(name, current, direction);
                        let value = match direction {
                            Direction::From => {
                                self.cast_model_value(model_dictionary_type(), value)
                            }
                            Direction::To => value,
                        };
                        Some(
                            sigil_quote!(Python { $T(helper)($C(value)) })
                                .expect("Python tagged model field conversion builds"),
                        )
                    }
                    _ if super::emit_models::is_object_schema(name, self.ir)
                        && self.recursive_reference(name, current) =>
                    {
                        match direction {
                            Direction::From => {
                                let model = self.model_type(expr, current);
                                let value = self.cast_model_value(model_dictionary_type(), value);
                                Some(
                                    sigil_quote!(Python { $T(model).from_dict($C(value)) })
                                        .expect("Python recursive model field conversion builds"),
                                )
                            }
                            Direction::To => Some(
                                sigil_quote!(Python { $C(value).to_dict() })
                                    .expect("Python recursive model field serialization builds"),
                            ),
                        }
                    }
                    _ => None,
                }
            }
            IrTypeExpr::Array(inner) => {
                let local = self.model_local_name("item");
                let item = sigil_quote!(Python { $N(local.as_str()) })
                    .expect("Python recursive list local builds");
                let item = self.model_field_conversion_seen(
                    inner,
                    item,
                    current,
                    direction,
                    seen_aliases,
                )?;
                let value = match direction {
                    Direction::From => self.cast_model_value(
                        type_application(
                            TypeName::primitive("list"),
                            vec![TypeName::primitive("object")],
                        ),
                        value,
                    ),
                    Direction::To => value,
                };
                Some(
                    sigil_quote!(Python {
                        [$C(item) for $N(local.as_str()) in $C(value)]
                    })
                    .expect("Python recursive list conversion builds"),
                )
            }
            IrTypeExpr::Map(inner) => {
                let key = self.model_local_name("key");
                let local = self.model_local_name("value");
                let item = sigil_quote!(Python { $N(local.as_str()) })
                    .expect("Python recursive map local builds");
                let item = self.model_field_conversion_seen(
                    inner,
                    item,
                    current,
                    direction,
                    seen_aliases,
                )?;
                let value = match direction {
                    Direction::From => self.cast_model_value(model_dictionary_type(), value),
                    Direction::To => value,
                };
                Some(
                    sigil_quote!(Python {
                        {$N(key.as_str()): $C(item) for $N(key.as_str()), $N(local.as_str()) in $C(value).items()}
                    })
                    .expect("Python recursive map conversion builds"),
                )
            }
            IrTypeExpr::Nullable(inner) => {
                let converted = self.model_field_conversion_seen(
                    inner,
                    value.clone(),
                    current,
                    direction,
                    seen_aliases,
                )?;
                if crate::generators::union_match::accepts_null(inner, self.ir) {
                    Some(converted)
                } else {
                    Some(
                        sigil_quote!(Python {
                            $C(converted) if $C(value) is not None else None
                        })
                        .expect("Python nullable model field conversion builds"),
                    )
                }
            }
            _ => None,
        }
    }

    fn cast_model_value(&self, ty: TypeName, value: CodeBlock) -> CodeBlock {
        sigil_quote!(Python {
            $T(TypeName::importable("typing", "cast"))($T(ty), $C(value))
        })
        .expect("Python model field cast builds")
    }

    fn model_local_name(&self, base: &str) -> String {
        let modules = self
            .ir
            .schemas
            .keys()
            .map(|name| name.to_snake_case())
            .collect::<BTreeSet<_>>();
        let mut index = 0;
        loop {
            let candidate = if index == 0 {
                format!("_{base}")
            } else {
                format!("_{base}_{index}")
            };
            if !modules.contains(&candidate) {
                return candidate;
            }
            index += 1;
        }
    }

    fn matching_ir(&self, direction: Direction) -> &IrSpec {
        match direction {
            Direction::From => self.ir,
            Direction::To => &self.public_shapes,
        }
    }

    fn reaches(&self, from: &str, target: &str, seen: &mut BTreeSet<String>) -> bool {
        if from == target {
            return true;
        }
        if !seen.insert(from.to_string()) {
            return false;
        }
        let Some(schema) = self.ir.schemas.get(from) else {
            return false;
        };
        fn names(expr: &IrTypeExpr, out: &mut Vec<String>) {
            match expr {
                IrTypeExpr::Named(name) => out.push(name.clone()),
                IrTypeExpr::ScalarEnum { representation, .. } => names(representation, out),
                IrTypeExpr::Array(inner) | IrTypeExpr::Map(inner) | IrTypeExpr::Nullable(inner) => {
                    names(inner, out)
                }
                IrTypeExpr::Union(members) => {
                    for member in members {
                        names(member, out);
                    }
                }
                _ => {}
            }
        }
        let mut refs = Vec::new();
        for expr in schema_expressions(&schema.kind) {
            names(expr, &mut refs);
        }
        refs.iter().any(|name| self.reaches(name, target, seen))
    }

    pub fn files(&self, header: &str) -> Result<Vec<FileInfo>, GeneratorError> {
        if self.expressions.is_empty() {
            return Ok(Vec::new());
        }
        let render = || -> Result<String, SigilStitchError> {
            let mut file = FileSpec::builder_with("_union.py", Python::new())
                .header(super::emit_models::future_annotations_header())
                .add_import(ImportSpec::named("..runtime", "_union_support"));
            let mut references = ShapeReferences::default();
            for expr in &self.expressions {
                for direction in [Direction::From, Direction::To] {
                    file = file.add_function(
                        FunSpec::builder(&self.function(expr, direction))
                            .add_param(ParameterSpec::of("value", TypeName::primitive("object")))
                            .returns(vec![TypeName::primitive("object")])
                            .body(self.body(expr, direction, &mut references)?)
                            .build()?,
                    );
                }
            }
            let mut done = BTreeSet::new();
            let mut next_predicate = 0;
            loop {
                if let Some((name, direction)) = references.named.pop_first() {
                    if !done.insert((name.clone(), direction)) {
                        continue;
                    }
                    let shape = union_match::plan_named_shape(&name, self.matching_ir(direction));
                    let (concrete, fallback) = union_match::selection_shapes(shape.clone());
                    for (scope, shape) in [
                        (MatchScope::Any, shape),
                        (MatchScope::Concrete, concrete),
                        (MatchScope::Fallback, fallback),
                    ] {
                        file = file.add_function(emit_predicate(
                            &shape_function(&name, scope, direction),
                            &shape,
                            direction,
                            &mut references,
                        )?);
                    }
                } else if let Some((shape, direction)) =
                    references.predicates.get(next_predicate).cloned()
                {
                    file = file.add_function(emit_predicate(
                        &predicate_name(next_predicate),
                        &shape,
                        direction,
                        &mut references,
                    )?);
                    next_predicate += 1;
                } else {
                    break;
                }
            }
            file.build()?.render(super::PYTHON_LINE_LENGTH)
        };
        let body = render().map_err(|error| {
            GeneratorError::emission(ArtifactKind::Model, "Python union conversion", error)
        })?;
        Ok(vec![
            FileInfo::model("_union.py".to_string(), format!("{header}{body}")),
            FileInfo::runtime(
                "_union_support.py".to_string(),
                format!("{header}{}", include_str!("runtime/union_support.py.txt")),
            ),
        ])
    }

    fn invoke(
        &self,
        expr: &IrTypeExpr,
        value: CodeBlock,
        direction: Direction,
    ) -> Result<CodeBlock, SigilStitchError> {
        let function = self.function(expr, direction);
        sigil_quote!(Python { $N(function)($C(value)) })
    }

    fn body(
        &self,
        expr: &IrTypeExpr,
        direction: Direction,
        references: &mut ShapeReferences,
    ) -> Result<CodeBlock, SigilStitchError> {
        let value = sigil_quote!(Python { value })?;
        match expr {
            IrTypeExpr::Union(members) => {
                self.union_body(members, false, None, direction, references)
            }
            IrTypeExpr::Named(name) => match self.ir.schemas.get(name).map(|schema| &schema.kind) {
                Some(IrSchemaKind::Union(union)) => self.union_body(
                    &union.members,
                    union.nullable,
                    Some(name),
                    direction,
                    references,
                ),
                Some(IrSchemaKind::Alias(inner)) => {
                    let conversion = self.invoke(inner, value, direction)?;
                    sigil_quote!(Python { return $C(conversion) })
                }
                Some(IrSchemaKind::TaggedUnion(union)) if self.needs_conversion(expr) => {
                    self.tagged_body(union, direction, references)
                }
                Some(IrSchemaKind::TaggedUnion(_)) => {
                    let module = name.to_snake_case();
                    let helper = format!("{module}_{}", direction.suffix());
                    let value = match direction {
                        Direction::From => sigil_quote!(Python { _union_support.as_dict(value) })?,
                        Direction::To => self.coerce(
                            expr,
                            // This module is imported locally below, not at file
                            // scope: keep its type reference lexical as well.
                            TypeName::associated_type(
                                TypeName::primitive(&module),
                                None,
                                &name.to_pascal_case(),
                            ),
                            value,
                        ),
                    };
                    sigil_quote!(Python {
                        from . import $N(module.as_str())
                        return $N(module).$N(helper)($C(value))
                    })
                }
                Some(_) if is_object_schema(name, self.ir) => match direction {
                    Direction::From => {
                        let module = name.to_snake_case();
                        let class = name.to_pascal_case();
                        // The shared helper adds dependency edges even for an
                        // acyclic schema. Load a model only after selection, when
                        // its containing module has finished initialization.
                        sigil_quote!(Python {
                            from . import $N(module.as_str())
                            return $N(module).$N(class).from_dict(_union_support.as_dict(value))
                        })
                    }
                    Direction::To => {
                        sigil_quote!(Python { return _union_support.model_to_dict(value) })
                    }
                },
                Some(IrSchemaKind::Enum(enumeration))
                    if !matches!(
                        enumeration.value_type,
                        crate::ir::types::IrEnumValueType::Mixed
                    ) =>
                {
                    match direction {
                        Direction::From => {
                            let module = name.to_snake_case();
                            let class = name.to_pascal_case();
                            sigil_quote!(Python {
                                from . import $N(module.as_str())
                                return $N(module).$N(class)(value)
                            })
                        }
                        Direction::To => {
                            sigil_quote!(Python { return _union_support.scalar(value) })
                        }
                    }
                }
                _ => sigil_quote!(Python { return value }),
            },
            IrTypeExpr::Array(inner) => {
                let item = self.invoke(inner, sigil_quote!(Python { item })?, direction)?;
                sigil_quote!(Python { return [$C(item) for item in _union_support.as_list(value)] })
            }
            IrTypeExpr::Map(inner) => {
                let item = self.invoke(inner, sigil_quote!(Python { item })?, direction)?;
                sigil_quote!(Python { return {key: $C(item) for key, item in $if(direction == Direction::To) { _union_support.mapping(value) } $else { _union_support.as_dict(value) }.items()} })
            }
            IrTypeExpr::Nullable(inner) => {
                let conversion = self.invoke(inner, value, direction)?;
                if union_match::accepts_null(inner, self.ir) {
                    sigil_quote!(Python { return $C(conversion) })
                } else {
                    sigil_quote!(Python { return None if value is None else $C(conversion) })
                }
            }
            IrTypeExpr::ScalarEnum { .. } => {
                let conversion = self.invoke(&conversion_representation(expr), value, direction)?;
                sigil_quote!(Python { return $C(conversion) })
            }
            _ => sigil_quote!(Python { return _union_support.scalar(value) }),
        }
    }

    fn union_body(
        &self,
        members: &[IrTypeExpr],
        nullable: bool,
        schema: Option<&str>,
        direction: Direction,
        references: &mut ShapeReferences,
    ) -> Result<CodeBlock, SigilStitchError> {
        if self.strategy == UntaggedUnionStrategy::Raw {
            return sigil_quote!(Python { return value });
        }
        let label = schema.unwrap_or("inline");
        let candidates = if let Some(name) = schema {
            let IrSchemaKind::Union(union) = &self.ir.schemas[name].kind else {
                unreachable!("named union helper owns a union schema")
            };
            union_match::plan_union_members(name, union, self.matching_ir(direction))
        } else {
            union_match::plan_inline_union_members(members, self.matching_ir(direction))
        };
        let strict = self.strategy == UntaggedUnionStrategy::Strict;
        let mut checks = Vec::new();
        let mut concrete = Vec::new();
        let mut fallbacks = Vec::new();
        for UnionMemberMatch {
            type_expr,
            shape,
            fallback,
            ..
        } in candidates
        {
            let check = condition(
                &shape,
                sigil_quote!(Python { value })?,
                direction,
                references,
            )?;
            let check = if strict && !fallback {
                let index = checks.len().to_string();
                checks.push(check);
                sigil_quote!(Python { matches[$L(index)] })?
            } else {
                check
            };
            let convert = self.invoke(&type_expr, sigil_quote!(Python { value })?, direction)?;
            let branch = sigil_quote!(Python {
                if $C(check) {
                    return $C(convert)
                }
            })?;
            if fallback {
                fallbacks.push(branch);
            } else {
                concrete.push(branch);
            }
        }
        let ambiguous = format!("Ambiguous {label} untagged union value");
        let unmatched = format!("No matching {label} untagged union member");
        sigil_quote!(Python {
            $if(strict && !checks.is_empty()) {
                matches = [$for(check in &checks; separator = ", ") { $C(check.clone()) }]
                if sum(matches) > 1 {
                    raise ValueError($S(ambiguous))
                }
            }
            $C_each(concrete)
            $if(nullable) {
                if value is None {
                    return None
                }
            }
            $C_each(fallbacks)
            $if(strict) { raise ValueError($S(unmatched)) } $else { return value }
        })
    }

    fn tagged_body(
        &self,
        union: &crate::ir::types::IrTaggedUnion,
        direction: Direction,
        references: &mut ShapeReferences,
    ) -> Result<CodeBlock, SigilStitchError> {
        use crate::ir::types::TaggingStyle;
        let mut branches = Vec::new();
        for variant in &union.variants {
            let tag = &union.discriminator_field;
            let label = &variant.discriminator_value;
            let source = match (&union.tagging, direction) {
                (TaggingStyle::Adjacent { content_field }, Direction::From) => {
                    sigil_quote!(Python { data[$S(content_field.as_str())] })?
                }
                (TaggingStyle::External, Direction::From) => {
                    sigil_quote!(Python { data[$S(label.as_str())] })?
                }
                _ => sigil_quote!(Python { value })?,
            };
            let converted = self.invoke(&variant.content_type, source, direction)?;
            let guard = match direction {
                Direction::From => match &union.tagging {
                    TaggingStyle::External => sigil_quote!(Python { $S(label.as_str()) in data })?,
                    _ => sigil_quote!(Python { data.get($S(tag.as_str())) == $S(label.as_str()) })?,
                },
                Direction::To => {
                    let shapes = union_match::plan_inline_union_members(
                        std::slice::from_ref(&variant.content_type),
                        self.matching_ir(direction),
                    );
                    let shape =
                        JsonShape::AnyOf(shapes.into_iter().map(|member| member.shape).collect());
                    condition(
                        &shape,
                        sigil_quote!(Python { value })?,
                        direction,
                        references,
                    )?
                }
            };
            let result = match (&union.tagging, direction) {
                (_, Direction::From) => sigil_quote!(Python { return $C(converted) })?,
                (TaggingStyle::Internal, Direction::To) => sigil_quote!(Python {
                    result = dict(_union_support.as_dict($C(converted)))
                    result[$S(tag.as_str())] = $S(label.as_str())
                    return result
                })?,
                (TaggingStyle::Adjacent { content_field }, Direction::To) => sigil_quote!(Python {
                    return {$S(tag.as_str()): $S(label.as_str()), $S(content_field.as_str()): $C(converted)}
                })?,
                (TaggingStyle::External, Direction::To) => {
                    sigil_quote!(Python { return {$S(label.as_str()): $C(converted)} })?
                }
            };
            branches.push(sigil_quote!(Python {
                if $C(guard) { $C(result) }
            })?);
        }
        sigil_quote!(Python {
            $if(matches!(direction, Direction::From)) { data = _union_support.as_dict(value) }
            $C_each(branches)
            raise ValueError("No matching tagged union variant")
        })
    }
}

fn schema_expressions(kind: &IrSchemaKind) -> Vec<&IrTypeExpr> {
    match kind {
        IrSchemaKind::Object(object) => object
            .properties
            .values()
            .map(|property| &property.type_expr)
            .chain(object.additional_properties.iter())
            .collect(),
        IrSchemaKind::Alias(inner) => vec![inner],
        IrSchemaKind::Union(union) => union.members.iter().collect(),
        IrSchemaKind::Intersection(intersection) => intersection.members.iter().collect(),
        IrSchemaKind::TaggedUnion(union) => union
            .variants
            .iter()
            .map(|variant| &variant.content_type)
            .collect(),
        IrSchemaKind::Enum(_) => Vec::new(),
    }
}

fn model_dictionary_type() -> TypeName {
    type_application(
        TypeName::primitive("dict"),
        vec![TypeName::primitive("str"), TypeName::primitive("object")],
    )
}

fn conversion_representation(expr: &IrTypeExpr) -> IrTypeExpr {
    let IrTypeExpr::ScalarEnum {
        representation,
        representation_nullable,
        ..
    } = expr
    else {
        return expr.clone();
    };
    // Restore only this lowering boundary. Inner finite facts still determine
    // authored null matches and nested strict errors after delegation.
    if *representation_nullable {
        IrTypeExpr::Nullable(representation.clone())
    } else {
        representation.as_ref().clone()
    }
}

fn shape_function(name: &str, scope: MatchScope, direction: Direction) -> String {
    let suffix = match scope {
        MatchScope::Any => "any",
        MatchScope::Concrete => "concrete",
        MatchScope::Fallback => "fallback",
    };
    format!(
        "match_{}_{suffix}_{}",
        name.to_snake_case(),
        direction.suffix()
    )
}

fn condition(
    shape: &JsonShape,
    value: CodeBlock,
    direction: Direction,
    references: &mut ShapeReferences,
) -> Result<CodeBlock, SigilStitchError> {
    match shape {
        JsonShape::Any => sigil_quote!(Python { True }),
        JsonShape::Never => sigil_quote!(Python { False }),
        JsonShape::Null => sigil_quote!(Python { $C(value) is None }),
        JsonShape::Boolean => sigil_quote!(Python { isinstance($C(value), bool) }),
        JsonShape::String => sigil_quote!(Python { isinstance($C(value), str) }),
        JsonShape::Binary => sigil_quote!(Python { isinstance($C(value), bytes) }),
        JsonShape::Number { integer } => {
            sigil_quote!(Python { _union_support.number($C(value), $if(*integer) { True } $else { False }) })
        }
        JsonShape::Literal(literal) => {
            let literal = python_literal(literal)?;
            sigil_quote!(Python { _union_support.equal($C(value), $C(literal)) })
        }
        JsonShape::Reference { schema, scope, .. } => {
            references.named.insert((schema.clone(), direction));
            let function = shape_function(schema, *scope, direction);
            sigil_quote!(Python { $N(function)($C(value)) })
        }
        JsonShape::AnyOf(shapes) | JsonShape::AllOf(shapes) if shapes.len() == 1 => {
            condition(&shapes[0], value, direction, references)
        }
        JsonShape::Nullable(_)
        | JsonShape::Array(_)
        | JsonShape::Map(_)
        | JsonShape::MapWithConcreteValue { .. }
        | JsonShape::Object(_)
        | JsonShape::ObjectWithAdditional { .. }
        | JsonShape::AnyOf(_)
        | JsonShape::AllOf(_) => {
            let predicate = references.predicate(shape, direction);
            sigil_quote!(Python { $N(predicate)($C(value)) })
        }
    }
}

fn emit_predicate(
    name: &str,
    shape: &JsonShape,
    direction: Direction,
    references: &mut ShapeReferences,
) -> Result<FunSpec, SigilStitchError> {
    // Lower composite predicates as short-circuiting checks. The same private
    // predicate can be called by any union arm or container comprehension,
    // rather than expanding the complete shape repeatedly in the caller.
    let body = match shape {
        JsonShape::Nullable(inner) => {
            let inner = condition(
                inner,
                sigil_quote!(Python { value })?,
                direction,
                references,
            )?;
            sigil_quote!(Python {
                if value is None { return True }
                return $C(inner)
            })?
        }
        JsonShape::Array(inner) => {
            let item = condition(
                inner,
                sigil_quote!(Python { _item })?,
                direction,
                references,
            )?;
            sigil_quote!(Python {
                if not _union_support.is_list(value) { return False }
                return all($C(item) for _item in _union_support.as_list(value))
            })?
        }
        JsonShape::Map(inner) => {
            let item = condition(
                inner,
                sigil_quote!(Python { _item })?,
                direction,
                references,
            )?;
            match direction {
                Direction::From => sigil_quote!(Python {
                    if not _union_support.is_dict(value) { return False }
                    return all($C(item) for _item in _union_support.as_dict(value).values())
                }),
                Direction::To => sigil_quote!(Python {
                    if not _union_support.is_mapping(value) { return False }
                    return all($C(item) for _item in _union_support.mapping(value).values())
                }),
            }?
        }
        JsonShape::MapWithConcreteValue { values, concrete } => {
            let matches_map = condition(
                &JsonShape::Map(values.clone()),
                sigil_quote!(Python { value })?,
                direction,
                references,
            )?;
            let item = condition(
                concrete,
                sigil_quote!(Python { _item })?,
                direction,
                references,
            )?;
            let lookup = match direction {
                Direction::From => sigil_quote!(Python { _union_support.as_dict(value).values() }),
                Direction::To => sigil_quote!(Python { _union_support.mapping(value).values() }),
            }?;
            sigil_quote!(Python {
                if not $C(matches_map) { return False }
                return any($C(item) for _item in $C(lookup))
            })?
        }
        JsonShape::Object(properties) | JsonShape::ObjectWithAdditional { properties, .. } => {
            let checks = properties
                .iter()
                .map(|property| {
                    let wire = property.wire_name.as_str();
                    let field = python_field_name(wire);
                    let access = sigil_quote!(Python { _union_support.get(value, $S(wire), $S(field.as_str())) })?;
                    let inner = condition(&property.shape, access, direction, references)?;
                    if property.required {
                        sigil_quote!(Python {
                            if not _union_support.has(value, $S(wire), $S(field), True) {
                                return False
                            }
                            if not $C(inner) { return False }
                        })
                    } else {
                        sigil_quote!(Python {
                            if _union_support.has(value, $S(wire), $S(field), False) {
                                if not $C(inner) { return False }
                            }
                        })
                    }
                })
                .collect::<Result<Vec<_>, SigilStitchError>>()?;
            let result = if let JsonShape::ObjectWithAdditional { values, .. } = shape {
                let item = condition(
                    values,
                    sigil_quote!(Python { _item })?,
                    direction,
                    references,
                )?;
                let keys = properties
                    .iter()
                    .map(|property| property.wire_name.as_str());
                sigil_quote!(Python {
                    extra_values = _union_support.extras(value, [$for(key in keys; separator = ", ") { $S(key) }])
                    return all($C(item) for _item in extra_values)
                })?
            } else {
                sigil_quote!(Python { return True })?
            };
            sigil_quote!(Python {
                if not _union_support.is_object(value) { return False }
                $C_each(checks)
                $C(result)
            })?
        }
        JsonShape::AnyOf(shapes) | JsonShape::AllOf(shapes) => {
            let any = matches!(shape, JsonShape::AnyOf(_));
            let checks = shapes
                .iter()
                .map(|shape| {
                    let inner = condition(
                        shape,
                        sigil_quote!(Python { value })?,
                        direction,
                        references,
                    )?;
                    if any {
                        sigil_quote!(Python { if $C(inner) { return True } })
                    } else {
                        sigil_quote!(Python { if not $C(inner) { return False } })
                    }
                })
                .collect::<Result<Vec<_>, SigilStitchError>>()?;
            sigil_quote!(Python {
                $C_each(checks)
                return $if(any) { False } $else { True }
            })?
        }
        _ => {
            let test = condition(
                shape,
                sigil_quote!(Python { value })?,
                direction,
                references,
            )?;
            sigil_quote!(Python { return $C(test) })?
        }
    };
    FunSpec::builder(name)
        .add_param(ParameterSpec::of("value", TypeName::primitive("object")))
        .returns(vec![TypeName::primitive("bool")])
        .body(body)
        .build()
}

fn python_literal(value: &serde_json::Value) -> Result<CodeBlock, SigilStitchError> {
    match value {
        serde_json::Value::Null => sigil_quote!(Python { None }),
        serde_json::Value::Bool(value) => {
            sigil_quote!(Python { $if(*value) { True } $else { False } })
        }
        serde_json::Value::String(value) => sigil_quote!(Python { $S(value.as_str()) }),
        serde_json::Value::Number(value) => sigil_quote!(Python { $L(value.to_string()) }),
        serde_json::Value::Array(values) => {
            let values = values
                .iter()
                .map(python_literal)
                .collect::<Result<Vec<_>, _>>()?;
            sigil_quote!(Python { [$for(value in &values; separator = ", ") { $C(value.clone()) }] })
        }
        serde_json::Value::Object(values) => {
            let values = values
                .iter()
                .map(|(key, value)| Ok((key, python_literal(value)?)))
                .collect::<Result<Vec<_>, SigilStitchError>>()?;
            sigil_quote!(Python { {$for((key, value) in &values; separator = ", ") { $S(key.as_str()): $C(value.clone()) }} })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sigil_stitch::assert_rendered;

    #[test]
    fn nullable_conversion_restoration_retains_inner_finite_facts() {
        let inner = IrTypeExpr::ScalarEnum {
            representation: Box::new(IrTypeExpr::Primitive(
                crate::ir::types::IrPrimitive::Integer,
            )),
            representation_is_constraint: true,
            representation_nullable: true,
            values: vec![1.into(), serde_json::Value::Null],
        };
        let outer = IrTypeExpr::ScalarEnum {
            representation: Box::new(inner.clone()),
            representation_is_constraint: true,
            representation_nullable: true,
            values: vec![1.into(), serde_json::Value::Null],
        };
        assert_eq!(
            conversion_representation(&outer),
            IrTypeExpr::Nullable(Box::new(inner))
        );
    }

    #[test]
    fn finite_json_literals_keep_python_punctuation_and_boolean_kinds() {
        let literal =
            python_literal(&serde_json::json!({"quoted'key": [true, null, 2, "line\nvalue"]}))
                .unwrap();
        assert_rendered!(
            Python::new(),
            literal,
            "{\"quoted'key\": [True, None, 2, \"line\\nvalue\"]}\n"
        );
    }

    #[test]
    fn union_selection_counts_before_delegating() {
        let parsed = crate::parser::parse_content_yaml("openapi: 3.1.0\ninfo: {title: test, version: '1'}\npaths: {}\ncomponents:\n  schemas:\n    Choice:\n      anyOf:\n        - {type: boolean}\n        - {type: integer}\n").unwrap();
        let ir = crate::ir::lower::lower(parsed).unwrap();
        let plan = ValuePlan::new(&ir, UntaggedUnionStrategy::Strict);
        let body = plan
            .body(
                &IrTypeExpr::Named("Choice".into()),
                Direction::From,
                &mut ShapeReferences::default(),
            )
            .unwrap();
        assert_rendered!(
            Python::new(),
            width = 120,
            body,
            "matches = [isinstance(value, bool), _union_support.number(value, True)]\nif sum(matches) > 1:\n    raise ValueError(\"Ambiguous Choice untagged union value\")\n\nif matches[0]:\n    return value_1_from_dict(value)\n\nif matches[1]:\n    return value_2_from_dict(value)\n\nraise ValueError(\"No matching Choice untagged union member\")\n"
        );
    }

    #[test]
    fn recursive_model_types_keep_module_imports_semantic() {
        let parsed = crate::parser::parse_content_yaml(include_str!(
            "../../../tests/fixtures/valid/python-untagged-union.yaml"
        ))
        .unwrap();
        let ir = crate::ir::lower::lower(parsed).unwrap();
        let plan = ValuePlan::new(&ir, UntaggedUnionStrategy::FirstMatch);
        let ty = plan.model_type(&IrTypeExpr::Named("Recursive".into()), "Branch");
        let code = sigil_quote!(Python { children: list[$T(ty)] }).unwrap();
        assert_rendered!(
            Python::new(),
            code,
            "from . import recursive\n\n\nchildren: list[recursive.Recursive]\n"
        );
    }

    #[test]
    fn model_field_conversion_expands_aliases_and_avoids_module_name_locals() {
        let parsed = crate::parser::parse_content_yaml(include_str!(
            "../../../tests/fixtures/valid/python-tagged-alias-scope.yaml"
        ))
        .unwrap();
        let ir = crate::ir::lower::lower(parsed).unwrap();
        let plan = ValuePlan::new(&ir, UntaggedUnionStrategy::FirstMatch);

        let recursive_list = plan
            .model_field_conversion(
                &IrTypeExpr::Array(Box::new(IrTypeExpr::Named("Item".into()))),
                sigil_quote!(Python { data }).unwrap(),
                "Branch",
                Direction::From,
            )
            .unwrap();
        assert_rendered!(
            Python::new(),
            width = 150,
            recursive_list,
            "from typing import cast\n\nfrom . import item\n\n\n[item.item_from_dict(cast(dict[str, object], _item)) for _item in cast(list[object], data)]\n"
        );

        let alias = plan
            .model_field_conversion(
                &IrTypeExpr::Named("Alias".into()),
                sigil_quote!(Python { data }).unwrap(),
                "Envelope",
                Direction::From,
            )
            .unwrap();
        assert_rendered!(
            Python::new(),
            alias,
            "from typing import cast\n\nfrom .item import item_from_dict\n\n\nitem_from_dict(cast(dict[str, object], data))\n"
        );
    }

    #[test]
    fn recursive_tagged_types_do_not_require_planned_union_codecs() {
        let source = "openapi: 3.1.0\ninfo: {title: Recursive, version: '1'}\npaths: {}\ncomponents:\n  schemas:\n    Variant:\n      oneOf:\n        - {$ref: '#/components/schemas/Branch'}\n        - {$ref: '#/components/schemas/Leaf'}\n      discriminator: {propertyName: kind}\n    Branch:\n      type: object\n      required: [kind, children]\n      properties:\n        kind: {type: string, enum: [branch]}\n        children: {type: array, items: {$ref: '#/components/schemas/Variant'}}\n    Leaf:\n      type: object\n      required: [kind]\n      properties:\n        kind: {type: string, enum: [leaf]}\n";
        let ir =
            crate::ir::lower::lower(crate::parser::parse_content_yaml(source).unwrap()).unwrap();
        let plan = ValuePlan::new(&ir, UntaggedUnionStrategy::FirstMatch);
        assert!(
            plan.expressions.is_empty(),
            "pure tagged graph needs no union planner"
        );
        let ty = plan.model_type(&IrTypeExpr::Named("Variant".into()), "Branch");
        let code = sigil_quote!(Python { children: list[$T(ty)] }).unwrap();
        assert_rendered!(
            Python::new(),
            code,
            "from . import variant\n\n\nchildren: list[variant.Variant]\n"
        );
    }

    #[test]
    fn selected_model_imports_stay_inside_the_converter() {
        let parsed = crate::parser::parse_content_yaml(include_str!(
            "../../../tests/fixtures/valid/python-untagged-union.yaml"
        ))
        .unwrap();
        let ir = crate::ir::lower::lower(parsed).unwrap();
        let plan = ValuePlan::new(&ir, UntaggedUnionStrategy::FirstMatch);
        let body = plan
            .body(
                &IrTypeExpr::Named("DependentEnvelope".into()),
                Direction::From,
                &mut ShapeReferences::default(),
            )
            .unwrap();
        assert_rendered!(
            Python::new(),
            body,
            "from . import dependent_envelope\n\nreturn dependent_envelope.DependentEnvelope.from_dict(\n    _union_support.as_dict(value)\n)\n"
        );
    }

    #[test]
    fn tagged_selection_projects_public_payloads_without_mutating_wire_ir() {
        let parsed = crate::parser::parse_content_yaml(include_str!(
            "../../../tests/fixtures/valid/python-untagged-union.yaml"
        ))
        .unwrap();
        let ir = crate::ir::lower::lower(parsed).unwrap();
        let plan = ValuePlan::new(&ir, UntaggedUnionStrategy::Strict);
        let JsonShape::AnyOf(wire) =
            union_match::plan_named_shape("AdjacentMember", plan.matching_ir(Direction::From))
        else {
            panic!("adjacent wire variants retain their wrappers");
        };
        assert!(
            wire.iter()
                .all(|variant| matches!(variant, JsonShape::Object(fields)
            if fields.iter().any(|field| field.wire_name == "kind")
                && fields.iter().any(|field| field.wire_name == "payload")))
        );
        assert_eq!(
            union_match::plan_named_shape("AdjacentMember", plan.matching_ir(Direction::To)),
            JsonShape::AnyOf(vec![
                union_match::plan_named_shape("TaggedRecord", &ir),
                union_match::plan_named_shape("NumericTwo", &ir),
            ]),
        );
        assert!(matches!(
            ir.schemas["AdjacentMember"].kind,
            IrSchemaKind::TaggedUnion(_)
        ));
    }
}
