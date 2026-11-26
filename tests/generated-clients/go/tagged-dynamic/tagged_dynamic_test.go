package taggeddynamic_test

import (
	"encoding/json"
	"testing"

	"example.com/sdk/models"
)

func TestDynamicOptInKeepsLegacyAny(t *testing.T) {
	var value models.Choice
	if err := json.Unmarshal([]byte(`{"kind":"unknown","limit":9007199254740993}`), &value); err != nil {
		t.Fatal(err)
	}
	decoded, ok := value.(map[string]any)
	if !ok || decoded["kind"] != "unknown" || decoded["limit"] != float64(9007199254740992) {
		t.Fatalf("expected legacy dynamic conversion, got %#v", value)
	}
}
