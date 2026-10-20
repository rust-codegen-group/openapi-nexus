# JVM generated-client tests

Java and Kotlin each have an independent Gradle consumer build. The public CLI
generates the six inputs in `fixtures.txt` into ignored `generated/` directories.
Generated manifests and source are used unchanged. Each fixture has a separate
consumer subproject, so identically named SDK classes do not share a classpath.

```bash
just golden-jvm::test
just golden-jvm::test-java
just golden-jvm::test-kotlin

# Prepare once, then run a normal Gradle test directly or from an IDE.
just golden-jvm::prepare java
gradle -p tests/generated-clients/jvm/java test
gradle -p tests/generated-clients/jvm/java :cases:any-union:test
```

Test sources and dependency lockfiles are checked in under each build's `cases/`
tree. Update locks intentionally with
`gradle -p tests/generated-clients/jvm/java test --write-locks` (and the
corresponding Kotlin command). A single Gradle build runs all fixtures for a
backend and retains normal Gradle caches; Rust tests no longer write JVM programs, patch Gradle files,
launch Gradle, or acquire global directory locks. Interrupted processes therefore
cannot leave the previous harness's lock directories behind.

The cases retain charset/header decoding, union dispatch, explicit nulls, and
tagged/untagged shapes. They additionally verify that string/number alternatives
win before `Any`, that fallback payloads are preserved, and that discriminator,
adjacent, and external round-trips retain complete JSON and decoded payloads.

The runner respects `JAVA_HOME` or Gradle's active JVM. Current local validation
uses Homebrew JDK 27 and Gradle 9.8.1; CI also runs the JVM validation on JDK 27.
Generated libraries keep their declared Java 17 compilation target. The Foojay
resolver supplies that compiler where needed, without forcing the consumer tests
to run on an older JVM. No patch-versioned JDK installation path is baked into
the tests.

On JDK 27, the generated Kotlin clients' Gson 2.11.0 emits final-field reflection
warnings; Kotlin's Gradle plugin also reports Gradle 10 deprecations. Tests do
not suppress either warning or rewrite generated dependencies to hide them.

Run this matrix separately from generator coverage. `just ci` and the CI JVM
Client Tests matrix include it. Regenerate after any generator or fixture change;
do not edit generated dependencies to make a test pass.
