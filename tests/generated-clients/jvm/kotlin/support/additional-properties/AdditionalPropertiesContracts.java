import static org.junit.jupiter.api.Assertions.*;

import com.example.sdk.models.*;
import com.example.sdk.apis.DefaultApi;
import com.example.sdk.apis.EchoMixedBadRequest;
import com.example.sdk.apis.EchoMixedException;
import com.example.sdk.runtime.ApiClient;
import com.google.gson.*;
import com.google.gson.stream.JsonReader;
import com.google.gson.stream.JsonWriter;
import java.io.IOException;
import java.io.StringWriter;
import java.lang.reflect.Method;
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
        roundTrip("{\"known_field\":\"fixed\",\"knownField\":\"shadow\",\"extra\":{\"nil\":null,\"large\":9007199254740993}}", OpaqueMap.class);
    }

    @Test void typedExtensionsKeepTheirNestedWireNames() {
        MixedMap value = roundTrip("{\"known_field\":\"fixed\",\"extra\":{\"wire_value\":\"nested\"}}", MixedMap.class);
        assertEquals("nested", value.getAdditionalProperties().get("extra").getWireValue());
    }

    @Test void existingPureDictionaryCodecStillKeepsItsEntries() {
        TypedMap value = roundTrip("{\"extra\":{\"wire_value\":\"nested\"},\"nil\":null}", TypedMap.class);
        assertInstanceOf(NestedValue.class, value.getAdditionalProperties().get("extra"));
    }

    @Test void oldJavaConstructorsAndIndependentDefaultsRemainUsable() {
        MixedMap first = new MixedMap("fixed");
        MixedMap second = new MixedMap("fixed");
        first.getAdditionalProperties().put("extra", new NestedValue("new"));
        assertEquals("{\"known_field\":\"fixed\",\"extra\":{\"wire_value\":\"new\"}}", gson.toJson(first));
        assertTrue(second.getAdditionalProperties().isEmpty());
        assertEquals("{\"known_field\":\"fixed\"}", gson.toJson(second));
        assertNotEquals(first, second);
        assertTrue(new OptionalKnown(null).getAdditionalProperties().isEmpty());
        assertTrue(new OptionalKnown().getAdditionalProperties().isEmpty());
        NamedOpaque opaque = new NamedOpaque("ordinary", List.of("ordinary"));
        assertEquals("ordinary", opaque.getKnownField());
        assertEquals("{\"known_field\":\"ordinary\",\"known_values\":[\"ordinary\"]}", gson.toJson(opaque));
    }

    @Test void componentsAndDefaultCopyKeepTypedExtensionStorage() throws Exception {
        MixedMap value = roundTrip("{\"known_field\":\"fixed\",\"extra\":{\"wire_value\":\"nested\"}}", MixedMap.class);
        assertEquals("fixed", value.component1());
        assertSame(value.getAdditionalProperties(), value.component2());
        // This is the JVM entrypoint used by Kotlin's ordinary value.copy().
        Method copy = MixedMap.class.getDeclaredMethod("copy$default", MixedMap.class, String.class, Map.class, int.class, Object.class);
        MixedMap duplicate = (MixedMap) copy.invoke(null, value, null, null, 3, null);
        assertEquals(value, duplicate);
        assertSame(value.getAdditionalProperties(), duplicate.getAdditionalProperties());
        assertEquals(gson.toJsonTree(value), gson.toJsonTree(duplicate));
        MixedMap changed = value.copy("changed", value.getAdditionalProperties());
        assertEquals("changed", changed.component1());
        assertEquals("nested", changed.getAdditionalProperties().get("extra").getWireValue());
    }

    @Test void opaqueValuesKeepNullsLargeNumbersAndStorageLookingKeys() {
        OpaqueMap value = roundTrip("{\"known_field\":\"fixed\",\"additionalProperties\":null,\"\":[true,null],\"large\":9007199254740993}", OpaqueMap.class);
        assertTrue(value.getAdditionalProperties().get("additionalProperties").isJsonNull());
        assertEquals("9007199254740993", value.getAdditionalProperties().get("large").getAsString());
    }

    @Test void nullableDeepAndRawContainersRetainExtensionNulls() {
        NullableValues nullable = roundTrip("{\"known_field\":\"fixed\",\"nil\":null,\"model\":{\"wire_value\":\"nested\"}}", NullableValues.class);
        assertNull(nullable.getAdditionalProperties().get("nil"));
        assertInstanceOf(NestedValue.class, nullable.getAdditionalProperties().get("model"));
        DeepValues deep = roundTrip("{\"known_field\":\"fixed\",\"list\":[{\"keep_key\":{\"wire_value\":\"nested\"},\"nil\":null},null]}", DeepValues.class);
        assertInstanceOf(NestedValue.class, deep.getAdditionalProperties().get("list").get(0).get("keep_key"));
        roundTrip("{\"known_field\":\"fixed\",\"list\":[{\"nil\":null,\"large\":9007199254740993},null]}", RawContainers.class);
    }

    @Test void scalarAndEnumExtensionValuesKeepTheirDeclaredTypes() {
        LongValues longs = roundTrip("{\"known_field\":\"fixed\",\"large\":9007199254740993,\"negative\":-2}", LongValues.class);
        assertEquals(9007199254740993L, longs.getAdditionalProperties().get("large"));
        StatusValues status = roundTrip("{\"known_field\":\"fixed\",\"status\":\"active\"}", StatusValues.class);
        assertInstanceOf(Status.class, status.getAdditionalProperties().get("status"));
    }

    @Test void scalarFailuresUseTheOriginalNativeReader() {
        for (String invalid : List.of("1.5", "2147483648", "-2147483649", "\"bad\"")) {
            RuntimeException direct = assertThrows(RuntimeException.class, () -> gson.fromJson(invalid, Integer.class));
            RuntimeException mixed = assertThrows(RuntimeException.class, () -> gson.fromJson("{\"known_field\":\"fixed\",\"value\":" + invalid + "}", IntegerValues.class));
            assertEquals(direct.getClass(), mixed.getClass());
        }
    }

    @Test void namedFieldsKeepNativeIntegerErrorsAndDuplicateHandling() {
        for (String wire : List.of("{\"count\":1.5}", "{\"count\":2147483648}", "{\"count\":1.5,\"count\":2}")) {
            RuntimeException plain = assertThrows(RuntimeException.class, () -> gson.fromJson(wire, PlainNumbers.class));
            RuntimeException mixed = assertThrows(RuntimeException.class, () -> gson.fromJson(wire, OpenNumbers.class));
            assertEquals(plain.getClass(), mixed.getClass());
        }
        assertEquals(2, gson.fromJson("{\"count\":1,\"count\":2}", OpenNumbers.class).getCount());
    }

    @Test void ordinaryTreeCoercionRemainsDistinctFromStreamingCoercion() {
        for (String wire : List.of("{\"count\":1.5}", "{\"count\":2147483648}")) {
            JsonElement tree = JsonParser.parseString(wire);
            assertEquals(gson.fromJson(tree, PlainNumbers.class).getCount(), gson.fromJson(tree, OpenNumbers.class).getCount());
        }
    }

    @Test void ordinaryFieldNullOmissionAndSerializeNullsStayUnchanged() {
        for (Gson codec : List.of(gson, new GsonBuilder().serializeNulls().create())) {
            for (String wire : List.of("{}", "{\"count\":null}", "{\"count\":2,\"optional_count\":null,\"nullable_value\":null}")) {
                assertEquals(codec.toJsonTree(codec.fromJson(wire, PlainNumbers.class)), codec.toJsonTree(codec.fromJson(wire, OpenNumbers.class)));
            }
        }
    }

    @Test void extensionEntryNullsDoNotForceOrdinaryNestedFieldNulls() {
        // Gson can construct the same missing-required-field value as before.
        NestedValue nested = gson.fromJson("{}", NestedValue.class);
        MixedMap value = new MixedMap("fixed");
        value.getAdditionalProperties().put("model", nested);
        value.getAdditionalProperties().put("nil", null);
        assertEquals("{\"known_field\":\"fixed\",\"model\":{},\"nil\":null}", gson.toJson(value));
        Gson explicit = new GsonBuilder().serializeNulls().create();
        assertEquals("{\"known_field\":\"fixed\",\"model\":{\"wire_value\":null},\"nil\":null}", explicit.toJson(value));
    }

    @Test void declaredFieldsCannotBeImpersonatedByExtensions() {
        OptionalKnown value = new OptionalKnown(null);
        value.getAdditionalProperties().put("known_field", new JsonPrimitive("collision"));
        assertThrows(IllegalArgumentException.class, () -> gson.toJson(value));
        MixedMap mixed = new MixedMap("fixed");
        mixed.getAdditionalProperties().put("known_field", new NestedValue("bad"));
        assertThrows(IllegalArgumentException.class, () -> gson.toJson(mixed));
    }

    @Test void authoredStorageNamesAreNotRenamed() {
        StorageCollision value = roundTrip("{\"additionalProperties\":\"known\",\"additional_properties\":{\"wire_value\":\"extra\"}}", StorageCollision.class);
        assertEquals("known", value.getAdditionalProperties());
        assertEquals("extra", value.getAdditionalProperties2().get("additional_properties").getWireValue());
        assertEquals("known", value.component1());
    }

    @Test void aliasesRetainKotlinTransparency() {
        AliasValues value = roundTrip("{\"known_field\":\"fixed\",\"key\":{\"wire_value\":\"nested\"}}", AliasValues.class);
        assertEquals("nested", value.getAdditionalProperties().get("key").getWireValue());
    }

    @Test void containingModelsRetainMixedAndPureDictionaryValues() {
        Holder value = roundTrip("{\"mixed\":{\"known_field\":\"fixed\",\"extra\":{\"wire_value\":\"nested\"}},\"opaque\":{\"known_field\":\"fixed\",\"nil\":null},\"dictionaries\":[{\"extra\":{\"wire_value\":\"nested\"}}]}", Holder.class);
        assertInstanceOf(TypedMap.class, value.getDictionaries().get(0));
    }

    @Test void recursiveValuesKeepTypedExtensions() {
        Recursive value = roundTrip("{\"wire_value\":\"root\",\"child\":{\"wire_value\":\"leaf\"}}", Recursive.class);
        assertInstanceOf(Recursive.class, value.getAdditionalProperties().get("child"));
    }

    @Test void namedUnionBoundariesKeepTheirPoliciesAndFlatPayloads() {
        roundTrip("{\"known_field\":\"fixed\",\"model\":{\"wire_value\":\"nested\"},\"scalar\":2}", Choices.class);
        roundTrip("{\"known_field\":\"fixed\",\"extra\":{\"wire_value\":\"nested\"}}", MixedChoice.class);
    }

    @Test void closedAndImplicitObjectsKeepTheirExistingBehavior() {
        String wire = "{\"known_field\":\"fixed\",\"extra\":null}";
        assertEquals("{\"known_field\":\"fixed\"}", gson.toJson(gson.fromJson(wire, Closed.class)));
        assertEquals("{\"known_field\":\"fixed\"}", gson.toJson(gson.fromJson(wire, Implicit.class)));
    }

    @Test void rootNullAndWrongKindsUseNativeFailures() {
        assertNull(gson.fromJson("null", MixedMap.class));
        assertEquals("null", gson.toJson(null, MixedMap.class));
        for (String wire : List.of("[]", "2", "\"bad\"")) {
            assertThrows(JsonSyntaxException.class, () -> gson.fromJson(wire, MixedMap.class));
        }
    }

    @Test void configuredValueAndNamedFieldAdaptersAreRetained() {
        TypeAdapter<NestedValue> custom = new TypeAdapter<>() {
            public NestedValue read(JsonReader reader) throws IOException {
                return new NestedValue(reader.nextString());
            }
            public void write(JsonWriter writer, NestedValue value) throws IOException {
                if (value == null) writer.nullValue(); else writer.value(value.getWireValue());
            }
        };
        Gson codec = new GsonBuilder().registerTypeAdapter(NestedValue.class, custom).create();
        MixedMap mixed = codec.fromJson("{\"known_field\":\"fixed\",\"extra\":\"custom\"}", MixedMap.class);
        assertEquals("custom", mixed.getAdditionalProperties().get("extra").getWireValue());
        assertEquals("{\"known_field\":\"fixed\",\"extra\":\"custom\"}", codec.toJson(mixed));
        OpenNumbers named = codec.fromJson("{\"count\":1,\"nullable_value\":\"named\"}", OpenNumbers.class);
        assertEquals("named", named.getNullableValue().getWireValue());
        assertEquals("{\"count\":1,\"nullable_value\":\"named\"}", codec.toJson(named));
    }

    @Test void configuredFieldNamesRemainOwnedByTheOrdinaryDelegate() {
        Gson codec = new GsonBuilder().setFieldNamingPolicy(FieldNamingPolicy.UPPER_CAMEL_CASE).create();
        OpenNumbers value = codec.fromJson("{\"Count\":2,\"extra\":null}", OpenNumbers.class);
        assertEquals(2, value.getCount());
        assertEquals(java.util.Set.of("extra"), value.getAdditionalProperties().keySet());
        assertEquals("{\"Count\":2,\"extra\":null}", codec.toJson(value));
        value.getAdditionalProperties().put("Count", JsonNull.INSTANCE);
        assertThrows(IllegalArgumentException.class, () -> codec.toJson(value));
    }

    @Test void configuredSpecialNumericOutputRetainsItsNativeRepresentation() {
        Gson codec = new GsonBuilder().serializeSpecialFloatingPointValues().create();
        String wire = "{\"count\":2,\"weight\":\"NaN\"}";
        String plain = codec.toJson(codec.fromJson(wire, PlainNumbers.class));
        OpenNumbers mixed = codec.fromJson(wire, OpenNumbers.class);
        assertEquals(plain, codec.toJson(mixed));
        assertEquals(plain, codec.toJson(codec.fromJson(JsonParser.parseString(wire), OpenNumbers.class)));
        assertTrue(codec.toJsonTree(mixed).getAsJsonObject().get("weight").getAsJsonPrimitive().isNumber());
        mixed.getAdditionalProperties().put("nil", JsonNull.INSTANCE);
        assertEquals(plain.substring(0, plain.length() - 1) + ",\"nil\":null}", codec.toJson(mixed));
    }

    @Test void extensionNullPolicyDoesNotLeakIntoOrdinarySiblingValues() throws IOException {
        MixedMap value = new MixedMap("fixed");
        value.getAdditionalProperties().put("nil", null);
        StringWriter buffer = new StringWriter();
        JsonWriter writer = new JsonWriter(buffer);
        writer.setSerializeNulls(false);
        writer.beginObject();
        writer.name("mixed");
        gson.getAdapter(MixedMap.class).write(writer, value);
        assertFalse(writer.getSerializeNulls());
        writer.name("ordinary");
        writer.nullValue();
        writer.endObject();
        assertEquals("{\"mixed\":{\"known_field\":\"fixed\",\"nil\":null}}", buffer.toString());
    }

    @Test void writerSettingsAreRestoredOnExtensionWriteFailure() {
        MixedMap value = new MixedMap("fixed");
        value.getAdditionalProperties().put("nil", null);
        JsonWriter writer = new JsonWriter(new StringWriter()) {
            @Override public JsonWriter nullValue() throws IOException {
                throw new IOException("extension-write");
            }
        };
        writer.setSerializeNulls(false);
        IOException failure = assertThrows(IOException.class, () -> gson.getAdapter(MixedMap.class).write(writer, value));
        assertEquals("extension-write", failure.getMessage());
        assertFalse(writer.getSerializeNulls());
    }

    private DefaultApi api(int status, String reply, String request) {
        OkHttpClient client = new OkHttpClient.Builder().addInterceptor(chain -> {
            assertEquals("/mixed", chain.request().url().encodedPath());
            okio.Buffer buffer = new okio.Buffer();
            chain.request().body().writeTo(buffer);
            assertEquals(JsonParser.parseString(request), JsonParser.parseString(buffer.readUtf8()));
            return new Response.Builder().request(chain.request()).protocol(Protocol.HTTP_1_1)
                .code(status).message("Mixed object").header("Content-Type", "application/json")
                .body(ResponseBody.create(reply, MediaType.get("application/json"))).build();
        }).build();
        return new DefaultApi(new ApiClient("https://additional.test", client, null, Map.of()));
    }

    @Test void httpRequestAndResponseRetainTypedMixedExtensions() {
        String wire = "{\"known_field\":\"fixed\",\"extra\":{\"wire_value\":\"nested\"}}";
        MixedMap value = api(200, wire, wire).echoMixed(gson.fromJson(wire, MixedMap.class)).getStatus200();
        assertEquals("nested", value.getAdditionalProperties().get("extra").getWireValue());
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(value));
    }

    @Test void typedHttpErrorsKeepMixedExtensionsAndLazyNativeFailures() {
        String request = "{\"known_field\":\"fixed\"}";
        String wire = "{\"known_field\":\"fixed\",\"extra\":{\"wire_value\":\"nested\"}}";
        EchoMixedException error = assertThrows(EchoMixedException.class,
            () -> api(400, wire, request).echoMixed(new MixedMap("fixed")));
        EchoMixedBadRequest detail = assertInstanceOf(EchoMixedBadRequest.class, error.getDetail());
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(detail.body()));
        EchoMixedException malformed = assertThrows(EchoMixedException.class,
            () -> api(400, "{\"known_field\":\"fixed\",\"bad\":false}", request).echoMixed(new MixedMap("fixed")));
        EchoMixedBadRequest bad = assertInstanceOf(EchoMixedBadRequest.class, malformed.getDetail());
        assertThrows(JsonParseException.class, bad::body);
    }
}
