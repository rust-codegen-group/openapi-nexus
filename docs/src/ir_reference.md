# IR Reference

The Intermediate Representation (`IrSpec`) is the version-agnostic data structure that generators consume. The lowering pass resolves all `$ref` references, classifies schemas, and normalizes operations into a flat list.

## IrSpec

```rust
pub struct IrSpec {
    pub info: IrInfo,
    pub servers: Vec<IrServer>,
    pub schemas: IndexMap<String, IrSchema>,
    pub operations: Vec<IrOperation>,
    pub security_schemes: IndexMap<String, IrSecurityScheme>,
    pub security: Vec<IrSecurityRequirement>,
}
```

All maps use `IndexMap` for deterministic iteration order.

## Schemas

Each `IrSchema` has a `name`, optional `description`, and a classified `kind`:

```rust
pub enum IrSchemaKind {
    Object(IrObject),
    Enum(IrEnum),
    TaggedUnion(IrTaggedUnion),
    Union(IrUnion),
    Intersection(IrIntersection),
    Alias(IrTypeExpr),
}
```

### Object

An object with named properties and optional additional properties:

```rust
pub struct IrObject {
    pub properties: IndexMap<String, IrProperty>,
    pub additional_properties: Option<IrTypeExpr>,
}
```

Each `IrProperty` carries `type_expr`, `required`, `nullable`, optional `description`, `default_value`, `format`, and `validation`.

### Enum

A typed enumeration with string, integer, number, or mixed values:

```rust
pub struct IrEnum {
    pub value_type: IrEnumValueType,
    pub values: Vec<IrEnumValue>,
}
```

### TaggedUnion

A discriminated union (oneOf with a discriminator property):

```rust
pub struct IrTaggedUnion {
    pub discriminator_property: String,
    pub style: TaggedUnionStyle,
    pub variants: Vec<IrTaggedVariant>,
}
```

Styles: `InternallyTagged`, `ExternallyTagged`, `AdjacentlyTagged`, `Untagged`.

### Union

An untagged union (oneOf/anyOf without a discriminator):

```rust
pub struct IrUnion {
    pub members: Vec<IrTypeExpr>,
}
```

### Intersection

An allOf intersection:

```rust
pub struct IrIntersection {
    pub members: Vec<IrTypeExpr>,
}
```

### Alias

A type alias wrapping a single type expression (e.g., from a `$ref`):

```rust
Alias(IrTypeExpr)
```

OpenAPI 3.1/3.2 boolean component schemas are schema documents, not boolean-valued
primitives. A `true` component lowers to `Alias(IrTypeExpr::Any)`; `{type: boolean}`
continues to lower to `Alias(IrTypeExpr::Primitive(IrPrimitive::Boolean))`.
The parser retains a `false` component (which rejects every value). Lowering
omits that component from the IR and generated models when no retained model or
operation references it, so an unused declaration cannot block a usable client.
Ordinary components and named aliases remain public models even when no operation
uses them; a reference to `false` from any such model still requires an
uninhabited/bottom type and produces an explicit error naming the component.
Retained model constraints are checked before tagged/composition lowering can
consume discriminator fields. Operation references are also checked in the final
IR, including nested containers, request and response bodies, response headers,
and 3.2 streaming item types. Overridden parameters that leave no reference in
the final IR do not force an error.
`false` must never be silently widened to `Any`.

Dependency collection and support policy are separate. The crate-private
`SchemaDependencies` interface returns borrowed target names, deduplicated in
first-use order. Its native schema adapters collect structural dependencies before
projection; its IR adapter collects retained named-type dependencies afterward.
It does not resolve aliases or decide whether a dependency is valid or supported.
The boolean-component policy consumes those facts without owning schema/IR
traversal. This separation does not introduce a bottom type or change composition
normalization.

For raw-spec integrations, the 3.1/3.2 `Components.schemas` maps contain `Schema`
values. Match `Schema::Boolean` / `Schema::Object`, and use `Schema::resolve` or
`Schema::from_ref` to retain boolean targets through references. The object-only
`ObjectSchema::from_ref` API reports `ErrorRef::BooleanSchema` for either boolean
target. OpenAPI 3.0 component parsing and other schema positions are unchanged;
this is not full support for boolean schemas at every JSON Schema position.

## Type Expressions

`IrTypeExpr` represents type references throughout the IR:

```rust
pub enum IrTypeExpr {
    Named(String),
    Primitive(IrPrimitive),
    StringLiteral(String),
    StringEnum(Vec<String>),
    ScalarEnum {
        representation: Box<IrTypeExpr>,
        representation_is_constraint: bool,
        representation_nullable: bool,
        values: Vec<serde_json::Value>,
    },
    Array(Box<IrTypeExpr>),
    Map(Box<IrTypeExpr>),
    Nullable(Box<IrTypeExpr>),
    Union(Vec<IrTypeExpr>),
    Any,
}
```

Nullability belongs to the node that declares it. OpenAPI 3.0 `nullable: true`
and OpenAPI 3.1/3.2 type sets containing `null` retain that distinction in nested
arrays and maps:

