# Golden tests: Python

set working-directory := '..'

# Update all Python .golden files
update: update-httpx update-requests

# Update Python httpx .golden files
@update-httpx:
    UPDATE_GOLDEN=1 cargo test --test golden_tests_python_httpx -- --nocapture

# Update Python requests .golden files
@update-requests:
    UPDATE_GOLDEN=1 cargo test --test golden_tests_python_requests -- --nocapture

# Check Python formatting without modifying generated files or fixtures.
@ruff-check:
    uvx --from ruff==0.17.5 ruff format --isolated --line-length 100 --extension golden:python --config 'include = ["*.py.golden"]' --check --diff tests/golden/python/python-httpx tests/golden/python/python-requests

# Check the registered Python golden packages without generating fresh clients.
compile: compile-httpx compile-requests

@compile-httpx:
    ./scripts/golden-build-python.sh httpx --compile-only

@compile-requests:
    ./scripts/golden-build-python.sh requests --compile-only

# Validate all Python goldens, public declarations and native consumers
build: build-httpx build-requests

# Validate Python httpx goldens and fresh native consumers
@build-httpx:
    ./scripts/golden-build-python.sh httpx

# Validate Python requests goldens and fresh native consumers
@build-requests:
    ./scripts/golden-build-python.sh requests
