package taggedstrict_test

import (
	"encoding/json"
	"strings"
	"testing"

	"example.com/sdk/models"
)

func TestStrictReportsAmbiguityAcrossAuthoredMembers(t *testing.T) {
	var value models.Ambiguous
	if err := json.Unmarshal([]byte(`{"label":"both"}`), &value); err == nil || !strings.Contains(err.Error(), "multiple concrete members") {
		t.Fatalf("expected an ambiguity error, got %v", err)
	}
	if err := json.Unmarshal([]byte(`{"label":"both","kind":"beta_alt"}`), &value); err != nil {
		t.Fatal(err)
	}
	if value.Beta == nil || value.Alpha != nil || string(value.Tag) != `"beta_alt"` {
		t.Fatalf("the authored tag disambiguates the branch: %#v", value)
	}
}

func TestProjectedScopeStillRunsTheSelectedNestedCodec(t *testing.T) {
	var value models.Scoped
	if err := json.Unmarshal([]byte(`{"scope":"local","label":"both"}`), &value); err == nil || !strings.Contains(err.Error(), "ambiguous inferred tagged union Ambiguous") {
		t.Fatalf("the selected nested strict codec must reject its ambiguity, got %v", err)
	}
	const valid = `{"kind":"beta_alt","label":"both","scope":"local"}`
	if err := json.Unmarshal([]byte(valid), &value); err != nil {
		t.Fatal(err)
	}
	encoded, err := json.Marshal(value)
	if err != nil || string(encoded) != valid {
		t.Fatalf("the nested codec must receive the tag without losing the scope: %s (%v)", encoded, err)
	}
}

func TestProjectedObjectStillPropagatesNestedCodecErrors(t *testing.T) {
	var value models.Nested
	const wire = `{"kind":"annotated","detail":{"label":"x","count":null},"child":{"label":"both"}}`
	err := json.Unmarshal([]byte(wire), &value)
	if err == nil || !strings.Contains(err.Error(), "ambiguous inferred tagged union Ambiguous") {
		t.Fatalf("the projected object must still run its selected nested codec, got %v", err)
	}
}
