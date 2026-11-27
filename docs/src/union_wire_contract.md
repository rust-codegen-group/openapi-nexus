# Union Wire Contract

`UnionWirePlan` defines how to select an authored union member from JSON. This
contract describes the shared planning interface and the requirements for codec
adapters that adopt it. The shared IR and selection interface do not, on their
own, change generated clients or expose a CLI option.

## Shared facts and target adapters

OpenAPI 3.0, 3.1, and 3.2 lower to the same wire model. Keep three facts distinct:

- **Authored constraints** describe what can match: presence, JSON kind, finite
  values, composition, and object and container shape. An inferred discriminator
  is evidence from these constraints, not a new restriction on the schema.
- **Member identity** is the authored alternative in declaration order, not a
  schema name, generated identifier, or tag spelling. Several legal spellings
  can select the same member and do not add candidates.
- **Explicit mapping** is a dispatch hint to an authored member; it does not
  change which values satisfy the schema. Existing explicit-mapping behavior is
  outside the inferred-discriminator contract. OpenAPI 3.2
  [`defaultMapping`](https://spec.openapis.org/oas/v3.2.0.html#discriminator-object)
  is separate future work, not permission inferred from an optional property.

The shared planner supplies structural eligibility, member identity, and
selection. It also projects the discriminator's declaration domain: presence
is separate from the set of finite values, which can be empty or include null.
A non-finite domain is not a fixed label. The domain does not alone determine
eligibility; direct and nested selection must also use the full authored shape.
Target adapters own names, public model representations, payload codecs, and
structured sigil-stitch emission. A native fast path is appropriate only when it
gives the same selection and wire behavior for the applicable policy. Required,
unique, fixed labels prove tag identity only, not codec equivalence. An adapter
may keep a legacy fixed-label path outside its adopted policy for compatibility;
that does not make the old path conform to this contract.

## Tag presence and ownership

Match the full wire value before projecting the selected payload for its codec.
At a given wire path the discriminator has one owner: the payload model if it
represents the tag faithfully, otherwise the union. A projection applies only at
that use site. Read requirements are separate from write ownership: even when
an enclosing case owns the output tag, a nested codec that consumes it needs
it in its input. Do not rewrite a reusable referenced model or discard a tag
that its codec needs. Serialization emits the tag at its original path at most
once; competing payload or extension values must not overwrite it silently.

An inferred adjacent envelope projects the content field as the payload. Keep
that projection only when each payload-bearing alternative requires the
content field; this does not require the tag itself to be present. A tag-only
alternative can share the envelope when its declared property is the common
tag and has a required, fixed string label. Otherwise keep the original object
alternatives rather than treating a missing or unrelated field as the payload.

`additionalProperties` is local to each object schema. Evaluate each `allOf`
conjunct against the full object using only that conjunct's property declarations.
A tag declared by an object is not an extra property of that object, even when
projected out for decoding. A sibling that does not declare it can still constrain
it as an extra property.

Skip the tag on output when it was absent. A present `null` is not absence and
matches only when the schema admits it. Preserve the exact legal label on output;
do not canonicalize it or substitute a schema name. An unknown present value is
not eligible merely because a member permits absence.

## Selection

| Strategy | Result |
| --- | --- |
| `FirstMatch` | Select the first structurally matching concrete member in declaration order. Use the first matching opaque alternative only if no concrete member matches. |
| `Strict` | Use the same eligibility and opaque precedence, but reject multiple matching concrete members. Count authored members, not matching spellings or predicate paths. |
| `Raw` | Preserve the original JSON subtree without member selection, field renaming, or payload conversion. Do not present it as a decoded typed member. |

In the typed policies, no eligible member is a no-match result. Selection uses
structural evidence, not full JSON Schema validation; `Strict` checks ambiguity
at the selection seam rather than certifying `oneOf` or `anyOf` validity. Invoke
only the selected member codec. Propagate its errors, including nested errors,
instead of retrying a different member or falling back to raw.

## Backend compatibility

Rust's reqwest, ureq, and aioduct adapters can share a native codec implementation.
Ordinary Rust unions recover a proven required string literal separately for
each aligned authored branch when the model predicate retains only `String`.
This supplements legacy selection without changing the model, raw passthrough,
or the matching policy for other constraints.

Python's httpx and requests adapters should use the same selection for direct
model helpers and nested values. Java and Kotlin can retain their native model
and Gson adapter representations. These are adoption requirements, not claims
that those backends already implement the contract.

TypeScript's [`preserve` mode](ts_config.md) can retain converter-free declarations
by default; a future explicit runtime selection option must not be silently
ignored.

Go uses typed `FirstMatch` for generalized inferred internal tags by default.
The `dynamic` compatibility option keeps the former `any` representation;
it is neither typed dispatch nor lossless raw JSON, particularly for large
numbers. Lossless `Raw` preserves the JSON subtree without decoding it into
Go model values.

## Validation

Test lowering and selection in-process across OpenAPI versions. Run the same
anonymized JSON cases through native consumers of each adopting backend to verify
branch selection, errors, and serialization. Include exact multi-valued tags,
absent versus null tags, overlapping payloads, referenced `allOf` tag ownership,
object-local extra-property checks, and nested or recursive use sites.

The shared contract matrix tests wire behavior; golden comparisons and generated
client builds separately test output stability and compilability. The planner
matrix alone does not establish backend conformance. See
[Golden Testing](golden_testing.md) for native consumer orchestration.
