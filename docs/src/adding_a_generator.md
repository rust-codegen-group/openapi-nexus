# Adding a Generator

This guide walks through adding a new language generator to openapi-nexus.

## 1. Create the crate

Create a new crate under `crates/generators/`:

```bash
cargo new --lib crates/generators/openapi-nexus-<lang>
```

Add it to the workspace `members` list in the root `Cargo.toml`:

```toml
members = [
    # ...
    "crates/generators/openapi-nexus-<lang>",
]
```

Add workspace dependencies:

```toml
# In the new crate's Cargo.toml
[dependencies]
openapi-nexus-core.workspace = true
openapi-nexus-ir.workspace = true
sigil-stitch.workspace = true
```

## 2. Add a GeneratorType variant

In `crates/openapi-nexus-core/src/generator_type.rs`, add a new variant:

```rust
pub enum GeneratorType {
    TypeScriptFetch,
    GoHttp,
    MyLang,  // <-- new
}
```

Implement the `Display` and `FromStr` traits to map the CLI name (e.g., `"my-lang"`) to the variant.

## 3. Implement CodeGenerator

```rust
use openapi_nexus_core::traits::code_generator::CodeGenerator;
use openapi_nexus_core::traits::file_writer::FileWriter;
use openapi_nexus_ir::types::IrSpec;

pub struct MyLangCodeGenerator { /* config fields */ }

impl CodeGenerator for MyLangCodeGenerator {
    fn language(&self) -> Language { Language::MyLang }
    fn generator_type(&self) -> GeneratorType { GeneratorType::MyLang }

    fn generate(&self, ir: &IrSpec) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        // Transform IrSpec into generated source files
        todo!()
    }
}
```

## 4. Implement FileWriter

`FileWriter` writes `Vec<FileInfo>` to disk. A default implementation is provided, so you typically only need:

```rust
impl FileWriter for MyLangCodeGenerator {}
```

## 5. Register in the orchestrator

In `crates/openapi-nexus/src/openapi_code_generator.rs`, inside `OpenApiCodeGenerator::new()`:

```rust
registry.register_generator(Box::new(
    MyLangCodeGenerator::new(/* config */)
));
```

## 6. Add golden tests

1. Create `crates/generators/openapi-nexus-<lang>/tests/golden_tests_<lang>.rs`
2. Use the `run_golden_test` harness from `openapi-nexus-test-utils`
3. Create golden files: `tests/golden/<lang>/<generator-id>/`
4. Generate initial goldens with `UPDATE_GOLDEN=1 cargo test --test golden_tests_<lang>`

See the [Golden Testing](golden_testing.md) chapter for details.

## 7. Add to CI

In `.github/workflows/ci.yml`, add a new entry to the golden-build matrix with the appropriate language toolchain setup and `just golden::build-<lang>` command.
