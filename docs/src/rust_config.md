# Rust Generator Configuration

All three Rust backends (`rust-reqwest`, `rust-ureq`, `rust-aioduct`) share the same configuration options.

The `rust-aioduct` backend has an additional `[aioduct]` section for controlling aioduct-specific features.

## Generated Error Handling

Every operation keeps its generated error enum, so callers can match a specific status and use its typed body:

```rust
match api.create_resource(&request).await {
    Err(CreateResourceError::NotFound(error)) => {
        if let Ok(body) = error.body() {
            println!("missing resource: {}", body.message);
        }
    }
    Err(error) => return Err(error.into()),
    Ok(response) => use_response(response),
}
```

Each operation error also implements `Into<ApiCallError>`. Use the common type when a workflow calls multiple operations and does not need their distinct body types:

```rust
use generated_sdk::runtime::error::ApiCallError;

async fn run_workflow(
    api: &ResourcesApi<'_>,
    request: &CreateResourceRequest,
    resource_id: &str,
) -> Result<(), ApiCallError> {
    api.create_resource(request).await?;
    api.refresh_resource(resource_id).await?;
    Ok(())
}
```

`ApiCallError` exposes the operation identifier (generated from the HTTP method and path if the OpenAPI document omits one) and optional HTTP status, native headers, and raw response body. Transport errors and error-body decoding failures remain available through `std::error::Error::source()`.

Conversion consumes the operation-specific error, performs one heap allocation
for the pointer-sized erased representation, and erases its decoded payload
type. Operation-specific errors remain inline until conversion. Match the
operation error before conversion when its decoded payload is needed.

### `thiserror`

`thiserror` does not chain two `From` conversions for `?`. A single generic adapter on the application error converts every generated operation error directly:

```rust
#[derive(Debug, thiserror::Error)]
#[error("SDK request failed: {source}")]
struct AppError {
    #[source]
    source: ApiCallError,
}

impl<E> From<E> for AppError
where
    E: Into<ApiCallError>,
{
    fn from(error: E) -> Self {
        Self {
            source: error.into(),
        }
    }
}
```

Do not also put `#[from]` on the `ApiCallError` field; the generic implementation already includes `ApiCallError` itself.

### SNAFU

SNAFU 0.9's generic source conversion provides the same direct `?` behavior:

```rust
#[derive(Debug, snafu::Snafu)]
#[snafu(context(false))]
struct AppError {
    #[snafu(source(from(generic)))]
    source: ApiCallError,
}
```

## Untagged Union Deserialization

Untagged unions keep their existing typed variants and untagged serialization.
Deserialization selects members using the shared IR's JSON shapes, required wire
keys, finite scalar values (including numeric and boolean enums), nested arrays
and maps, and productive recursive references. This prevents an earlier member
with an unconstrained Rust integer field from swallowing a later member's payload.
Repeated recursive checks reuse results within one member-selection pass, without
changing candidate order or sharing cached results across decoded values.

### `untagged_union_strategy`

| Value | Generated API and behavior |
| --- | --- |
| `"firstMatch"` (default) | Keep typed variants; select the first structural match in schema order, preferring concrete members to opaque fallbacks. |
| `"strict"` | Keep the same typed API; reject multiple concrete matches before deserializing any member. Use an opaque fallback only when no concrete member matches. |
| `"raw"` | Explicitly replace each untagged union with an untagged enum containing only `Raw(serde_json::Value)`. Preserve known, ambiguous, unmatched, and null JSON without typed selection. |

Both typed policies reject unmatched values. After selection, the chosen member
is deserialized once; its errors (including nested strict-union errors) propagate
instead of being hidden by another member or an opaque fallback. Structural
matching is not a full JSON Schema validator: formats, bounds, and other validation
keywords are not selection rules, and native Serde decoding can still reject a
selected payload. Serialization does not add schema validation.

The typed enum does not gain synthetic `Raw` or `Null` variants. Explicitly nullable
use-site expressions continue to use `Option<Union>`; decode a standalone nullable
union through `Option<Union>` as well. As before, a named union's root nullable
flag alone does not change its Rust enum or automatically wrap references to it.
Explicit `raw` changes the union's Rust API, so callers
construct and match `Union::Raw(value)` rather than typed variants. It does not
change tagged unions or turn ordinary models into raw JSON.

```toml
[rust-reqwest]
untagged_union_strategy = "strict"
```

The same setting works for `rust-ureq` and `rust-aioduct`, or as a CLI override:

```sh
openapi-nexus generate --input api.yaml --output sdk --generators rust-reqwest \
  --generator-config rust-reqwest.untagged_union_strategy=raw
```

Typed decoding retains the selected member's declared payload, not arbitrary
unknown fields. Dictionary/additional-property representation and multi-member
`allOf` behavior are unchanged. Choose `raw` when the full JSON must be retained.

## Full Example

