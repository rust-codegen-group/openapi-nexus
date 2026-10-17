use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use openapi_nexus::generators::java::okhttp::JavaOkhttpCodeGenerator;
use openapi_nexus::generators::kotlin::okhttp::KotlinOkhttpCodeGenerator;
use openapi_nexus::test_utils::{generate_files, read_fixture};

fn empty_config() -> toml::value::Table {
    toml::value::Table::new()
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after UNIX_EPOCH")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "openapi-nexus-{name}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temporary project directory should be created");
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct LockDir {
    path: PathBuf,
}

impl LockDir {
    fn acquire(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("openapi-nexus-{name}.lock"));
        let deadline = Instant::now() + Duration::from_secs(300);
        loop {
            match fs::create_dir(&path) {
                Ok(()) => return Self { path },
                Err(err) if err.kind() == ErrorKind::AlreadyExists && Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(250));
                }
                Err(err) => panic!("failed to acquire {name} lock at {}: {err}", path.display()),
            }
        }
    }
}

impl Drop for LockDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn write_generated_project(files: &HashMap<String, String>, root: &Path) {
    for (relative_path, contents) in files {
        let path = root.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("generated parent directory should be created");
        }
        fs::write(path, contents).expect("generated file should be written");
    }
}

const CHARSET_SMOKE_JAVA: &str = r#"
import com.example.sdk.apis.CreateResourceServerError;
import com.example.sdk.apis.CreateResourceException;
import java.nio.charset.StandardCharsets;
import okhttp3.Headers;

public final class CharsetSmoke {
    public static void main(String[] args) {
        byte[] raw = "{\"message\":\"caf\u00e9\",\"retryable\":true}".getBytes(StandardCharsets.ISO_8859_1);
        Headers headers = Headers.of(
            "Content-Type", "application/json; charset=iso-8859-1",
            "Retry-After", "120"
        );
        CreateResourceServerError detail = new CreateResourceServerError(503, headers, raw);
        String message = detail.body().getMessage();
        if (!"caf\u00e9".equals(message)) {
            throw new AssertionError("expected charset-aware message, got: " + message);
        }
        CreateResourceException error = new CreateResourceException(
            503, "Service Unavailable", message, headers, raw, detail
        );
        if (!"120".equals(error.getRetryAfterHeader())) {
            throw new AssertionError("expected Retry-After header");
        }
    }
}
"#;

const CHARSET_SMOKE_KOTLIN_JAVA: &str = r#"
import com.example.sdk.apis.CreateResourceException;
import com.example.sdk.apis.CreateResourceServerError;
import java.nio.charset.StandardCharsets;
import okhttp3.Headers;

public final class CharsetSmoke {
    public static void main(String[] args) {
        byte[] raw = "{\"message\":\"caf\u00e9\",\"retryable\":true}".getBytes(StandardCharsets.ISO_8859_1);
        Headers headers = Headers.of(
            "Content-Type", "application/json; charset=iso-8859-1",
            "Retry-After", "120"
        );
        CreateResourceServerError detail = new CreateResourceServerError(503, headers, raw);
        String message = detail.body().getMessage();
        if (!"caf\u00e9".equals(message)) {
            throw new AssertionError("expected charset-aware message, got: " + message);
        }
        CreateResourceException error = new CreateResourceException(
            detail, 503, "Service Unavailable", message, headers, raw
        );
        if (!"120".equals(error.retryAfterHeader())) {
            throw new AssertionError("expected Retry-After header");
        }
    }
}
"#;

const CLOSED_SUM_SMOKE: &str = r#"
import com.example.sdk.models.Choice;
import com.google.gson.Gson;

public final class ClosedSumSmoke {
    public static void main(String[] args) {
        Gson gson = new Gson();
        Choice choice = gson.fromJson("[1,2]", Choice.class);
        if (!(choice instanceof Choice.VariantArray array) || array.value0().size() != 2) {
            throw new AssertionError("expected typed array union case");
        }
        String choiceJson = gson.toJson(new Choice.VariantString("ok"), Choice.class);
        if (!"\"ok\"".equals(choiceJson)) {
            throw new AssertionError("expected typed primitive union serialization, got " + choiceJson);
        }

        Choice nullChoice = gson.fromJson("null", Choice.class);
        if (!(nullChoice instanceof Choice.VariantNull)) {
            throw new AssertionError("expected typed null union case");
        }
        String nullJson = gson.toJson(Choice.VariantNull.INSTANCE, Choice.class);
        if (!"null".equals(nullJson)) {
            throw new AssertionError("expected typed null union serialization, got " + nullJson);
        }

    }
}
"#;

