# Kotlin Generator Configuration

The `kotlin-okhttp` backend keeps sealed classes and typed data-class variants
for untagged unions, selecting a member before invoking its existing Gson codec.

```toml
[kotlin-okhttp]
package_name = "com.example.sdk"
untagged_union_strategy = "firstMatch"
all_of_strategy = "composed"
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

Kotlin `typealias` declarations remain transparent. Compatible object/map
`allOf` intersections retain their source-facing member properties while a
private Gson adapter projects each member and merges one flat JSON object. A
single-member alias therefore keeps its constructor/getter API without adding a
wire wrapper key. Unsupported, nullable or incompatible intersections use the
raw `JsonElement` representation. Ordinary objects, aliases, and enums keep
existing APIs and codecs; inferred tagged union policy is configured separately.
Explicit dictionary and mixed-object additional properties have the
representations described below.

`all_of_strategy = "composed"` (default) enables the conservative compatible
composition plan. Set it to `"raw"` to represent every intersection as a
`JsonElement`-backed model while preserving the original JSON value. The
allOf policy is independent of `untagged_union_strategy`.

Raw mode uses Gson's native tree adapter and temporarily enables writer null
serialization, restoring its prior value even on failure. This preserves null
entries inside raw trees without changing ordinary model null-field policy.
An enclosing model named `VariantRaw` uses `VariantRawVariant` for its raw case.

The policy can also be set through the CLI:

```sh
openapi-nexus generate --input api.yaml --output sdk --generators kotlin-okhttp \
  --generator-config kotlin-okhttp.untagged_union_strategy=strict
```

For raw allOf values, use:

```sh
openapi-nexus generate --input api.yaml --output sdk --generators kotlin-okhttp \
  --generator-config kotlin-okhttp.all_of_strategy=raw
```

## Inferred internally tagged unions

`tagged_union_strategy` controls inferred internal discriminators without a
separate required fixed label for every member. This is independent of
`untagged_union_strategy`. Explicit mappings, distinct fixed tags, and adjacent
and external representations keep their existing codecs.

| Policy | Behavior |
| --- | --- |
| `firstMatch` (default) | Select the first structurally matching concrete authored member, or the first opaque match if no concrete member matches. |
| `strict` | Use the same eligibility rules and reject more than one matching concrete member. |
| `raw` | Emit only `VariantRaw(JsonElement)` and return the original JSON subtree without typed selection or conversion. |

The adapter checks the full authored object before invoking one payload decoder.
If the native payload can own its discriminator, its existing case shape is
unchanged. Otherwise the case has a separate `discriminator` field:
`JsonElement?`. A language `null` is an absent field; `JsonNull.INSTANCE` is
an explicit JSON null when allowed. The serializer preserves the supplied wire
label, rejects a conflicting discriminator in the payload, and validates the
assembled value against the selected member and its common constraints.
When a native payload cannot retain the full wire shape, `value0` holds the
projected `JsonObject` (or `JsonElement` for non-object alternatives). Existing
compatible native decoders still enforce their own payload rules in that case.
This does not alter standalone payload models.

For example, set `tagged_union_strategy = "strict"` under `[kotlin-okhttp]`
or pass `--generator-config kotlin-okhttp.tagged_union_strategy=strict`.
See [Union Wire Contract](union_wire_contract.md) for structural selection and
wire ownership details.

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
they share names such as `TypeToken` or `JsonElement`. Mixed-object preservation and
allOf composition use separate adapters so each member retains its configured
codec and wire keys.

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
composition lowering, opaque named-alias codecs, tagged wrapper extension storage
and generation formatting remain separate concerns; compatible named/map `allOf`
composition is handled by the policy described above.
