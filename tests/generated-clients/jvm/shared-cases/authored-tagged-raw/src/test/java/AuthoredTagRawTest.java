import com.example.sdk.models.Choice;
import com.google.gson.Gson;
import com.google.gson.JsonParser;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

public final class AuthoredTagRawTest {
    @Test
    void bypassesSelectionAndRetainsOriginalJsonTree() {
        var gson = new Gson();
        for (var json : new String[] {
            "{\"kind\":\"unknown\",\"wire_key\":{\"null_value\":null,\"exact_integer\":9007199254740993}}",
            "[null,{\"untouched\":true}]",
            "null"
        }) {
            var parsed = assertInstanceOf(Choice.VariantRaw.class, gson.fromJson(json, Choice.class));
            var serialized = gson.toJson(parsed, Choice.class);
            assertEquals(JsonParser.parseString(json), JsonParser.parseString(serialized));
            if (json.contains("exact_integer")) assertTrue(serialized.contains("9007199254740993"));
        }
    }
}