const OBJECT_UNION_SMOKE: &str = r#"
import com.example.sdk.models.OneOfObjects;
import com.google.gson.Gson;

public final class ObjectUnionSmoke {
    public static void main(String[] args) {
        Gson gson = new Gson();
        OneOfObjects value = gson.fromJson("{\"user\":{\"id\":1}}", OneOfObjects.class);
        if (!(value instanceof OneOfObjects.VariantNestedObject)) {
            throw new AssertionError("expected nested object union case");
        }
    }
}
"#;

const ANY_UNION_SMOKE: &str = r#"
import com.example.sdk.models.Config;
import com.google.gson.Gson;

public final class AnyUnionSmoke {
    public static void main(String[] args) {
        Gson gson = new Gson();
        Config booleanValue = gson.fromJson("true", Config.class);
        if (!(booleanValue instanceof Config.VariantAny)) {
            throw new AssertionError("expected Any union case for boolean");
        }
        Config objectValue = gson.fromJson("{\"key\":1}", Config.class);
        if (!(objectValue instanceof Config.VariantAny)) {
            throw new AssertionError("expected Any union case for object");
        }
    }
}
"#;

const TAGGED_SUM_SMOKE: &str = r#"
import com.example.sdk.models.Setup;
import com.google.gson.Gson;

public final class TaggedSumSmoke {
    public static void main(String[] args) {
        Gson gson = new Gson();
        Setup setup = gson.fromJson(
            "{\"kind\":\"SETUP_QUICK\",\"name\":\"x\",\"engine\":\"e\"}",
            Setup.class
        );
        if (!(setup instanceof Setup.VariantSetupQuick)) {
            throw new AssertionError("expected typed discriminator case");
        }
        String setupJson = gson.toJson(setup, Setup.class);
        if (!setupJson.contains("\"kind\":\"SETUP_QUICK\"")) {
            throw new AssertionError("expected discriminator serialization, got " + setupJson);
        }
    }
}
"#;

const TAGGED_SHAPES_SMOKE: &str = r#"
import com.example.sdk.models.AdjacentlyTaggedEnum;
import com.example.sdk.models.ExternallyTaggedEnum;
import com.example.sdk.models.VariantA;
import com.google.gson.Gson;

public final class TaggedShapesSmoke {
    public static void main(String[] args) {
        Gson gson = new Gson();
        VariantA payload = new VariantA("value", 7);

        AdjacentlyTaggedEnum adjacent =
            new AdjacentlyTaggedEnum.VariantAdjacentlyTaggedEnumVariantA(payload);
        String adjacentJson = gson.toJson(adjacent, AdjacentlyTaggedEnum.class);
        if (!adjacentJson.contains("\"ty\":\"ADJACENTLY_TAGGED_ENUM_VARIANT_A\"")
            || !adjacentJson.contains("\"data\"")) {
            throw new AssertionError("expected adjacent wire shape, got " + adjacentJson);
        }
        if (!(gson.fromJson(adjacentJson, AdjacentlyTaggedEnum.class)
            instanceof AdjacentlyTaggedEnum.VariantAdjacentlyTaggedEnumVariantA)) {
            throw new AssertionError("expected adjacent union case");
        }

        ExternallyTaggedEnum external =
            new ExternallyTaggedEnum.VariantExternallyTaggedEnumVariantA(payload);
        String externalJson = gson.toJson(external, ExternallyTaggedEnum.class);
        if (!externalJson.contains("\"EXTERNALLY_TAGGED_ENUM_VARIANT_A\"")) {
            throw new AssertionError("expected external wire shape, got " + externalJson);
        }
        if (!(gson.fromJson(externalJson, ExternallyTaggedEnum.class)
            instanceof ExternallyTaggedEnum.VariantExternallyTaggedEnumVariantA)) {
            throw new AssertionError("expected external union case");
        }
    }
}
"#;

fn add_java_charset_smoke(root: &Path) {
    let smoke_dir = root.join("smoke");
    fs::create_dir_all(&smoke_dir).expect("smoke source directory should be created");
    fs::write(smoke_dir.join("CharsetSmoke.java"), CHARSET_SMOKE_JAVA)
        .expect("Java smoke test should be written");

    let build_gradle = root.join("build.gradle");
    let mut build = fs::read_to_string(&build_gradle).expect("build.gradle should be readable");
    build.push_str(
        r#"

sourceSets {
    smoke {
        java {
            srcDirs("smoke")
        }
        compileClasspath += sourceSets.main.output + sourceSets.main.compileClasspath
        runtimeClasspath += output + compileClasspath
    }
}

tasks.register("charsetSmoke", JavaExec) {
    dependsOn("compileSmokeJava")
    classpath = sourceSets.smoke.runtimeClasspath
    mainClass = "CharsetSmoke"
}
"#,
    );
    fs::write(build_gradle, build).expect("build.gradle should be updated");
}

