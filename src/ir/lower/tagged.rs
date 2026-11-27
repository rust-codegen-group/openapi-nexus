//! Shared facts for inferred tagged-union discriminator constraints.
//!
//! Version adapters provide access to their raw schema shapes. The policy for
//! following referenced enum values and retaining optional tags lives here so
//! OAS 3.0, 3.1 and 3.2 do not drift.

use std::collections::HashSet;

use super::composition::CompositionSchema;

/// A shared property name is not sufficient for an external tag. Look for a
/// value discriminator across the whole union before interpreting the key as
/// the branch identity.
pub(super) fn fixed_single_field_patterns<S: super::wire::WireSchema>(
    members: &[S::Member],
) -> Option<Vec<Option<crate::ir::tagged_enum_pattern::TaggedEnumPattern>>> {
    use crate::ir::{tagged_enum_pattern::TaggedEnumPattern, types::IrJsonKind};
    use serde_json::Value;

    if members.len() < 2 {
        return None;
    }
    let mut field = None;
    let mut labels = HashSet::new();
    members
        .iter()
        .map(|member| {
            let object = S::object(member)?;
            let (key, property) = sole_required_property::<S>(object)?;
            if object.kinds() != [IrJsonKind::Object]
                || !uncomposed(object)
                || !object.enum_values().is_empty()
                || field.get_or_insert(key) != &key
            {
                return None;
            }
            let property = S::object(property)?;
            let [Value::String(label)] = property.enum_values() else {
                return None;
            };
            let kinds = property.kinds();
            if (!kinds.is_empty() && kinds != [IrJsonKind::String])
                || !uncomposed(property)
                || !labels.insert(label.as_str())
            {
                return None;
            }
            Some(Some(TaggedEnumPattern::InternallyTagged {
                variant_name: label.clone(),
                tag_field: key.to_owned(),
            }))
        })
        .collect()
}

fn uncomposed<S: super::wire::WireSchema>(schema: &S) -> bool {
    schema.all_of().is_empty()
        && schema.one_of().is_empty()
        && schema.any_of().is_empty()
        && schema.constant().is_none()
}

fn sole_required_property<S: super::wire::WireSchema>(object: &S) -> Option<(&str, &S::Member)> {
    let mut properties = object.properties().iter();
    let (key, property) = properties.next()?;
    // No other required property may be hidden in additional properties.
    (properties.next().is_none() && object.required() == [key.as_str()])
        .then_some((key.as_str(), property))
}

/// Validate key-based dispatch using authored property names, not the names
/// already normalized by pattern detection for target identifiers.
pub(super) fn has_distinct_external_keys<S: super::wire::WireSchema>(
    members: &[S::Member],
) -> bool {
    let mut keys = HashSet::new();
    members.iter().all(|member| {
        S::object(member)
            .and_then(|object| object.properties().first_key_value())
            .is_some_and(|(key, _)| keys.insert(key.as_str()))
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Facts {
    pub values: Vec<String>,
    pub optional: bool,
    pub constrained: bool,
}

/// Resolve the legal string values and requiredness of an inferred tag.
pub(super) fn facts<S: CompositionSchema>(
    member: &S::Member,
    tag_field: &str,
    fallback: &str,
    resolve: impl Fn(&str) -> Option<S::Member>,
) -> Facts {
    let mut constraints = Vec::new();
    let mut required = false;
    let mut visited = HashSet::new();
    collect::<S>(
        member,
        tag_field,
        &resolve,
        &mut visited,
        &mut constraints,
        &mut required,
    );

    let constrained = !constraints.is_empty();
    let values = constraints
        .into_iter()
        .reduce(|current, constraint| {
            current
                .into_iter()
                .filter(|value| constraint.contains(value))
                .collect()
        })
        .unwrap_or_else(|| vec![fallback.to_string()]);

    Facts {
        values,
        optional: !required,
        constrained,
    }
}

/// Keep the additive IR field empty for the historical single-value case when
/// its canonical value already is the authored wire value. A referenced
/// singleton enum still needs to retain its one legal value when it differs
/// from the schema-name fallback.
pub(super) fn discriminator_values(facts: &Facts, fallback: &str) -> Vec<String> {
    if facts.constrained
        && (facts.values.len() > 1 || facts.values.first().is_some_and(|value| value != fallback))
    {
        facts.values.clone()
    } else {
        Vec::new()
    }
}

fn collect<S: CompositionSchema>(
    member: &S::Member,
    tag_field: &str,
    resolve: &impl Fn(&str) -> Option<S::Member>,
    visited: &mut HashSet<String>,
    constraints: &mut Vec<Vec<String>>,
    required: &mut bool,
) {
    if let Some(reference) = S::reference(member) {
        if visited.insert(reference.to_string())
            && let Some(target) = resolve(reference)
        {
            collect::<S>(&target, tag_field, resolve, visited, constraints, required);
        }
        return;
    }
    let Some(object) = S::object(member) else {
        return;
    };
    *required |= object.required().iter().any(|field| field == tag_field);
    if let Some(property) = object.property(tag_field)
        && let Some(values) = collect_enum::<S>(property, resolve, visited)
    {
        constraints.push(values);
    }
    for member in object.all_of() {
        collect::<S>(member, tag_field, resolve, visited, constraints, required);
    }
}

fn collect_enum<S: CompositionSchema>(
    member: &S::Member,
    resolve: &impl Fn(&str) -> Option<S::Member>,
    visited: &mut HashSet<String>,
) -> Option<Vec<String>> {
    if let Some(reference) = S::reference(member) {
        if visited.insert(reference.to_string())
            && let Some(target) = resolve(reference)
        {
            return collect_enum::<S>(&target, resolve, visited);
        }
        return None;
    }
    let object = S::object(member)?;
    let values: Vec<_> = object
        .enum_values()
        .iter()
        .filter_map(|value| value.as_str().map(ToOwned::to_owned))
        .collect();
    (!values.is_empty()).then_some(values)
}
