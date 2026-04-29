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

# Type-check all Python goldens
build: build-httpx build-requests

# Type-check Python httpx goldens with pyright
@build-httpx:
    ./scripts/golden-build-python.sh httpx

# Type-check Python requests goldens with pyright
@build-requests:
    ./scripts/golden-build-python.sh requests