```toml
[rust-reqwest]
crate_name = "my-api-client"
workspace_mode = true
workspace_deps = "workspace_version"
untagged_union_strategy = "firstMatch"

[rust-reqwest.extra_derives.structs]
derives = ["PartialEq", "Eq"]

[rust-reqwest.extra_derives.enums]
derives = ["Hash"]

[rust-reqwest.extra_derives.unions]
derives = ["PartialEq"]

[rust-reqwest.extra_derives.response_structs]
derives = ["PartialEq"]

[rust-reqwest.extra_derives.per_type.MySpecialSchema]
derives = ["Default"]

[rust-reqwest.utoipa]
enabled = true
dependency = '{ version = "5" }'
```

## Options Reference

### `crate_name` / `package_name`

Override the generated crate name. Defaults to the spec title converted to kebab-case.

```toml
crate_name = "my-api-client"
```

### `workspace_mode`

When `true`, the generated `Cargo.toml` uses `version.workspace = true` and `edition.workspace = true` instead of inline values. Also emits `[lints] workspace = true`.

```toml
workspace_mode = true
```

### `workspace_deps`

Controls how dependencies are declared in the generated `Cargo.toml`.

| Mode | Behavior |
|------|----------|
| `"explicit"` (default) | Inline version specs: `serde = { version = "1", features = ["derive"] }` |
| `"workspace_version"` | Workspace with features: `serde = { workspace = true, features = ["derive"] }` |
| `"full"` | Fully delegated: `serde.workspace = true` |

```toml
workspace_deps = "workspace_version"
```

### `extra_derives`

Add custom derive macros to generated types. Each category targets a different schema kind:

- `structs` — object schemas
- `enums` — string and integer enums
- `unions` — tagged and untagged unions (including explicit raw wrappers)
- `response_structs` — per-operation response type wrappers

```toml
[rust-reqwest.extra_derives.structs]
derives = ["PartialEq", "Eq"]
dependencies = { fake = '"2"' }
```

The `dependencies` field adds entries to the generated `Cargo.toml`.
For untagged unions, the emitter owns Serde implementations and, when enabled,
utoipa implementations. Equivalent short, crate-qualified, and absolute derive
paths (such as `Deserialize`, `serde::Deserialize`, and `::serde::Deserialize`)
do not add conflicting implementations.

### `extra_derives.per_type`

Target a specific schema by name:

```toml
[rust-reqwest.extra_derives.per_type.UserProfile]
derives = ["Default", "Hash"]
```

### `utoipa`

Native [utoipa](https://github.com/juhaku/utoipa) integration for OpenAPI schema generation at runtime.

```toml
[rust-reqwest.utoipa]
enabled = true
dependency = '{ version = "5" }'
```

When enabled:

- Structs, string enums, integer enums, intersections, and aliases get `#[derive(utoipa::ToSchema)]`
- Tagged unions (internal/adjacent) and untagged unions get manual `impl utoipa::PartialSchema + ToSchema` using `OneOfBuilder` (the derive macro doesn't support these patterns)
- The `utoipa` crate is added to generated `Cargo.toml` using the `dependency` spec
- Variant schemas for internal/adjacent tagged unions are emitted as standalone files (not inlined) so they can be referenced by `PartialSchema`

The `dependency` field accepts any valid TOML inline table or string that would appear after `utoipa = ` in Cargo.toml. If omitted, defaults to `"*"`.

You do NOT need to add `utoipa::ToSchema` to `extra_derives` when using this config. The `[utoipa]` section handles everything, including the cases where the derive macro cannot be used.

### `aioduct` (rust-aioduct only)

Configure aioduct-specific dependency features for the generated crate. This section only applies to the `rust-aioduct` generator.

```toml
[rust-aioduct.aioduct]
version = "0.2"
runtime = "tokio"
tls = "rustls-ring"
compression = ["gzip", "brotli", "zstd"]
features = ["tracing", "http3"]
```

All fields are optional. Defaults: `runtime = "tokio"`, `tls = "rustls-ring"`, no compression, no extra features. The `json` feature is always included.

#### `version`

Override the aioduct version requirement. Defaults to `"0.2"`.

#### `runtime`

Which async runtime to use. One of:

| Value | Description |
|-------|-------------|
| `"tokio"` (default) | Tokio runtime |
| `"smol"` | smol runtime |
| `"compio"` | compio (io_uring) runtime |

#### `tls`

TLS backend selection:

| Value | Description |
|-------|-------------|
| `"rustls-ring"` (default) | rustls with ring crypto |
| `"rustls-aws-lc-rs"` | rustls with AWS-LC crypto |
| `"false"` | Disable TLS (HTTP-only) |

#### `compression`

List of decompression codecs to enable. Valid values: `"gzip"`, `"brotli"`, `"zstd"`, `"deflate"`.

```toml
compression = ["gzip", "zstd"]
```

#### `features`

Pass-through feature flags appended to the aioduct dependency. Use this for features not covered by the structured fields above (e.g., `"tracing"`, `"otel"`, `"http3"`, `"hickory-dns"`, `"doh"`, `"dot"`, `"blocking"`, `"tower"`).

```toml
features = ["tracing", "http3", "blocking"]
```
