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
