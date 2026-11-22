//! Shared facts for inferred tagged-union discriminator constraints.
//!
//! Version adapters provide access to their raw schema shapes. The policy for
//! following referenced enum values and retaining optional tags lives here so
//! OAS 3.0, 3.1 and 3.2 do not drift.

use std::collections::HashSet;

use super::composition::CompositionSchema;

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
