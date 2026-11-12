import static org.junit.jupiter.api.Assertions.*;

import com.google.gson.Gson;
import com.google.gson.JsonParser;
import org.junit.jupiter.api.Test;

class AdditionalHelperTypesTest {
    private final Gson gson = new Gson();

    private <T> T roundTrip(String wire, Class<T> type) {
        T value = gson.fromJson(wire, type);
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(value));
        return value;
    }

    @Test void nestedAuthoredModelNamesKeepTheirDeclaredIdentity() {
        com.example.sdk.models.Map map = roundTrip("{\"list\":[{\"key\":{\"wire_value\":\"nested\"}}]}", com.example.sdk.models.Map.class);
        assertInstanceOf(com.example.sdk.models.JsonElement.class, map.getAdditionalProperties().get("list").get(0).get("key"));
    }

    @Test void authoredIntegerCannotShadowBoxedPrimitiveValues() {
        com.example.sdk.models.Integer integer = roundTrip("{\"count\":2}", com.example.sdk.models.Integer.class);
        assertInstanceOf(java.lang.Integer.class, integer.getAdditionalProperties().get("count"));
    }

    @Test void authoredGsonCannotShadowItsOwnAdapterTypes() {
        com.example.sdk.models.Gson namedGson = roundTrip("{\"large\":9007199254740993}", com.example.sdk.models.Gson.class);
        assertInstanceOf(java.lang.Long.class, namedGson.getAdditionalProperties().get("large"));
    }
}
