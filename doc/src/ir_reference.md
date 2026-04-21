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

## Type Expressions

`IrTypeExpr` represents type references throughout the IR:

```rust
pub enum IrTypeExpr {
    Named(String),
    Primitive(IrPrimitive),
    Array(Box<IrTypeExpr>),
    Map(Box<IrTypeExpr>),
    Nullable(Box<IrTypeExpr>),
    Union(Vec<IrTypeExpr>),
    Literal(serde_json::Value),
    Unknown,
}
```

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
