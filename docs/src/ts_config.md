# TypeScript Generator Configuration

## Full Example

```toml
[generators.typescript-fetch]
file_naming_convention = "PascalCase"
package_scope = "@myorg"
package_name = "my-api-client"
generate_package = true
ts_target = "ES2020"
ts_module = "ES2020"
ts_lib = ["ES2020", "DOM"]
generate_esm_config = true
include_build_scripts = true
emit_enum_constants = true
emit_type_guards = true
property_naming = "camelCase"
untagged_union_strategy = "firstMatch"
indent = "    "
```

## Options Reference

### `file_naming_convention`

Controls the file naming style. One of `"PascalCase"`, `"camelCase"`, `"kebab-case"`, `"snake_case"`. Defaults to `"PascalCase"`.

### `package_scope`

NPM package scope prefix, e.g. `"@myorg"`. If set, the generated `package.json` uses `"name": "@myorg/my-api-client"`.

### `package_name`

Override the generated package name. Defaults to the spec title converted to kebab-case.

### `generate_package`

Whether to generate npm package files (`package.json`, `tsconfig.json`, `tsconfig.esm.json`). Defaults to `true`.

### `ts_target`

TypeScript compiler target. Defaults to `"ES2020"`.

### `ts_module`

TypeScript module system. One of `"commonjs"`, `"ES2020"`, `"ES2022"`, `"ESNext"`. Defaults to `"ES2020"`.

### `ts_lib`

TypeScript compiler lib array. Accepts a TOML array or comma-separated string. Defaults to `["ES2020", "DOM"]`.

### `generate_esm_config`

Whether to generate an ESM tsconfig (`tsconfig.esm.json`). Defaults to `true`.

### `include_build_scripts`

Whether to include build scripts in `package.json`. Defaults to `true`.

### `emit_enum_constants`

When `true`, emits a companion const object alongside each enum type alias:

```typescript
export type ItemKind = 'BOOK' | 'MOVIE' | 'MUSIC';

export const ItemKind = {
    BOOK: 'BOOK' as const,
    MOVIE: 'MOVIE' as const,
    MUSIC: 'MUSIC' as const,
};
```

Consumers can use the same import for both type annotations and runtime value comparisons:

```typescript
import { ItemKind } from "@scope/package";

function getLabel(kind: ItemKind) {
    switch (kind) {
        case ItemKind.BOOK: return "Book";
        // ...
    }
}
```

Handles string, integer, number, and mixed-value enums. Quotes keys that aren't valid JavaScript identifiers. Defaults to `false`.

### `emit_type_guards`

When `true`, emits `is*` type guard functions alongside each tagged union type alias:

```typescript
export type Shape = ({ kind: 'circle' } & Circle) | ({ kind: 'rectangle' } & Rectangle);

export function isCircle(value: Shape): value is ({ kind: 'circle' } & Circle) {
    return value.kind === 'circle';
}

export function isRectangle(value: Shape): value is ({ kind: 'rectangle' } & Rectangle) {
    return value.kind === 'rectangle';
}
```

The narrowing expression depends on the tagging style:
- **Internal/Adjacent**: `value.<field> === '<value>'`
- **External**: `'<value>' in value`

Variants with "UNSPECIFIED" in the discriminator value, discriminator values equal to the field name, or bare string-literal content types are skipped. Defaults to `false`.

### `toolchain`

Selects the type checking and build toolchain. One of `"tsc"` (default) or `"vp"`.

- `"tsc"` — bare TypeScript compiler. Generates `tsconfig.json` only. Build script: `tsc`.
- `"vp"` — vite-plus. Generates `vite.config.ts` with `pack` config for library output (ESM + .d.ts). Build script: `vp pack`. Check script: `vp check --no-fmt`.

When `toolchain = "vp"`, the generated `package.json` includes `vite-plus` and `typescript` as dev dependencies, and scripts use `vp pack` (library mode) rather than `vp build` (app mode).

```toml
[generators.typescript-fetch]
toolchain = "vp"
```

### `indent`

Controls the indentation string used in generated TypeScript files. Accepts any string; common values are `"  "` (2 spaces, default), `"    "` (4 spaces), or `"\t"` (tabs).

```toml
[generators.typescript-fetch]
indent = "    "
```

### `property_naming`

Controls the naming convention for properties in generated interfaces. When set to `"camelCase"`, generates dual-type model files:

- A `Name$Wire` interface preserving the original wire-format property names (snake_case, kebab-case, etc.)
- A `Name` interface with camelCase property names for ergonomic usage
- `nameFromJSON(json: Name$Wire): Name` converter function
- `nameToJSON(value: Name): Name$Wire` converter function

```toml
[generators.typescript-fetch]
property_naming = "camelCase"
```

Example output for a schema with snake_case properties:

```typescript
export interface User$Wire {
  readonly user_id: number;
  readonly first_name: string;
  readonly last_name: string;
}

export interface User {
  readonly userId: number;
  readonly firstName: string;
  readonly lastName: string;
}

export function userFromJSON(json: User$Wire): User {
  return {
    userId: json.user_id,
    firstName: json.first_name,
    lastName: json.last_name,
  };
}

export function userToJSON(value: User): User$Wire {
  return {
    user_id: value.userId,
    first_name: value.firstName,
    last_name: value.lastName,
  };
}
```

