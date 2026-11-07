//! One value-planning seam for model declarations, converters and HTTP payloads.
//!
//! Symbol requirements, public/wire representations and runtime effects are
//! deliberately separate facts. In particular an identity representation does
//! not erase a strict union check, and raw changes a union's public projection.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashSet};

use heck::{ToLowerCamelCase, ToPascalCase};
use sigil_stitch::error::SigilStitchError;
use sigil_stitch::prelude::{CodeBlock, TypeName, sigil_quote};
use sigil_stitch::spec::{
    file_spec::FileSpec, fun_spec::FunSpec, modifiers::Visibility, parameter_spec::ParameterSpec,
};

use crate::generators::type_application;
use crate::generators::union_match::{
    JsonShape, UnionMemberMatch, plan_named_shape, plan_union_members, referenced_shapes,
    selection_shapes,
};
use crate::ir::types::{IrPrimitive, IrSchemaKind, IrSpec, IrTypeExpr, IrUnion};

use super::config::UntaggedUnionStrategy;
use super::layout;
use super::matching::MatchPlan;
use super::sigil_emit::{EmitFlags, build_type_alias, fn_base_name};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Direction {
    FromWire,
    ToWire,
}

impl Direction {
    pub(super) fn source(self) -> &'static str {
        match self {
            Self::FromWire => "json",
            Self::ToWire => "value",
        }
    }
    pub(super) fn suffix(self) -> &'static str {
        match self {
            Self::FromWire => "FromJSON",
            Self::ToWire => "ToJSON",
        }
    }
    pub(super) fn source_key(self, property: &crate::ir::types::IrProperty) -> String {
        match self {
            Self::FromWire => property.name.clone(),
            Self::ToWire => property.name.to_lower_camel_case(),
        }
    }
    pub(super) fn target_key(self, property: &crate::ir::types::IrProperty) -> String {
        match self {
            Self::FromWire => property.name.to_lower_camel_case(),
            Self::ToWire => property.name.clone(),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Scope<'a> {
    current: Option<&'a str>,
    models: &'static str,
    readonly_arrays: bool,
    current_helper: Option<usize>,
    nested_arrays: bool,
    readonly_input: bool,
}

impl<'a> Scope<'a> {
    pub(super) fn model(name: &'a str) -> Self {
        Self {
            current: Some(name),
            models: "./",
            readonly_arrays: true,
            current_helper: None,
            nested_arrays: false,
            readonly_input: false,
        }
    }
    pub(super) fn api() -> Self {
        Self {
            current: None,
            models: "../models/",
            readonly_arrays: false,
            current_helper: None,
            nested_arrays: false,
            readonly_input: false,
        }
    }

    fn helpers(readonly_arrays: bool, nested_arrays: bool, index: usize) -> Self {
        Self {
            current: None,
            models: "../models/",
            readonly_arrays,
            current_helper: Some(index),
            nested_arrays,
            readonly_input: false,
        }
    }
}

#[derive(Clone, Copy)]
enum Representation {
    Public,
    Wire,
}

#[derive(PartialEq)]
struct ValueHelper {
    expr: IrTypeExpr,
    readonly_arrays: bool,
    nested_arrays: bool,
}

pub(super) struct TypeScriptPlan<'a> {
    ir: &'a IrSpec,
    flags: EmitFlags,
    converters: HashSet<String>,
    wire_symbols: HashSet<String>,
    unknown: HashSet<String>,
    unions: BTreeMap<String, Vec<UnionMemberMatch>>,
    recursive_shapes: BTreeMap<String, JsonShape>,
    local_type_names: HashSet<String>,
    conversions: Vec<ValueHelper>,
    matching: MatchPlan,
}

pub(super) struct ValuePlan<'a, 'ir> {
    plan: &'a TypeScriptPlan<'ir>,
    expr: &'a IrTypeExpr,
    scope: Scope<'a>,
}

