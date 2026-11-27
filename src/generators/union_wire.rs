//! Language-independent selection over authored JSON. Target adapters bind
//! the selected branch to their native model and invoke its codec once.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::{Map, Number, Value};

use crate::ir::types::{
    IrJsonKind, IrSchemaKind, IrSpec, IrTaggedUnion, IrTypeExpr, IrWireShape, TaggingStyle,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Strategy {
    #[default]
    FirstMatch,
    Strict,
    Raw,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection<'a> {
    /// Index of an authored alternative, before any concrete/fallback partitioning.
    Branch(usize),
    Raw(&'a Value),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionError {
    NoMatch,
    Ambiguous(Vec<usize>),
}

/// One immutable index per generation. It neither lowers OpenAPI again nor
/// decides how a selected branch is represented in a target language.
pub struct UnionWirePlans<'ir> {
    plans: BTreeMap<&'ir str, UnionWirePlan<'ir>>,
}

impl<'ir> UnionWirePlans<'ir> {
    pub fn build(ir: &'ir IrSpec, strategy: Strategy) -> Self {
        let plans = ir
            .schemas
            .iter()
            .filter_map(|(name, schema)| {
                let (wire, tagged) = match &schema.kind {
                    IrSchemaKind::TaggedUnion(union) => {
                        let wire = union.inferred_wire.as_ref()?;
                        if wire.branches.len() != union.variants.len() {
                            return None;
                        }
                        (wire, Some(union))
                    }
                    IrSchemaKind::Union(union) => (union.authored_wire.as_ref()?, None),
                    _ => return None,
                };
                let bindings = (0..wire.branches.len())
                    .map(|index| {
                        tagged.map_or(WireBinding::Whole, |union| tagged_binding(union, index, ir))
                    })
                    .collect();
                Some((
                    name.as_str(),
                    UnionWirePlan {
                        ir,
                        tagged,
                        branches: &wire.branches,
                        common: Some(&wire.common),
                        bindings,
                        strategy,
                    },
                ))
            })
            .collect();
        Self { plans }
    }

    pub fn get(&self, use_site: &str) -> Option<&UnionWirePlan<'ir>> {
        self.plans.get(use_site)
    }
}

pub struct UnionWirePlan<'ir> {
    ir: &'ir IrSpec,
    tagged: Option<&'ir IrTaggedUnion>,
    branches: &'ir [IrWireShape],
    common: Option<&'ir IrWireShape>,
    bindings: Vec<WireBinding<'ir>>,
    strategy: Strategy,
}

/// A conservative description of the root values a representation must handle.
/// Shape matching, not these flags, decides whether a particular value is eligible.
#[derive(Clone, Copy)]
pub(crate) struct RootKinds {
    pub(crate) object: bool,
    pub(crate) non_object: bool,
}

impl RootKinds {
    const NONE: Self = Self {
        object: false,
        non_object: false,
    };
    const BOTH: Self = Self {
        object: true,
        non_object: true,
    };

    fn only(object: bool) -> Self {
        Self {
            object,
            non_object: !object,
        }
    }

    fn intersect(self, other: Self) -> Self {
        Self {
            object: self.object && other.object,
            non_object: self.non_object && other.non_object,
        }
    }

    fn union(self, other: Self) -> Self {
        Self {
            object: self.object || other.object,
            non_object: self.non_object || other.non_object,
        }
    }
}

impl<'ir> UnionWirePlan<'ir> {
    /// True when the legacy branch labels agree with distinct authored singletons
    /// and each label is required. This proves tag identity only. It does not
    /// prove that a target codec respects the shapes or preserves the payload.
    pub fn has_distinct_fixed_tags(&self) -> bool {
        let Some(union) = self.tagged else {
            return false;
        };
        let mut seen = HashSet::new();
        !union.variants.is_empty()
            && union.variants.iter().enumerate().all(|(index, variant)| {
                self.tag_domain(index).is_some_and(|domain| {
                    matches!(domain.values.as_deref(), Some([Value::String(label)])
                        if !domain.allows_absent && label == &variant.discriminator_value
                            && seen.insert(variant.discriminator_value.as_str()))
                })
            })
    }

