# TypeScript generated-client consumers

```sh
just golden-typescript-clients::test
```

Requires Rust, Node.js `^22.18.0 || ^24.11.0 || >=26.0.0`, and Vite+ 1.5.1
installed alongside TypeScript 7.0.2:

```sh
npm install -g typescript@7.0.2 vite-plus@1.5.1
```

The current global Vite+ CLI resolves the TypeScript API beside the CLI when
packing declarations, so the generated package's compiler alone is insufficient.
The strict consumer check itself uses that package's installed compiler.

- `prepare` runs the public generator CLI with an explicit empty configuration,
  producing fresh, unmodified preserve and camelCase clients plus untagged-union
  clients using the default, strict, and raw policies with tagged type guards.
- `build` copies each client to a temporary build directory, runs `vp install`,
  reports the resolved Vite+ component versions with `vp --version`, runs
  full `vp check` and `vp pack`, then checks again after packing and
  copies its bundle/declarations back.
  Building outside the repository avoids Vite+ skipping gitignored generated
  sources. Checks run directly on the original generated source, manifests,
  and configuration, without formatter rewrites or compatibility patches. The generated formatter
  configuration excludes `dist/`, so checks do not reformat build output.
- `typecheck.ts` checks the actual packed declarations with strict typing and
  `exactOptionalPropertyTypes`, including negative dictionary-type and readonly
  `allOf` assertions for preserve, ergonomic, and wire declarations, as well as
  ergonomic versus raw-wire union contracts and readonly union arrays.
- `tests/*.test.mjs` uses Node's native test runner against the packed camelCase
  client. It asserts full payload equality in both JSON conversion directions,
  typed/nested/recursive dictionaries, JSON/model allOf composition, search-event variants,
  omitted fields, collisions with renamed fields, and prototype-sensitive keys.
  Union cases check member and containing-request conversion independently in
  both directions, nullable aliases, arrays/maps, raw fallback priority,
  deterministic ambiguity, strict failures, literal checks, and own fields.
  Nested unions also retain inner strict errors; nullable/allOf fallbacks cannot
  shadow concrete members. Adjacent-tagged unit/payload cases, actual Blob/File
  members, and arbitrary-JSON aliases run in all relevant conversion policies.
  Inline array/dictionary member unions share the configured dispatch policy;
  mixed concrete/opaque open dictionaries retain nested conversion and strict
  errors. Single-member allOf aliases preserve nullable maps and arrays.
  Conversion contracts also cover moving a union through an alias/property,
  inline numeric enums, typed additional-value selection, recursive open
  dictionaries/lists, and raw subtrees inside converting parents. Tagged models
  named `Record` or `Readonly` retain their public converters; external tagged
  selection checks typed extensions, and nullable enums retain their finite
  constraints. HTTP tests exercise nested request/success/error containers,
  required/optional nullable bodies, and multipart JSON parts. Selection controls
  retain nested enum intersections, authored concrete null matches beside opaque
  fallbacks, untyped scalar enum values, and supplied-null ambiguity without
  confusing body omission. Enum metadata retains a typed nullable boundary around
  nonnullable delegates while preserving already-nullable inner strict errors.
  Nested finite constraints keep outer-only null permission consistent between
  extracted property wrappers and array items.
  Packed declarations pin
  raw multipart inputs, raw aliases, tagged guards, singleton dictionary readonly
  indexes, and the established readonly-outer/plain-inner nested array contract.
  Frozen nested inputs remain usable; static conversion/matching helpers do not
  leak into the public barrel.
- `isolation-consumer.ts` is an ordinary consumer of one generated model. Its
  native `tsc` project and Node test prove helper imports do not pull unrelated
  model graphs into that consumer. The separate isolation fixture intentionally
  contains an unsupported same-value circular alias: only the selected usable
  graph is compiled here, not its entire barrel. The five supported packages
  above still receive their full unmodified Vite+ checks and packed-declaration
  checks. Compilation stays in the just recipe, never inside a test program.

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