pub(super) fn representation_is_unknown(expr: &IrTypeExpr, aliases: &HashSet<String>) -> bool {
    match expr.representation() {
        IrTypeExpr::Any => true,
        IrTypeExpr::Named(name) => aliases.contains(name),
        IrTypeExpr::Nullable(inner) => representation_is_unknown(inner, aliases),
        IrTypeExpr::Union(members) => members
            .iter()
            .any(|member| representation_is_unknown(member, aliases)),
        _ => false,
    }
}

/// Normalize only the rendered representation. Candidate identity and runtime
/// effects remain in the value plan, even when integer | number renders number.
pub(super) fn union_type(members: Vec<TypeName>) -> TypeName {
    fn flatten(members: Vec<TypeName>, out: &mut Vec<TypeName>) {
        for member in members {
            if let TypeName::Union(members) = member {
                flatten(members, out);
            } else if !out.contains(&member) {
                out.push(member);
            }
        }
    }
    let mut out = Vec::new();
    flatten(members, &mut out);
    if out.contains(&TypeName::primitive("unknown")) {
        return TypeName::primitive("unknown");
    }
    if out.len() == 1 {
        out.pop().expect("one union type")
    } else {
        TypeName::union(out)
    }
}

impl<'a> TypeScriptPlan<'a> {
    pub(super) fn new(ir: &'a IrSpec, flags: EmitFlags) -> Self {
        let mut result = Self {
            ir,
            flags,
            converters: HashSet::new(),
            wire_symbols: HashSet::new(),
            unknown: HashSet::new(),
            unions: BTreeMap::new(),
            recursive_shapes: BTreeMap::new(),
            local_type_names: ir
                .schemas
                .keys()
                .map(|name| name.to_pascal_case())
                .collect(),
            conversions: Vec::new(),
            matching: MatchPlan::default(),
        };
        // Monotone worklists retain named-reference edges, including recursive
        // payloads. No expression is expanded into an unbounded type tree.
        loop {
            let previous = (
                result.converters.len(),
                result.wire_symbols.len(),
                result.unknown.len(),
            );
            for (name, schema) in &ir.schemas {
                let unknown = match &schema.kind {
                    IrSchemaKind::Alias(expr) => result.is_unknown(expr),
                    IrSchemaKind::Union(union) => {
                        union.members.iter().any(|expr| result.is_unknown(expr))
                    }
                    IrSchemaKind::Intersection(all) => {
                        !all.members.is_empty()
                            && all.members.iter().all(|expr| result.is_unknown(expr))
                    }
                    _ => false,
                };
                if unknown {
                    result.unknown.insert(name.clone());
                }
                if !flags.property_naming_camel_case {
                    continue;
                }
                let has_symbols = match &schema.kind {
                    IrSchemaKind::Object(_) | IrSchemaKind::Union(_) => true,
                    IrSchemaKind::TaggedUnion(union) => !union.variants.is_empty(),
                    IrSchemaKind::Alias(expr) => {
                        result.references_wire(expr) || result.has_effect(expr)
                    }
                    IrSchemaKind::Intersection(all) => all
                        .members
                        .iter()
                        .any(|expr| result.references_wire(expr) || result.has_effect(expr)),
                    IrSchemaKind::Enum(_) => false,
                };
                if has_symbols {
                    result.wire_symbols.insert(name.clone());
                }
                let has_converter = has_symbols
                    || match &schema.kind {
                        IrSchemaKind::Alias(expr) => result.has_effect(expr),
                        IrSchemaKind::Intersection(all) => {
                            all.members.iter().any(|expr| result.has_effect(expr))
                        }
                        _ => false,
                    };
                if has_converter {
                    result.converters.insert(name.clone());
                }
            }
            if previous
                == (
                    result.converters.len(),
                    result.wire_symbols.len(),
                    result.unknown.len(),
                )
            {
                break;
            }
        }
        if flags.property_naming_camel_case
            && flags.untagged_union_strategy != UntaggedUnionStrategy::Raw
        {
            let shapes: BTreeMap<_, _> = ir
                .schemas
                .keys()
                .map(|name| (name.clone(), plan_named_shape(name, ir)))
                .collect();
            let mut referenced = BTreeSet::new();
            for shape in shapes.values() {
                referenced_shapes(shape, &mut referenced);
            }
            result.recursive_shapes = shapes
                .into_iter()
                .filter(|(name, _)| referenced.contains(name))
                .collect();
            for (name, schema) in &ir.schemas {
                if let IrSchemaKind::Union(union) = &schema.kind {
                    result
                        .unions
                        .insert(name.clone(), plan_union_members(name, union, ir));
                }
            }
        }
        // Plan compound expressions once, before any emitter runs. Runtime
        // helpers retain model/API array projections and never become barrel
        // exports. They only factor existing conversions into short functions.
        let mut expressions = Vec::new();
        for schema in ir.schemas.values() {
            match &schema.kind {
                IrSchemaKind::Object(object) => {
                    expressions.extend(
                        object
                            .properties
                            .values()
                            .map(|property| (&property.type_expr, true)),
                    );
                    expressions
                        .extend(object.additional_properties.iter().map(|expr| (expr, true)));
                }
                IrSchemaKind::Alias(expr) => expressions.push((expr, true)),
                IrSchemaKind::Union(union) => {
                    expressions.extend(union.members.iter().map(|expr| (expr, true)))
                }
                IrSchemaKind::Intersection(all) => {
                    expressions.extend(all.members.iter().map(|expr| (expr, true)))
                }
                IrSchemaKind::TaggedUnion(union) => {
                    for variant in &union.variants {
                        expressions.push((&variant.content_type, true));
                        expressions.extend(
                            variant
                                .additional_properties
                                .iter()
                                .map(|expr| (expr, true)),
                        );
                    }
                }
                IrSchemaKind::Enum(_) => {}
            }
        }
        for operation in &ir.operations {
            if let Some(body) = &operation.request_body {
                expressions.extend(body.content.values().map(|expr| (expr, false)));
            }
            for response in &operation.responses {
                expressions.extend(
                    response
                        .content
                        .values()
                        .chain(response.item_content.values())
                        .map(|expr| (expr, false)),
                );
            }
        }
        for (expr, readonly) in expressions {
            result.register_conversion(expr, readonly);
        }
        for schema in ir.schemas.values() {
            if let IrSchemaKind::Intersection(all) = &schema.kind
                && let Some(object) =
                    super::sigil_emit::open_intersection_object(all, ir, &result.unknown)
            {
                for property in object.properties.values() {
                    result.register_conversion(&property.type_expr, true);
                }
                if let Some(additional) = &object.additional_properties {
                    result.register_conversion(additional, true);
                }
            }
        }
        for operation in &ir.operations {
            if let Some(body) = &operation.request_body {
                for media in body.content.keys() {
                    if crate::generators::multipart::media_type_base(media) != "multipart/form-data"
                    {
                        continue;
                    }
                    if let Some(parts) =
                        crate::generators::multipart::multipart_parts_for_request_body(
                            body, media, ir,
                        )
                    {
                        for part in parts {
                            if part.value_encoding
                                == crate::generators::multipart::MultipartValueEncoding::Json
                            {
                                result.register_conversion(&part.type_expr, false);
                            }
                        }
                    }
                }
            }
        }
        for members in result.unions.values() {
            for member in members {
                result.matching.register(&member.shape);
            }
        }
        let mut required_matchers = result.matching.references();
        let mut pending_matchers = required_matchers.clone();
        while let Some(name) = pending_matchers.pop_first() {
            let shape = result
                .recursive_shapes
                .get(&name)
                .expect("productive reference has a named shape plan");
            let (concrete, fallback) = selection_shapes(shape.clone());
            result.matching.register(&concrete);
            result.matching.register(&fallback);
            let mut dependencies = BTreeSet::new();
            referenced_shapes(shape, &mut dependencies);
            for dependency in dependencies {
                if required_matchers.insert(dependency.clone()) {
                    pending_matchers.insert(dependency);
                }
            }
        }
        result
            .recursive_shapes
            .retain(|name, _| required_matchers.contains(name));
        result
    }

