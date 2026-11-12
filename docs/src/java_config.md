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
Explicit additional-property storage is described below. Use raw mode when all
JSON must be retained, including unknown keys, null-valued object entries, and
large numeric literals. Raw writing restores the surrounding Gson writer's null
policy; ordinary sibling model fields keep their normal null omission.

The policy can also be set through the CLI:

```sh
openapi-nexus generate --input api.yaml --output sdk --generators java-okhttp \
  --generator-config java-okhttp.untagged_union_strategy=strict
```

## Additional properties

Objects with explicit `additionalProperties` expose typed, mutable storage through
`getAdditionalProperties()`. This includes pure dictionaries with no named fields;
they remain ordinary model classes rather than `Map` subtypes. Existing named
constructor parameters and getters stay unchanged, with an independent empty map
available on each constructed or decoded object.

```java
MixedMap value = new MixedMap("fixed");
value.getAdditionalProperties().put("keep_key", new NestedValue("nested"));
// Gson writes {"known_field":"fixed","keep_key":{"wire_value":"nested"}}
```

The Gson adapter flattens extensions into the same JSON object, retaining keys
verbatim. Opaque values (`true` or `{}`) use `JsonElement` so null entries and large
numeric literals survive without coercion to `Object`/double. Declared typed
values use their native Gson adapters, including model/enum/nullable values,
containers, recursive references and configured untagged-union policies.

Storage is transient to the ordinary Gson model delegate. Known fields retain
their configured codecs, native scalar failures and null omission; extension
entries preserve their own nulls without changing surrounding writer settings.
The runtime distinguishes Gson's streaming and tree readers and uses its native
tree writer so their existing coercion and configured numeric output stay intact.
This support depends on the tree reader/writer classes in the unchanged Gson
2.11.0 dependency; native contracts cover both input modes.
Aliases keep their existing `value` wrapper instead of acquiring a different wire
shape. No generated dependency version or general validation policy changes.

If an authored field/getter already uses the storage name, the storage and getter
receive a numeric suffix, such as `additionalProperties2` and
`getAdditionalProperties2()`. Authored members are not renamed. An extension key
equal to a declared wire key raises `IllegalArgumentException` on serialization,
even if the named field is currently omitted, rather than silently overwriting it.

Closed objects and those without explicit additional properties keep their
existing behavior. Shared lowering, general inline-composition type loss,
multi-member `allOf`, tagged-wrapper extension storage and generation formatting
are separate concerns, not redesigned by this object-storage repair.
