import com.example.sdk.models.*;
import com.example.sdk.apis.*;
import com.google.gson.*;
import com.google.gson.stream.JsonWriter;
import java.io.StringWriter;
import java.io.Writer;
import java.io.IOException;
import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

class RawUnionTest {
    private final Gson gson = new Gson();

    @Test void httpSuccessAndErrorsPreserveUnknownRawTrees() {
        String wire = "{\"unknown\":null,\"number\":9007199254740993}";
        GetChoiceResponse response = UnionHttpTestSupport.api(200, wire).getChoice();
        assertInstanceOf(FiniteChoice.VariantRaw.class, response.getStatus200());
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(response.getStatus200(), FiniteChoice.class));
        GetChoiceException error = assertThrows(GetChoiceException.class, () -> UnionHttpTestSupport.api(400, wire).getChoice());
        GetChoiceBadRequest detail = assertInstanceOf(GetChoiceBadRequest.class, error.getDetail());
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(detail.body(), FiniteChoice.class));
    }

    @Test void knownAmbiguousUnknownAndNullTreesRoundTrip() {
        for (String wire : new String[] {"null", "42", "[null,9007199254740993]", "{\"unknown\":null,\"revision\":2}", "{\"wire_name\":\"both\"}"}) {
            FiniteChoice value = gson.fromJson(wire, FiniteChoice.class);
            FiniteChoice.VariantRaw raw = assertInstanceOf(FiniteChoice.VariantRaw.class, value);
            assertEquals(JsonParser.parseString(wire), raw.getValue0());
            assertEquals(JsonParser.parseString(wire), gson.toJsonTree(value, FiniteChoice.class));
            assertEquals(JsonParser.parseString(wire), JsonParser.parseString(gson.toJson(value, FiniteChoice.class)));
        }
    }

    @Test void rawCaseNamesDoNotCollideWithTheEnclosingClass() {
        VariantRaw value = gson.fromJson("{\"null\":null}", VariantRaw.class);
        assertInstanceOf(VariantRaw.VariantRawVariant.class, value);
        assertEquals(JsonParser.parseString("{\"null\":null}"), gson.toJsonTree(value, VariantRaw.class));
    }

    @Test void containingModelsKeepRawNullsButStillOmitOrdinaryNullFields() {
        String raw = "{\"unknown\":null,\"number\":9007199254740993}";
        String wire = "{\"caption\":\"raw\",\"choice\":" + raw + ",\"choices\":[null],\"by_name\":{\"keep_key\":null}}";
        ChoiceEnvelope value = gson.fromJson(wire, ChoiceEnvelope.class);
        assertInstanceOf(FiniteChoice.VariantRaw.class, value.getChoices().get(0));
        assertInstanceOf(FiniteChoice.VariantRaw.class, value.getByName().get("keep_key"));
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(value));
    }

    @Test void writerNullPolicyIsRestoredAfterSuccessAndFailure() throws Exception {
        FiniteChoice value = gson.fromJson("{\"unknown\":null}", FiniteChoice.class);
        TypeAdapter<FiniteChoice> adapter = gson.getAdapter(FiniteChoice.class);
        StringWriter text = new StringWriter();
        JsonWriter writer = new JsonWriter(text);
        writer.setSerializeNulls(false);
        adapter.write(writer, value);
        assertFalse(writer.getSerializeNulls());
        assertEquals("{\"unknown\":null}", text.toString());
        Writer broken = new Writer() {
            public void write(char[] buffer, int offset, int length) throws IOException { throw new IOException("broken writer"); }
            public void flush() {}
            public void close() {}
        };
        JsonWriter failing = new JsonWriter(broken);
        failing.setSerializeNulls(false);
        assertThrows(IOException.class, () -> adapter.write(failing, value));
        assertFalse(failing.getSerializeNulls());
    }
}
