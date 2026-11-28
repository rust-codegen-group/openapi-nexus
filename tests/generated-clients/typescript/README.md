# TypeScript generated-client consumers

```sh
just golden-typescript-clients::test
```

Requires Rust, Node.js `^22.18.0 || ^24.11.0 || >=26.0.0`, and Vite+ 1.6.0
installed alongside TypeScript 7.0.2:

```sh
npm install -g typescript@7.0.2 vite-plus@1.6.0
```

The current global Vite+ CLI resolves the TypeScript API beside the CLI when
packing declarations, so the generated package's compiler alone is insufficient.
The strict consumer check itself uses that package's installed compiler.

[The fixture plan](fixtures.txt) records each SDK's source spec and generator
options once, and marks it `pack` or `source`. All cases use the explicit
[repository test configuration](../config.toml). Consumer imports still select
which declarations, bundles, and source graphs the contracts exercise.

- `prepare` refreshes the stable `generated/` directory for IDE use. It only
  generates SDKs; it does not install dependencies or produce bundles.
- `build` generates into a fresh temporary tree outside the checkout. Each
  `pack` case runs `vp install`, `vp --version`, `vp check`, `vp pack`, then
  `vp check` again. The strict declaration contracts use an installed package's
  local compiler. The `source` case takes a different route: the compiler
  builds only the graph selected by the isolation consumer, without packing it.
- `test` performs the same build, then runs the native Node tests against its
  bundles and compiled isolation consumer. Both validation commands clean up
  their own tree and leave the IDE output alone. No generated files are patched
  or formatted by the validation recipes.
- `typecheck.ts` checks the actual packed declarations with strict typing and
  `exactOptionalPropertyTypes`, including negative dictionary-type and readonly
  `allOf` assertions for preserve, ergonomic, and wire declarations, as well as
  ergonomic versus raw-wire union contracts and readonly union arrays.
- `typecheck.tagged-allof.ts` checks required variant fields, recursive members,
  nested configuration payloads, readonly values, type guards, and the
   distinct ergonomic/wire/preserve declarations of composed tagged variants.
   Overlapping views retain both nested required fields and authored-name collisions.
   Optional and multi-valued tag declarations accept every legal wire shape.
- `tests/*.test.mjs` uses Node's native test runner against the packed camelCase
  client. It asserts full payload equality in both JSON conversion directions,
  typed/nested/recursive dictionaries, JSON/model allOf composition, tagged event variants,
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
  Tagged allOf cases independently assert both directions for neutral variants,
  recursive members, nested configuration and result payloads, multiple references, inline-only
   payloads, fields beside the tag or allOf, and unchanged ordinary/unit controls.
   Overlapping allOf cases retain recursive objects, nullable/optional fields,
   arrays and maps while leaving opaque JSON subtrees and dictionary keys alone.
   Optional/multi-valued discriminators independently cover omitted tags,
   explicit variant tags and both referenced enum values in both JSON directions.
- `isolation-consumer.ts` is an ordinary consumer of one generated model. Its
  native `tsc` project and Node test prove helper imports do not pull unrelated
  model graphs into that consumer. The separate isolation fixture intentionally
  contains an unsupported same-value circular alias: only the selected usable
  graph is compiled here, not its entire barrel. `pack` cases still receive
  their full Vite+ checks and are consumed through their packed outputs.

Generation, compilation, and invocation of the native runner stay in just/CI.

CI runs this in the existing TypeScript/Vite+ golden-build job, after its snapshot
builds, reusing the installed toolchain and generator cache. No separate CI job is
needed. `just ci` also includes these consumer checks.
Default preserve mode has no JSON converters: its packed public dictionary types
are checked, but it is not presented as a second conversion-path runtime test.
