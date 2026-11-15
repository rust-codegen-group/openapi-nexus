import static org.junit.jupiter.api.Assertions.*;

import com.example.sdk.models.AliasListMap;
import com.example.sdk.models.AliasMap;
import com.example.sdk.models.MixedAliasMap;
import com.example.sdk.models.MixedControls;
import com.example.sdk.models.OptionalAliases;
import com.example.sdk.models.NestedAliases;
import com.example.sdk.models.Ordinary;
import com.example.sdk.models.RawMap;
import com.example.sdk.models.SiblingControls;
import com.example.sdk.models.TypedValue;
import com.example.sdk.runtime.OpaqueJsonAdapterFactory;
import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParseException;
import com.google.gson.JsonParser;
import com.google.gson.JsonPrimitive;
import com.google.gson.TypeAdapter;
import com.google.gson.reflect.TypeToken;
import com.google.gson.stream.JsonWriter;
import java.io.IOException;
import java.io.StringWriter;
import java.math.BigInteger;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import org.junit.jupiter.api.Test;

class AnyAliasTest {
    private static final String OPAQUE = "{\"nested_null\":null,\"positive\":9007199254740993,\"negative\":-9007199254740993,\"decimal\":1234567890.01234567890123456789,\"exponent\":1.234567890123456789e+40,\"array\":[null,{},[],true,\"text\"]}";
    private final Gson gson = new Gson();

    private void assertWire(String expected, Object value) {
        assertJson(JsonParser.parseString(expected), JsonParser.parseString(gson.toJson(value)));
    }

    // Gson's JsonPrimitive equality can compare numbers through doubleValue().
    // Check numeric token strings too so rounding cannot pass this contract.
    private void assertJson(JsonElement expected, JsonElement actual) {
        if (expected.isJsonObject()) {
            assertTrue(actual.isJsonObject(), actual.toString());
            JsonObject object = expected.getAsJsonObject();
            assertEquals(object.keySet(), actual.getAsJsonObject().keySet());
            for (String key : object.keySet()) {
                assertJson(object.get(key), actual.getAsJsonObject().get(key));
            }
        } else if (expected.isJsonArray()) {
            assertTrue(actual.isJsonArray(), actual.toString());
            JsonArray array = expected.getAsJsonArray();
            assertEquals(array.size(), actual.getAsJsonArray().size());
            for (int index = 0; index < array.size(); index++) {
                assertJson(array.get(index), actual.getAsJsonArray().get(index));
            }
        } else if (expected.isJsonNull()) {
            assertTrue(actual.isJsonNull(), actual.toString());
        } else {
            assertTrue(actual.isJsonPrimitive(), actual.toString());
            JsonPrimitive primitive = expected.getAsJsonPrimitive();
            assertEquals(primitive.isNumber(), actual.getAsJsonPrimitive().isNumber());
            assertEquals(primitive.isBoolean(), actual.getAsJsonPrimitive().isBoolean());
            assertEquals(primitive.isString(), actual.getAsJsonPrimitive().isString());
            assertEquals(primitive.getAsString(), actual.getAsString());
        }
    }

    @Test void opaqueAliasesRetainNullEntriesAndExactNumericTokens() {
        String wire = "{\"value\":" + OPAQUE + ",\"values\":[" + OPAQUE + "]}";
        Ordinary value = gson.fromJson(wire, Ordinary.class);

        Map<?, ?> object = assertInstanceOf(Map.class, value.getValue());
        assertTrue(object.containsKey("nested_null"));
        assertNull(object.get("nested_null"));
        assertEquals("9007199254740993", assertInstanceOf(Number.class, object.get("positive")).toString());
        assertEquals("-9007199254740993", assertInstanceOf(Number.class, object.get("negative")).toString());
        assertInstanceOf(Map.class, value.getValues().get(0));
        assertWire(wire, value);
    }

    @Test void aliasChainsAndNestedListMapContainersRetainOpaqueValues() {
        String wire = "{\"chain\":" + OPAQUE + ",\"lists\":[[" + OPAQUE + ",null],[]],\"maps\":{\"entry\":[" + OPAQUE + ",null],\"empty\":[]}}";
        NestedAliases value = gson.fromJson(wire, NestedAliases.class);

        assertInstanceOf(Map.class, value.getChain());
        assertInstanceOf(Map.class, value.getLists().get(0).get(0));
        assertNull(value.getLists().get(0).get(1));
        assertNull(value.getMaps().get("entry").get(1));
        assertWire(wire, value);
    }

