# Rust generated-client tests

These are standalone consumer test projects, not tests of the generator crate.
Their ordinary Cargo manifests depend on freshly generated clients through path
dependencies. Test source is checked in as Rust files; no Rust test invokes Cargo,
assembles another test program, or changes a generated manifest.

## Run

From the repository root:

```bash
just golden-rust::test          # all three backends
just golden-rust::test-reqwest  # one backend
```

The recipe uses the public CLI to generate fixture crates into the backend's
ignored `generated/` directory, then runs `cargo test --locked` once for that
consumer project. It inherits the active toolchain, `RUSTFLAGS`, Cargo settings,
and test concurrency settings. Generated client files are never edited. Each
backend retains its target directory, so dependencies are not rebuilt per case.

After generation, the project can be used directly from Cargo or an IDE:

```bash
just golden-rust::prepare reqwest
cargo test --locked --manifest-path tests/generated-clients/rust/reqwest/Cargo.toml
cargo test --locked --manifest-path tests/generated-clients/rust/reqwest/Cargo.toml --test nested_nullable
```

Add or change generation cases in the [Rust fixture plan](fixtures.txt). The
plan selects each output's backends, input, config profile, and overrides. The
in-process registration check (`cargo test --test rust_client_fixture_plan` from
the repository root) requires the planned paths to match the generated path
dependencies in each checked-in Cargo manifest. Cargo itself validates dependency
package names and the lockfiles when running the native consumers.

Regenerate after changing the generator or a fixture. Preparation generates into
a sibling staging directory and replaces the managed `generated/` tree only
once all of that backend's crates are ready. Do not edit ignored generated
clients to make a test pass.

## Coverage

| Fixture | Checks |
| --- | --- |
| `typed-error-responses` | HTTP errors, metadata, uniform errors, success dispatch, body handling, and layout; existing backend-specific cases are retained |
| `nested-nullable` | Seven shared HTTP decoding cases per backend, including strict controls and invalid-body preservation |
| `rust-error-payload-shapes` | Compiles as a dev-dependency of every consumer project, retaining the previous composite-payload compile checks |
| `rust-union-policies` | Structural member selection, recursive and opaque branches, and raw untagged unions |
| `rust-inferred-tagged-wire` | Wire-preserving discriminator state, composed tag ownership, nested unions, and independent first-match, strict, and raw tagged policies |

The reqwest project also retains compile-time propagation checks for uniform
errors, thiserror, and Snafu; ureq retains its synchronous propagation check.

The root `just test-all` and `just coverage-lcov` cover the generator workspace,
not these projects. CI runs a separate Rust Client Tests matrix, and `just ci`
includes `just golden-rust::test`. Golden comparison and all-golden compilation
remain separate checks. JVM consumer tests have their own Gradle projects under
`tests/generated-clients/jvm/` and run through `just golden-jvm::test`.