fn add_kotlin_charset_smoke(root: &Path) {
    let smoke_dir = root.join("smoke");
    fs::create_dir_all(&smoke_dir).expect("smoke source directory should be created");
    fs::write(
        smoke_dir.join("CharsetSmoke.java"),
        CHARSET_SMOKE_KOTLIN_JAVA,
    )
    .expect("Kotlin smoke test should be written");

    let build_gradle = root.join("build.gradle.kts");
    let mut build = fs::read_to_string(&build_gradle).expect("build.gradle.kts should be readable");
    build.push_str(
        r#"

sourceSets {
    create("smoke") {
        java.srcDirs("smoke")
        compileClasspath += sourceSets["main"].output + sourceSets["main"].compileClasspath
        runtimeClasspath += output + compileClasspath
    }
}

tasks.register<JavaExec>("charsetSmoke") {
    dependsOn("compileSmokeJava")
    classpath = sourceSets["smoke"].runtimeClasspath
    mainClass.set("CharsetSmoke")
}
"#,
    );
    fs::write(build_gradle, build).expect("build.gradle.kts should be updated");
}

fn add_java_closed_sum_smoke(root: &Path, source: &str, class_name: &str, task_name: &str) {
    let smoke_dir = root.join("smoke");
    fs::create_dir_all(&smoke_dir).expect("smoke source directory should be created");
    fs::write(smoke_dir.join(format!("{class_name}.java")), source)
        .expect("closed-sum Java smoke test should be written");

    let build_gradle = root.join("build.gradle");
    let mut build = fs::read_to_string(&build_gradle).expect("build.gradle should be readable");
    let gradle = r#"

sourceSets {
    smoke {
        java {
            srcDirs("smoke")
        }
        compileClasspath += sourceSets.main.output + sourceSets.main.compileClasspath
        runtimeClasspath += output + compileClasspath
    }
}

    tasks.register("{task_name}", JavaExec) {
        dependsOn("compileSmokeJava")
        classpath = sourceSets.smoke.runtimeClasspath
        mainClass = "{class_name}"
}
"#
    .replace("{task_name}", task_name)
    .replace("{class_name}", class_name);
    build.push_str(&gradle);
    fs::write(build_gradle, build).expect("build.gradle should be updated");
}

fn add_kotlin_closed_sum_smoke(root: &Path, source: &str, class_name: &str, task_name: &str) {
    let smoke_dir = root.join("smoke");
    fs::create_dir_all(&smoke_dir).expect("smoke source directory should be created");
    let source = source.replace(".value0()", ".getValue0()");
    fs::write(smoke_dir.join(format!("{class_name}.java")), source)
        .expect("closed-sum Kotlin smoke test should be written");

    let build_gradle = root.join("build.gradle.kts");
    let mut build = fs::read_to_string(&build_gradle).expect("build.gradle.kts should be readable");
    let gradle = r#"

sourceSets {
    create("smoke") {
        java.srcDirs("smoke")
        compileClasspath += sourceSets["main"].output + sourceSets["main"].compileClasspath
        runtimeClasspath += output + compileClasspath
    }
}

    tasks.register<JavaExec>("{task_name}") {
        dependsOn("compileSmokeJava")
        classpath = sourceSets["smoke"].runtimeClasspath
        mainClass.set("{class_name}")
}
"#
    .replace("{task_name}", task_name)
    .replace("{class_name}", class_name);
    build.push_str(&gradle);
    fs::write(build_gradle, build).expect("build.gradle.kts should be updated");
}

fn gradle_charset_smoke(root: &Path) -> std::process::Output {
    let _lock = LockDir::acquire("gradle-charset-smoke");
    let mut cmd = Command::new("gradle");
    if let Some(java_home) = std::env::var_os("JAVA_HOME") {
        cmd.env("JAVA_HOME", java_home);
    }
    cmd.arg("--no-daemon")
        .arg("--max-workers=1")
        .arg("--quiet")
        .arg("charsetSmoke")
        .env("GRADLE_OPTS", "-Dorg.gradle.daemon=false")
        .current_dir(root)
        .output()
        .expect("generated JVM charset smoke should run")
}

