# Go Generator Configuration

```toml
[go-http]
module_path = "example.com/sdk"
all_of_strategy = "composed"
```

## Object and map composition

The default `composed` policy retains named object members as embedded Go fields
and gives anonymous map members named storage such as `AdditionalProperties`.
Generated JSON methods combine the members into one flat wire object rather
than inheriting one embedded member's JSON methods and losing its siblings.

```go
value := models.Pair{
    Base: models.Base{KnownField: "fixed"},
    Other: models.Other{OtherField: 7},
}
// json.Marshal(value): {"known_field":"fixed","other_field":7}
```

Existing named member entrypoints and promoted fields remain available. Repeated
member names or map-storage collisions receive numeric suffixes. Aliases and
nested object compositions are followed without rewriting ordinary model codecs.

On reading, each member receives its own declared keys and globally unknown keys.
A sibling's declared keys are not duplicated into extension storage, so editing a
named field does not leave stale values in a map member. Typed members retain their
native `encoding/json` behavior. If member writes produce different JSON encodings
for the same key, marshaling returns an error instead of silently overwriting data.
Projection retains input key order and duplicate keys, including native
case-insensitive field matching and errors from earlier duplicate values.

This is not a complete JSON Schema validator. It uses retained IR object facts and
conservative representation compatibility; ordinary member validation, optional
fields, null behavior and dynamic `any` number conversion remain unchanged.
In particular, `any` entries still follow Go's native floating-point conventions.
It does not infer missing shared-lowering facts or redesign ordinary dictionary
codecs.

## Raw escape hatch

Set `all_of_strategy = "raw"` to emit every intersection as an alias of
`json.RawMessage`, preserving JSON kinds, nulls and exact numeric literals without
typed member selection. The same raw representation is the default fallback for
non-object intersections, nullable members, unproductive composition cycles and
object/map types that cannot be conservatively composed. Nullable members also
use raw JSON when reached through nested compositions or aliases: no pointer
embedding is needed, and a root `null` is not replaced by an empty object. This
conservative policy applies even when a sibling is non-nullable; raw mode is not
intersection validation. No unconditional generation error or invalid anonymous
embedding is introduced for these shapes.

```bash
openapi-nexus generate --input api.yaml --output sdk --generators go-http \
  --generator-config go-http.all_of_strategy=raw
```

Raw values intentionally do not enforce intersection constraints. Use them when
wire preservation is more useful than inferred typed composition. This policy
does not change untagged unions, ordinary objects, other generators or generated
dependencies.
