package composition_test

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"reflect"
	"strings"
	"testing"

	"example.com/sdk/apis"
	"example.com/sdk/models"
	"example.com/sdk/runtime"
)

func assertJSON(t *testing.T, expected string, value any) {
	t.Helper()
	data, err := json.Marshal(value)
	if err != nil {
		t.Fatal(err)
	}
	var want, got any
	if err := json.Unmarshal([]byte(expected), &want); err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(data, &got); err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(want, got) {
		t.Fatalf("want %s; got %s", expected, data)
	}
}

func TestHTTPRequestsAndSuccessResponsesKeepFlatComposition(t *testing.T) {
	wire := `{"known_field":"fixed","other_field":7,"nil":null}`
	captured := make(chan []byte, 1)
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != "POST" || r.URL.Path != "/composition" {
			t.Errorf("unexpected request %s %s", r.Method, r.URL.Path)
		}
		body, err := io.ReadAll(r.Body)
		if err != nil {
			t.Error(err)
		}
		captured <- body
		w.Header().Set("Content-Type", "application/json")
		_, _ = io.WriteString(w, wire)
	}))
	defer server.Close()
	var request models.Pair
	if err := json.Unmarshal([]byte(wire), &request); err != nil {
		t.Fatal(err)
	}
	response, err := apis.NewDefaultAPI(runtime.NewClient(server.URL)).EchoComposition(context.Background(), &request)
	if err != nil {
		t.Fatal(err)
	}
	assertJSON(t, wire, json.RawMessage(<-captured))
	assertJSON(t, wire, response.Status200)
}

func TestLazyHTTPErrorsRetainTypedCompositionAndNativeDecodeFailures(t *testing.T) {
	for _, wire := range []string{`{"known_field":"fixed","other_field":7,"nil":null}`, `{"known_field":false}`} {
		t.Run(wire, func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				w.Header().Set("Content-Type", "application/json")
				w.WriteHeader(400)
				_, _ = io.WriteString(w, wire)
			}))
			defer server.Close()
			_, err := apis.NewDefaultAPI(runtime.NewClient(server.URL)).EchoComposition(context.Background(), &models.Pair{})
			var failure *apis.EchoCompositionError
			if !errors.As(err, &failure) {
				t.Fatalf("unexpected HTTP error %v", err)
			}
			detail, ok := failure.Detail.(*apis.EchoCompositionBadRequest)
			if !ok {
				t.Fatalf("unexpected detail %T", failure.Detail)
			}
			body, decodeErr := detail.Body()
			if wire == `{"known_field":false}` {
				if decodeErr == nil {
					t.Fatal("native failure hidden")
				}
			} else {
				if decodeErr != nil {
					t.Fatal(decodeErr)
				}
				assertJSON(t, wire, body)
			}
		})
	}
}

func TestObjectAndMapMembersKeepFlatWireShape(t *testing.T) {
	wire := `{"known_field":"fixed","extra":{"nil":null},"knownField":"shadow"}`
	var value models.OpenAllOf
	if err := json.Unmarshal([]byte(wire), &value); err != nil {
		t.Fatal(err)
	}
	if value.Base.KnownField != "fixed" {
		t.Fatalf("lost named member: %#v", value)
	}
	assertJSON(t, wire, value)
	assertJSON(t, `{"known_field":"fixed"}`, models.OpenAllOf{Base: models.Base{KnownField: "fixed"}})
}

func TestNamedMembersWithCustomCodecsDoNotHideSiblings(t *testing.T) {
	wire := `{"known_field":"fixed","other_field":7,"nil":null}`
	var value models.Pair
	if err := json.Unmarshal([]byte(wire), &value); err != nil {
		t.Fatal(err)
	}
	if value.Base.KnownField != "fixed" || value.Other.OtherField != 7 {
		t.Fatalf("lost member: %#v", value)
	}
	if _, exists := value.Base.AdditionalProperties["other_field"]; exists {
		t.Fatal("sibling captured as extension")
	}
	assertJSON(t, wire, value)
	assertJSON(t, `{"known_field":"fixed","other_field":7}`, models.Pair{
		Base: models.Base{KnownField: "fixed"}, Other: models.Other{OtherField: 7},
	})
}

func TestNestedAndAliasCompositionsKeepTheirMemberEntrypoints(t *testing.T) {
	wire := `{"known_field":"fixed","other_field":7,"nil":null}`
	var nested models.Nested
	if err := json.Unmarshal([]byte(wire), &nested); err != nil {
		t.Fatal(err)
	}
	if nested.Pair.Other.OtherField != 7 {
		t.Fatal("lost nested member")
	}
	assertJSON(t, wire, nested)
	var aliased models.Aliased
	if err := json.Unmarshal([]byte(wire), &aliased); err != nil {
		t.Fatal(err)
	}
	if aliased.Alias.KnownField != "fixed" {
		t.Fatal("lost alias member")
	}
	assertJSON(t, wire, aliased)
}

