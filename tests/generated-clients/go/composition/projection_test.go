package composition_test

import (
	"encoding/json"
	"testing"

	"example.com/sdk/models"
)

func TestProjectionRetainsNativeCaseMatchingOrder(t *testing.T) {
	for _, wire := range []string{
		`{"id":"extension","ID":"member"}`,
		`{"ID":"member","id":"extension"}`,
		`{"id":"extension","\u0049D":"member","ID":"last"}`,
	} {
		t.Run(wire, func(t *testing.T) {
			var direct models.CaseBase
			if err := json.Unmarshal([]byte(wire), &direct); err != nil {
				t.Fatal(err)
			}
			var composed models.CaseSingle
			if err := json.Unmarshal([]byte(wire), &composed); err != nil {
				t.Fatal(err)
			}
			if composed.CaseBase.Id != direct.Id {
				t.Fatalf("native member %q became %q", direct.Id, composed.CaseBase.Id)
			}
			data, err := json.Marshal(direct)
			if err != nil {
				t.Fatal(err)
			}
			assertJSON(t, string(data), composed)
		})
	}
}

func TestNestedProjectionFiltersSiblingsWithoutReorderingCaseDistinctKeys(t *testing.T) {
	wire := `{"id":"extension","other_field":7,"ID":"member"}`
	var value models.CaseNested
	if err := json.Unmarshal([]byte(wire), &value); err != nil {
		t.Fatal(err)
	}
	if value.CasePair.CaseBase.Id != "member" || value.CasePair.Other.OtherField != 7 {
		t.Fatalf("lost member: %#v", value)
	}
	if _, exists := value.CasePair.CaseBase.AdditionalProperties["other_field"]; exists {
		t.Fatal("sibling captured as extension")
	}
	assertJSON(t, wire, value)
}

func TestProjectionRetainsNativeErrorsFromEarlierDuplicateKeys(t *testing.T) {
	wire := `{"ID":false,"ID":"member"}`
	var direct models.CaseBase
	if err := json.Unmarshal([]byte(wire), &direct); err == nil {
		t.Fatal("native control accepted invalid field")
	}
	var value models.CaseSingle
	if err := json.Unmarshal([]byte(wire), &value); err == nil {
		t.Fatal("projection hid the earlier duplicate's native error")
	}
}
