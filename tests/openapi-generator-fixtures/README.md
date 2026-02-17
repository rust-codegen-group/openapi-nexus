# OpenAPI Generator Fixtures

These test fixtures are imported from [OpenAPITools/openapi-generator](https://github.com/OpenAPITools/openapi-generator).

## Source

- **oas30**: `modules/openapi-generator/src/test/resources/3_0` (OpenAPI 3.0 specs)
- **oas31**: `modules/openapi-generator/src/test/resources/3_1` (OpenAPI 3.1 specs)
- **oas32**: Reserved for future OpenAPI 3.2 fixtures (empty; upstream has no 3_2 directory)

## License

The upstream OpenAPI Generator project is licensed under the [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0). These fixtures are used in openapi-nexus under the same terms. The openapi-nexus project is dual-licensed under MIT OR Apache-2.0.

## Re-syncing

To refresh fixtures from upstream, run from the repository root:

```bash
./scripts/sync-openapi-generator-fixtures.sh
```
