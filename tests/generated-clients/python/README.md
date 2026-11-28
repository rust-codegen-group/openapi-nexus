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

[The fixture plan](fixtures.txt) lists each output directory, input schema,
Python package and optional policy override once for both backends. The same plan
supplies the generated packages to Pyright and their import roots to Pyright
and `unittest`. Add a fixture there; do not maintain another list of generated
paths.

`build <backend>` type-checks, and `test <backend>` type-checks and runs native
tests, against a fresh, disposable tree allocated for that invocation. Both
commands leave the checkout's prepared SDKs alone. For IDE use, run
`just golden-python-clients::prepare <backend>` to refresh `generated/<backend>/`
and use its generated `pyrightconfig.json` for declaration checks. Both backends
use the same contracts. The interpreter is `${PYTHON:-python3}` (locally Python
3.15.2), while the SDK minimum and type-check target remain Python 3.12.

The union, additional-property and inline fixtures cover all three policies.
The tagged recursive fixture covers arrays, maps, optional recursive values,
lossless decode/encode, unknown tags and runtime imports. The recursive alias
fixture covers alias-wrapped tagged payloads and schema names that could shadow
converter modules inside comprehensions.

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
to select a checker.

Run `just golden-python::ruff-check` to check both sets of Python goldens with
Ruff 0.17.5 at 100 columns. The recipe requires `uvx` and does not rewrite any
files; the golden tests still verify that the checked files match generator
output.
