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
- **Intersections** (allOf): dual type aliases + spread-based converters
- **Unions** (oneOf): dual type aliases + cast-through converters

Referenced types that are themselves convertible (objects, intersections, unions) get their converter functions called recursively. Enums and simple aliases pass through unchanged.

Defaults to `"preserve"` (no renaming, single interface, no converters).

## Barrel Exports

When either feature is enabled, `models/index.ts` automatically emits value re-exports:

```typescript
// Enum const: single line covers both type and value
export { ItemKind } from './ItemKind';

// Tagged union: separate type and guard re-exports
export type { Shape } from './Shape';
export { isCircle, isRectangle } from './Shape';
```
