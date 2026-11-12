import static org.junit.jupiter.api.Assertions.*;

import com.google.gson.Gson;
import com.google.gson.JsonParser;
import org.junit.jupiter.api.Test;

class AdditionalHelperTypesTest {
    private final Gson gson = new Gson();

    private <T> T roundTrip(String wire, Class<T> type) {
        T value = gson.fromJson(wire, type);
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(value, type));
        return value;
    }

    @Test void authoredJsonElementDoesNotReplaceOpaqueGsonValues() {
        com.example.sdk.models.JsonElement value = roundTrip("{\"known_field\":\"fixed\",\"nil\":null,\"large\":9007199254740993}", com.example.sdk.models.JsonElement.class);
        assertTrue(value.getAdditionalProperties().get("nil").isJsonNull());
    }

    @Test void nestedAuthoredTypesStayQualifiedInMixedMapStorage() {
        com.example.sdk.models.Map value = roundTrip("{\"known_field\":\"fixed\",\"list\":[{\"key\":{\"known_field\":\"nested\",\"nil\":null}}]}", com.example.sdk.models.Map.class);
        assertInstanceOf(com.example.sdk.models.JsonElement.class, value.getAdditionalProperties().get("list").get(0).get("key"));
    }

    @Test void authoredGsonKeepsItsOwnFactoryAndPrimitiveValues() {
        com.example.sdk.models.Gson value = roundTrip("{\"known_field\":\"fixed\",\"large\":9007199254740993}", com.example.sdk.models.Gson.class);
        assertEquals(9007199254740993L, value.getAdditionalProperties().get("large"));
    }

    @Test void authoredFactoryLookingNamesRemainOrdinaryModels() {
        com.example.sdk.models.GsonAdditionalPropertiesAdapterFactory value = new com.example.sdk.models.GsonAdditionalPropertiesAdapterFactory("authored");
        assertEquals("{\"label\":\"authored\"}", gson.toJson(value));
    }
}
