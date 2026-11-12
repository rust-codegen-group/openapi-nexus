# Go generated-client consumers

Run `just golden-go::test` to generate fresh default/raw/nullable clients and
execute these ordinary Go tests. `just golden-go::build` includes them before
compiling goldens.
The tests do not invoke generators, compilers or other runners, and generated
source and module manifests are not patched before execution.

The composed consumer checks flat object/map allOf payloads, original embedded
member entrypoints, named and anonymous dictionaries, nested aliases, repeated
members, storage collisions, native errors, containing models, HTTP request/success/
lazy error bodies and raw fallbacks.
The raw consumer checks exact numeric literals, nulls and non-object JSON kinds.
Projection tests compare native member order, case-insensitive matching and
duplicate-key failures. The nullable consumer compiles nested nullable members and
checks raw null/object preservation through aliases, nested compositions and reuse.
