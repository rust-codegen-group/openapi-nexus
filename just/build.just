# Build commands

# Build all crates
@build-all:
    cargo build

# Build release
@build-release:
    cargo build --release

# Build specific crate
@build-crate CRATE:
    cargo build -p {{ CRATE }}

# Check (compile without running)
@check:
    cargo check

# Check all targets
@check-all:
    cargo check --all-targets --all-features
