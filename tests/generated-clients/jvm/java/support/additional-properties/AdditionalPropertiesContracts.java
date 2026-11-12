import static org.junit.jupiter.api.Assertions.*;

import com.example.sdk.models.*;
import com.google.gson.*;
import com.google.gson.Gson;
import com.google.gson.reflect.TypeToken;
import com.google.gson.stream.JsonReader;
import com.google.gson.stream.JsonWriter;
import com.example.sdk.apis.DefaultApi;
import com.example.sdk.apis.EchoMixedBadRequest;
import com.example.sdk.apis.EchoMixedException;
import com.example.sdk.runtime.ApiClient;
import java.io.IOException;
import java.io.StringWriter;
import java.util.List;
import java.util.Map;
import org.junit.jupiter.api.Test;
import okhttp3.MediaType;
import okhttp3.OkHttpClient;
import okhttp3.Protocol;
import okhttp3.Response;
import okhttp3.ResponseBody;

abstract class AdditionalPropertiesContracts {
    final Gson gson = new Gson();

    <T> T roundTrip(String wire, Class<T> type) {
        T value = gson.fromJson(wire, type);
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(value, type));
        assertEquals(JsonParser.parseString(wire), JsonParser.parseString(gson.toJson(value, type)));
        return value;
    }

    @Test void opaqueExtensionsKeepVerbatimKeysAndValues() {
        roundTrip("""
            {"known_field":"fixed","knownField":"shadow","extra_key":{"nil":null,"large":9007199254740993}}
            """, OpaqueMap.class);
    }

    @Test void typedExtensionsKeepTheirNestedWireNames() {
        MixedMap value = roundTrip("""
            {"known_field":"fixed","extra_key":{"wire_value":"nested"}}
            """, MixedMap.class);
        assertEquals("nested", value.getAdditionalProperties().get("extra_key").getWireValue());
    }

    @Test void pureDictionaryKeepsItsEntries() {
        TypedMap value = roundTrip("{\"extra_key\":{\"wire_value\":\"nested\"}}", TypedMap.class);
        assertInstanceOf(NestedValue.class, value.getAdditionalProperties().get("extra_key"));
    }

    @Test void defaultsAreMutableAndIndependentWithoutChangingConstructors() {
        TypedMap first = new TypedMap();
        TypedMap second = new TypedMap();
        first.getAdditionalProperties().put("new_key", new NestedValue("new"));
        assertEquals("{\"new_key\":{\"wire_value\":\"new\"}}", gson.toJson(first));
        assertTrue(second.getAdditionalProperties().isEmpty());
        assertEquals("{\"known_field\":\"fixed\"}", gson.toJson(new MixedMap("fixed")));
        assertTrue(gson.fromJson("{}", TypedMap.class).getAdditionalProperties().isEmpty());
        assertEquals("{}", gson.toJson(new TypedMap()));
    }

    @Test void opaqueValuesKeepNullsNumbersAndStorageLookingKeys() {
        String wire = """
            {"additionalProperties":null,"": [true,null],"large":9007199254740993,"quote'\\\\key":{"null":null}}
            """;
        RawMap value = roundTrip(wire, RawMap.class);
        assertTrue(value.getAdditionalProperties().get("additionalProperties").isJsonNull());
        assertEquals("9007199254740993", value.getAdditionalProperties().get("large").getAsString());
    }

    @Test void nullableDeepAndRawContainersKeepEntryNulls() {
        NullableValues nullable = roundTrip("{\"nil\":null,\"model\":{\"wire_value\":\"nested\"}}", NullableValues.class);
        assertNull(nullable.getAdditionalProperties().get("nil"));
        assertInstanceOf(NestedValue.class, nullable.getAdditionalProperties().get("model"));
        DeepValues deep = roundTrip("{\"list\":[{\"keep_key\":{\"wire_value\":\"nested\"},\"nil\":null},null]}", DeepValues.class);
        assertInstanceOf(NestedValue.class, deep.getAdditionalProperties().get("list").get(0).get("keep_key"));
        roundTrip("{\"list\":[{\"nil\":null,\"large\":9007199254740993},null]}", RawContainers.class);
    }

    @Test void scalarAndEnumValuesKeepTheirDeclaredTypes() {
        LongValues longs = roundTrip("{\"large\":9007199254740993,\"negative\":-2}", LongValues.class);
        assertEquals(9007199254740993L, longs.getAdditionalProperties().get("large"));
        roundTrip("{\"count\":3,\"nil\":null}", IntegerValues.class);
        StatusValues status = roundTrip("{\"status\":\"active\"}", StatusValues.class);
        assertInstanceOf(Status.class, status.getAdditionalProperties().get("status"));
    }

    @Test void nativeScalarErrorsAreNotChangedByTreeConversion() {
        for (String invalid : List.of("1.5", "2147483648", "-2147483649", "\"bad\"")) {
            RuntimeException direct = assertThrows(RuntimeException.class, () -> gson.fromJson(invalid, java.lang.Integer.class));
            RuntimeException dictionary = assertThrows(RuntimeException.class, () -> gson.fromJson("{\"value\":" + invalid + "}", IntegerValues.class));
            assertEquals(direct.getClass(), dictionary.getClass());
        }
    }

    @Test void namedFieldsKeepNativeIntegerErrorsAndDuplicateHandling() {
        for (String wire : List.of("{\"count\":1.5}", "{\"count\":2147483648}", "{\"count\":1.5,\"count\":2}")) {
            RuntimeException plain = assertThrows(RuntimeException.class, () -> gson.fromJson(wire, PlainNumbers.class));
            RuntimeException opened = assertThrows(RuntimeException.class, () -> gson.fromJson(wire, OpenNumbers.class));
            assertEquals(plain.getClass(), opened.getClass());
        }
        assertEquals(2, gson.fromJson("{\"count\":1,\"count\":2}", OpenNumbers.class).getCount());
    }

    @Test void ordinaryTreeReaderCoercionRemainsDifferentFromStreamingReaderCoercion() {
        for (String wire : List.of("{\"count\":1.5}", "{\"count\":2147483648}")) {
            JsonElement tree = JsonParser.parseString(wire);
            PlainNumbers plain = gson.fromJson(tree, PlainNumbers.class);
            OpenNumbers opened = gson.fromJson(tree, OpenNumbers.class);
            assertEquals(plain.getCount(), opened.getCount());
        }
    }

    @Test void configuredSpecialFloatingPointWritingKeepsTheOrdinaryDelegateOutput() {
        Gson codec = new GsonBuilder().serializeSpecialFloatingPointValues().create();
        String wire = "{\"count\":2,\"weight\":\"NaN\"}";
        String plain = codec.toJson(codec.fromJson(wire, PlainNumbers.class));
        OpenNumbers opened = codec.fromJson(wire, OpenNumbers.class);
        assertEquals(plain, codec.toJson(opened));
        assertEquals(plain, codec.toJson(codec.fromJson(JsonParser.parseString(wire), OpenNumbers.class)));
        assertTrue(codec.toJsonTree(opened).getAsJsonObject().get("weight").getAsJsonPrimitive().isNumber());
        opened.getAdditionalProperties().put("nil", JsonNull.INSTANCE);
        assertEquals(plain.substring(0, plain.length() - 1) + ",\"nil\":null}", codec.toJson(opened));
    }

    @Test void ordinaryFieldNullPolicyIsUnchangedIncludingSerializeNulls() {
        for (Gson codec : List.of(gson, new GsonBuilder().serializeNulls().create())) {
            for (String wire : List.of("{}", "{\"count\":null}", "{\"count\":2,\"optional_count\":null,\"nullable_value\":null}")) {
                assertEquals(codec.toJsonTree(codec.fromJson(wire, PlainNumbers.class)),
                    codec.toJsonTree(codec.fromJson(wire, OpenNumbers.class)));
            }
        }
    }

    @Test void extensionEntryNullsDoNotForceNullFieldsInsideDeclaredModelValues() {
        TypedMap value = new TypedMap();
        value.getAdditionalProperties().put("model", new NestedValue(null));
        value.getAdditionalProperties().put("nil", null);
        assertEquals("{\"model\":{},\"nil\":null}", gson.toJson(value));
        Gson explicitNulls = new GsonBuilder().serializeNulls().create();
        assertEquals("{\"model\":{\"wire_value\":null},\"nil\":null}", explicitNulls.toJson(value));
    }

    @Test void absentDeclaredFieldsCannotBeImpersonatedByExtensions() {
        OptionalKnown value = new OptionalKnown(null);
        value.getAdditionalProperties().put("known_field", new JsonPrimitive("collision"));
        assertThrows(IllegalArgumentException.class, () -> gson.toJson(value));
        MixedMap mixed = new MixedMap("fixed");
        mixed.getAdditionalProperties().put("known_field", new NestedValue("bad"));
        assertThrows(IllegalArgumentException.class, () -> gson.toJson(mixed));
    }

    @Test void authoredStorageNamesAreNotRenamedOrOverwritten() {
        StorageCollision value = roundTrip("{\"additionalProperties\":\"known\",\"additional_properties\":{\"wire_value\":\"extra\"}}", StorageCollision.class);
        assertEquals("known", value.getAdditionalProperties());
        assertEquals("extra", value.getAdditionalProperties2().get("additional_properties").getWireValue());
    }

    @Test void aliasesKeepTheirExistingWrapperRepresentation() {
        AliasValues values = roundTrip("{\"key\":{\"value\":{\"wire_value\":\"nested\"}}}", AliasValues.class);
        assertEquals("nested", values.getAdditionalProperties().get("key").getValue().getWireValue());
        DictionaryAlias alias = roundTrip("{\"value\":{\"key\":{\"wire_value\":\"nested\"}}}", DictionaryAlias.class);
        assertInstanceOf(TypedMap.class, alias.getValue());
    }

    @Test void containingModelsKeepTypedDictionaries() {
        String wire = """
            {"mixed":{"known_field":"fixed","key":{"wire_value":"nested"}},"opaque":{"known_field":"fixed","nil":null},"dictionaries":[{"key":{"wire_value":"nested"}}]}
            """;
        Holder value = roundTrip(wire, Holder.class);
        assertInstanceOf(TypedMap.class, value.getDictionaries().get(0));
    }

    @Test void recursiveImportsAndValueAdaptersRemainUsable() {
        Recursive recursive = roundTrip("{\"wire_value\":\"root\",\"child\":{\"wire_value\":\"leaf\"}}", Recursive.class);
        assertInstanceOf(Recursive.class, recursive.getAdditionalProperties().get("child"));
        roundTrip("{\"one\":{\"two\":{}}}", PureRecursive.class);
        Nodes nodes = roundTrip("{\"root\":{\"wire_value\":\"root\",\"children\":{\"leaf\":{\"wire_value\":\"leaf\",\"children\":{}}}}}", Nodes.class);
        assertInstanceOf(Nodes.class, nodes.getAdditionalProperties().get("root").getChildren());
    }

    @Test void namedUnionsRetainTheDictionaryPayloadAndItsPolicyBoundary() {
        roundTrip("{\"model\":{\"wire_value\":\"nested\"},\"scalar\":2}", Choices.class);
        roundTrip("{\"key\":{\"wire_value\":\"nested\"}}", DictionaryChoice.class);
        roundTrip("{\"known_field\":\"fixed\",\"key\":{\"wire_value\":\"nested\"}}", MixedChoice.class);
    }

    @Test void closedAndImplicitObjectsKeepTheirExistingBehavior() {
        String wire = "{\"known_field\":\"fixed\",\"extra\":null}";
        assertEquals("{\"known_field\":\"fixed\"}", gson.toJson(gson.fromJson(wire, Closed.class)));
        assertEquals("{\"known_field\":\"fixed\"}", gson.toJson(gson.fromJson(wire, Implicit.class)));
    }

    @Test void rootNullAndWrongRootKindsKeepNativeFailures() {
        assertNull(gson.fromJson("null", TypedMap.class));
        assertEquals("null", gson.toJson(null, TypedMap.class));
        for (String wire : List.of("[]", "2", "\"bad\"")) {
            assertThrows(JsonSyntaxException.class, () -> gson.fromJson(wire, TypedMap.class));
        }
    }

    @Test void configuredValueAdaptersAndNamedFieldAdaptersAreRetained() {
        TypeAdapter<NestedValue> custom = new TypeAdapter<>() {
            public NestedValue read(JsonReader reader) throws IOException {
                return new NestedValue(reader.nextString());
            }
            public void write(JsonWriter writer, NestedValue value) throws IOException {
                if (value == null) writer.nullValue(); else writer.value(value.getWireValue());
            }
        };
        Gson codec = new GsonBuilder().registerTypeAdapter(NestedValue.class, custom).create();
        TypedMap value = codec.fromJson("{\"key\":\"custom\"}", TypedMap.class);
        assertEquals("custom", value.getAdditionalProperties().get("key").getWireValue());
        assertEquals("{\"key\":\"custom\"}", codec.toJson(value));
        OpenNumbers named = codec.fromJson("{\"count\":1,\"nullable_value\":\"named\"}", OpenNumbers.class);
        assertEquals("named", named.getNullableValue().getWireValue());
        assertEquals("{\"count\":1,\"nullable_value\":\"named\"}", codec.toJson(named));
    }

    @Test void configuredFieldNamesStillBelongToTheNamedFieldDelegate() {
        Gson codec = new GsonBuilder().setFieldNamingPolicy(FieldNamingPolicy.UPPER_CAMEL_CASE).create();
        OpenNumbers value = codec.fromJson("{\"Count\":2,\"extra\":null}", OpenNumbers.class);
        assertEquals(2, value.getCount());
        assertEquals(java.util.Set.of("extra"), value.getAdditionalProperties().keySet());
        assertEquals("{\"Count\":2,\"extra\":null}", codec.toJson(value));
        value.getAdditionalProperties().put("Optional_count", JsonNull.INSTANCE);
        // SerializedName on optional_count wins over the configured naming policy.
        assertTrue(codec.toJson(value).contains("Optional_count"));
        value.getAdditionalProperties().put("Count", JsonNull.INSTANCE);
        assertThrows(IllegalArgumentException.class, () -> codec.toJson(value));
    }

    private DefaultApi api(String path, int status, String reply, String request) {
        OkHttpClient transport = new OkHttpClient.Builder().addInterceptor(chain -> {
            assertEquals(path, chain.request().url().encodedPath());
            okio.Buffer buffer = new okio.Buffer();
            chain.request().body().writeTo(buffer);
            assertEquals(JsonParser.parseString(request), JsonParser.parseString(buffer.readUtf8()));
            return new Response.Builder().request(chain.request()).protocol(Protocol.HTTP_1_1)
                .code(status).message(status == 200 ? "OK" : "Bad Request")
                .header("Content-Type", "application/json")
                .body(ResponseBody.create(reply, MediaType.get("application/json"))).build();
        }).build();
        return new DefaultApi(new ApiClient("https://additional.test", transport, null, Map.of()));
    }

    @Test void httpRequestAndSuccessResponseRetainMixedExtensions() throws IOException {
        String wire = "{\"known_field\":\"fixed\",\"extra\":{\"wire_value\":\"nested\"}}";
        MixedMap value = api("/mixed", 200, wire, wire).echoMixed(gson.fromJson(wire, MixedMap.class)).getStatus200();
        assertEquals("nested", value.getAdditionalProperties().get("extra").getWireValue());
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(value));
    }

    @Test void httpRequestAndSuccessResponseRetainPureDictionaryEntries() throws IOException {
        String wire = "{\"extra\":{\"wire_value\":\"nested\"}}";
        TypedMap value = api("/dictionary", 200, wire, wire).echoDictionary(gson.fromJson(wire, TypedMap.class)).getStatus200();
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(value));
    }

    @Test void typedHttpErrorsRetainDictionaryBodiesAndLazyFailures() {
        String request = "{\"known_field\":\"fixed\"}";
        String wire = "{\"extra\":{\"wire_value\":\"nested\"}}";
        EchoMixedException error = assertThrows(EchoMixedException.class,
            () -> api("/mixed", 400, wire, request).echoMixed(new MixedMap("fixed")));
        EchoMixedBadRequest detail = assertInstanceOf(EchoMixedBadRequest.class, error.detail());
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(detail.body()));
        EchoMixedException malformed = assertThrows(EchoMixedException.class,
            () -> api("/mixed", 400, "{\"bad\":false}", request).echoMixed(new MixedMap("fixed")));
        EchoMixedBadRequest bad = assertInstanceOf(EchoMixedBadRequest.class, malformed.detail());
        assertThrows(JsonParseException.class, bad::body);
    }

    @Test void extensionNullPreservationDoesNotLeakIntoOrdinaryValues() throws IOException {
        TypedMap value = new TypedMap();
        value.getAdditionalProperties().put("nil", null);
        StringWriter buffer = new StringWriter();
        JsonWriter writer = new JsonWriter(buffer);
        writer.setSerializeNulls(false);
        writer.beginObject();
        writer.name("dictionary");
        gson.getAdapter(TypedMap.class).write(writer, value);
        assertFalse(writer.getSerializeNulls());
        writer.name("ordinary");
        writer.nullValue();
        writer.endObject();
        assertEquals("{\"dictionary\":{\"nil\":null}}", buffer.toString());
    }

    @Test void writerNullPolicyIsRestoredWhenAnExtensionWriteFails() throws IOException {
        TypedMap value = new TypedMap();
        value.getAdditionalProperties().put("nil", null);
        JsonWriter writer = new JsonWriter(new StringWriter()) {
            @Override public JsonWriter nullValue() throws IOException {
                throw new IOException("extension-write");
            }
        };
        writer.setSerializeNulls(false);
        IOException failure = assertThrows(IOException.class, () -> gson.getAdapter(TypedMap.class).write(writer, value));
        assertEquals("extension-write", failure.getMessage());
        assertFalse(writer.getSerializeNulls());
    }
}
