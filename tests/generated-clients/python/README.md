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

`prepare` generates both fixtures under all three policies; `build` runs strict pyright against the
unchanged output; `test` uses `${PYTHON:-python3}` (locally Python 3.15.0).
The declared SDK minimum remains Python 3.12, which is also the type-check target.
Both backends run the same contracts. Tests never patch generated source.

For complete validation, run `just golden-python::build-httpx` or
`just golden-python::build-requests`. Each checks every golden and then runs these
fresh declaration and native contracts, sharing one temporary dependency
environment and one checker. These same entrypoints are used by `golden-build-all`,
`just ci`, and the CI matrix; no extra Python test step is appended in CI.
The temporary environment uses the active interpreter (CI `3.x`); pyright retains
its explicit 3.12 static target. Standalone consumer recipes also accept `PYRIGHT`
to select the checker and require their dependencies in the selected environment.