    @Test void dictionaryAnythingAliasValuesRetainNullEntriesAndNumericTokens() {
        String wire = "{\"extra\":" + OPAQUE + ",\"nil\":null,\"empty\":{}}";
        AliasMap value = gson.fromJson(wire, AliasMap.class);

        assertInstanceOf(Map.class, value.getAdditionalProperties().get("extra"));
        assertTrue(value.getAdditionalProperties().containsKey("nil"));
        assertNull(value.getAdditionalProperties().get("nil"));
        assertWire(wire, value);
    }

    @Test void dictionaryListAliasValuesRetainNullAndEmptyLists() {
        String wire = "{\"extra\":[" + OPAQUE + ",null],\"nil\":null,\"empty\":[]}";
        AliasListMap value = gson.fromJson(wire, AliasListMap.class);

        assertInstanceOf(Map.class, value.getAdditionalProperties().get("extra").get(0));
        assertNull(value.getAdditionalProperties().get("extra").get(1));
        assertTrue(value.getAdditionalProperties().containsKey("nil"));
        assertNull(value.getAdditionalProperties().get("nil"));
        assertTrue(value.getAdditionalProperties().get("empty").isEmpty());
        assertWire(wire, value);
    }

    @Test void mixedAliasExtensionsRemainFlatAndPreserveOpaqueValues() {
        String wire = "{\"known_field\":\"fixed\",\"extra\":" + OPAQUE + ",\"nil\":null,\"empty\":{}}";
        MixedAliasMap value = gson.fromJson(wire, MixedAliasMap.class);

        assertEquals("fixed", value.getKnownField());
        assertFalse(value.getAdditionalProperties().containsKey("known_field"));
        assertNull(value.getAdditionalProperties().get("nil"));
        assertWire(wire, value);
    }

    @Test void directRawDictionaryRemainsTheWirePreservingControl() {
        String wire = "{\"extra\":" + OPAQUE + ",\"nil\":null,\"empty\":{}}";
        RawMap value = gson.fromJson(wire, RawMap.class);

        assertInstanceOf(JsonElement.class, value.getAdditionalProperties().get("extra"));
        assertTrue(value.getAdditionalProperties().get("nil").isJsonNull());
        assertWire(wire, value);
    }

    @Test void ordinaryTypedSiblingsKeepDefaultNullOmission() {
        String wire = "{\"value\":" + OPAQUE + ",\"typed\":{\"wire_value\":\"typed\",\"optional_note\":null,\"nullable_note\":null}}";
        SiblingControls value = gson.fromJson(wire, SiblingControls.class);

        assertNull(value.getTyped().getOptionalNote());
        assertNull(value.getTyped().getNullableNote());
        assertWire("{\"value\":" + OPAQUE + ",\"typed\":{\"wire_value\":\"typed\"}}", value);
        Gson explicitNulls = new GsonBuilder().serializeNulls().create();
        assertJson(JsonParser.parseString(wire), JsonParser.parseString(explicitNulls.toJson(value)));
    }

    @Test void optionalAliasFieldsKeepOrdinaryNullOmission() {
        OptionalAliases value = gson.fromJson("{}", OptionalAliases.class);
        assertWire("{}", value);
        Gson explicitNulls = new GsonBuilder().serializeNulls().create();
        assertJson(JsonParser.parseString("{\"value\":null,\"values\":null}"), explicitNulls.toJsonTree(value));
    }

    @Test void mixedDeclaredAliasFieldsAndExtensionsKeepTheirSeparateNullPolicies() {
        String wire = "{\"value\":" + OPAQUE + ",\"typed\":{\"wire_value\":\"typed\",\"optional_note\":null},\"extra\":" + OPAQUE + ",\"nil\":null}";
        String expected = "{\"value\":" + OPAQUE + ",\"typed\":{\"wire_value\":\"typed\"},\"extra\":" + OPAQUE + ",\"nil\":null}";
        for (MixedControls value : List.of(gson.fromJson(wire, MixedControls.class),
                gson.fromJson(JsonParser.parseString(wire), MixedControls.class))) {
            assertNull(value.getTyped().getOptionalNote());
            assertTrue(value.getAdditionalProperties().containsKey("nil"));
            assertWire(expected, value);
        }
    }

