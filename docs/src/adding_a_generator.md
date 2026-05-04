# Adding a Generator

This guide walks through adding a new language generator to openapi-nexus.

## 1. Add a GeneratorType variant

In `src/codegen/generator_type.rs`, add a new variant:

```rust
pub enum GeneratorType {
    TypeScriptFetch,
    GoHttp,
    // ...
    MyLang,  // <-- new
}
```

Implement the `Display` and `FromStr` traits to map the CLI name (e.g., `"my-lang"`) to the variant.

Also add the corresponding `Language` variant in `src/codegen/language.rs` if the target language doesn't exist yet.

## 2. Create the generator module

Create a new directory under `src/generators/`:

```
src/generators/my_lang/
├── mod.rs              Module root, exports the generator struct
├── sigil_emit.rs       Model emission (IR schemas → target-language types)
└── sigil_emit_api.rs   API emission (IR operations → client methods)
```

Add the module to `src/generators/mod.rs`.

## 3. Implement CodeGenerator

```rust
use crate::codegen::traits::code_generator::CodeGenerator;
use crate::codegen::traits::file_writer::{FileInfo, FileWriter};
use crate::ir::types::IrSpec;

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

In `src/generators/registry.rs`, add the generator to the registry:

```rust
GeneratorType::MyLang => {
    Box::new(MyLangCodeGenerator::new(/* config */))
}
```

## 6. Add golden tests

1. Create `tests/golden_tests_my_lang.rs`
2. Use the `run_golden_test` harness from `openapi_nexus::codegen::test_utils`
3. Create golden files: `tests/golden/my_lang/my-lang/<fixture>/`
4. Generate initial goldens with `UPDATE_GOLDEN=1 cargo test --test golden_tests_my_lang`

See the [Golden Testing](golden_testing.md) chapter for details.

## 7. Add to CI

In `.github/workflows/ci.yml`, add a new entry to the golden-build matrix with the appropriate language toolchain setup and compile-check command.

Add a Justfile submodule at `just/golden-my-lang.just` with `update` and `build` recipes.
