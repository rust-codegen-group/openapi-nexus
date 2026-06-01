# Authentication

Every generated SDK includes a runtime authentication module with built-in support for Bearer tokens, API keys, and HTTP Basic auth. Security schemes declared in your OpenAPI spec are reflected in the generated client interface — you supply the credentials when constructing the client, and they are automatically attached to every outgoing request.

## Authenticator Interface

All generators expose an `Authenticator` interface (trait in Rust, abstract base class in Python) that the generated client calls before each request. The runtime ships with three concrete implementations:

| Auth Type | Class / Struct | Wire Format |
|-----------|---------------|-------------|
| Bearer token | `BearerAuth` | `Authorization: Bearer <token>` |
| API key | `ApiKeyAuth` | Configurable header, query parameter, or cookie |
| HTTP Basic | `BasicAuth` | `Authorization: Basic <base64(user:pass)>` |

## Bearer Tokens

### Static Token

Pass a string at construction time. The same token is sent with every request.

**TypeScript**

```ts
import { Configuration, DefaultApi } from "./runtime";

const config = new Configuration({
  basePath: "https://api.example.com",
  accessToken: "my-static-token",
});

const api = new DefaultApi(config);
```

**Go**

```go
import "example.com/sdk/runtime"

client := runtime.NewClient("https://api.example.com",
    runtime.WithAuth(runtime.BearerAuth{Token: "my-static-token"}),
)
```

**Rust**

```rust
use sdk::runtime::{Client, BearerAuth};

let client = Client::new("https://api.example.com")
    .with_auth(BearerAuth::new("my-static-token"));
```

**Python**

```python
from sdk.runtime import Client, BearerAuth

client = Client(
    base_url="https://api.example.com",
    authenticator=BearerAuth("my-static-token"),
)
```

**Java**

```java
import com.example.sdk.runtime.ApiClient;
import com.example.sdk.runtime.BearerAuth;

ApiClient client = new ApiClient(
    "https://api.example.com",
    new OkHttpClient(),
    new BearerAuth("my-static-token"),
    Map.of()
);
```

**Kotlin**

```kotlin
import com.example.sdk.runtime.ApiClient
import com.example.sdk.runtime.BearerAuth

val client = ApiClient(
    baseUrl = "https://api.example.com",
    client = OkHttpClient(),
    authenticator = BearerAuth("my-static-token"),
    defaultHeaders = emptyMap()
)
```

### Dynamic Token Provider

When the token changes over time — after user login, or when refreshing an OAuth2 access token — pass a **function** instead of a string. The function is called on every request, so it always produces the current token.

**TypeScript**

```ts
const config = new Configuration({
  basePath: "https://api.example.com",
  accessToken: () => getCurrentBearerToken(),  // called per-request
});
```

The `accessToken` field accepts `string | Promise<string> | ((name?, scopes?) => string | Promise<string>)`. A plain function is called synchronously; an async function or Promise is awaited.

**Go**

```go
client := runtime.NewClient("https://api.example.com",
    runtime.WithAuth(runtime.BearerAuth{
        TokenProvider: func() string {
            return getCurrentToken()  // called per-request
        },
    }),
)
```

If both `Token` and `TokenProvider` are set, the provider takes precedence.

**Rust**

```rust
use sdk::runtime::{Client, BearerAuth};

let client = Client::new("https://api.example.com")
    .with_auth(BearerAuth::from_provider(|| {
        get_current_token()  // called per-request
    }));
```

`BearerAuth::new(token)` stores a static string. `BearerAuth::from_provider(fn)` wraps your closure in an `Arc` and evaluates it on every `authenticate()` call. The closure must be `Fn() -> String + Send + Sync + 'static`.

**Python**

```python
from sdk.runtime import Client, BearerAuth

client = Client(
    base_url="https://api.example.com",
    authenticator=BearerAuth(lambda: get_current_token()),  # called per-request
)
```

`BearerAuth` accepts `str | Callable[[], str]`. If you pass a callable, it is invoked inside `auth_headers()` on every request.

