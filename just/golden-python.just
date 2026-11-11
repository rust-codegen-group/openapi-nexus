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

# Validate all Python goldens, public declarations and native consumers
build: build-httpx build-requests

# Validate Python httpx goldens and fresh native consumers
@build-httpx:
    ./scripts/golden-build-python.sh httpx

# Validate Python requests goldens and fresh native consumers
@build-requests:
    ./scripts/golden-build-python.sh requests