    pub fn binding(&self, branch: usize) -> Option<WireBinding<'ir>> {
        self.bindings.get(branch).copied()
    }

    /// Constraints applied before testing any of the ordered branches.
    pub fn common_shape(&self) -> Option<&'ir IrWireShape> {
        self.common
    }

    pub fn branches(&self) -> &[IrWireShape] {
        self.branches
    }

    /// Whether this authored alternative can require an object or whole-value
    /// binding. The common gate can rule out a representation but supplies no
    /// branch-selection evidence.
    pub(crate) fn possible_root_kinds(&self, branch: usize) -> RootKinds {
        let Some(branch) = self.branches.get(branch) else {
            return RootKinds::NONE;
        };
        let mut visiting = HashSet::new();
        self.common
            .into_iter()
            .chain([branch])
            .fold(RootKinds::BOTH, |kinds, shape| {
                kinds.intersect(possible_root_kinds(
                    shape,
                    &self.ir.wire_schemas,
                    &mut visiting,
                ))
            })
    }

    /// Project the discriminator's declaration domain, without discarding the
    /// other fields needed for actual selection. `None` is not a fixed tag.
    pub fn tag_domain(&self, branch: usize) -> Option<TagDomain<'ir>> {
        let field = match self.binding(branch)? {
            WireBinding::Internal { field, .. } | WireBinding::Adjacent { field, .. } => field,
            WireBinding::Whole | WireBinding::External { .. } => return None,
        };
        let schemas = &self.ir.wire_schemas;
        let mut visiting = HashSet::new();
        let parts = self
            .common
            .into_iter()
            .chain([self.branches.get(branch)?])
            .map(|shape| field_projection(shape, field, schemas, &mut visiting))
            .collect();
        let projected = FieldProjection::combine(parts, true);
        Some(TagDomain {
            field,
            allows_absent: projected.allows_absent,
            values: finite_values(&projected.present, schemas, &mut HashSet::new()),
        })
    }

    /// Prove a required singleton string property from one authored branch.
    /// This is a necessary condition for that branch, not an eligibility check
    /// for the whole object or an inference of a discriminator.
    pub(crate) fn required_string_literal(&self, branch: usize, field: &str) -> Option<String> {
        let shape = self.branches.get(branch)?;
        let schemas = &self.ir.wire_schemas;
        let projection = field_projection(shape, field, schemas, &mut HashSet::new());
        if projection.allows_absent {
            return None;
        }
        let values = finite_values(&projection.present, schemas, &mut HashSet::new())?;
        let [Value::String(value)] = values.as_slice() else {
            return None;
        };
        Some(value.clone())
    }

    /// Reference implementation of the selection contract. This is structural
    /// dispatch, not a full JSON Schema validator, and never invokes a decoder.
    pub fn select<'wire>(&self, value: &'wire Value) -> Result<Selection<'wire>, SelectionError> {
        if self.strategy == Strategy::Raw {
            return Ok(Selection::Raw(value));
        }
        let mut memo = HashMap::new();
        if self.common.is_some_and(|shape| {
            matching(shape, value, &self.ir.wire_schemas, &mut memo) == Evidence::NoMatch
        }) {
            return Err(SelectionError::NoMatch);
        }
        let mut fallback = None;
        let mut concrete = Vec::new();
        for (index, branch) in self.branches.iter().enumerate() {
            match matching(branch, value, &self.ir.wire_schemas, &mut memo) {
                Evidence::NoMatch => {}
                Evidence::Opaque => {
                    fallback.get_or_insert(index);
                }
                Evidence::Concrete if self.strategy == Strategy::FirstMatch => {
                    return Ok(Selection::Branch(index));
                }
                Evidence::Concrete => concrete.push(index),
            }
        }
        match concrete.as_slice() {
            [] => fallback
                .map(Selection::Branch)
                .ok_or(SelectionError::NoMatch),
            [index] => Ok(Selection::Branch(*index)),
            _ => Err(SelectionError::Ambiguous(concrete)),
        }
    }
}

/// Where the codec reads and writes the discriminator relative to the payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireBinding<'ir> {
    Whole,
    Internal {
        field: &'ir str,
        owner: TagOwner,
        read: TagRead,
    },
    Adjacent {
        field: &'ir str,
        content: &'ir str,
    },
    External {
        key: &'ir str,
    },
}

