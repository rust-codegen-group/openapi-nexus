# Kotlin Generator Configuration

The `kotlin-okhttp` backend keeps sealed classes and typed data-class variants
for untagged unions, selecting a member before invoking its existing Gson codec.

```toml
[kotlin-okhttp]
package_name = "com.example.sdk"
untagged_union_strategy = "firstMatch"
```

| Policy | Behavior |
| --- | --- |
| `firstMatch` (default) | First structural match in authored order; concrete members precede opaque fallbacks. |
| `strict` | Same typed API; reject multiple authored concrete matches before decoding. Use a fallback only if no concrete member matches. |
| `raw` | Explicit `VariantRaw(JsonElement)` API, preserving known, ambiguous, unmatched, and null JSON without typed selection. |

Typed policies reject unmatched non-null values. Selection uses required wire
keys, finite scalar constraints (including numeric and boolean enums), nested
arrays/maps, and productive recursion. Numeric matching compares exact decimal
significands and unbounded exponents, without rounding through `Double` or imposing
Gson's decimal-accessor scale cap. Recursive predicates cache both successful and
failed checks by JSON-node identity. Matching is not full JSON Schema validation:
format bounds remain with the existing native codecs.

After selection, the adapter invokes the original member's codec once. Native
decode errors propagate; they do not trigger another candidate or raw fallback.
Writing uses the declared payload type, so a nested sealed union uses its own
adapter instead of reflecting its runtime `Variant*` wrapper as a `value0` field.
Both typed policies retain the existing root-null behavior: nonnullable unions
return Kotlin `null`, while nullable unions use `VariantNull`.

Kotlin `typealias` declarations remain transparent. Intersections retain their
existing wrapper fields (for example, `leftPart` and `rightPart`); this is not an
`allOf` codec redesign. Ordinary objects, aliases, enum declarations, and tagged
unions keep their existing APIs/codecs. Dictionary and additional-property repair
is outside this change.

Raw mode uses Gson's native tree adapter and temporarily enables writer null
serialization, restoring its prior value even on failure. This preserves null
entries inside raw trees without changing ordinary model null-field policy.
An enclosing model named `VariantRaw` uses `VariantRawVariant` for its raw case.

The policy can also be set through the CLI:

```sh
openapi-nexus generate --input api.yaml --output sdk --generators kotlin-okhttp \
  --generator-config kotlin-okhttp.untagged_union_strategy=strict
```