**Java**

```java
import java.util.function.Supplier;

ApiClient client = new ApiClient(
    "https://api.example.com",
    new OkHttpClient(),
    new BearerAuth(() -> getCurrentToken()),  // called per-request
    Map.of()
);
```

`BearerAuth` has two constructors: `BearerAuth(String token)` for static tokens, and `BearerAuth(Supplier<String> tokenProvider)` for dynamic ones.

**Kotlin**

```kotlin
val client = ApiClient(
    baseUrl = "https://api.example.com",
    client = OkHttpClient(),
    authenticator = BearerAuth { getCurrentToken() },  // called per-request
    defaultHeaders = emptyMap()
)
```

Kotlin's trailing-lambda syntax makes the provider form especially concise. `BearerAuth` has two constructors: `constructor(token: String)` and `constructor(tokenProvider: () -> String)`.

## API Key Authentication

`ApiKeyAuth` sends a named key in a header, query parameter, or cookie.

**Go**

```go
client := runtime.NewClient("https://api.example.com",
    runtime.WithAuth(runtime.ApiKeyAuth{
        Key:      "sk-abc123",
        Name:     "X-API-Key",
        Location: runtime.APIKeyInHeader,
    }),
)
```

**Rust**

```rust
use sdk::runtime::{Client, ApiKeyAuth};

let client = Client::new("https://api.example.com")
    .with_auth(ApiKeyAuth::new("X-API-Key", "sk-abc123"));
```

**Python**

```python
from sdk.runtime import Client, ApiKeyAuth

client = Client(
    base_url="https://api.example.com",
    authenticator=ApiKeyAuth(header_name="X-API-Key", api_key="sk-abc123"),
)
```

**Java**

```java
import com.example.sdk.runtime.ApiKeyAuth;
import com.example.sdk.runtime.ApiKeyLocation;

new ApiKeyAuth("sk-abc123", "X-API-Key", ApiKeyLocation.HEADER);
```

**Kotlin**

```kotlin
import com.example.sdk.runtime.ApiKeyAuth
import com.example.sdk.runtime.ApiKeyLocation

ApiKeyAuth("sk-abc123", "X-API-Key", ApiKeyLocation.HEADER)
```

## HTTP Basic Authentication

`BasicAuth` sends a base64-encoded `username:password` pair. Available in Go; for other languages, implement a custom `Authenticator`.

**Go**

```go
client := runtime.NewClient("https://api.example.com",
    runtime.WithAuth(runtime.BasicAuth{
        Username: "alice",
        Password: "s3cret",
    }),
)
```

## Custom Authenticators

Every generator's `Authenticator` is an open interface — you can implement it for custom auth schemes without modifying the generated code.

**TypeScript**

Use middleware:

```ts
const api = new DefaultApi(config).withPreMiddleware({
  pre: async (ctx) => {
    ctx.init.headers = { ...ctx.init.headers, 'X-Custom': 'value' };
    return ctx;
  },
});
```

**Go**

```go
type myAuth struct{}

func (myAuth) AuthenticateRequest(req *http.Request) error {
    req.Header.Set("X-Custom", "value")
    return nil
}
```

**Rust**

```rust
#[derive(Debug, Clone)]
struct MyAuth;

impl Authenticator for MyAuth {
    fn authenticate(&self, headers: &mut HeaderMap) -> Result<(), Error> {
        headers.insert(
            HeaderName::from_static("x-custom"),
            HeaderValue::from_static("value"),
        );
        Ok(())
    }
}
```

**Python**

```python
class MyAuth(Authenticator):
    def auth_headers(self) -> dict[str, str]:
        return {"X-Custom": "value"}
```

**Java**

```java
public class MyAuth implements Authenticator {
    @Override
    public void authenticate(Request.Builder builder) {
        builder.header("X-Custom", "value");
    }
}
```

**Kotlin**

```kotlin
class MyAuth : Authenticator {
    override fun authenticate(builder: Request.Builder) {
        builder.header("X-Custom", "value")
    }
}
```