Works with all schema kinds:
- **Objects**: dual interfaces + converters
- **Tagged unions** (internal, adjacent, external): dual type aliases + switch/if-chain converters
- **Intersections** (allOf): dual type aliases; compatible open objects share one
  conversion plan, while other intersections use spread-based converters
- **Untagged unions** (oneOf/anyOf): dual type aliases + member-selecting converters

Referenced types that are themselves convertible (objects, intersections, unions) get their converter functions called recursively. Enums and simple aliases pass through unchanged.

Defaults to `"preserve"` (no renaming, single interface, no converters).

### `untagged_union_strategy`

Controls untagged-union conversion when `property_naming = "camelCase"`:

- `"firstMatch"` (default): select the first structurally matching member and
  call its converter. Concrete members precede unconstrained JSON/object/map
  fallbacks, with schema order preserved within each group. Unmatched values
  pass through unchanged rather than making imperfect specifications unusable.
- `"strict"`: use the same structural selection, but throw `TypeError` for
  multiple concrete matches or no matching member. A raw fallback is considered
  only after the concrete members, so it does not itself create ambiguity.
- `"raw"`: do not select or convert union members. Both `Name` and `Name$Wire`
  expose the wire representation; pass original wire keys such as `input_schema`,
  not ergonomic keys such as `inputSchema`. This is an explicit escape hatch,
  not an identity cast pretending to return a renamed model.

```toml
[generators.typescript-fetch]
property_naming = "camelCase"
untagged_union_strategy = "strict"
```

The same choice is available through a CLI override:

```sh
openapi-nexus generate --input openapi.yaml --generators typescript-fetch \
  --generator-config typescript-fetch.property_naming=camelCase \
  --generator-config typescript-fetch.untagged_union_strategy=strict
```

Selection checks JSON kinds, required own fields, declared property shapes, and
literal/enum values in the appropriate wire or ergonomic naming direction.
Arrays and dictionaries inspect their element/value shapes; dictionary keys and
external variant tags stay verbatim. This is conversion dispatch, not full JSON
Schema validation: formats, numeric bounds, and other validation keywords are
not checked. Recursive references stop expanding the selection guard at a cycle;
the selected member converter still handles recursive payload conversion.

For example, a mixed `ClientTool | SearchTool` union can distinguish a required
`input_schema` / `inputSchema` field from a search member's required literal
`type`, then convert `inputSchema` and `maxUses` to `input_schema` and `max_uses`.
Conversions also work through containing arrays, dictionaries, and model aliases.
Declared additional-property behavior remains owned by the selected member's
converter. Tagged-union dispatch and preserve-mode generation are unchanged.

### Additional properties

Objects declaring `additionalProperties` expose a readonly string dictionary in
both naming modes, including objects that also declare named fields. Typed map
values keep their model types; arbitrary JSON values use `unknown`.

TypeScript's string index covers named fields too. For mixed schemas, the index
value type therefore includes the additional-value type, every declared field
type, and `undefined` when a declared field is optional. Named fields retain their
more precise types. For example, an optional known string plus string extensions
extends `Readonly<Record<string, string | undefined>>`.

In camelCase mode, both converters preserve extension keys verbatim. Typed model
values are converted recursively, including arrays, nullable values, and nested
maps; opaque JSON contents are not renamed. Omitted optional fields stay absent
on these open objects. Object/map `allOf` composition and open tagged variants
also retain their extensions. `additionalProperties: false` and unspecified
additional-property behavior are unchanged.

Compatible `allOf` dictionary contracts are narrowed together (for example,
`true` with `string` uses string values), and named fields are planned across
all members before conversion. This includes JSON unions that render as `unknown`:
intersecting an arbitrary-JSON dictionary with a model dictionary converts the
additional values as that model, without treating other members' named fields as
extensions. Their public intersection aliases retain the original member types
inside `Readonly<...>` in preserve, ergonomic, and wire declarations.
Aliases of models, arrays, maps, and nullable
values receive matching wire aliases and converters, including alias chains.
External tag keys stay verbatim while both the referenced payload and any typed
outer extension values are converted; each closed alternative remains closed.

An extension can collide with a renamed field: `{known_field: 'fixed',
knownField: 'extension'}` must still expose `value.knownField === 'fixed'`. The
converter retains the colliding extension under the enumerable symbol
`Symbol.for('openapi-nexus.additionalProperties')`; `nameToJSON` restores it.
Object spread preserves this metadata. Serialize through `nameToJSON`, not
`JSON.stringify(value)` or a JSON-based clone, which omits symbol metadata. Use
`Name$Wire` or `property_naming = "preserve"` when direct access to all original
wire keys is preferable.

## Barrel Exports

When either feature is enabled, `models/index.ts` automatically emits value re-exports:

```typescript
// Enum const: single line covers both type and value
export { ItemKind } from './ItemKind';

// Tagged union: separate type and guard re-exports
export type { Shape } from './Shape';
export { isCircle, isRectangle } from './Shape';
```
