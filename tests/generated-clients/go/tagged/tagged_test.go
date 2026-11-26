package tagged_test

import (
	"encoding/json"
	"errors"
	"reflect"
	"strings"
	"testing"

	"example.com/sdk/models"
)

func TestInferredUnionRejectsAnUnknownPresentDiscriminator(t *testing.T) {
	var value models.Choice
	wire := []byte(`{"kind":"unknown","label":"a","count":null}`)
	if err := json.Unmarshal(wire, &value); err == nil {
		t.Fatalf("unknown present tag must not match the optional branch: %#v", value)
	}
}

func TestAuthoredTagPresenceAndSpellings(t *testing.T) {
	for _, tc := range []struct {
		name, wire, caseTag, searchTag string
	}{
		{"absent", `{"label":"a","count":null}`, "", ""},
		{"custom", `{"kind":"custom","label":"a","count":null}`, `"custom"`, ""},
		{"explicit null", `{"kind":null,"label":"a","count":null}`, "null", ""},
		{"first search label", `{"kind":"search_v1","limit":9007199254740993}`, "", "search_v1"},
		{"second search label", `{"kind":"search_v2","limit":9007199254740993}`, "", "search_v2"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			var value models.Choice
			if err := json.Unmarshal([]byte(tc.wire), &value); err != nil {
				t.Fatal(err)
			}
			if string(value.Tag) != tc.caseTag || (tc.caseTag == "" && value.Tag != nil) {
				t.Fatalf("case-owned tag: got %q, want %q", value.Tag, tc.caseTag)
			}
			if tc.searchTag != "" {
				if value.Search == nil || value.Custom != nil || string(value.Search.Kind) != tc.searchTag || value.Search.Limit != 9007199254740993 {
					t.Fatalf("selected search payload did not retain its values: %#v", value)
				}
			} else if value.Custom == nil || value.Search != nil || value.Custom.Label != "a" || value.Custom.Count != nil {
				t.Fatalf("selected custom payload did not retain its values: %#v", value)
			}
			assertWire(t, tc.wire, value)
		})
	}
}

func TestConstructedVariantsValidateMembershipAndTagOwnership(t *testing.T) {
	assertWire(t, `{"label":"a","kind":"custom","count":null}`, models.Choice{
		Tag: json.RawMessage(`"custom"`), Custom: &models.Note{Label: "a"},
	})
	// The reusable model keeps its ordinary standalone semantics.
	assertWire(t, `{"label":"a"}`, models.Note{Label: "a"})
	for _, tc := range []struct {
		value models.Choice
		want  string
	}{
		{models.Choice{}, "exactly one branch"},
		{models.Choice{Custom: &models.Note{}, Search: &models.Search{}}, "exactly one branch"},
		{models.Choice{Tag: json.RawMessage(`"search_v1"`), Search: &models.Search{Kind: "search_v1"}}, "owns its discriminator"},
		{models.Choice{Tag: json.RawMessage(`"unknown"`), Custom: &models.Note{}}, "does not match its member"},
	} {
		if _, err := json.Marshal(tc.value); err == nil || !strings.Contains(err.Error(), tc.want) {
			t.Errorf("want %q, got %v", tc.want, err)
		}
	}
}

func TestFirstMatchUsesAuthoredOrder(t *testing.T) {
	var value models.Ambiguous
	if err := json.Unmarshal([]byte(`{"label":"both"}`), &value); err != nil {
		t.Fatal(err)
	}
	if value.Alpha == nil || value.Beta != nil {
		t.Fatalf("expected the first authored alternative, got %#v", value)
	}
	assertWire(t, `{"label":"both"}`, value)
}

func TestNativePayloadCannotReadCaseAliasesAsTheDiscriminator(t *testing.T) {
	var choice models.Choice
	if err := json.Unmarshal([]byte(`{"kind":"search_v1","limit":5,"KIND":"unknown"}`), &choice); err != nil {
		t.Fatal(err)
	}
	if choice.Search == nil || choice.Search.Kind != "search_v1" {
		t.Fatalf("payload must use the exact authored property, got %#v", choice)
	}
	assertWire(t, `{"kind":"search_v1","limit":5}`, choice)
}

func TestSelectedCodecKeepsNativeDecodeErrors(t *testing.T) {
	var value models.Choice
	// The selection probe considers the last duplicate. The native decoder
	// still sees the earlier incompatible payload value and returns its error.
	err := json.Unmarshal([]byte(`{"kind":"custom","label":0,"label":"a","count":null}`), &value)
	var typeError *json.UnmarshalTypeError
	if !errors.As(err, &typeError) || typeError.Field != "label" {
		t.Fatalf("want native label type error, got %v", err)
	}
}

