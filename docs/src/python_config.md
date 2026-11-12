# Python Generator Configuration

Both `python-httpx` and `python-requests` accept the same options. Generated
clients require Python 3.12 or newer.

```toml
[generators.python-httpx]
package_name = "my_client"
untagged_union_strategy = "firstMatch"

[generators.python-requests]
package_name = "my_client"
untagged_union_strategy = "strict"
```

The equivalent CLI override is:

```sh
openapi-nexus generate --input openapi.yaml --output client \
  --generators python-httpx \
  --generator-config python-httpx.untagged_union_strategy=strict
```

## `package_name`

Overrides the generated import package and project name. The default is derived
from the API title.

## `untagged_union_strategy`

Controls conversion of untagged `oneOf` and `anyOf` values, not general schema
validation. Both keywords use this policy; `strict` does not impose `oneOf`
cardinality on every `anyOf` elsewhere in a specification.

| Value        | Member selection                              | No match                          | Public representation             |
| ------------ | --------------------------------------------- | --------------------------------- | --------------------------------- |
| `firstMatch` | First concrete match, in schema order         | Preserve the original value       | Existing dataclass/primitive union |
| `strict`     | Reject multiple concrete matches              | Raise `ValueError`                | Existing dataclass/primitive union |
| `raw`        | Do not select or convert the union subtree     | Preserve the original value       | Dynamic `object`                   |

The default is `firstMatch`. Concrete alternatives take precedence over opaque
`Any`/empty-object fallbacks, even when the fallback appears first. Strict counts
original concrete members before calling any converter; an opaque fallback does
not create false ambiguity. Selected nested unions retain their own policy and
errors rather than silently trying another member. Nullable wrappers permit
null without inventing an extra authored member.

Matching uses required fields, present optional fields, JSON kinds, and retained
finite enum/literal constraints. Booleans are not numbers despite Python's
`True == 1`. Arrays, dictionary values, aliases, and productive recursion retain
the same conversion boundaries. Formats and numeric/string bounds do not serve
as member-selection validators.

For tagged members inside an untagged union, outbound matching follows the
existing public payload representation. The selected tagged converter restores
its wire wrapper only after selection; strict ambiguity counting never calls a
converter to probe whether a member matches.

## Conversion entrypoints

Existing dataclass constructors and `to_dict` / `from_dict` methods remain public.
For a named untagged union `Choice`, its module also provides
`choice_from_dict(value)` and `choice_to_dict(value)`. These helpers accept or
return arbitrary JSON values, not only dictionaries; their names align with the
existing Python tagged-union convention. They are not added to package barrels.

Model fields, JSON HTTP payloads, success responses, and lazy error bodies use
the same conversion plan. Direct typed members can therefore be used in union
fields without manual conversion before `json.dumps`. Required union request
bodies containing `None` are encoded as JSON `null`, not an absent HTTP body.
Optional request bodies retain the existing convention that `None` means omit.

In `raw` mode, supply JSON-compatible values yourself; generated dataclasses
inside a raw subtree are deliberately not auto-serialized. Ordinary containing
objects still use their dataclass conversion. Default unmatched values are kept
as an escape hatch, so annotations describe the normal matching path rather than
proving validation. Choose `strict` to reject unmatched inputs.

Dictionary/additional-property model preservation and multi-member `allOf`
composition are separate concerns and are not redesigned by this option.

## Additional properties

An object with explicit `additionalProperties` gets typed mutable
`additional_properties` storage, including a pure dictionary with no named fields.
The storage is a keyword-only dataclass field with an independent empty-dict
default. `from_dict` separates known wire keys from extension keys; `to_dict`
flattens the entries back into the same JSON object, without a storage wrapper.

```python
value = MixedMap(
    known_field="fixed",
    additional_properties={"keep_key": NestedValue(wire_value="nested")},
)
value.to_dict()  # {"known_field": "fixed", "keep_key": {"wire_value": "nested"}}
```

Extension keys remain verbatim, even when they resemble renamed Python fields.
Opaque values (`true` or `{}`) keep arbitrary JSON, including null entries and
large integers. Typed values use the declared model/container/alias converters;
recursive references and union policies use the same boundaries as ordinary
values. The storage does not make a pure dictionary a `dict` subtype: use its
`additional_properties` field and normal `from_dict` / `to_dict` entrypoints.

If an authored field already uses the Python name `additional_properties`, the
storage name gains trailing underscores until it is distinct. Authored fields
are not renamed. Supplying an extension key equal to a declared wire key raises
`ValueError` on serialization, even when that named field is currently omitted;
extensions never overwrite or impersonate named fields.

Objects without explicit additional properties and closed objects keep their
existing behavior. Ordinary named-field codecs and optional-null omission are
unchanged. This is not new JSON Schema validation or an `allOf`/tagged-wrapper
representation redesign.

Value conversion follows the retained schema facts. General inline composed
additional-property schemas currently lowered as opaque values remain opaque;
this storage repair does not change that shared lowering behavior.
