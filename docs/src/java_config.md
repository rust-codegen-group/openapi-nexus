# Java Generator Configuration

The `java-okhttp` backend emits Java 17 sealed interfaces and records for unions,
with Gson adapters that retain the selected member's declared payload.

## Untagged union selection

```toml
[java-okhttp]
package_name = "com.example.sdk"
untagged_union_strategy = "firstMatch"
```

| Policy | Behavior |
| --- | --- |
| `firstMatch` (default) | Keep typed variants; choose the first structural match in schema order, with concrete members before opaque fallbacks. |
| `strict` | Keep the same typed API; reject multiple authored concrete matches before decoding. Use an opaque fallback only when no concrete member matches. |
| `raw` | Explicitly emit only `VariantRaw(JsonElement)`, preserving known, ambiguous, unmatched, and null JSON without typed selection. |

Both typed policies reject unmatched non-null values. Matching uses required wire
keys, finite scalar constraints (including numeric and boolean enums), nested
arrays and maps, and productive recursive references. Finite numbers use exact
`BigDecimal` significands and `BigInteger` exponents, retaining
distinctions above double precision without Gson's decimal-accessor scale limit.
Recursive checks share results by JSON node identity and predicate within one
selection, not across decoded values. Private recursive-cache types are qualified
so models named `Integer` or
`Boolean` do not shadow the runtime's boxed types.

After selection, the original member is decoded once through its existing Gson
codec. Its errors, including nested strict-union errors, propagate rather than
being hidden by another member or opaque fallback. This is structural selection,
not full JSON Schema validation: formats, numeric bounds, and ordinary Gson
coercion or overflow behavior are not redesigned. Serialization does not add
schema validation.

### Null and representation compatibility

Typed policies preserve Java's existing root-null API: JSON `null` becomes Java
`null` for a nonnullable union, or the existing `VariantNull.INSTANCE` for a
nullable union. No synthetic raw variant is added to typed unions. Explicit raw
mode instead wraps JSON null in `VariantRaw(JsonNull.INSTANCE)`.
As with typed cases, a case name equal to its enclosing interface is
disambiguated (for example, `VariantRaw.VariantRawVariant`).

Java aliases remain objects with a `value` field, and `allOf` models keep their
existing member-wrapper fields. Matchers follow those wire boundaries; this is
not an alias-codec or multi-member `allOf` redesign. Tagged unions and ordinary
models keep their existing APIs and codecs.

Typed variants retain declared member payloads, not arbitrary unknown fields.
Dictionary/additional-property repairs remain separate. Use raw mode when all
JSON must be retained, including unknown keys, null-valued object entries, and
large numeric literals. Raw writing restores the surrounding Gson writer's null
policy; ordinary sibling model fields keep their normal null omission.

The policy can also be set through the CLI:

```sh
openapi-nexus generate --input api.yaml --output sdk --generators java-okhttp \
  --generator-config java-okhttp.untagged_union_strategy=strict
```
