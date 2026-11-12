import com.example.sdk.models.*;
import com.example.sdk.apis.*;
import com.example.sdk.runtime.ApiClient;
import com.google.gson.*;
import com.google.gson.stream.JsonWriter;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.Map;
import okhttp3.*;
import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

class DictionaryTest {
    private final Gson gson = new Gson();
    private static final String VALUES = "{\"keep_key\":{\"wire_value\":\"nested\"}}";

    private <T> T roundTrip(String wire, Class<T> type) {
        T value = gson.fromJson(wire, type);
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(value, type));
        assertEquals(JsonParser.parseString(wire), JsonParser.parseString(gson.toJson(value, type)));
        return value;
    }

    @Test void stringDictionariesAreUsableMapsWithVerbatimKeys() {
        Labels labels = roundTrip("{\"snake_key\":\"one\",\"snakeKey\":\"two\",\"additionalProperties\":\"not_storage\",\"\":\"empty\",\"quote\\\"$key\":\"value\"}", Labels.class);
        assertEquals("one", labels.getAdditionalProperties().get("snake_key"));
        labels.getAdditionalProperties().put("new_key", "three");
        assertEquals("three", gson.toJsonTree(labels).getAsJsonObject().get("new_key").getAsString());
        assertTrue(roundTrip("{}", Labels.class).getAdditionalProperties().isEmpty());
        assertTrue(new Labels().getAdditionalProperties().isEmpty());
        assertEquals("not_storage", labels.getAdditionalProperties().get("additionalProperties"));
    }

    @Test void modelValuesDecodeAsTheirDeclaredModels() {
        Values values = roundTrip(VALUES, Values.class);
        assertInstanceOf(NestedValue.class, values.getAdditionalProperties().get("keep_key"));
        assertEquals("nested", values.getAdditionalProperties().get("keep_key").getWireValue());
    }

    @Test void dictionaryNullPolicyDoesNotChangeOrdinaryMemberFieldPolicy() {
        Values values = gson.fromJson("{\"keep_key\":{\"wire_value\":\"nested\",\"ordinary_null\":null}}", Values.class);
        assertEquals(JsonParser.parseString(VALUES), gson.toJsonTree(values));
    }

    @Test void modelValuesKeepNativeIntegerDecodeErrors() {
        for (String number : new String[] {"2147483648", "-2147483649", "1.5"}) {
            String value = "{\"wire_value\":\"nested\",\"native_int\":" + number + "}";
            assertThrows(JsonParseException.class, () -> gson.fromJson(value, NestedValue.class), number);
            assertThrows(JsonParseException.class, () -> gson.fromJson("{\"value\":" + value + "}", ValueHolder.class), number);
            assertThrows(JsonParseException.class, () -> gson.fromJson("{\"key\":" + value + "}", Values.class), number);
            assertThrows(JsonParseException.class, () -> gson.fromJson("{\"key\":" + value + "}", NullableValues.class), number);
            assertThrows(JsonParseException.class, () -> gson.fromJson("{\"key\":[" + value + "]}", Lists.class), number);
            assertThrows(JsonParseException.class, () -> gson.fromJson("{\"key\":{\"nested\":" + value + "}}", NestedMaps.class), number);
        }
    }

    @Test void scalarValuesKeepNativeIntegerDecodeErrors() {
        for (String number : new String[] {"2147483648", "-2147483649", "1.5"}) {
            assertThrows(JsonParseException.class, () -> gson.fromJson(number, Integer.class), number);
            assertThrows(JsonParseException.class, () -> gson.fromJson("{\"key\":" + number + "}", Integers.class), number);
        }
    }

    @Test void validIntegerBoundariesStayExact() {
        Integers values = roundTrip("{\"min\":-2147483648,\"max\":2147483647}", Integers.class);
        assertEquals(Integer.MIN_VALUE, values.getAdditionalProperties().get("min"));
        assertEquals(Integer.MAX_VALUE, values.getAdditionalProperties().get("max"));
        roundTrip("{\"key\":{\"wire_value\":\"nested\",\"native_int\":2147483647}}", Values.class);
    }

    @Test void opaqueTreesPreserveNestedNullsAndLargeNumbers() {
        String wire = "{\"null_key\":null,\"unknown\":{\"null_value\":null,\"number\":9007199254740993},\"array\":[null,true,1.25],\"huge\":1e10000}";
        JsonValues values = roundTrip(wire, JsonValues.class);
        assertSame(JsonNull.INSTANCE, values.getAdditionalProperties().get("null_key"));
        assertEquals("9007199254740993", values.getAdditionalProperties().get("unknown").getAsJsonObject().get("number").getAsString());
        assertEquals("1e10000", values.getAdditionalProperties().get("huge").getAsString());
        roundTrip(wire, EmptySchemaValues.class);
    }

    @Test void primitiveValuesKeepTheirNativeTypesAndPrecision() {
        Numbers numbers = roundTrip("{\"large\":9007199254740993,\"negative\":-2}", Numbers.class);
        assertEquals(9007199254740993L, numbers.getAdditionalProperties().get("large"));
        Flags flags = roundTrip("{\"yes\":true,\"no\":false}", Flags.class);
        assertEquals(Boolean.TRUE, flags.getAdditionalProperties().get("yes"));
        assertEquals(Boolean.FALSE, flags.getAdditionalProperties().get("no"));
    }

    @Test void nullableDictionaryEntriesRemainPresent() {
        NullableValues values = roundTrip("{\"absent_value\":null,\"present\":{\"wire_value\":\"yes\"}}", NullableValues.class);
        assertTrue(values.getAdditionalProperties().containsKey("absent_value"));
        assertNull(values.getAdditionalProperties().get("absent_value"));
        assertEquals("yes", values.getAdditionalProperties().get("present").getWireValue());
    }

    @Test void containerValuesRetainTheirDeclaredElementModels() {
        Lists lists = roundTrip("{\"keep_key\":[{\"wire_value\":\"one\"},{\"wire_value\":\"two\"}]}", Lists.class);
        assertEquals("two", lists.getAdditionalProperties().get("keep_key").get(1).getWireValue());
        NestedMaps maps = roundTrip("{\"outer_key\":" + VALUES + "}", NestedMaps.class);
        assertEquals("nested", maps.getAdditionalProperties().get("outer_key").get("keep_key").getWireValue());
    }

    @Test void nullEntriesAndOpaqueTreesSurviveInsideContainerValues() {
        roundTrip("{\"outer\":{\"nil\":null,\"present\":{\"wire_value\":\"yes\"}}}", NestedNullableMaps.class);
        roundTrip("{\"outer\":[{\"nested_null\":null,\"number\":9007199254740993},null]}", RawLists.class);
    }

    @Test void containingFieldsListsAndAliasesKeepMapTypes() {
        String wire = "{\"labels\":{\"keep_key\":\"yes\"},\"values\":" + VALUES + ",\"dictionaries\":[" + VALUES + "],\"aliases\":" + VALUES + ",\"json_values\":{\"nil\":null}}";
        Envelope value = roundTrip(wire, Envelope.class);
        assertEquals("yes", value.getLabels().getAdditionalProperties().get("keep_key"));
        assertEquals("nested", value.getDictionaries().get(0).getAdditionalProperties().get("keep_key").getWireValue());
        assertEquals("nested", value.getAliases().getAdditionalProperties().get("keep_key").getWireValue());
        assertNull(value.getOrdinaryNull());
    }

    @Test void recursiveModelValuesStayTyped() {
        String wire = "{\"root\":{\"wire_value\":\"parent\",\"children\":{\"leaf\":{\"wire_value\":\"child\",\"children\":{}}}}}";
        Nodes nodes = roundTrip(wire, Nodes.class);
        assertEquals("child", nodes.getAdditionalProperties().get("root").getChildren().getAdditionalProperties().get("leaf").getWireValue());
    }

    @Test void dictionaryAndUnionBoundariesDelegateNormally() {
        roundTrip("{\"keep_key\":\"text\"}", LabelChoice.class);
        roundTrip("42", LabelChoice.class);
        Choices choices = roundTrip("{\"object\":{\"keep_key\":\"text\"},\"scalar\":42}", Choices.class);
        assertInstanceOf(LabelChoice.VariantLabels.class, choices.getAdditionalProperties().get("object"));
        assertInstanceOf(LabelChoice.VariantInteger.class, choices.getAdditionalProperties().get("scalar"));
    }

    @Test void rootNullAndNonObjectInputDoNotBecomeEmptyDictionaries() {
        assertNull(roundTrip("null", Labels.class));
        for (String wire : new String[] {"[]", "42", "\"text\"", "true"}) {
            assertThrows(JsonParseException.class, () -> gson.fromJson(wire, Labels.class));
        }
    }

    @Test void writerNullPolicyIsRestoredOnSuccessAndFailure() throws Exception {
        JsonValues values = gson.fromJson("{\"nil\":null}", JsonValues.class);
        TypeAdapter<JsonValues> adapter = gson.getAdapter(JsonValues.class);
        for (boolean previous : new boolean[] {false, true}) {
            StringWriter text = new StringWriter();
            JsonWriter writer = new JsonWriter(text);
            writer.setSerializeNulls(previous);
            adapter.write(writer, values);
            assertEquals(previous, writer.getSerializeNulls());
            assertEquals("{\"nil\":null}", text.toString());
            Writer broken = new Writer() {
                public void write(char[] buffer, int offset, int length) throws IOException {
                    if (new String(buffer, offset, length).contains("null")) throw new IOException("broken null writer");
                }
                public void flush() {}
                public void close() {}
            };
            JsonWriter failing = new JsonWriter(broken);
            failing.setSerializeNulls(previous);
            assertThrows(IOException.class, () -> adapter.write(failing, values));
            assertEquals(previous, failing.getSerializeNulls());
        }
    }

    private DictionaryApi api(int status, String wire) {
        OkHttpClient client = new OkHttpClient.Builder().addInterceptor(chain -> {
            assertEquals("/labels", chain.request().url().encodedPath());
            okio.Buffer request = new okio.Buffer();
            chain.request().body().writeTo(request);
            assertEquals(JsonParser.parseString("{\"keep_key\":\"sent\"}"), JsonParser.parseString(request.readUtf8()));
            return new Response.Builder().request(chain.request()).protocol(Protocol.HTTP_1_1)
                .code(status).message("Dictionary").header("Content-Type", "application/json")
                .body(ResponseBody.create(wire, MediaType.get("application/json"))).build();
        }).build();
        return new DictionaryApi(new ApiClient("https://dictionary.test", client, null, Map.of()));
    }

    @Test void httpRequestAndResponseBodiesKeepDictionaryEntries() {
        Labels request = new Labels();
        request.getAdditionalProperties().put("keep_key", "sent");
        EchoLabelsResponse response = api(200, "{\"response_key\":\"received\"}").echoLabels(request);
        assertEquals("received", response.getStatus200().getAdditionalProperties().get("response_key"));
    }

    @Test void typedHttpErrorBodiesRetainNestedModelsAndRawBytes() {
        Labels request = new Labels();
        request.getAdditionalProperties().put("keep_key", "sent");
        EchoLabelsException error = assertThrows(EchoLabelsException.class, () -> api(400, VALUES).echoLabels(request));
        EchoLabelsBadRequest detail = assertInstanceOf(EchoLabelsBadRequest.class, error.getDetail());
        assertEquals("nested", detail.body().getAdditionalProperties().get("keep_key").getWireValue());
        assertArrayEquals(VALUES.getBytes(StandardCharsets.UTF_8), error.rawBody());
    }
}
