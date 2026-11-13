import static org.junit.jupiter.api.Assertions.*;

import com.example.sdk.models.MixedAlias;
import com.google.gson.Gson;
import com.google.gson.JsonParser;
import org.junit.jupiter.api.Test;

class AllOfRawTest {
    @Test void rawPolicyRetainsAllJsonKindsNullsAndExactNumbers() {
        Gson gson = new Gson();
        for (String wire : new String[]{"null", "42", "[]", "{\"number\":9007199254740993,\"nil\":null}"}) {
            MixedAlias value = gson.fromJson(wire, MixedAlias.class);
            assertNotNull(value);
            assertEquals(JsonParser.parseString(wire), value.getValue());
            assertEquals(wire, gson.toJson(value));
        }
    }
}
