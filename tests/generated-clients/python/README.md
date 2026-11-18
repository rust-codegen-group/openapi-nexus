# Native Python generated-client consumers

Generation and type checking are separate from the standard-library `unittest`
runner. Tests exercise generated dataclasses, aliases, policy behavior, and each
transport's actual JSON request encoder with in-process transports. No test
launches a generator, compiler, shell, or another runner.

Additional-property consumers cover pure and mixed objects, verbatim keys, opaque
null/number preservation, typed containers and aliases, recursive dependencies,
storage-name collisions, union boundaries and HTTP response/error conversion.

Install `httpx`, `requests`, and `pyright` in the active Python environment, then:

```sh
just golden-python-clients::test httpx
just golden-python-clients::test requests
```

`prepare` generates the union, additional-property and inline additional-property
fixtures under all three policies, plus the pure tagged recursive fixture in the
`recursive-tagged` subdirectory with package name `recursive_tagged_payloads`.
Its named `allOf` payload and plain-object control cover arrays, maps, optional
recursive values, explicit construction, lossless decode/encode, unknown tags,
and runtime imports. It also generates `recursive-alias-scope` with package name
`alias_tagged_scope`, covering alias-wrapped tagged payloads and schema names that
could shadow converter modules inside comprehensions. `build` runs strict pyright
against the unchanged output; `test` uses `${PYTHON:-python3}` (locally Python
3.15.0). The declared SDK minimum remains Python 3.12, which is also the
type-check target. Both backends run the same contracts. Tests never patch
generated source.

The shared [inline additional-property union fixture](../../fixtures/valid/inline-additional-union.yaml)
uses independent `inline_default`, `inline_strict`, and `inline_raw` packages under
`generated/<backend>/inline/<policy>/`. Native contracts compare inline anyOf,
inline oneOf, and referenced integer/string alternatives, including nested maps
and arrays. Both conversion directions reject unmatched booleans, lists, objects,
and nulls under `strict`, while `default` and `raw` preserve their complete wire
values. Public declaration checks retain typed default/strict entries and opaque
raw entries. Generation, checking, and runner orchestration remain in just.

Public contracts also cover constrained nullable arrays through enclosing
selections, mixed object/string dictionary type sets, and normalized helper-name
collisions. Strict selection rejects an integer array when the authored outer
enum permits only null, while default/raw preserve that wire value; null remains
a valid round-trip. Mixed maps retain nested integer/string alternatives and
reject inner booleans under strict, while raw keeps the opaque subtree. Collision
cases require the promoted helper and authored model to remain distinct.

For complete validation, run `just golden-python::build-httpx` or
`just golden-python::build-requests`. Each checks every golden and then runs these
fresh declaration and native contracts, sharing one temporary dependency
environment and one checker. These same entrypoints are used by `golden-build-all`,
`just ci`, and the CI matrix; no extra Python test step is appended in CI.
The temporary environment uses the active interpreter (CI `3.x`); pyright retains
its explicit 3.12 static target. Standalone consumer recipes also accept `PYRIGHT`
to select the checker and require their dependencies in the selected environment.