    pub(super) fn ir(&self) -> &'a IrSpec {
        self.ir
    }
    pub(super) fn flags(&self) -> EmitFlags {
        self.flags
    }
    pub(super) fn has_converter(&self, name: &str) -> bool {
        self.converters.contains(name)
    }
    pub(super) fn has_wire_type(&self, name: &str) -> bool {
        self.wire_symbols.contains(name)
    }
    pub(super) fn unknown_aliases(&self) -> &HashSet<String> {
        &self.unknown
    }

    pub(super) fn utility_type(&self, name: &str) -> TypeName {
        if self.local_type_names.contains(name) {
            TypeName::qualified("globalThis", name)
        } else {
            TypeName::primitive(name)
        }
    }

    pub(super) fn matching_file(
        &self,
        ts: &sigil_stitch::lang::typescript::TypeScript,
    ) -> Result<Option<FileSpec>, SigilStitchError> {
        self.matching
            .file(&self.recursive_shapes, &self.utility_type("Record"), ts)
    }

    fn register_conversion(&mut self, expr: &IrTypeExpr, readonly: bool) {
        self.register_conversion_at(expr, readonly, false);
    }

    /// Selection metadata is transparent to conversion, except that extracting
    /// property nullability removed a real wrapper the delegate still needs.
    /// Restore that wrapper without changing legacy public type projections.
    /// Keep its inner finite facts: they decide whether null belongs to the
    /// wrapper or must reach an already-nullable delegate's strict policy.
    fn conversion_representation(expr: &IrTypeExpr) -> Cow<'_, IrTypeExpr> {
        match expr {
            IrTypeExpr::ScalarEnum {
                representation,
                representation_nullable,
                ..
            } => {
                if *representation_nullable {
                    Cow::Owned(IrTypeExpr::Nullable(representation.clone()))
                } else {
                    Self::conversion_representation(representation)
                }
            }
            _ => Cow::Borrowed(expr),
        }
    }

    fn register_conversion_at(&mut self, expr: &IrTypeExpr, readonly: bool, nested: bool) {
        if !self.has_effect(expr) {
            return;
        }
        if matches!(expr, IrTypeExpr::ScalarEnum { .. }) {
            self.register_conversion_at(&Self::conversion_representation(expr), readonly, nested);
            return;
        }
        if !matches!(
            expr,
            IrTypeExpr::Array(_)
                | IrTypeExpr::Map(_)
                | IrTypeExpr::Nullable(_)
                | IrTypeExpr::Union(_)
        ) {
            return;
        }
        let helper = ValueHelper {
            expr: expr.clone(),
            readonly_arrays: readonly,
            nested_arrays: nested,
        };
        if self.conversions.contains(&helper) {
            return;
        }
        self.conversions.push(helper);
        match expr {
            IrTypeExpr::Array(inner) | IrTypeExpr::Map(inner) | IrTypeExpr::Nullable(inner) => {
                let child_nested = match expr {
                    IrTypeExpr::Array(_) => true,
                    IrTypeExpr::Map(_) => false,
                    _ => nested,
                };
                self.register_conversion_at(inner, readonly, child_nested);
            }
            IrTypeExpr::Union(members) => {
                for member in
                    crate::generators::union_match::plan_inline_union_members(members, self.ir)
                {
                    self.matching.register(&member.shape);
                }
                for member in members {
                    self.register_conversion_at(member, readonly, nested);
                }
            }
            _ => {}
        }
    }

    fn conversion_index(&self, expr: &IrTypeExpr, scope: Scope<'_>) -> usize {
        self.conversions
            .iter()
            .position(|helper| {
                &helper.expr == expr
                    && helper.readonly_arrays == scope.readonly_arrays
                    && helper.nested_arrays == scope.nested_arrays
            })
            .expect("compound conversion was registered by the value plan")
    }

    fn helper_type(&self, expr: &IrTypeExpr, scope: Scope<'_>, wire: bool) -> TypeName {
        let index = self.conversion_index(expr, scope);
        let name = format!("Value${}{}", index, if wire { "Wire" } else { "" });
        if scope.current_helper == Some(index) {
            TypeName::primitive(&name)
        } else {
            TypeName::importable_type(&Self::helper_module(index, scope), &name)
        }
    }

    fn helper_module(index: usize, scope: Scope<'_>) -> String {
        let prefix = if scope.current_helper.is_some() {
            "./"
        } else {
            "../runtime/"
        };
        format!("{prefix}valueConversion${index}")
    }

    fn converter_symbol(
        &self,
        expr: &IrTypeExpr,
        scope: Scope<'_>,
        direction: Direction,
    ) -> TypeName {
        let expr = Self::conversion_representation(expr);
        match expr.as_ref() {
            IrTypeExpr::Named(name) => {
                let pascal = name.to_pascal_case();
                let symbol = format!("{}{}", fn_base_name(&pascal), direction.suffix());
                if scope.current == Some(name) {
                    TypeName::primitive(&symbol)
                } else {
                    TypeName::importable(&format!("{}{pascal}", scope.models), &symbol)
                }
            }
            _ => {
                let index = self.conversion_index(&expr, scope);
                let name = format!("convert${}{}", index, direction.suffix());
                if scope.current_helper == Some(index) {
                    TypeName::primitive(&name)
                } else {
                    TypeName::importable(&Self::helper_module(index, scope), &name)
                }
            }
        }
    }

    pub(super) fn conversion_files(
        &self,
        ts: &sigil_stitch::lang::typescript::TypeScript,
    ) -> Result<Vec<FileSpec>, SigilStitchError> {
        let mut files = Vec::with_capacity(self.conversions.len());
        for (index, helper) in self.conversions.iter().enumerate() {
            // Separate dependency graphs: importing one model must not force
            // unrelated model types or converters into its consumer's program.
            let mut file =
                FileSpec::builder_with(&format!("valueConversion${index}.ts"), ts.clone());
            let expr = &helper.expr;
            let scope = Scope::helpers(helper.readonly_arrays, helper.nested_arrays, index);
            for (wire, representation) in [
                (true, Representation::Wire),
                (false, Representation::Public),
            ] {
                let name = format!("Value${index}{}", if wire { "Wire" } else { "" });
                file = file.add_type(build_type_alias(
                    &name,
                    self.type_name(expr, scope, representation, helper.nested_arrays),
                    None,
                )?);
            }
            for direction in [Direction::FromWire, Direction::ToWire] {
                let source = direction.source();
                // Converters only read inputs. Accept deeply readonly arrays
                // (including multipart inputs) without making API results or
                // the established readonly-outer/plain-inner model types wider.
                let input = self.type_name(
                    expr,
                    Scope {
                        readonly_input: true,
                        ..scope
                    },
                    match direction {
                        Direction::FromWire => Representation::Wire,
                        Direction::ToWire => Representation::Public,
                    },
                    helper.nested_arrays,
                );
                let output = self.helper_type(expr, scope, direction == Direction::ToWire);
                let body = match expr {
                    IrTypeExpr::Array(inner) => {
                        let converter = self.converter_symbol(
                            inner,
                            Scope {
                                nested_arrays: true,
                                ..scope
                            },
                            direction,
                        );
                        sigil_quote!(TypeScript { return $N(source).map($T(converter)); })?
                    }
                    IrTypeExpr::Map(inner) => {
                        let converted = self.conversion(
                            inner,
                            sigil_quote!(TypeScript { item })?,
                            Scope {
                                nested_arrays: false,
                                ..scope
                            },
                            direction,
                        )?;
                        let mapping = sigil_quote!(TypeScript { Object.entries($N(source) ?? {}).map(([key, item]) => [key, $C(converted)]) })?;
                        let value = layout::call(
                            sigil_quote!(TypeScript { Object.fromEntries })?,
                            &[mapping],
                            9,
                        )?;
                        sigil_quote!(TypeScript { return $C(value); })?
                    }
                    IrTypeExpr::Nullable(inner) => {
                        // A wrapper adds null permission; it must not intercept
                        // null already owned by an inner union's strict policy.
                        let standalone_null =
                            !crate::generators::union_match::accepts_null(inner, self.ir);
                        let converted = self.conversion(
                            inner,
                            sigil_quote!(TypeScript { $N(source) })?,
                            scope,
                            direction,
                        )?;
                        sigil_quote!(TypeScript {
                            $if(standalone_null) { if ($N(source) == null) { return null; } }
                            return $C(converted);
                        })?
                    }
                    IrTypeExpr::Union(members) => self.union_body(
                        &IrUnion {
                            members: members.clone(),
                            nullable: false,
                        },
                        scope,
                        output.clone(),
                        direction,
                        "inline",
                        false,
                    )?,
                    _ => unreachable!("only compound conversions have helpers"),
                };
                file = file.add_function(
                    FunSpec::builder(&format!("convert${index}{}", direction.suffix()))
                        .visibility(Visibility::Public)
                        .add_param(ParameterSpec::of(source, input))
                        .returns(vec![output])
                        .body(body)
                        .build()?,
                );
            }
            files.push(file.build()?);
        }
        Ok(files)
    }

    pub(super) fn value<'p>(&'p self, expr: &'p IrTypeExpr, scope: Scope<'p>) -> ValuePlan<'p, 'a> {
        ValuePlan {
            plan: self,
            expr,
            scope,
        }
    }

    pub(super) fn has_effect(&self, expr: &IrTypeExpr) -> bool {
        if !self.flags.property_naming_camel_case {
            return false;
        }
        match expr {
            IrTypeExpr::Named(name) => self.has_converter(name),
            IrTypeExpr::Array(inner) | IrTypeExpr::Map(inner) | IrTypeExpr::Nullable(inner) => {
                self.has_effect(inner)
            }
            IrTypeExpr::ScalarEnum { representation, .. } => self.has_effect(representation),
            IrTypeExpr::Union(_) => {
                self.flags.untagged_union_strategy != UntaggedUnionStrategy::Raw
            }
            _ => false,
        }
    }

    fn references_wire(&self, expr: &IrTypeExpr) -> bool {
        match expr {
            IrTypeExpr::Named(name) => self.has_wire_type(name),
            IrTypeExpr::Array(inner) | IrTypeExpr::Map(inner) | IrTypeExpr::Nullable(inner) => {
                self.references_wire(inner)
            }
            IrTypeExpr::ScalarEnum { representation, .. } => self.references_wire(representation),
            IrTypeExpr::Union(members) => members.iter().any(|expr| self.references_wire(expr)),
            _ => false,
        }
    }

    pub(super) fn is_unknown(&self, expr: &IrTypeExpr) -> bool {
        representation_is_unknown(expr, &self.unknown)
    }

    fn type_name(
        &self,
        expr: &IrTypeExpr,
        scope: Scope<'_>,
        representation: Representation,
        nested: bool,
    ) -> TypeName {
        match expr {
            IrTypeExpr::Named(name) => {
                let pascal = name.to_pascal_case();
                let symbol =
                    if matches!(representation, Representation::Wire) && self.has_wire_type(name) {
                        format!("{pascal}$Wire")
                    } else {
                        pascal.clone()
                    };
                if scope.current == Some(name) {
                    TypeName::primitive(&symbol)
                } else {
                    TypeName::importable_type(&format!("{}{pascal}", scope.models), &symbol)
                }
            }
            IrTypeExpr::Primitive(IrPrimitive::Binary) => TypeName::union(vec![
                TypeName::primitive("Blob"),
                TypeName::primitive("File"),
            ]),
            IrTypeExpr::Primitive(primitive) => TypeName::primitive(match primitive {
                IrPrimitive::Boolean => "boolean",
                IrPrimitive::Integer
                | IrPrimitive::IntegerWithFormat(_)
                | IrPrimitive::Number
                | IrPrimitive::NumberWithFormat(_) => "number",
                _ => "string",
            }),
            IrTypeExpr::ScalarEnum {
                representation: inner,
                ..
            } => self.type_name(inner, scope, representation, nested),
            IrTypeExpr::StringLiteral(value) => TypeName::string_literal(value.clone()),
            IrTypeExpr::StringEnum(values) => union_type(
                values
                    .iter()
                    .map(|value| TypeName::string_literal(value.clone()))
                    .collect(),
            ),
            IrTypeExpr::Array(inner) => {
                let element = self.type_name(inner, scope, representation, true);
                if scope.readonly_input {
                    return type_application(self.utility_type("ReadonlyArray"), vec![element]);
                }
                // sigil-stitch cannot render adjacent readonly array modifiers.
                // Preserve the existing readonly-outer/plain-inner contract.
                if nested || !scope.readonly_arrays {
                    TypeName::array(element)
                } else {
                    TypeName::readonly_array(element)
                }
            }
            IrTypeExpr::Map(inner) => type_application(
                self.utility_type("Record"),
                vec![
                    TypeName::primitive("string"),
                    self.type_name(inner, scope, representation, false),
                ],
            ),
            IrTypeExpr::Nullable(inner) => {
                if self.is_unknown(inner) {
                    TypeName::primitive("unknown")
                } else {
                    union_type(vec![
                        self.type_name(inner, scope, representation, nested),
                        TypeName::primitive("null"),
                    ])
                }
            }
            IrTypeExpr::Union(members) => {
                if members.iter().any(|expr| self.is_unknown(expr)) {
                    return TypeName::primitive("unknown");
                }
                let projection = if self.flags.property_naming_camel_case
                    && self.flags.untagged_union_strategy == UntaggedUnionStrategy::Raw
                {
                    Representation::Wire
                } else {
                    representation
                };
                union_type(
                    members
                        .iter()
                        .map(|expr| self.type_name(expr, scope, projection, nested))
                        .collect(),
                )
            }
            IrTypeExpr::Any => TypeName::primitive("unknown"),
        }
    }

    fn conversion(
        &self,
        expr: &IrTypeExpr,
        access: CodeBlock,
        scope: Scope<'_>,
        direction: Direction,
    ) -> Result<CodeBlock, SigilStitchError> {
        self.conversion_at(expr, access, scope, direction, 9)
    }

    fn conversion_at(
        &self,
        expr: &IrTypeExpr,
        access: CodeBlock,
        scope: Scope<'_>,
        direction: Direction,
        column: usize,
    ) -> Result<CodeBlock, SigilStitchError> {
        if !self.has_effect(expr) {
            return Ok(access);
        }
        let function = self.converter_symbol(expr, scope, direction);
        layout::call(
            sigil_quote!(TypeScript { $T(function) })?,
            &[access],
            column,
        )
    }

    pub(super) fn union_body(
        &self,
        union: &IrUnion,
        scope: Scope<'_>,
        output: TypeName,
        direction: Direction,
        label: &str,
        named: bool,
    ) -> Result<CodeBlock, SigilStitchError> {
        let source = direction.source();
        if self.flags.untagged_union_strategy == UntaggedUnionStrategy::Raw {
            return sigil_quote!(TypeScript { return $N(source); });
        }
        let members = if named {
            self.unions
                .get(scope.current.unwrap_or(label))
                .cloned()
                .unwrap_or_default()
        } else {
            crate::generators::union_match::plan_inline_union_members(&union.members, self.ir)
        };
        let strict = self.flags.untagged_union_strategy == UntaggedUnionStrategy::Strict;
        let mut checks = Vec::new();
        let mut concrete = Vec::new();
        let mut fallback = Vec::new();
        let mut terminal = None;
        for member in members {
            let check = self.matching.condition(
                &member.shape,
                sigil_quote!(TypeScript { candidate })?,
                direction,
                false,
            )?;
            let condition = if strict && !member.fallback {
                let index = checks.len();
                checks.push(check);
                // sigil-stitch literal arguments take text rather than usize.
                sigil_quote!(TypeScript { matches[$L(index.to_string())] })?
            } else {
                check
            };
            let conversion_expr = Self::conversion_representation(&member.type_expr);
            let input_type = if matches!(
                conversion_expr.as_ref(),
                IrTypeExpr::Array(_)
                    | IrTypeExpr::Map(_)
                    | IrTypeExpr::Nullable(_)
                    | IrTypeExpr::Union(_)
            ) && self.has_effect(&member.type_expr)
            {
                self.helper_type(&conversion_expr, scope, direction == Direction::FromWire)
            } else {
                self.type_name(
                    &member.type_expr,
                    scope,
                    match direction {
                        Direction::FromWire => Representation::Wire,
                        Direction::ToWire => Representation::Public,
                    },
                    scope.nested_arrays,
                )
            };
            let value = self.conversion(
                &member.type_expr,
                sigil_quote!(TypeScript { $N(source) as unknown as $T(input_type) })?,
                scope,
                direction,
            )?;
            if member.shape == JsonShape::Any {
                terminal = Some(sigil_quote!(TypeScript { return $C(value); })?);
                break;
            }
            let branch = sigil_quote!(TypeScript { if ($C(condition)) { return $C(value); } })?;
            if member.fallback {
                fallback.push(branch);
            } else {
                concrete.push(branch);
            }
        }
        let ambiguous = format!("Ambiguous {label} untagged union value");
        let unmatched = format!("No matching {label} untagged union member");
        let needs_candidate = !concrete.is_empty() || !fallback.is_empty() || union.nullable;
        let terminal = match terminal {
            Some(terminal) => terminal,
            None if strict => sigil_quote!(TypeScript { throw new TypeError($S(unmatched)); })?,
            None => sigil_quote!(TypeScript { return $N(source) as unknown as $T(output); })?,
        };
        // A standalone null allowance must not bypass original concrete member
        // counts or their delegates, including nested strict errors.
        let checked_candidates = layout::array(&checks, 18)?;
        sigil_quote!(TypeScript {
            $if(needs_candidate) { const candidate: $T(TypeName::primitive("unknown")) = $N(source); }
            $if(strict && !checks.is_empty()) {
                const matches = $C(checked_candidates);
                if (matches.filter(Boolean).length > 1) { throw new TypeError($S(ambiguous)); }
            }
            $C_each(concrete)
            $if(union.nullable) { if (candidate === null) { return null; } }
            $C_each(fallback)
            $C(terminal)
        })
    }
}