/// Whether the original payload codec can be called with the tag removed.
/// A codec specifically projected for the union case can use a different input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagRead {
    Keep,
    Strip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagOwner {
    /// One payload codec owns the property, including a nested tagged union.
    Payload,
    /// No payload codec owns the property. The case carries its state.
    Case,
    /// Several payload codecs, or only some alternatives, can carry the tag.
    /// The case must ensure a single authoritative value on output.
    CaseWithPayloadProjection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConflictingTag;

impl WireBinding<'_> {
    /// None is absent; Some(JSON null) is present.
    pub fn tag(self, wire: &Value) -> Option<&Value> {
        match self {
            Self::Internal { field, .. } | Self::Adjacent { field, .. } => wire.get(field),
            Self::Whole | Self::External { .. } => None,
        }
    }

    /// The input to the original selected payload codec. This is separate from
    /// output ownership: several composed members may still need to read a tag.
    pub fn payload<'wire>(self, wire: &'wire Value) -> Option<Cow<'wire, Value>> {
        match self {
            Self::Whole
            | Self::Internal {
                read: TagRead::Keep,
                ..
            } => Some(Cow::Borrowed(wire)),
            Self::Internal { field, .. } => {
                let object = wire.as_object()?;
                if object.contains_key(field) {
                    let mut payload = object.clone();
                    payload.remove(field);
                    Some(Cow::Owned(Value::Object(payload)))
                } else {
                    Some(Cow::Borrowed(wire))
                }
            }
            Self::Adjacent { content, .. } => wire.get(content).map(Cow::Borrowed),
            Self::External { key } => wire.get(key).map(Cow::Borrowed),
        }
    }

    /// Write the case-owned tag once, or leave the payload-owned value in
    /// place. A conflicting value is never overwritten. This does not
    /// validate authored value membership; eligibility uses the full shape.
    pub fn write_tag(
        self,
        output: &mut Map<String, Value>,
        tag: Option<&Value>,
    ) -> Result<(), ConflictingTag> {
        match self {
            Self::Internal {
                field,
                owner: TagOwner::Payload,
                ..
            } => {
                if tag.is_some() && output.get(field) != tag {
                    return Err(ConflictingTag);
                }
            }
            Self::Internal { field, .. } | Self::Adjacent { field, .. } => {
                if let Some(existing) = output.get(field) {
                    if Some(existing) != tag {
                        return Err(ConflictingTag);
                    }
                } else if let Some(tag) = tag {
                    output.insert(field.to_owned(), tag.clone());
                }
            }
            Self::Whole | Self::External { .. } if tag.is_some() => return Err(ConflictingTag),
            Self::Whole | Self::External { .. } => {}
        }
        Ok(())
    }
}

