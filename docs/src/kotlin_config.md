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
unions keep their existing APIs/codecs. Explicit dictionary and mixed-object
additional properties have the representations described below.

Raw mode uses Gson's native tree adapter and temporarily enables writer null
serialization, restoring its prior value even on failure. This preserves null
entries inside raw trees without changing ordinary model null-field policy.
An enclosing model named `VariantRaw` uses `VariantRawVariant` for its raw case.

The policy can also be set through the CLI:

```sh
openapi-nexus generate --input api.yaml --output sdk --generators kotlin-okhttp \
  --generator-config kotlin-okhttp.untagged_union_strategy=strict
```

## Pure dictionary models

A named object with no named properties and an explicit `additionalProperties`
schema generates a data class with `additionalProperties: MutableMap<String, V>`
storage, defaulting to an empty map. Its Gson adapter reads and writes a flat JSON
object: the storage property's name is not added to the wire representation.

```kotlin
val labels = Labels()
labels.additionalProperties["keep_key"] = "value"
val json = Gson().toJson(labels) // {"keep_key":"value"}
```

Keys stay verbatim. Values retain their declared model, scalar, nullable or
container types; `additionalProperties: true` and `{}` use `JsonElement` values
to preserve arbitrary JSON, null entries and large numeric literals. Dictionary
aliases remain transparent and containing fields, lists, maps and unions use the
same adapter. JSON null at the root stays Kotlin null; non-object roots fail
instead of becoming empty dictionaries.

Dictionary entry nulls are preserved without changing the writer's surrounding
null policy. Values decode from the original JSON reader through their declared
Gson codecs, preserving native decode errors as well as ordinary model null-field
omission. Authored model types remain distinct from Gson helper types, even when
they share names such as `TypeToken` or `JsonElement`. Mixed-object preservation
is described below; `allOf` wrapper redesign remains separate.

## Mixed-object additional properties

An object with named fields and explicit `additionalProperties` gets a typed
`var additionalProperties: MutableMap<String, V>` as its final defaulted primary
constructor property. Each instance starts with an independent empty map.

```kotlin
val value = MixedMap(knownField = "fixed")
value.additionalProperties["keep_key"] = NestedValue(wireValue = "nested")
// Gson writes {"known_field":"fixed","keep_key":{"wire_value":"nested"}}
val duplicate = value.copy() // Retains the same map, as a normal shallow copy.
```

Existing named constructor arguments and component positions remain unchanged;
an overload retains the old Java named-field constructor entrypoint. The appended
storage participates in generated copy, equality, hash and component operations.
Adding a data-class property changes generated binary signatures: regenerate and
recompile consumers rather than mixing old and new SDK binaries.

The storage field is transient to Gson's ordinary delegate, and the flat-object
adapter handles it separately. Extension keys stay verbatim. A storage-name clash
adds a numeric suffix such as `additionalProperties2`, without renaming authored
members. An extension key equal to a declared wire key raises
`IllegalArgumentException` on writing, even if that named field is omitted.

Opaque values (`true` or `{}`) use `JsonElement` to retain null entries and large
numeric literals. Declared models, enums, containers, nullable values, recursive
references and transparent aliases keep their native value codecs; union values
retain the configured selection policy. The actual configured Gson instance
continues to own named fields, field naming and custom value adapters.

Native streaming/tree coercion differences, configured numeric writing and
ordinary null omission are retained. Extension entry nulls do not force null
fields inside ordinary model values or alter the surrounding writer's policy,
including on failure. This support uses the tree reader/writer implementation
classes from the unchanged Gson 2.11.0 dependency; native consumers cover both
input modes.

Pure-dictionary codecs are unchanged. Closed/implicit objects, general inline
composition lowering, opaque named-alias codecs, multi-member `allOf`, tagged
wrapper extension storage and generation formatting remain separate concerns.
