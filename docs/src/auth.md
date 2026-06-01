# Authentication

Every generated SDK includes a runtime authentication module with built-in support for Bearer tokens, API keys, and HTTP Basic auth. Security schemes declared in your OpenAPI spec are reflected in the generated client interface — you supply the credentials when constructing the client, and they are automatically attached to every outgoing request.

## Usage Patterns

### Static Token

The simplest case: a long-lived token known at startup.

**TypeScript**

```ts
import { Configuration, PetsApi } from "@example/petstore";

const api = new PetsApi(new Configuration({
  basePath: "https://api.example.com",
  accessToken: process.env.API_TOKEN,
}));

const pet = await api.getPetById({ petId: 42 });
```

**Go**

```go
client := runtime.NewClient("https://api.example.com",
    runtime.WithAuth(runtime.BearerAuth{Token: os.Getenv("API_TOKEN")}),
)
api := sdk.NewPetsApi(client)
pet, err := api.GetPetById(context.Background(), 42)
```

**Python**

```python
import os
from sdk import PetsApi
from sdk.runtime import Client, BearerAuth

api = PetsApi(Client(
    base_url="https://api.example.com",
    authenticator=BearerAuth(os.environ["API_TOKEN"]),
))
pet = api.get_pet_by_id(pet_id=42)
```

**Rust**

```rust
use sdk::PetsApi;
use sdk::runtime::{Client, BearerAuth};

let client = Client::new("https://api.example.com")
    .with_auth(BearerAuth::new(std::env::var("API_TOKEN").unwrap()));
let api = PetsApi::new(client);
let pet = api.get_pet_by_id(42).await?;
```

### Dynamic Token (OAuth2 refresh)

When tokens expire and need periodic refresh, pass a function that returns the current token. The function is called on **every request**, so it always picks up the latest value.

**TypeScript**

```ts
class TokenStore {
  private token: string | null = null;
  private expiresAt = 0;

  async getToken(): Promise<string> {
    if (Date.now() > this.expiresAt) {
      const res = await fetch("/oauth/token", {
        method: "POST",
        body: new URLSearchParams({
          grant_type: "client_credentials",
          client_id: process.env.CLIENT_ID!,
          client_secret: process.env.CLIENT_SECRET!,
        }),
      });
      const data = await res.json();
      this.token = data.access_token;
      this.expiresAt = Date.now() + data.expires_in * 1000;
    }
    return this.token!;
  }
}

const store = new TokenStore();
const api = new PetsApi(new Configuration({
  basePath: "https://api.example.com",
  accessToken: () => store.getToken(),  // evaluated per-request
}));
```

**Go**

```go
type TokenStore struct {
    mu        sync.Mutex
    token     string
    expiresAt time.Time
}

func (s *TokenStore) GetToken() string {
    s.mu.Lock()
    defer s.mu.Unlock()
    if time.Now().After(s.expiresAt) {
        // refresh logic ...
        s.token = refreshedToken
        s.expiresAt = time.Now().Add(1 * time.Hour)
    }
    return s.token
}

store := &TokenStore{}
client := runtime.NewClient("https://api.example.com",
    runtime.WithAuth(runtime.BearerAuth{
        TokenProvider: store.GetToken,  // evaluated per-request
    }),
)
```

**Python**

```python
import time
import httpx
from sdk import PetsApi
from sdk.runtime import Client, BearerAuth

class TokenStore:
    def __init__(self):
        self._token: str | None = None
        self._expires_at = 0.0

    def get_token(self) -> str:
        if time.time() > self._expires_at:
            resp = httpx.post("https://auth.example.com/oauth/token", data={
                "grant_type": "client_credentials",
                "client_id": os.environ["CLIENT_ID"],
                "client_secret": os.environ["CLIENT_SECRET"],
            })
            data = resp.json()
            self._token = data["access_token"]
            self._expires_at = time.time() + data["expires_in"]
        return self._token

store = TokenStore()
api = PetsApi(Client(
    base_url="https://api.example.com",
    authenticator=BearerAuth(store.get_token),  # evaluated per-request
))
```

