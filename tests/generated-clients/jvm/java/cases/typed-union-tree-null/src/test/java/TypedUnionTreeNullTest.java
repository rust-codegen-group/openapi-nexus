import com.example.sdk.models.AdjacentChoice;
import com.example.sdk.models.AnyChoice;
import com.example.sdk.models.Choice;
import com.example.sdk.models.ExternalChoice;
import com.example.sdk.models.InternalChoice;
import com.example.sdk.models.PayloadA;
import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonIOException;
import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.google.gson.Strictness;
import com.google.gson.stream.JsonWriter;
import java.io.IOException;
import java.io.StringWriter;
import java.util.stream.Stream;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.Arguments;
import org.junit.jupiter.params.provider.MethodSource;
import static org.junit.jupiter.api.Assertions.*;

final class TypedUnionTreeNullTest {
    private final Gson gson = new Gson();
    private static final String PAYLOAD = "{\"known_field\":\"fixed\",\"nil\":null,"
        + "\"nested\":{\"nil\":null,\"values\":[null,{\"nil\":null}]},\"big\":18446744073709551615}";

    static Stream<Arguments> typedUnions() {
        return Stream.of(
            Arguments.of(Choice.class, Choice.VariantPayloadA.class, PAYLOAD),
            Arguments.of(AnyChoice.class, AnyChoice.VariantPayloadA.class, PAYLOAD),
            Arguments.of(InternalChoice.class, InternalChoice.VariantA.class, "{\"kind\":\"a\"," + PAYLOAD.substring(1)),
            Arguments.of(AdjacentChoice.class, AdjacentChoice.VariantA.class, "{\"kind\":\"a\",\"data\":" + PAYLOAD + "}"),
            Arguments.of(ExternalChoice.class, ExternalChoice.VariantA.class, "{\"a\":" + PAYLOAD + "}")
        );
    }

