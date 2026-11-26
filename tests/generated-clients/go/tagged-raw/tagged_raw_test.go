package taggedraw_test

import (
	"bytes"
	"encoding/json"
	"testing"

	"example.com/sdk/models"
)

func TestRawSkipsDispatchAndPreservesJSON(t *testing.T) {
	for _, wire := range []string{
		`null`, `42`, `[]`, `{"kind":"unknown","a_b":9007199254740993,"a_b":9e+999}`,
	} {
		var value models.Choice
		if err := json.Unmarshal([]byte(wire), &value); err != nil {
			t.Fatal(err)
		}
		if !bytes.Equal(value, []byte(wire)) {
			t.Fatalf("raw value: want %s, got %s", wire, value)
		}
		data, err := json.Marshal(value)
		if err != nil || string(data) != wire {
			t.Fatalf("raw output: want %s, got %s (%v)", wire, data, err)
		}
	}
}

func TestRawNestedFieldsDistinguishNullFromAbsence(t *testing.T) {
	for _, tc := range []struct {
		wire string
		tag  string
	}{
		{`{}`, ""},
		{`{"value":null}`, "null"},
	} {
		var value models.Envelope
		if err := json.Unmarshal([]byte(tc.wire), &value); err != nil {
			t.Fatal(err)
		}
		if string(value.Value) != tc.tag || (tc.tag == "" && value.Value != nil) {
			t.Fatalf("nested alias: want %q, got %q", tc.tag, value.Value)
		}
		data, err := json.Marshal(value)
		if err != nil || string(data) != tc.wire {
			t.Fatalf("nested output: want %s, got %s (%v)", tc.wire, data, err)
		}
	}
	const wire = `{"history":[null,{"large_number":9007199254740993}]}`
	var history models.Envelope
	if err := json.Unmarshal([]byte(wire), &history); err != nil {
		t.Fatal(err)
	}
	data, err := json.Marshal(history)
	if err != nil || string(data) != wire {
		t.Fatalf("raw array: want %s, got %s (%v)", wire, data, err)
	}
}
