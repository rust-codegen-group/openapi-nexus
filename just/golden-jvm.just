# Native consumer tests for freshly generated Java and Kotlin clients.
set working-directory := '..'

test: test-java test-kotlin

@prepare BACKEND:
    bash ./scripts/generate-jvm-client-fixtures.sh {{ BACKEND }}

@test-java: (prepare "java")
    gradle --no-daemon --max-workers=2 -p tests/generated-clients/jvm/java test

@test-kotlin: (prepare "kotlin")
    gradle --no-daemon --max-workers=2 -p tests/generated-clients/jvm/kotlin test
