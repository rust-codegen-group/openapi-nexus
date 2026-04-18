# OpenAPI Nexus Justfile

mod test 'just/test.just'
mod build 'just/build.just'
mod lint 'just/lint.just'
mod generate 'just/generate.just'
mod golden 'just/golden.just'

# List all available commands
[private]
help:
    @just --list --list-submodules
