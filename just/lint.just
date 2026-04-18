# Linting and formatting commands

set working-directory := '..'

# Run cargo fmt
@fmt:
    cargo fmt

# Run cargo clippy
@clippy:
    cargo clippy

# Run clippy with warnings as errors (required before committing)
@strict:
    cargo clippy --all-targets --all-features -- -D warnings

# Run all checks (fmt + clippy)
@all:
    cargo fmt
    cargo clippy

# Run all checks with warnings as errors
@all-strict:
    cargo fmt
    cargo clippy --all-targets --all-features -- -D warnings