**Rust**

```rust
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use sdk::PetsApi;
use sdk::runtime::{Client, BearerAuth};

struct TokenStore {
    token: Mutex<String>,
    expires_at: Mutex<Instant>,
}

impl TokenStore {
    fn get_token(&self) -> String {
        let mut expires = self.expires_at.lock().unwrap();
        if Instant::now() > *expires {
            // refresh logic ...
            *self.token.lock().unwrap() = refreshed_token;
            *expires = Instant::now() + Duration::from_secs(3600);
        }
        self.token.lock().unwrap().clone()
    }
}

let store = Arc::new(TokenStore { /* ... */ });
let s = Arc::clone(&store);
let client = Client::new("https://api.example.com")
    .with_auth(BearerAuth::from_provider(move || s.get_token()));
let api = PetsApi::new(client);
```

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

`ApiKeyAuth` sends a named key in a header, query parameter, or cookie. Like `BearerAuth`, it supports both static keys and dynamic key providers evaluated per-request.

### Static Key

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

**TypeScript**

TypeScript handles API keys through the `apiKey` field on `ConfigurationParameters`, which supports the same `string | Promise<string> | ((name: string) => string | Promise<string>)` union as `accessToken`. Because the wire placement (header name, query parameter, or cookie) depends on the OpenAPI security scheme definition, the key is available for use in custom middleware rather than being auto-wired:

```ts
const config = new Configuration({
  basePath: "https://api.example.com",
  apiKey: (name) => getApiKey(name),  // called with the scheme name
});
```

### Dynamic Key Provider

**Go**

```go
client := runtime.NewClient("https://api.example.com",
    runtime.WithAuth(runtime.ApiKeyAuth{
        KeyProvider: func() string {
            return getCurrentApiKey()  // called per-request
        },
        Name:     "X-API-Key",
        Location: runtime.APIKeyInHeader,
    }),
)
```

**Rust**

```rust
let client = Client::new("https://api.example.com")
    .with_auth(ApiKeyAuth::from_provider("X-API-Key", || {
        get_current_api_key()  // called per-request
    }));
```

**Python**

```python
client = Client(
    base_url="https://api.example.com",
    authenticator=ApiKeyAuth(
        header_name="X-API-Key",
        api_key=lambda: get_current_api_key(),  # called per-request
    ),
)
```

**Java**

```java
new ApiKeyAuth(() -> getCurrentApiKey(), "X-API-Key", ApiKeyLocation.HEADER);
```

**Kotlin**

```kotlin
ApiKeyAuth({ getCurrentApiKey() }, "X-API-Key", ApiKeyLocation.HEADER)
```

## HTTP Basic Authentication

`BasicAuth` sends a base64-encoded `username:password` pair. Available in Go and TypeScript; for other languages, implement a custom `Authenticator`.

### Static Credentials

**Go**

```go
client := runtime.NewClient("https://api.example.com",
    runtime.WithAuth(runtime.BasicAuth{
        Username: "alice",
        Password: "s3cret",
    }),
)
```

**TypeScript**

```ts
const config = new Configuration({
  basePath: "https://api.example.com",
  username: "alice",
  password: "s3cret",
});
```

### Dynamic Credentials

**Go**

```go
client := runtime.NewClient("https://api.example.com",
    runtime.WithAuth(runtime.BasicAuth{
        UsernameProvider: func() string { return getCurrentUser() },
        PasswordProvider: func() string { return getCurrentPass() },
    }),
)
```

**TypeScript**

```ts
const config = new Configuration({
  basePath: "https://api.example.com",
  username: () => getCurrentUser(),
  password: () => getCurrentPass(),
});
```

The `username` and `password` fields accept `string | (() => string | Promise<string>)`. A plain function is called synchronously; an async function is awaited. Basic auth is only applied when no `Authorization` header has already been set by `accessToken`.

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
