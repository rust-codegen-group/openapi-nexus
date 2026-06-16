# Multipart and Binary Example

This example shows the two binary paths that openapi-nexus treats differently:

- `multipart/form-data` request bodies use operation-specific request body models. Binary parts use the generated `UploadFile` wrapper so callers can provide the HTTP `filename`.
- `application/octet-stream` request and response bodies stay as raw binary values for the target language.

Generate a client from this spec:

```bash
openapi-nexus generate \
  --input examples/multipart-binary/openapi.yaml \
  --output generated/multipart-binary/typescript \
  --generators typescript-fetch
```

Use a separate output directory per generator when generating more than one target language.

In the generated clients, the multipart `file` field remains the wire field name, while the filename can be supplied separately:

```ts
await new AvatarsApi().uploadAvatar({
  body: {
    file: { data: new Blob([bytes], { type: 'image/png' }), filename: 'avatar.png' },
    profile: { display_name: 'Ada Lovelace' },
    purpose: 'profile',
  },
});
```

If no filename is provided, generated clients fall back to the multipart field name. In this example the fallback is `file`.
