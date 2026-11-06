# Migrating from 0.2.3 to 0.3.0

## Rust library integrations

`OpenApiCodeGenerator::generate(&config)` now returns
`Result<GenerationReport, PipelineError>`. An outer `Err` means the shared parse or
lowering pipeline failed. An outer `Ok` does not guarantee every selected target
succeeded: inspect `report.is_success()` or `report.failures`.

```rust
use openapi_nexus::{GenerationReport, OpenApiCodeGenerator, PipelineError};
use openapi_nexus::config::Config;

fn generate(config: &Config) -> Result<GenerationReport, PipelineError> {
    let generator = OpenApiCodeGenerator::new(config);
    let report = generator.generate(config)?;
    for failure in &report.failures {
        eprintln!("{failure}");
    }
    // The caller must decide whether a partial result is acceptable.
    Ok(report)
}
```

Custom `CodeGenerator` implementations now return
`Result<Vec<FileInfo>, GeneratorError>`. Custom `FileWriter` implementations return
`Result<(), WriteError>` from `write_files`; handle the concrete error variants
instead of relying on dynamic error strings.

Update configuration error matches/construction from
`ConfigError::Validation(message)` to `ConfigError::Validation { message }`, and
likewise for `ParseOverrides`.

Code constructing OpenAPI 3.0 `ObjectSchema` values must use
`ExclusiveBound::Boolean(...)` or `ExclusiveBound::Number(...)` for exclusive-bound
fields. JSON/YAML boolean flags and legacy numeric forms are both accepted.

Code constructing `IrTaggedVariant` values must initialize
`additional_properties`: normally `None`, or `Some(value_type)` for extensions on
an external wrapper. Internal-variant extensions remain part of the payload type.

## CLI automation

Treat nonzero generator exit codes as failures. Multi-generator runs still collect
per-target failures while continuing other selected targets, but the overall CLI
command now exits unsuccessfully when any target fails. Do not treat partial output
or a logged completion message as proof of success.

## Java and Kotlin generated clients

Regenerate clients and recompile consumers. Supported unions now expose typed
alternatives instead of untyped `Object`/`Any` values. Use the names in each
generated model: they depend on schema shape and must not be guessed globally.

For example, the union-with-any fixture generates Kotlin
`Config.VariantString(value0)` and Java `new Config.VariantString(value0)`; payloads
are accessed as `value0` in Kotlin and `value0()` in Java. This is an example, not a
universal naming contract for every union.

Keep the generated Gson adapters: they serialize the schema's original wire shape,
not a generic wrapper containing the variant class name. Confirm round trips with
the schemas your application actually uses.

Generated Kotlin projects use Kotlin Gradle plugin 2.2.21; the generated JVM
toolchain target remains 17. Java closed-sum models use sealed interfaces/records
and require the generated project's Java 17 target or newer.

## TypeScript generated clients

Regenerate and rebuild clients. Declared additional-property dictionaries are now
readonly, including compatible `allOf` aliases. Construct replacement objects
instead of assigning directly through their string indexes. Readonly is a type
contract; generated runtime objects are not frozen.

In camelCase mode, named fields are renamed but extension keys stay verbatim.
Typed additional values are converted recursively; opaque JSON contents are not
renamed. Use `FromJSON` and `ToJSON` at the wire/domain boundary.

For a collision such as `{known_field: 'fixed', knownField: 'extension'}`, ergonomic
access remains `value.knownField === 'fixed'`. The extension is retained under
enumerable `Symbol.for('openapi-nexus.additionalProperties')` metadata.
`{...value}` retains it; `JSON.stringify(value)` and JSON-based cloning do not.
Serialize through the generated `ToJSON` converter to restore the complete wire
payload. Preserve mode and `$Wire` types are alternatives when original wire-key
access is preferable.

Compatible object/map `allOf` values are planned together, including arbitrary-JSON
types that render as `unknown`. Ambiguous multi-member unions retain their prior
conversion policy; this release does not introduce universal runtime validation.

## Installation and security scope

Use the versioned `0.3.0` release installers after the release exists, or install
the `0.3.0` crate after crates.io publication. The nightly workflow has been removed.

The ZIP security fix removes an unused fixture-generator build dependency. It is
not a claim that generated SDKs have received new archive-validation features or
that every dependency has undergone a complete security audit.