fn tagged_binding<'ir>(union: &'ir IrTaggedUnion, index: usize, ir: &IrSpec) -> WireBinding<'ir> {
    let variant = &union.variants[index];
    let field = &union.discriminator_field;
    match &union.tagging {
        TaggingStyle::Internal => {
            let access = root_tag_access(&variant.content_type, field, ir, &mut HashSet::new());
            let (owner, read) = match access {
                RootTagAccess::None => (TagOwner::Case, TagRead::Strip),
                RootTagAccess::One => (TagOwner::Payload, TagRead::Keep),
                RootTagAccess::Projected => (TagOwner::CaseWithPayloadProjection, TagRead::Keep),
            };
            WireBinding::Internal { field, owner, read }
        }
        TaggingStyle::Adjacent { content_field } => WireBinding::Adjacent {
            field,
            content: content_field,
        },
        TaggingStyle::External => WireBinding::External {
            key: &variant.discriminator_value,
        },
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RootTagAccess {
    None,
    One,
    Projected,
}

impl RootTagAccess {
    fn alternatives(mut members: impl Iterator<Item = Self>) -> Self {
        let first = members.next().unwrap_or(Self::None);
        if members.all(|member| member == first) {
            first
        } else {
            Self::Projected
        }
    }

    fn compose(self, other: Self) -> Self {
        match (self, other) {
            (Self::None, access) | (access, Self::None) => access,
            _ => Self::Projected,
        }
    }
}

fn root_tag_access<'ir>(
    expr: &'ir IrTypeExpr,
    field: &str,
    ir: &'ir IrSpec,
    visiting: &mut HashSet<&'ir str>,
) -> RootTagAccess {
    use RootTagAccess::{None as NoAccess, One};
    match expr.representation() {
        IrTypeExpr::Nullable(inner) => root_tag_access(inner, field, ir, visiting),
        IrTypeExpr::Union(members) => RootTagAccess::alternatives(
            members
                .iter()
                .map(|member| root_tag_access(member, field, ir, visiting)),
        ),
        IrTypeExpr::Named(name) if visiting.insert(name.as_str()) => {
            let access = match ir.schemas.get(name).map(|schema| &schema.kind) {
                Some(IrSchemaKind::Object(object)) if object.properties.contains_key(field) => One,
                Some(IrSchemaKind::Alias(inner)) => root_tag_access(inner, field, ir, visiting),
                Some(IrSchemaKind::Intersection(all)) => {
                    all.members.iter().fold(NoAccess, |access, member| {
                        access.compose(root_tag_access(member, field, ir, visiting))
                    })
                }
                Some(IrSchemaKind::Union(union)) => RootTagAccess::alternatives(
                    union
                        .members
                        .iter()
                        .map(|member| root_tag_access(member, field, ir, visiting)),
                ),
                Some(IrSchemaKind::TaggedUnion(union)) => match &union.tagging {
                    TaggingStyle::Internal if union.discriminator_field == field => One,
                    TaggingStyle::Internal => {
                        RootTagAccess::alternatives(union.variants.iter().map(|variant| {
                            root_tag_access(&variant.content_type, field, ir, visiting)
                        }))
                    }
                    TaggingStyle::Adjacent { content_field }
                        if union.discriminator_field == field || content_field == field =>
                    {
                        One
                    }
                    TaggingStyle::External => {
                        RootTagAccess::alternatives(union.variants.iter().map(|variant| {
                            if variant.discriminator_value == field {
                                One
                            } else {
                                NoAccess
                            }
                        }))
                    }
                    TaggingStyle::Adjacent { .. } => NoAccess,
                },
                _ => NoAccess,
            };
            visiting.remove(name.as_str());
            access
        }
        _ => NoAccess,
    }
}

/// A conservative projection of the root tag, for declarations or fast-path
/// proofs. Branch eligibility still uses the full authored object and its rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagDomain<'ir> {
    pub field: &'ir str,
    pub allows_absent: bool,
    /// `None` means no proven finite set; `Some([])` excludes present values.
    /// JSON null is a value in this set, never a spelling of absence.
    pub values: Option<Vec<Value>>,
}

struct FieldProjection {
    allows_absent: bool,
    present: IrWireShape,
}

impl FieldProjection {
    fn open() -> Self {
        Self {
            allows_absent: true,
            present: IrWireShape::Any,
        }
    }

    fn impossible() -> Self {
        Self {
            allows_absent: false,
            present: IrWireShape::Never,
        }
    }

    fn combine(parts: Vec<Self>, conjunctive: bool) -> Self {
        let allows_absent = if conjunctive {
            parts.iter().all(|node| node.allows_absent)
        } else {
            parts.iter().any(|node| node.allows_absent)
        };
        let shapes = parts.into_iter().map(|node| node.present).collect();
        let present = if conjunctive {
            IrWireShape::AllOf(shapes)
        } else {
            IrWireShape::AnyOf(shapes)
        };
        Self {
            allows_absent,
            present,
        }
    }
}

fn field_projection<'ir>(
    shape: &'ir IrWireShape,
    field: &str,
    schemas: &'ir BTreeMap<String, IrWireShape>,
    visiting: &mut HashSet<&'ir str>,
) -> FieldProjection {
    match shape {
        IrWireShape::Object(object) => FieldProjection {
            allows_absent: !object.required.contains(field),
            present: object
                .properties
                .get(field)
                .unwrap_or(&object.additional_properties)
                .clone(),
        },
        IrWireShape::Finite(values) => {
            let objects = values.iter().filter_map(Value::as_object);
            FieldProjection {
                allows_absent: objects.clone().any(|object| !object.contains_key(field)),
                present: IrWireShape::Finite(
                    objects
                        .filter_map(|object| object.get(field).cloned())
                        .collect(),
                ),
            }
        }
        IrWireShape::AllOf(parts) | IrWireShape::OneOf(parts) | IrWireShape::AnyOf(parts) => {
            let projected = parts
                .iter()
                .map(|part| field_projection(part, field, schemas, visiting))
                .collect();
            FieldProjection::combine(projected, matches!(shape, IrWireShape::AllOf(_)))
        }
        IrWireShape::Reference(name) => {
            if !visiting.insert(name) {
                return FieldProjection::open();
            }
            let projected = schemas
                .get(name)
                .map_or_else(FieldProjection::open, |target| {
                    field_projection(target, field, schemas, visiting)
                });
            visiting.remove(name.as_str());
            projected
        }
        IrWireShape::Any | IrWireShape::Kind(IrJsonKind::Object) => FieldProjection::open(),
        IrWireShape::Never | IrWireShape::Kind(_) | IrWireShape::Array(_) => {
            FieldProjection::impossible()
        }
    }
}