func TestNestedUnionReceivesItsOriginalTagAndNumberEvidenceIsExact(t *testing.T) {
	wire := `{"kind":"search_v2","limit":9}`
	var nested models.Nested
	if err := json.Unmarshal([]byte(wire), &nested); err != nil {
		t.Fatal(err)
	}
	if nested.Custom == nil || nested.Custom.Search == nil || nested.Custom.Search.Kind != "search_v2" || nested.Tag != nil {
		t.Fatalf("nested codec did not receive and own the tag: %#v", nested)
	}
	assertWire(t, wire, nested)
	for _, number := range []string{"9007199254740992", "9007199254740994"} {
		if err := json.Unmarshal([]byte(`{"kind":"number","amount":`+number+`}`), &nested); err == nil {
			t.Errorf("rounded neighboring number %s must not match const", number)
		}
	}
	wire = `{"kind":"number","amount":9007199254740993}`
	if err := json.Unmarshal([]byte(wire), &nested); err != nil {
		t.Fatal(err)
	}
	if nested.Number == nil || nested.Custom != nil || nested.Number.Amount != 9007199254740993 {
		t.Fatalf("exact number did not select its branch: %#v", nested)
	}
	assertWire(t, wire, nested)
}

func TestInferredObjectUnionCanSelectAScalarReference(t *testing.T) {
	const wire = `9007199254740993`
	var value models.Nested
	if err := json.Unmarshal([]byte(wire), &value); err != nil {
		t.Fatal(err)
	}
	if value.Counter == nil || value.Tag != nil {
		t.Fatalf("scalar member cannot carry an internal tag: %#v", value)
	}
	assertWire(t, wire, value)
}

func TestProjectedBranchRetainsNullInNestedNativeModel(t *testing.T) {
	const wire = `{"kind":"annotated","detail":{"label":"note","count":null}}`
	var value models.Nested
	if err := json.Unmarshal([]byte(wire), &value); err != nil {
		t.Fatal(err)
	}
	if value.Annotated == nil {
		t.Fatalf("expected annotated member, got %#v", value)
	}
	assertWire(t, wire, value)
}

func TestOrdinaryNestedModelFieldsAndArrayUseTypedCodecs(t *testing.T) {
	wire := `{"value":{"label":"a","count":null},"history":[{"label":"b","count":2,"kind":null}]}`
	var envelope models.Envelope
	if err := json.Unmarshal([]byte(wire), &envelope); err != nil {
		t.Fatal(err)
	}
	if envelope.Value == nil || envelope.Value.Custom == nil || len(*envelope.History) != 1 || string((*envelope.History)[0].Tag) != "null" {
		t.Fatalf("nested model fields did not dispatch: %#v", envelope)
	}
	assertWire(t, wire, envelope)
}

func TestAuthoredEnvelopeAndClosedBranchConstraints(t *testing.T) {
	var value models.Projected
	for _, wire := range []string{
		`{"kind":"map","settings":{"limit":1}}`,
		`{"kind":"list","scope":"x","items":["wrong"]}`,
		`{"kind":"list","scope":"x","items":[1],"unknown":true}`,
		`{"kind":"map","scope":"x","settings":{"limit":1.5}}`,
	} {
		if err := json.Unmarshal([]byte(wire), &value); err == nil {
			t.Errorf("expected shape mismatch for %s", wire)
		}
	}
	for _, wire := range []string{
		`{"kind":"map","scope":"x","settings":{"limit":1}}`,
		`{"kind":"list_alt","scope":"x","items":[true,1.5]}`,
	} {
		if err := json.Unmarshal([]byte(wire), &value); err != nil {
			t.Fatal(err)
		}
		assertWire(t, wire, value)
	}
}

func assertWire(t *testing.T, wire string, value any) {
	t.Helper()
	data, err := json.Marshal(value)
	if err != nil {
		t.Fatal(err)
	}
	decode := func(data string) any {
		decoder := json.NewDecoder(strings.NewReader(data))
		decoder.UseNumber()
		var value any
		if err := decoder.Decode(&value); err != nil {
			t.Fatal(err)
		}
		return value
	}
	if !reflect.DeepEqual(decode(string(data)), decode(wire)) {
		t.Fatalf("want %s; got %s", wire, data)
	}
}