fn gradle_closed_sum_smoke(root: &Path, task_name: &str) -> std::process::Output {
    let _lock = LockDir::acquire("gradle-closed-sum-smoke");
    let mut cmd = Command::new("gradle");
    if let Some(java_home) = std::env::var_os("JAVA_HOME") {
        cmd.env("JAVA_HOME", java_home);
    }
    cmd.arg("--no-daemon")
        .arg("--max-workers=1")
        .arg("--quiet")
        .arg(task_name)
        .env("GRADLE_OPTS", "-Dorg.gradle.daemon=false")
        .current_dir(root)
        .output()
        .expect("generated JVM closed-sum smoke should run")
}

#[test]
fn java_error_detail_body_honors_response_charset() {
    let fixture = read_fixture("valid/typed-error-responses.yaml");
    let files = generate_files(&JavaOkhttpCodeGenerator::new(empty_config()), &fixture)
        .expect("typed error fixture should generate Java");
    let temp = TempDir::new("typed-http-error-java");
    write_generated_project(&files, &temp.path);
    add_java_charset_smoke(&temp.path);

    let output = gradle_charset_smoke(&temp.path);
    assert!(
        output.status.success(),
        "generated Java charset smoke failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn kotlin_error_detail_body_honors_response_charset() {
    let fixture = read_fixture("valid/typed-error-responses.yaml");
    let files = generate_files(&KotlinOkhttpCodeGenerator::new(empty_config()), &fixture)
        .expect("typed error fixture should generate Kotlin");
    let temp = TempDir::new("typed-http-error-kotlin");
    write_generated_project(&files, &temp.path);
    add_kotlin_charset_smoke(&temp.path);

    let output = gradle_charset_smoke(&temp.path);
    assert!(
        output.status.success(),
        "generated Kotlin charset smoke failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn java_closed_sum_round_trip_preserves_union_shapes() {
    let fixture = read_fixture("valid/type-aliases/closed-sum-payload-shapes.yaml");
    let files = generate_files(&JavaOkhttpCodeGenerator::new(empty_config()), &fixture)
        .expect("closed-sum fixture should generate Java");
    let temp = TempDir::new("closed-sum-java");
    write_generated_project(&files, &temp.path);
    add_java_closed_sum_smoke(
        &temp.path,
        CLOSED_SUM_SMOKE,
        "ClosedSumSmoke",
        "closedSumSmoke",
    );

    let output = gradle_closed_sum_smoke(&temp.path, "closedSumSmoke");
    assert!(
        output.status.success(),
        "generated Java closed-sum smoke failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn kotlin_closed_sum_round_trip_preserves_union_shapes() {
    let fixture = read_fixture("valid/type-aliases/closed-sum-payload-shapes.yaml");
    let files = generate_files(&KotlinOkhttpCodeGenerator::new(empty_config()), &fixture)
        .expect("closed-sum fixture should generate Kotlin");
    let temp = TempDir::new("closed-sum-kotlin");
    write_generated_project(&files, &temp.path);
    add_kotlin_closed_sum_smoke(
        &temp.path,
        CLOSED_SUM_SMOKE,
        "ClosedSumSmoke",
        "closedSumSmoke",
    );

    let output = gradle_closed_sum_smoke(&temp.path, "closedSumSmoke");
    assert!(
        output.status.success(),
        "generated Kotlin closed-sum smoke failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn java_untagged_object_union_selects_matching_required_fields() {
    let fixture = read_fixture("valid/comprehensive-schemas.yaml");
    let files = generate_files(&JavaOkhttpCodeGenerator::new(empty_config()), &fixture)
        .expect("comprehensive fixture should generate Java");
    let temp = TempDir::new("object-union-java");
    write_generated_project(&files, &temp.path);
    add_java_closed_sum_smoke(
        &temp.path,
        OBJECT_UNION_SMOKE,
        "ObjectUnionSmoke",
        "objectUnionSmoke",
    );

    let output = gradle_closed_sum_smoke(&temp.path, "objectUnionSmoke");
    assert!(
        output.status.success(),
        "generated Java object-union smoke failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn kotlin_untagged_object_union_selects_matching_required_fields() {
    let fixture = read_fixture("valid/comprehensive-schemas.yaml");
    let files = generate_files(&KotlinOkhttpCodeGenerator::new(empty_config()), &fixture)
        .expect("comprehensive fixture should generate Kotlin");
    let temp = TempDir::new("object-union-kotlin");
    write_generated_project(&files, &temp.path);
    add_kotlin_closed_sum_smoke(
        &temp.path,
        OBJECT_UNION_SMOKE,
        "ObjectUnionSmoke",
        "objectUnionSmoke",
    );

    let output = gradle_closed_sum_smoke(&temp.path, "objectUnionSmoke");
    assert!(
        output.status.success(),
        "generated Kotlin object-union smoke failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn java_any_union_is_a_last_resort_case() {
    let fixture = read_fixture("valid/type-aliases/union-with-any.yaml");
    let files = generate_files(&JavaOkhttpCodeGenerator::new(empty_config()), &fixture)
        .expect("Any union fixture should generate Java");
    let temp = TempDir::new("any-union-java");
    write_generated_project(&files, &temp.path);
    add_java_closed_sum_smoke(
        &temp.path,
        ANY_UNION_SMOKE,
        "AnyUnionSmoke",
        "anyUnionSmoke",
    );

    let output = gradle_closed_sum_smoke(&temp.path, "anyUnionSmoke");
    assert!(
        output.status.success(),
        "generated Java Any-union smoke failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn kotlin_any_union_is_a_last_resort_case() {
    let fixture = read_fixture("valid/type-aliases/union-with-any.yaml");
    let files = generate_files(&KotlinOkhttpCodeGenerator::new(empty_config()), &fixture)
        .expect("Any union fixture should generate Kotlin");
    let temp = TempDir::new("any-union-kotlin");
    write_generated_project(&files, &temp.path);
    add_kotlin_closed_sum_smoke(
        &temp.path,
        ANY_UNION_SMOKE,
        "AnyUnionSmoke",
        "anyUnionSmoke",
    );

    let output = gradle_closed_sum_smoke(&temp.path, "anyUnionSmoke");
    assert!(
        output.status.success(),
        "generated Kotlin Any-union smoke failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn java_tagged_closed_sum_round_trip_preserves_discriminator() {
    let fixture = read_fixture("valid/type-aliases/discriminated-union-mixed-unit-and-allof.yaml");
    let files = generate_files(&JavaOkhttpCodeGenerator::new(empty_config()), &fixture)
        .expect("tagged closed-sum fixture should generate Java");
    let temp = TempDir::new("tagged-closed-sum-java");
    write_generated_project(&files, &temp.path);
    add_java_closed_sum_smoke(
        &temp.path,
        TAGGED_SUM_SMOKE,
        "TaggedSumSmoke",
        "taggedSumSmoke",
    );

    let output = gradle_closed_sum_smoke(&temp.path, "taggedSumSmoke");
    assert!(
        output.status.success(),
        "generated Java tagged closed-sum smoke failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn kotlin_tagged_closed_sum_round_trip_preserves_discriminator() {
    let fixture = read_fixture("valid/type-aliases/discriminated-union-mixed-unit-and-allof.yaml");
    let files = generate_files(&KotlinOkhttpCodeGenerator::new(empty_config()), &fixture)
        .expect("tagged closed-sum fixture should generate Kotlin");
    let temp = TempDir::new("tagged-closed-sum-kotlin");
    write_generated_project(&files, &temp.path);
    add_kotlin_closed_sum_smoke(
        &temp.path,
        TAGGED_SUM_SMOKE,
        "TaggedSumSmoke",
        "taggedSumSmoke",
    );

    let output = gradle_closed_sum_smoke(&temp.path, "taggedSumSmoke");
    assert!(
        output.status.success(),
        "generated Kotlin tagged closed-sum smoke failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn java_tagged_shapes_round_trip_through_gson() {
    let fixture = read_fixture("valid/enum-repr/enum-repr.yaml");
    let files = generate_files(&JavaOkhttpCodeGenerator::new(empty_config()), &fixture)
        .expect("enum representation fixture should generate Java");
    let temp = TempDir::new("tagged-shapes-java");
    write_generated_project(&files, &temp.path);
    add_java_closed_sum_smoke(
        &temp.path,
        TAGGED_SHAPES_SMOKE,
        "TaggedShapesSmoke",
        "taggedShapesSmoke",
    );

    let output = gradle_closed_sum_smoke(&temp.path, "taggedShapesSmoke");
    assert!(
        output.status.success(),
        "generated Java tagged-shapes smoke failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn kotlin_tagged_shapes_round_trip_through_gson() {
    let fixture = read_fixture("valid/enum-repr/enum-repr.yaml");
    let files = generate_files(&KotlinOkhttpCodeGenerator::new(empty_config()), &fixture)
        .expect("enum representation fixture should generate Kotlin");
    let temp = TempDir::new("tagged-shapes-kotlin");
    write_generated_project(&files, &temp.path);
    add_kotlin_closed_sum_smoke(
        &temp.path,
        TAGGED_SHAPES_SMOKE,
        "TaggedShapesSmoke",
        "taggedShapesSmoke",
    );

    let output = gradle_closed_sum_smoke(&temp.path, "taggedShapesSmoke");
    assert!(
        output.status.success(),
        "generated Kotlin tagged-shapes smoke failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
