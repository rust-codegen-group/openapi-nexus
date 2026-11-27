import com.example.sdk.models.AdjacentChoice;
import com.example.sdk.models.InternalChoice;
import com.example.sdk.models.MappedChoice;
import com.google.gson.Gson;
import com.google.gson.JsonParseException;
import com.google.gson.JsonParser;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

public final class FixedDiscriminatorCodecTest {
    private final Gson gson = new Gson();

    @Test
    void internalTagHasToBeAJsonString() {
        roundTrip(InternalChoice.class, "{\"kind\":\"plain\"}");
        for (var json : new String[] {
            "{\"kind\":[\"plain\"]}",
            "{\"kind\":true,\"count\":2}",
            "{\"kind\":{}}",
            "{\"kind\":null}",
            "{\"kind\":\"unknown\"}",
            "{}"
        }) {
            assertThrows(JsonParseException.class, () -> gson.fromJson(json, InternalChoice.class), json);
        }
    }

    @Test
    void adjacentTagHasToBeAJsonString() {
        roundTrip(AdjacentChoice.class, "{\"kind\":\"7\"}");
        roundTrip(AdjacentChoice.class, "{\"kind\":\"details\",\"payload\":{\"count\":2}}");
        for (var json : new String[] {
            "{\"kind\":7}",
            "{\"kind\":[\"7\"]}",
            "{\"kind\":null}",
            "{}"
        }) {
            assertThrows(JsonParseException.class, () -> gson.fromJson(json, AdjacentChoice.class), json);
        }
    }

    @Test
    void explicitMappingConvertsOnlyScalarKeys() {
        for (var entry : new String[][] {{"\"7\"", "7"}, {"7", "7"}, {"true", "true"}}) {
            var mapped = gson.fromJson("{\"kind\":" + entry[0] + ",\"count\":2}", MappedChoice.class);
            assertEquals(JsonParser.parseString("{\"kind\":\"" + entry[1] + "\",\"count\":2}"),
                JsonParser.parseString(gson.toJson(mapped, MappedChoice.class)));
        }
    }

    @Test
    void explicitMappingRejectsCompoundAndMissingKeys() {
        for (var json : new String[] {
            "{\"kind\":[\"7\"],\"count\":2}",
            "{\"kind\":[7],\"count\":2}",
            "{\"kind\":{},\"count\":2}",
            "{\"kind\":null,\"count\":2}",
            "{\"count\":2}"
        }) {
            assertThrows(JsonParseException.class, () -> gson.fromJson(json, MappedChoice.class), json);
        }
    }

    private <T> void roundTrip(Class<T> type, String json) {
        var model = gson.fromJson(json, type);
        assertEquals(JsonParser.parseString(json),
            JsonParser.parseString(gson.toJson(model, type)));
    }
}
