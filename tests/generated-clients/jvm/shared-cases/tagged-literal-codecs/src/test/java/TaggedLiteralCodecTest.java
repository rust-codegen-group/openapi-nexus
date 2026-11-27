import com.example.sdk.models.ExternalChoice;
import com.google.gson.Gson;
import com.google.gson.JsonParseException;
import com.google.gson.JsonParser;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

public final class TaggedLiteralCodecTest {
    private final Gson gson = new Gson();

    @Test
    void externalCasesRetainThePayloadInsideTheAuthoredKey() {
        for (var json : new String[] {
            "{\"plain\":\"format_v1\"}",
            "{\"compact\":\"7\"}",
            "{\"details\":{\"count\":2}}"
        }) {
            var value = gson.fromJson(json, ExternalChoice.class);
            assertEquals(JsonParser.parseString(json),
                JsonParser.parseString(gson.toJson(value, ExternalChoice.class)));
        }
    }

    @Test
    void decodedLiteralIsATypedPayloadAndCannotBeWrittenAsAnotherValue() {
        assertEquals(new ExternalChoice.VariantPlain("format_v1"),
            gson.fromJson("{\"plain\":\"format_v1\"}", ExternalChoice.class));
        assertThrows(JsonParseException.class,
            () -> gson.toJson(new ExternalChoice.VariantPlain("other"), ExternalChoice.class));
    }

    @Test
    void rejectsAWrongExternalLiteralWithoutCoercingItsJsonKind() {
        for (var json : new String[] {
            "{\"plain\":\"other\"}",
            "{\"plain\":[\"format_v1\"]}",
            "{\"plain\":null}",
            "{\"compact\":7}"
        }) {
            assertThrows(JsonParseException.class, () -> gson.fromJson(json, ExternalChoice.class), json);
        }
    }
}
