package nullable_test

import (
	"encoding/json"
	"testing"

	"example.com/sdk/models"
)

func TestNullableCompositionsRetainNullAndExactObjectValues(t *testing.T) {
	for _, wire := range []string{`null`, `{"label":"kept","number":9007199254740993,"nil":null}`} {
		t.Run(wire, func(t *testing.T) {
			for name, value := range map[string]any{
				"object": new(models.InlineNullable),
				"map":    new(models.NullableMap),
				"double": new(models.DoubleNullable),
				"nested": new(models.NestedNullable),
				"alias":  new(models.AliasComposition),
			} {
				t.Run(name, func(t *testing.T) {
					if err := json.Unmarshal([]byte(wire), value); err != nil {
						t.Fatal(err)
					}
					data, err := json.Marshal(value)
					if err != nil {
						t.Fatal(err)
					}
					if string(data) != wire {
						t.Fatalf("want %s; got %s", wire, data)
					}
				})
			}
		})
	}
}

func TestNullableCompositionCanBeResetAndContained(t *testing.T) {
	var value models.DoubleNullable
	for _, wire := range []string{`{"label":"before"}`, `null`, `{"label":"after"}`} {
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
	var holder models.Holder
	if err := json.Unmarshal([]byte(`{"value":null}`), &holder); err != nil {
		t.Fatal(err)
	}
	data, err := json.Marshal(holder)
	if err != nil {
		t.Fatal(err)
	}
	if string(data) != `{"value":null}` {
		t.Fatalf("lost contained null: %s", data)
	}
}
