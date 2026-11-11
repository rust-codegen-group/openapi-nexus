import com.example.sdk.models.*;
import com.example.sdk.apis.*;
import com.google.gson.*;
import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

class RawUnionPolicyTest {
    private final Gson gson = new Gson();

    @Test void rawPreservesKnownAmbiguousUnmatchedAndNullJsonIncludingNullEntries() {
        for (String wire : new String[] {
            "null", "42", "{\"revision\":2,\"unknown\":null}",
            "{\"wire_name\":\"detail\",\"second_value\":\"two\"}",
            "{\"unknown\":[null,true,{\"nested_null\":null}]}"
        }) {
            FiniteChoice.VariantRaw value = assertInstanceOf(FiniteChoice.VariantRaw.class, gson.fromJson(wire, FiniteChoice.class));
            JsonElement expected = JsonParser.parseString(wire);
            assertEquals(expected, value.value0());
            assertEquals(expected, gson.toJsonTree(value, FiniteChoice.class));
            assertEquals(expected, JsonParser.parseString(gson.toJson(value, FiniteChoice.class)));
        }
    }

    @Test void rawWrapperCanAlsoWriteAJavaNullPayload() {
        assertEquals(JsonNull.INSTANCE, gson.toJsonTree(new FiniteChoice.VariantRaw(null), FiniteChoice.class));
    }

    @Test void rawCaseNamesDoNotCollideWithTheirEnclosingInterface() {
        String wire = "{\"nullable\":null}";
        VariantRaw.VariantRawVariant value = assertInstanceOf(VariantRaw.VariantRawVariant.class, gson.fromJson(wire, VariantRaw.class));
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(value, VariantRaw.class));
    }

    @Test void rawUnionSubtreesDoNotEraseOrdinaryContainingModelFields() {
        String wire = "{\"caption\":\"envelope\",\"choice\":{\"nullable\":null},\"choices\":[42],\"by_name\":{\"keep_key\":false}}";
        ChoiceEnvelope value = gson.fromJson(wire, ChoiceEnvelope.class);
        assertEquals("envelope", value.getCaption());
        assertInstanceOf(FiniteChoice.VariantRaw.class, value.getChoice());
        assertInstanceOf(FiniteChoice.VariantRaw.class, value.getChoices().get(0));
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(value));
    }

    @Test void rawWritingRestoresTheSurroundingGsonNullPolicy() {
        String wire = "{\"caption\":\"envelope\",\"choice\":{\"nullable\":null},\"choices\":[],\"by_name\":{},\"ordinary_null\":null}";
        ChoiceEnvelope value = gson.fromJson(wire, ChoiceEnvelope.class);
        JsonObject expected = JsonParser.parseString(wire).getAsJsonObject();
        expected.remove("ordinary_null");
        assertEquals(expected, gson.toJsonTree(value));
        assertEquals(expected, JsonParser.parseString(gson.toJson(value)));
    }

    @Test void rawLargeNumbersNeverPassThroughDoublePrecision() {
        String wire = "{\"big\":18446744073709551615,\"nullable\":null}";
        FiniteChoice value = gson.fromJson(wire, FiniteChoice.class);
        JsonObject tree = gson.toJsonTree(value, FiniteChoice.class).getAsJsonObject();
        assertEquals("18446744073709551615", tree.get("big").getAsString());
        assertTrue(gson.toJson(value, FiniteChoice.class).contains("18446744073709551615"));
        assertTrue(tree.has("nullable"));
    }

    @Test void rawHttpSuccessPreservesUnmatchedPayloads() throws Exception {
        String wire = "{\"unmatched\":true,\"nullable\":null}";
        FiniteChoice.VariantRaw value = assertInstanceOf(FiniteChoice.VariantRaw.class, UnionHttpTestSupport.api(200, wire).getChoice().getStatus200());
        assertEquals(JsonParser.parseString(wire), value.value0());
    }

    @Test void rawHttpErrorBodiesPreserveUnmatchedPayloads() {
        String wire = "{\"unmatched\":true,\"nullable\":null}";
        GetChoiceException error = assertThrows(GetChoiceException.class, () -> UnionHttpTestSupport.api(400, wire).getChoice());
        GetChoiceBadRequest detail = assertInstanceOf(GetChoiceBadRequest.class, error.detail());
        FiniteChoice.VariantRaw value = assertInstanceOf(FiniteChoice.VariantRaw.class, detail.body());
        assertEquals(JsonParser.parseString(wire), value.value0());
    }
}