fn finite_values<'ir>(
    shape: &'ir IrWireShape,
    schemas: &'ir BTreeMap<String, IrWireShape>,
    visiting: &mut HashSet<&'ir str>,
) -> Option<Vec<Value>> {
    match shape {
        IrWireShape::Never => Some(Vec::new()),
        IrWireShape::Kind(IrJsonKind::Null) => Some(vec![Value::Null]),
        IrWireShape::Kind(IrJsonKind::Boolean) => Some(vec![Value::Bool(false), Value::Bool(true)]),
        IrWireShape::Finite(values) => Some(values.clone()),
        IrWireShape::AllOf(parts) => {
            let mut values = parts
                .iter()
                .find_map(|part| finite_values(part, schemas, visiting))?;
            values.retain(|value| {
                matching(shape, value, schemas, &mut HashMap::new()) != Evidence::NoMatch
            });
            Some(values)
        }
        IrWireShape::OneOf(parts) | IrWireShape::AnyOf(parts) => {
            let mut values = Vec::new();
            for part in parts {
                for value in finite_values(part, schemas, visiting)? {
                    if !values.iter().any(|seen| literals_equal(seen, &value)) {
                        values.push(value);
                    }
                }
            }
            Some(values)
        }
        IrWireShape::Reference(name) => {
            if !visiting.insert(name) {
                return None;
            }
            let values = schemas
                .get(name)
                .and_then(|target| finite_values(target, schemas, visiting));
            visiting.remove(name.as_str());
            values
        }
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Evidence {
    NoMatch,
    Opaque,
    Concrete,
}

impl Evidence {
    fn when(condition: bool) -> Self {
        if condition {
            Self::Concrete
        } else {
            Self::NoMatch
        }
    }
}

fn possible_root_kinds<'ir>(
    shape: &'ir IrWireShape,
    schemas: &'ir BTreeMap<String, IrWireShape>,
    visiting: &mut HashSet<&'ir str>,
) -> RootKinds {
    match shape {
        IrWireShape::Any => RootKinds::BOTH,
        IrWireShape::Never => RootKinds::NONE,
        IrWireShape::Kind(kind) => RootKinds::only(*kind == IrJsonKind::Object),
        IrWireShape::Object(_) => RootKinds::only(true),
        IrWireShape::Array(_) => RootKinds::only(false),
        IrWireShape::Finite(values) => values.iter().fold(RootKinds::NONE, |kinds, value| {
            kinds.union(RootKinds::only(value.is_object()))
        }),
        IrWireShape::AllOf(parts) => parts.iter().fold(RootKinds::BOTH, |kinds, part| {
            kinds.intersect(possible_root_kinds(part, schemas, visiting))
        }),
        IrWireShape::AnyOf(parts) | IrWireShape::OneOf(parts) => {
            parts.iter().fold(RootKinds::NONE, |kinds, part| {
                kinds.union(possible_root_kinds(part, schemas, visiting))
            })
        }
        IrWireShape::Reference(name) if visiting.insert(name.as_str()) => {
            let result = schemas.get(name).map_or(RootKinds::NONE, |shape| {
                possible_root_kinds(shape, schemas, visiting)
            });
            visiting.remove(name.as_str());
            result
        }
        IrWireShape::Reference(_) => RootKinds::NONE,
    }
}

type Memo<'ir> = HashMap<(&'ir str, *const Value), Evidence>;

