# Code generation commands

# Generate TypeScript from a spec file
@typescript INPUT OUTPUT:
    cargo run --bin openapi-nexus -- generate --input {{ INPUT }} --output {{ OUTPUT }} --generators typescript-fetch

# Generate Go from a spec file
@go INPUT OUTPUT:
    cargo run --bin openapi-nexus -- generate --input {{ INPUT }} --output {{ OUTPUT }} --generators go-http

# Generate both TypeScript and Go from a spec file
@all INPUT OUTPUT:
    cargo run --bin openapi-nexus -- generate --input {{ INPUT }} --output {{ OUTPUT }} --generators typescript-fetch,go-http

# Check TypeScript compilation in output directory
@check-ts OUTPUT_DIR:
    cd {{ OUTPUT_DIR }} && npx tsc --noEmit
