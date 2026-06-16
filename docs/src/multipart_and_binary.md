# Multipart and Binary Bodies

OpenAPI uses the same `type: string`, `format: binary` schema shape in several places, but generated clients intentionally expose different APIs depending on the HTTP body.

- Multipart binary parts use a generated upload wrapper so callers can provide a filename.
- Raw `application/octet-stream` request bodies stay as raw bytes or blob values.
- Binary response bodies stay as raw bytes or blob values.
- JSON and text multipart parts keep their normal generated model or scalar types.

See [`examples/multipart-binary/openapi.yaml`](https://github.com/adamcavendish/openapi-nexus/blob/master/examples/multipart-binary/openapi.yaml) for a complete spec.

## Multipart Uploads

For object-shaped `multipart/form-data` request bodies, openapi-nexus generates an operation-specific request body model. Binary properties in that model use an upload wrapper instead of the normal raw binary type.

Given this request body:

```yaml
requestBody:
  required: true
  content:
    multipart/form-data:
      schema:
        type: object
        required: [file, profile, purpose]
        properties:
          file:
            type: string
            format: binary
          profile:
            $ref: '#/components/schemas/ProfileAttributes'
          purpose:
            type: string
      encoding:
        file:
          contentType: image/png
        profile:
          contentType: application/json
        purpose:
          contentType: text/plain
```

The generated request model has a binary `file` upload field, a normal JSON `profile` field, and a normal text `purpose` field. The multipart field name remains `file`; the filename is read from the upload wrapper. If the caller does not provide a filename, generated clients fall back to the field name.

## Upload Filenames

Use the wrapper when the filename matters, for example when the multipart field name is `file` but the uploaded filename should be `avatar.png`.

### TypeScript

TypeScript accepts browser-native `File` values directly. It also accepts `{ data: Blob, filename?: string }` for runtimes or tests that have `Blob` but not `File`.

```ts
import { AvatarsApi } from './generated/apis/AvatarsApi';

const api = new AvatarsApi();

await api.uploadAvatar({
  body: {
    file: { data: new Blob([bytes], { type: 'image/png' }), filename: 'avatar.png' },
    profile: { display_name: 'Ada Lovelace' },
    purpose: 'profile',
  },
});

await api.uploadAvatar({
  body: {
    file: new File([bytes], 'avatar.png', { type: 'image/png' }),
    profile: { display_name: 'Ada Lovelace' },
    purpose: 'profile',
  },
});
```

Passing a plain `Blob` is still accepted, but the filename falls back to the multipart field name.

### Go

```go
body := &models.UploadAvatarMultipartRequestBody{
	File: runtime.NewUploadFile(pngBytes, "avatar.png"),
	Profile: models.ProfileAttributes{
		DisplayName: "Ada Lovelace",
	},
	Purpose: "profile",
}

resp, err := avatars.UploadAvatar(ctx, body)
```

Use `runtime.NewUploadFileBytes(pngBytes)` when the fallback filename is acceptable.

### Python

The same `UploadFile` wrapper is generated for `python-httpx` and `python-requests`.

```python
from generated.models.upload_avatar_multipart_request_body import UploadAvatarMultipartRequestBody
from generated.models.profile_attributes import ProfileAttributes
from generated.runtime import UploadFile

body = UploadAvatarMultipartRequestBody(
    file=UploadFile.from_bytes(png_bytes, filename="avatar.png"),
    profile=ProfileAttributes(display_name="Ada Lovelace"),
    purpose="profile",
)

client.avatars.upload_avatar(body=body)
```

Use `UploadFile.from_bytes(png_bytes)` when the fallback filename is acceptable.

### Java

```java
UploadAvatarMultipartRequestBody body = new UploadAvatarMultipartRequestBody(
    UploadFile.of(pngBytes, "avatar.png"),
    new ProfileAttributes(null, "Ada Lovelace"),
    "profile"
);

UploadAvatarResponse response = avatarsApi.uploadAvatar(body);
```

Use `UploadFile.ofBytes(pngBytes)` when the fallback filename is acceptable.

### Kotlin

```kotlin
val body = UploadAvatarMultipartRequestBody(
    file = UploadFile(pngBytes, "avatar.png"),
    profile = ProfileAttributes(displayName = "Ada Lovelace"),
    purpose = "profile",
)

val response = avatarsApi.uploadAvatar(body)
```

Use `UploadFile(pngBytes)` when the fallback filename is acceptable.

### Rust

The same `UploadFile` wrapper is generated for `rust-reqwest`, `rust-ureq`, and `rust-aioduct`.

```rust
let body = UploadAvatarMultipartRequestBody {
    file: UploadFile::new(png_bytes, "avatar.png"),
    profile: ProfileAttributes {
        display_name: "Ada Lovelace".to_string(),
        alt_text: None,
    },
    purpose: "profile".to_string(),
};

let response = avatars_api.upload_avatar(&body).await?;
```

Use `UploadFile::from_bytes(png_bytes)` when the fallback filename is acceptable.

## Raw Binary Bodies

For non-multipart `application/octet-stream` bodies, openapi-nexus does not use the upload wrapper. The method body type remains the normal binary type for the target language:

| Generator | Request body type |
|---|---|
| `typescript-fetch` | `Blob \| File` |
| `go-http` | `[]byte` |
| `python-httpx` | `bytes` |
| `python-requests` | `bytes` |
| `java-okhttp` | `byte[]` |
| `kotlin-okhttp` | `ByteArray` |
| `rust-reqwest` | `Vec<u8>` |
| `rust-ureq` | `Vec<u8>` |
| `rust-aioduct` | `Vec<u8>` |

This keeps ordinary binary request bodies and binary responses separate from multipart filename handling.

## Supported Multipart Shape

Multipart request bodies must be object-shaped. Each object property becomes one multipart part.

- Binary parts use `UploadFile` or `UploadFileInput`.
- String, number, boolean, and enum parts are emitted as text.
- Object and array parts are emitted as JSON.
- `encoding.<part>.contentType` controls the per-part `Content-Type` when present.

Schemas that do not describe an object-shaped multipart body are rejected by generators with an explicit unsupported multipart error.