| Schema meaning | IR shape |
| --- | --- |
| Map whose values may be null | `Map(Nullable(T))` |
| Array whose items may be null | `Array(Nullable(T))` |
| Array that may itself be null | `Nullable(Array(T))` |

Array aliases and references to promoted inline items preserve these wrappers.
Inline `additionalProperties` unions also use ordinary schema promotion in every
supported OpenAPI version. Their owner-scoped named schemas retain the same
alternatives, inline object members, and nullable-union facts as an equivalent
component reference. Arrays and maps keep their container shape while carrying
that naming context to nested union values; recursive `$ref` edges stay named.
Mixed OpenAPI 3.1/3.2 type sets retain every branch when no composition
assertion owns the node: promoting dictionary values in `[object, string]` does
not discard the string alternative, and array branches can carry the same
context. Object branches with properties or required fields are promoted as
named object members of that union, rather than replacing the union. When
`type` is combined with `oneOf`, `anyOf`, or `allOf`, the composition remains the
owning IR kind instead of being replaced by a terminal type-set alias; the
type-set planner is not applied across that composition boundary. Container
promotion keeps finite-enum constraints at the node that declared them,
including nullable arrays constrained to null only.

Synthetic schema names reserve the shared canonical model-word identity of all
authored components before allocation, avoiding case/separator collisions without
renaming authored models. Target-specific escaping and auxiliary artifact names
remain generator decisions. Kotlin's generator-owned symbol planner applies the
same reservation rule to Gson adapters and other package-local union declarations.
HTTP(S) references are rejected only in semantic branches consumed by lowering;
ignored siblings such as enum-displaced properties are not traversed. Local named
references remain terminal and are not expanded during that validation.

For object properties, the outer nullable wrapper is recorded in
`IrProperty.nullable`; nested wrappers remain in `type_expr`. A nullable
container does not make its contents nullable, and nullability is independent
of whether a property is required.

Inline scalar enum facts survive all three lowerers, including property, array
item, and dictionary value positions. `ScalarEnum` retains finite values together
with the pre-existing `representation`; existing string literal/enum expressions
already carry their values. TypeScript uses these facts for structural union
selection. Other generators consume the retained representation without changing
their enum policy. Rust callers exhaustively matching `IrTypeExpr` must account
for this variant.

Finite-value constraints apply to the original node: a nullable type with
`enum: [1]` still excludes null during selection, while `enum: [1, null]` permits
it. Extracting `IrProperty.nullable` does not widen that constraint. The
`representation_nullable` fact records a removed nullable wrapper at its exact
enum boundary: an outer enum intersects the inner constraints without moving
null permission across another enum. It defaults to false when constructing a
new `ScalarEnum` whose complete nullable representation is still present.
Nested finite constraints are intersected rather than replaced by outer values.
`representation_is_constraint` distinguishes an authored type or constrained
wrapper from the legacy string fallback used for an enum with no declared type.
The latter keeps its existing emitted type, but numeric/boolean/null enum values
are matched directly rather than intersected with a synthetic string check.
The `representation()` view exposes the legacy type beneath scalar metadata for
generators that do not use finite values. OpenAPI 3.1/3.2 nullable `oneOf`/`anyOf`
wrappers retain enums attached to the wrapper, including inside arrays and maps.
TypeScript conversion restores an extracted nullable wrapper around its original
delegate; matching metadata does not imply that a nonnullable delegate accepts
null. Null already accepted by the inner union still reaches its strict policy.
Inner finite constraints remain inside the restored wrapper so null excluded by
that inner constraint can still be admitted exclusively by the outer wrapper.

OpenAPI 3.0 boolean `exclusiveMinimum` / `exclusiveMaximum` flags retain both
`true` and `false` in validation metadata. The public 3.0 `ObjectSchema` fields
use `ExclusiveBound` to preserve those booleans as well as the legacy numeric
form already accepted by the parser. The numeric form retains its previous
lowering behavior; OpenAPI 3.1/3.2 numeric-bound handling is unchanged.

## Operations

Each `IrOperation` represents one HTTP method + path:

```rust
pub struct IrOperation {
    pub operation_id: String,
    pub tags: Vec<String>,
    pub method: String,
    pub path: String,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub deprecated: bool,
    pub parameters: Vec<IrParameter>,
    pub request_body: Option<IrRequestBody>,
    pub responses: Vec<IrResponse>,
    pub security: Vec<IrSecurityRequirement>,
}
```

Parameters carry a `ParameterLocation` (`Query`, `Header`, `Path`, `Cookie`).

Request bodies map media types to `IrTypeExpr`:

```rust
pub struct IrRequestBody {
    pub required: bool,
    pub content: IndexMap<String, IrTypeExpr>,
}
```

Responses carry status code, description, content map, and headers.

## Security Schemes

```rust
pub enum IrSecurityScheme {
    ApiKey { name, location, description },
    Http { scheme, bearer_format, description },
    OAuth2 { flows, description },
    OpenIdConnect { open_id_connect_url, description },
    MutualTls { description },
}
```
