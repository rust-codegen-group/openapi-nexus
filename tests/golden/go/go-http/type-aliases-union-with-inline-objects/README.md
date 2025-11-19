# union-type-with-inline-objects-test

Test fixture for union types (oneOf) with inline object schemas that should generate named interfaces

**Version:** 1.0.0

## Installation

```bash
go get example.com/sdk
```

## Usage

```go
package main

import (
    "context"
    "example.com/sdk"
)

func main() {
    sdk := union-type-with-inline-objects-test.New("https://api.example.com")
    // Use the SDK...
}
```
