import com.example.sdk.models.Choice;
import com.example.sdk.models.ChoiceList;
import com.example.sdk.models.Outer;
import com.google.gson.Gson;
import com.google.gson.JsonParseException;
import com.google.gson.JsonParser;
import com.google.gson.JsonPrimitive;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

public final class AuthoredWireShapeTest {
    private final Gson gson = new Gson();

    @Test
    void retainsRequiredNullBelowTheRootAndChecksCompositeLiteralsExactly() {
        var input = "{\"kind\":\"alpha_alt\",\"scope\":{\"parts\":[null,9007199254740993]},\"details\":[{\"value\":null}]}";
        assertInstanceOf(Choice.VariantAlpha.class, roundTrip(Choice.class, input));
        for (var invalid : new String[] {
            input.replace("9007199254740993", "9007199254740992"),
            input.replace("[null,9007199254740993]", "[null,9007199254740993,0]"),
            input.replace("{\"value\":null}", "{}")
        }) {
            assertThrows(JsonParseException.class, () -> gson.fromJson(invalid, Choice.class));
        }
    }

    @Test
    void acceptsAnAuthoredScalarWithoutInventingATag() {
        assertInstanceOf(Choice.VariantText.class, roundTrip(Choice.class, "\"literal\""));
        var invalid = new Choice.VariantText(new JsonPrimitive("literal"), new JsonPrimitive("alpha"));
        assertThrows(JsonParseException.class, () -> gson.toJson(invalid, Choice.class));
    }

    @Test
    void untaggedContainersWriteThroughTheDeclaredTaggedCodec() {
        roundTrip(ChoiceList.class, "[\"literal\",{\"kind\":null,\"count\":3}]");
    }

    @Test
    void projectedCommonFieldsDoNotBypassNestedStrictDecoder() {
        roundTrip(Outer.class, "{\"scope\":true,\"kind\":\"text_alt\",\"label\":\"x\"}");
        roundTrip(Outer.class, "{\"scope\":true,\"kind\":\"held\",\"child\":{\"kind\":\"size\",\"count\":3}}");
        for (var input : new String[] {
            "{\"scope\":true,\"label\":\"x\",\"count\":3}",
            "{\"scope\":true,\"kind\":\"held\",\"child\":{\"label\":\"x\",\"count\":3}}"
        }) {
            var ambiguous = assertThrows(JsonParseException.class, () -> gson.fromJson(input, Outer.class));
            assertTrue(ambiguous.getMessage().contains("inferred tagged union Inner"), ambiguous.getMessage());
        }
    }

    @Test
    void projectedWritePreservesNestedStrictErrorsAndAssemblesTheTagFirst() {
        var direct = JsonParser.parseString("{\"scope\":true,\"label\":\"x\"}").getAsJsonObject();
        var selected = new Outer.VariantText(direct, null);
        assertEquals(direct, JsonParser.parseString(gson.toJson(selected, Outer.class)));
        direct.addProperty("count", 3);
        assertNestedWriteError(selected);

        var tagged = new Outer.VariantText(direct, new JsonPrimitive("text_alt"));
        var taggedWire = JsonParser.parseString(gson.toJson(tagged, Outer.class)).getAsJsonObject();
        assertEquals("text_alt", taggedWire.get("kind").getAsString());
        assertFalse(direct.has("kind"), "writing must not modify the public projection");
        roundTrip(Outer.class, taggedWire.toString());

        var holder = JsonParser.parseString("{\"scope\":true,\"child\":{\"label\":\"x\"}}").getAsJsonObject();
        var held = new Outer.VariantHolder(holder, new JsonPrimitive("held"));
        roundTrip(Outer.class, gson.toJson(held, Outer.class));
        holder.getAsJsonObject("child").addProperty("count", 3);
        assertNestedWriteError(held);
    }

    private void assertNestedWriteError(Outer selected) {
        var ambiguous = assertThrows(JsonParseException.class, () -> gson.toJson(selected, Outer.class));
        assertTrue(ambiguous.getMessage().contains("ambiguous inferred tagged union Inner"), ambiguous.getMessage());
    }

    private <T> T roundTrip(Class<T> type, String json) {
        var parsed = gson.fromJson(json, type);
        assertEquals(JsonParser.parseString(json), JsonParser.parseString(gson.toJson(parsed, type)));
        return parsed;
    }
}
