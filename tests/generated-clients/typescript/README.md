# TypeScript generated-client consumers

```sh
just golden-typescript-clients::test
```

Requires Rust, Node.js `^22.18.0 || ^24.11.0 || >=26.0.0`, and Vite+ 1.5.0
installed alongside TypeScript 7.0.2:

```sh
npm install -g typescript@7.0.2 vite-plus@1.5.0
```

The current global Vite+ CLI resolves the TypeScript API beside the CLI when
packing declarations, so the generated package's compiler alone is insufficient.
The strict consumer check itself uses that package's installed compiler.

- `prepare` runs the public generator CLI with an explicit empty configuration,
  producing fresh, unmodified preserve and camelCase clients.
- `build` copies each client to a temporary build directory, runs `vp install`,
  reports the resolved Vite+ component versions with `vp --version`, runs
  `vp check --no-fmt` and `vp pack`, then copies its bundle/declarations back.
  Building outside the repository avoids Vite+ skipping gitignored generated
  sources. Generated source, manifests, and configuration are never patched.
- `typecheck.ts` checks the actual packed declarations with strict typing and
  `exactOptionalPropertyTypes`, including negative dictionary-type and readonly
  `allOf` assertions for preserve, ergonomic, and wire declarations.
- `tests/*.test.mjs` uses Node's native test runner against the packed camelCase
  client. It asserts full payload equality in both JSON conversion directions,
  typed/nested/recursive dictionaries, JSON/model allOf composition, search-event variants,
  omitted fields, collisions with renamed fields, and prototype-sensitive keys.

Generation and build orchestration stays in just/CI, not in the tests. After a
build, the runtime suite can be rerun directly:

```sh
node --test tests/generated-clients/typescript/tests/*.test.mjs
```

CI runs this in the existing TypeScript/Vite+ golden-build job, after its snapshot
builds, reusing the installed toolchain and generator cache. No separate CI job is
needed. `just ci` also includes these consumer checks.
Default preserve mode has no JSON converters: its packed public dictionary types
are checked, but it is not presented as a second conversion-path runtime test.