fn matching<'ir>(
    shape: &'ir IrWireShape,
    value: &Value,
    schemas: &'ir BTreeMap<String, IrWireShape>,
    memo: &mut Memo<'ir>,
) -> Evidence {
    use Evidence::{Concrete, NoMatch, Opaque};
    match shape {
        IrWireShape::Any => Opaque,
        IrWireShape::Never => NoMatch,
        IrWireShape::Kind(IrJsonKind::Object) => {
            if value.is_object() {
                Opaque
            } else {
                NoMatch
            }
        }
        IrWireShape::Kind(kind) => Evidence::when(match kind {
            IrJsonKind::Null => value.is_null(),
            IrJsonKind::Boolean => value.is_boolean(),
            IrJsonKind::Integer => value.as_number().is_some_and(is_integer),
            IrJsonKind::Number => value.is_number(),
            IrJsonKind::String => value.is_string(),
            IrJsonKind::Array => value.is_array(),
            IrJsonKind::Object => unreachable!("object kind handled separately"),
        }),
        IrWireShape::Finite(values) => Evidence::when(
            values
                .iter()
                .any(|candidate| literals_equal(candidate, value)),
        ),
        IrWireShape::Object(shape) => {
            let Some(object) = value.as_object() else {
                return NoMatch;
            };
            if !shape.required.iter().all(|key| object.contains_key(key)) {
                return NoMatch;
            }
            let mut evidence = if shape.properties.is_empty()
                && shape.required.is_empty()
                && !matches!(*shape.additional_properties, IrWireShape::Never)
            {
                Opaque
            } else {
                Concrete
            };
            for (key, value) in object {
                let property = shape
                    .properties
                    .get(key)
                    .unwrap_or(&shape.additional_properties);
                match matching(property, value, schemas, memo) {
                    NoMatch => return NoMatch,
                    matched => evidence = evidence.max(matched),
                }
            }
            evidence
        }
        IrWireShape::Array(items) => Evidence::when(value.as_array().is_some_and(|items_value| {
            items_value
                .iter()
                .all(|item| matching(items, item, schemas, memo) != NoMatch)
        })),
        IrWireShape::AllOf(parts) => {
            let mut evidence = Opaque;
            for part in parts {
                match matching(part, value, schemas, memo) {
                    NoMatch => return NoMatch,
                    matched => evidence = evidence.max(matched),
                }
            }
            evidence
        }
        // Nested alternative cardinality is resolved by the selected codec,
        // not by its containing branch's eligibility probe.
        IrWireShape::AnyOf(parts) | IrWireShape::OneOf(parts) => {
            let mut evidence = NoMatch;
            for part in parts {
                evidence = evidence.max(matching(part, value, schemas, memo));
                if evidence == Concrete {
                    break;
                }
            }
            evidence
        }
        IrWireShape::Reference(name) => {
            let key = (name.as_str(), std::ptr::from_ref(value));
            if let Some(result) = memo.get(&key) {
                return *result;
            }
            // The JSON tree remains immutable for this selection. A repeat
            // before finishing this key is a non-progressing reference cycle.
            memo.insert(key, NoMatch);
            let result = schemas
                .get(name)
                .map_or(NoMatch, |shape| matching(shape, value, schemas, memo));
            memo.insert(key, result);
            result
        }
    }
}

fn is_integer(number: &Number) -> bool {
    number.is_i64() || number.is_u64() || number.as_f64().is_some_and(|n| n.fract() == 0.0)
}

fn literals_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => numbers_equal(left, right),
        // JSON numeric equality applies at every depth of a literal, not just
        // when the entire literal is a scalar number.
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| literals_equal(left, right))
        }
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left.iter().all(|(key, value)| {
                    right
                        .get(key)
                        .is_some_and(|other| literals_equal(value, other))
                })
        }
        _ => left == right,
    }
}

fn numbers_equal(left: &Number, right: &Number) -> bool {
    if left == right {
        return true;
    }
    fn integral_float_matches(integer: &Number, float: &Number) -> bool {
        let Some(value) = float.as_f64().filter(|value| value.fract() == 0.0) else {
            return false;
        };
        if let Some(integer) = integer.as_i64().filter(|integer| *integer < 0) {
            value >= i64::MIN as f64 && value < 0.0 && value as i64 == integer
        } else if let Some(integer) = integer.as_u64() {
            value >= 0.0 && value < u64::MAX as f64 && value as u64 == integer
        } else {
            false
        }
    }
    match (left.is_f64(), right.is_f64()) {
        (false, true) => integral_float_matches(left, right),
        (true, false) => integral_float_matches(right, left),
        _ => false,
    }
}
