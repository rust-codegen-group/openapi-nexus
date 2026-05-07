# Rust Generator Configuration

All three Rust backends (`rust-reqwest`, `rust-ureq`, `rust-aioduct`) share the same configuration options.

## Full Example

```toml
[generators.rust-reqwest]
crate_name = "my-api-client"
workspace_mode = true
workspace_deps = "workspace_version"

[generators.rust-reqwest.extra_derives.structs]
derives = ["PartialEq", "Eq"]

[generators.rust-reqwest.extra_derives.enums]
derives = ["Hash"]

[generators.rust-reqwest.extra_derives.unions]
derives = ["PartialEq"]

[generators.rust-reqwest.extra_derives.response_structs]
derives = ["PartialEq"]

[generators.rust-reqwest.extra_derives.per_type.MySpecialSchema]
derives = ["Default"]

[generators.rust-reqwest.utoipa]
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
- `unions` — tagged unions (external tagging)
- `response_structs` — per-operation response type wrappers

```toml
[generators.rust-reqwest.extra_derives.structs]
derives = ["PartialEq", "Eq"]
dependencies = { fake = '"2"' }
```

The `dependencies` field adds entries to the generated `Cargo.toml`.

### `extra_derives.per_type`

Target a specific schema by name:

```toml
[generators.rust-reqwest.extra_derives.per_type.UserProfile]
derives = ["Default", "Hash"]
```

### `utoipa`

Native [utoipa](https://github.com/juhaku/utoipa) integration for OpenAPI schema generation at runtime.

```toml
[generators.rust-reqwest.utoipa]
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