    @Test
    void declaredUnionRetainsItsMembersExplicitNullEntry() {
        String wire = "{\"known_field\":\"fixed\",\"nil\":null}";
        PayloadA member = gson.fromJson(wire, PayloadA.class);
        Choice choice = gson.fromJson(wire, Choice.class);
        Choice.VariantPayloadA selected = assertInstanceOf(Choice.VariantPayloadA.class, choice);

        assertTrue(selected.value0().getAdditionalProperties().get("nil").isJsonNull());
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(selected.value0()));
        assertEquals(JsonParser.parseString(wire), JsonParser.parseString(gson.toJson(member)));
        assertEquals(JsonParser.parseString(wire), JsonParser.parseString(gson.toJson(choice, Choice.class)),
            "typed union omitted an ordinary member's explicit null");
    }

    @ParameterizedTest
    @MethodSource("typedUnions")
    <T> void streamAndTreeWritesPreserveCompleteMemberTrees(Class<T> type, Class<?> variant, String wire) {
        T value = gson.fromJson(wire, type);
        assertInstanceOf(variant, value);
        JsonObject tree = gson.toJsonTree(value, type).getAsJsonObject();
        String serialized = gson.toJson(value, type);
        assertEquals(JsonParser.parseString(wire), tree);
        assertEquals(JsonParser.parseString(wire), JsonParser.parseString(serialized));
        JsonObject payload = type.equals(AdjacentChoice.class) ? tree.getAsJsonObject("data")
            : type.equals(ExternalChoice.class) ? tree.getAsJsonObject("a") : tree;
        assertEquals("18446744073709551615", payload.get("big").getAsString());
        assertTrue(serialized.contains("\"big\":18446744073709551615"));
    }

    @ParameterizedTest
    @MethodSource("typedUnions")
    <T> void ordinaryMemberFieldsKeepTheirDefaultNullOmission(Class<T> type, Class<?> variant, String wire) {
        String input = wire.replace("\"known_field\":\"fixed\"", "\"known_field\":\"fixed\",\"optional_field\":null");
        T value = gson.fromJson(input, type);
        assertInstanceOf(variant, value);
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(value, type));
        assertEquals(JsonParser.parseString(wire), JsonParser.parseString(gson.toJson(value, type)));
    }

    @ParameterizedTest
    @MethodSource("typedUnions")
    <T> void writingRestoresBothSurroundingNullPolicies(Class<T> type, Class<?> variant, String wire) throws IOException {
        T value = gson.fromJson(wire, type);
        assertInstanceOf(variant, value);
        for (boolean serializeNulls : new boolean[] {false, true}) {
            StringWriter buffer = new StringWriter();
            JsonWriter writer = new JsonWriter(buffer);
            writer.setSerializeNulls(serializeNulls);
            Strictness strictness = serializeNulls ? Strictness.STRICT : Strictness.LEGACY_STRICT;
            writer.setStrictness(strictness);
            writer.setHtmlSafe(!serializeNulls);
            writer.beginObject();
            writer.name("choice");
            gson.getAdapter(type).write(writer, value);
            assertEquals(serializeNulls, writer.getSerializeNulls());
            assertEquals(strictness, writer.getStrictness());
            assertEquals(!serializeNulls, writer.isHtmlSafe());
            writer.name("ordinary");
            writer.nullValue();
            writer.endObject();

            JsonObject expected = new JsonObject();
            expected.add("choice", JsonParser.parseString(wire));
            if (serializeNulls) {
                expected.add("ordinary", JsonNull.INSTANCE);
            }
            assertEquals(expected, JsonParser.parseString(buffer.toString()));
        }
    }

    @ParameterizedTest
    @MethodSource("typedUnions")
    <T> void failedTreeWritesRestoreBothSurroundingNullPolicies(Class<T> type, Class<?> variant, String wire) {
        T value = gson.fromJson(wire, type);
        assertInstanceOf(variant, value);
        for (boolean serializeNulls : new boolean[] {false, true}) {
            IOException expected = new IOException("tree-null-write");
            JsonWriter writer = new JsonWriter(new StringWriter()) {
                @Override public JsonWriter nullValue() throws IOException {
                    throw expected;
                }
            };
            writer.setSerializeNulls(serializeNulls);
            Strictness strictness = serializeNulls ? Strictness.STRICT : Strictness.LEGACY_STRICT;
            writer.setStrictness(strictness);
            writer.setHtmlSafe(!serializeNulls);
            JsonIOException failure = assertThrows(JsonIOException.class, () -> gson.getAdapter(type).write(writer, value));
            assertSame(expected, failure.getCause());
            assertEquals(serializeNulls, writer.getSerializeNulls());
            assertEquals(strictness, writer.getStrictness());
            assertEquals(!serializeNulls, writer.isHtmlSafe());
        }
    }

    @Test
    void finalTreeWriteKeepsExistingHtmlEscaping() throws IOException {
        Choice value = new Choice.VariantString("<>&='");
        String expected = "\"\\u003c\\u003e\\u0026\\u003d\\u0027\"";
        Gson unescaped = new GsonBuilder().disableHtmlEscaping().create();
        assertEquals(expected, unescaped.toJson(value, Choice.class));
        for (boolean htmlSafe : new boolean[] {false, true}) {
            StringWriter buffer = new StringWriter();
            JsonWriter writer = new JsonWriter(buffer);
            writer.setHtmlSafe(htmlSafe);
            gson.getAdapter(Choice.class).write(writer, value);
            assertEquals(expected, buffer.toString());
            assertEquals(htmlSafe, writer.isHtmlSafe());
        }
    }

    @Test
    void primitiveMembersAndNullRootsKeepTheirWireShape() {
        assertEquals("\"fixed\"", gson.toJson(gson.fromJson("\"fixed\"", Choice.class), Choice.class));
        assertEquals("\"fixed\"", gson.toJson(gson.fromJson("\"fixed\"", AnyChoice.class), AnyChoice.class));
        assertEquals("null", gson.toJson(null, Choice.class));
        assertEquals("null", gson.toJson(null, AnyChoice.class));
    }
}
