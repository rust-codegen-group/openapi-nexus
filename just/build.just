# Build commands

set working-directory := '..'

# Build all
@build-all:
    cargo build

# Build release
@build-release:
    cargo build --release

# Check (compile without running)
@check:
    cargo check

# Check all targets
@check-all:
    cargo check --all-targets --all-features
