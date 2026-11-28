# Go generated-client consumers

Run `just golden-go::test` to generate the SDKs and execute all registered Go
consumer modules in a fresh temporary tree. `just golden-go::build` includes
these tests before compiling goldens. `just golden-go::prepare` only refreshes
`generated/` for checkout-local use; validation leaves that directory alone.

[The fixture manifest](fixtures.txt) lists each consumer directory, its input
schema, and any generator override. The consumer directory name also identifies
its SDK under `generated/`; each consumer's `go.mod` uses that relative path.
The tests do not invoke tools or other runners, and generated source and module
manifests are not patched before execution.

The composed consumer checks flat object/map allOf payloads, original embedded
member entrypoints, named and anonymous dictionaries, nested aliases, repeated
members, storage collisions, native errors, containing models, HTTP request/success/
lazy error bodies and raw fallbacks.
The raw consumer checks exact numeric literals, nulls and non-object JSON kinds.
Projection tests compare native member order, case-insensitive matching and
duplicate-key failures. The nullable consumer compiles nested nullable members and
checks raw null/object preservation through aliases, nested compositions and reuse.
The tagged consumers cover authored variants and the default, strict, raw, and
dynamic policies for inferred tagging.
