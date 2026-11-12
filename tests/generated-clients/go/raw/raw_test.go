package raw_test

import (
	"encoding/json"
	"testing"

	"example.com/sdk/models"
)

func TestExplicitRawPolicyKeepsEveryJSONKindAndExactNumbers(t *testing.T) {
	for _, wire := range []string{`null`, `42`, `[]`, `{"known_field":"fixed","large":9007199254740993,"nil":null}`} {
		var value models.OpenAllOf
		if err := json.Unmarshal([]byte(wire), &value); err != nil {
			t.Fatal(err)
		}
		data, err := json.Marshal(value)
		if err != nil {
			t.Fatal(err)
		}
		if string(data) != wire {
			t.Fatalf("want %s; got %s", wire, data)
		}
	}
}