func TestTypedAnonymousAndNamedDictionariesKeepValues(t *testing.T) {
	wire := `{"label":"known","extra":"typed"}`
	var direct models.TypedMap
	if err := json.Unmarshal([]byte(wire), &direct); err != nil {
		t.Fatal(err)
	}
	if direct.AdditionalProperties["extra"] != "typed" {
		t.Fatal("lost typed map")
	}
	assertJSON(t, wire, direct)
	var named models.NamedMap
	if err := json.Unmarshal([]byte(wire), &named); err != nil {
		t.Fatal(err)
	}
	if named.Strings.AdditionalProperties["extra"] != "typed" {
		t.Fatal("lost named dictionary")
	}
	assertJSON(t, wire, named)
	if err := json.Unmarshal([]byte(`{"label":"known","extra":2}`), &direct); err == nil {
		t.Fatal("native map error hidden")
	}
}

func TestRepeatedMembersAndAnonymousMapsAreDisambiguated(t *testing.T) {
	wire := `{"known_field":"fixed","nil":null}`
	var repeated models.Repeated
	if err := json.Unmarshal([]byte(wire), &repeated); err != nil {
		t.Fatal(err)
	}
	if repeated.Base.KnownField != repeated.Base2.KnownField {
		t.Fatal("duplicate member not decoded")
	}
	assertJSON(t, wire, repeated)
	var maps models.Maps
	if err := json.Unmarshal([]byte(`{"nil":null,"keep_key":[true]}`), &maps); err != nil {
		t.Fatal(err)
	}
	if len(maps.AdditionalProperties) != 2 || len(maps.AdditionalProperties2) != 2 {
		t.Fatal("missing map member")
	}
	assertJSON(t, `{"nil":null,"keep_key":[true]}`, maps)
}

func TestStorageDoesNotShadowAnAuthoredMember(t *testing.T) {
	var value models.StorageCollision
	wire := `{"label":"known","extra":true}`
	if err := json.Unmarshal([]byte(wire), &value); err != nil {
		t.Fatal(err)
	}
	if value.AdditionalProperties.Label != "known" {
		t.Fatal("authored member renamed")
	}
	if value.AdditionalProperties2["extra"] != true {
		t.Fatal("missing map storage")
	}
	assertJSON(t, wire, value)
}

func TestConflictingMemberWritesReturnAnErrorInsteadOfDroppingData(t *testing.T) {
	value := models.Repeated{Base: models.Base{KnownField: "one"}, Base2: models.Base{KnownField: "two"}}
	if _, err := json.Marshal(value); err == nil || !strings.Contains(err.Error(), "conflicting allOf property") {
		t.Fatalf("expected collision error, got %v", err)
	}
}

func TestNamedFieldEditsDoNotLeaveStaleCopiesInDictionaryMembers(t *testing.T) {
	var value models.OpenAllOf
	if err := json.Unmarshal([]byte(`{"known_field":"old","extra":true}`), &value); err != nil {
		t.Fatal(err)
	}
	if _, exists := value.AdditionalProperties["known_field"]; exists {
		t.Fatal("declared field duplicated in map storage")
	}
	value.Base.KnownField = "new"
	assertJSON(t, `{"known_field":"new","extra":true}`, value)
}

func TestSingleMemberAndContainingModelsKeepFlatPayloads(t *testing.T) {
	wire := `{"pair":{"known_field":"fixed","other_field":7},"nested":{"known_field":"fixed","other_field":7,"nil":null},"single":{"known_field":"fixed","nil":null}}`
	var holder models.Holder
	if err := json.Unmarshal([]byte(wire), &holder); err != nil {
		t.Fatal(err)
	}
	assertJSON(t, wire, holder)
}

func TestObjectCompositionRetainsNativeRootAndFieldFailures(t *testing.T) {
	for _, wire := range []string{`[]`, `42`, `"bad"`, `{"known_field":2}`} {
		var value models.Pair
		if err := json.Unmarshal([]byte(wire), &value); err == nil {
			t.Fatalf("accepted %s", wire)
		}
	}
	value := models.Pair{Base: models.Base{KnownField: "retained"}}
	if err := json.Unmarshal([]byte(`null`), &value); err != nil {
		t.Fatal(err)
	}
	if value.Base.KnownField != "retained" {
		t.Fatal("null changed existing object")
	}
}

func TestNonObjectAndCyclicIntersectionsUseRawJSONWithoutInvalidEmbedding(t *testing.T) {
	var scalar models.Scalar
	if err := json.Unmarshal([]byte(`9007199254740993`), &scalar); err != nil {
		t.Fatal(err)
	}
	if string(scalar) != `9007199254740993` {
		t.Fatal("raw number changed")
	}
	var array models.ArrayValue
	if err := json.Unmarshal([]byte(`["keep",null]`), &array); err != nil {
		t.Fatal(err)
	}
	assertJSON(t, `["keep",null]`, array)
	var cycle models.Cycle
	if err := json.Unmarshal([]byte(`{"nil":null}`), &cycle); err != nil {
		t.Fatal(err)
	}
	assertJSON(t, `{"nil":null}`, cycle)
	var incompatible models.Incompatible
	if err := json.Unmarshal([]byte(`{"label":"kept","raw":null}`), &incompatible); err != nil {
		t.Fatal(err)
	}
	assertJSON(t, `{"label":"kept","raw":null}`, incompatible)
}