    @Test void nativeTypedValuesInsideOpaqueAliasesKeepTheirOwnNullOmission() {
        TypedValue typed = gson.fromJson("{\"wire_value\":\"typed\"}", TypedValue.class);
        Ordinary value = new Ordinary(typed, List.of(typed));

        assertWire("{\"value\":{\"wire_value\":\"typed\"},\"values\":[{\"wire_value\":\"typed\"}]}", value);
    }

    @Test void constructedAnyMapsAndListsKeepTheirExistingRuntimeApi() {
        Map<String, Object> object = new LinkedHashMap<>();
        object.put("nested_null", null);
        object.put("number", new BigInteger("9007199254740993"));
        Ordinary value = new Ordinary(object, List.of(object));

        assertSame(object, value.getValue());
        assertSame(object, value.getValues().get(0));
        assertWire("{\"value\":{\"nested_null\":null,\"number\":9007199254740993},\"values\":[{\"nested_null\":null,\"number\":9007199254740993}]}", value);
    }

    @Test void runtimeAdaptersRetainRootNullAndEmptyValues() throws IOException {
        OpaqueJsonAdapterFactory factory = new OpaqueJsonAdapterFactory();
        TypeAdapter<Object> any = factory.create(gson, TypeToken.get(Object.class));
        TypeAdapter<List<Object>> list = factory.create(gson, new TypeToken<List<Object>>() {});
        assertNotNull(any);
        assertNotNull(list);

        assertNull(any.fromJson("null"));
        assertNull(list.fromJson("null"));
        assertEquals("null", any.toJson(null));
        assertEquals("null", list.toJson(null));
        for (String wire : List.of("{}", "[]", "{\"nested_null\":null}")) {
            assertJson(JsonParser.parseString(wire), JsonParser.parseString(any.toJson(any.fromJson(wire))));
        }
        assertTrue(list.fromJson("[]").isEmpty());
        assertEquals("[]", list.toJson(list.fromJson("[]")));
    }

    @Test void listAliasRejectsWrongJsonKindsInsteadOfSilentlyProducingEmptyLists() {
        TypeAdapter<List<Object>> list = new OpaqueJsonAdapterFactory().create(gson, new TypeToken<List<Object>>() {});
        assertNotNull(list);
        for (String wire : List.of("{}", "42", "\"text\"", "false")) {
            assertThrows(IllegalStateException.class, () -> list.fromJson(wire), wire);
            assertThrows(JsonParseException.class,
                () -> gson.fromJson("{\"value\":{},\"values\":" + wire + "}", Ordinary.class), wire);
        }
    }

    @Test void writerNullPolicyIsRestoredAfterOpaqueValueSuccess() throws IOException {
        TypeAdapter<Object> adapter = new OpaqueJsonAdapterFactory().create(gson, TypeToken.get(Object.class));
        assertNotNull(adapter);
        Object value = adapter.fromJson(OPAQUE);
        for (boolean serializeNulls : new boolean[] {false, true}) {
            StringWriter sink = new StringWriter();
            JsonWriter writer = new JsonWriter(sink);
            writer.setSerializeNulls(serializeNulls);
            writer.setHtmlSafe(true);
            writer.beginArray();
            adapter.write(writer, value);
            assertEquals(serializeNulls, writer.getSerializeNulls());
            assertTrue(writer.isHtmlSafe());
            writer.beginObject();
            writer.name("ordinary_null").nullValue();
            writer.endObject();
            writer.endArray();
            String sibling = serializeNulls ? "{\"ordinary_null\":null}" : "{}";
            assertJson(JsonParser.parseString("[" + OPAQUE + "," + sibling + "]"), JsonParser.parseString(sink.toString()));
        }
    }

    @Test void writerNullPolicyIsRestoredWhenOpaqueEmissionFails() throws IOException {
        TypeAdapter<Object> adapter = new OpaqueJsonAdapterFactory().create(gson, TypeToken.get(Object.class));
        assertNotNull(adapter);
        Object value = adapter.fromJson("{\"nested_null\":null}");
        JsonWriter writer = new JsonWriter(new StringWriter()) {
            @Override public JsonWriter nullValue() throws IOException {
                throw new IOException("opaque-null");
            }
        };
        writer.setSerializeNulls(false);
        writer.setHtmlSafe(true);

        IOException failure = assertThrows(IOException.class, () -> adapter.write(writer, value));
        assertEquals("opaque-null", failure.getMessage());
        assertFalse(writer.getSerializeNulls());
        assertTrue(writer.isHtmlSafe());
    }
}
