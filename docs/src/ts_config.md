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

## Barrel Exports

When either feature is enabled, `models/index.ts` automatically emits value re-exports:

```typescript
// Enum const: single line covers both type and value
export { ItemKind } from './ItemKind';

// Tagged union: separate type and guard re-exports
export type { Shape } from './Shape';
export { isCircle, isRectangle } from './Shape';
```