impl ValuePlan<'_, '_> {
    pub(super) fn convert_at(
        &self,
        access: CodeBlock,
        direction: Direction,
        column: usize,
    ) -> Result<CodeBlock, SigilStitchError> {
        self.plan
            .conversion_at(self.expr, access, self.scope, direction, column)
    }
    pub(super) fn accepts_null(&self) -> bool {
        crate::generators::union_match::accepts_null(self.expr, self.plan.ir)
    }

    pub(super) fn public_type(&self) -> TypeName {
        self.plan
            .type_name(self.expr, self.scope, Representation::Public, false)
    }
    pub(super) fn wire_type(&self) -> TypeName {
        self.plan
            .type_name(self.expr, self.scope, Representation::Wire, false)
    }
    pub(super) fn convert(
        &self,
        access: CodeBlock,
        direction: Direction,
    ) -> Result<CodeBlock, SigilStitchError> {
        self.plan
            .conversion(self.expr, access, self.scope, direction)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restoring_a_nullable_conversion_wrapper_keeps_inner_finite_facts() {
        for inner_values in [vec![1.into()], vec![1.into(), serde_json::Value::Null]] {
            let inner = IrTypeExpr::Nullable(Box::new(IrTypeExpr::Named("InnerChoice".into())))
                .with_scalar_enum(&inner_values, true);
            let outer = IrTypeExpr::Nullable(Box::new(inner))
                .with_scalar_enum(&[1.into(), serde_json::Value::Null], true);
            let (property, nullable) = outer.split_nullable();
            assert!(nullable);
            let IrTypeExpr::ScalarEnum { representation, .. } = &property else {
                panic!("outer finite constraint");
            };
            assert_eq!(
                TypeScriptPlan::conversion_representation(&property).as_ref(),
                &IrTypeExpr::Nullable(representation.clone()),
                "the delegate must retain the inner finite node that owns null acceptance"
            );
        }
    }
}
