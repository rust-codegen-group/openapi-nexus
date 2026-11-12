import com.example.sdk.models.*;
import com.google.gson.Gson;
import com.google.gson.JsonParser;
import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

class DictionaryHelperTypesTest {
    private final Gson gson = new Gson();
    private static final String WIRE = "{\"key\":{\"wire_value\":\"model\"}}";

    private <T> T roundTrip(String wire, Class<T> type) {
        T value = gson.fromJson(wire, type);
        assertEquals(JsonParser.parseString(wire), JsonParser.parseString(gson.toJson(value, type)));
        return value;
    }

    @Test void typeTokenModelDoesNotResolveToGsonTypeToken() {
        TokenValues value = roundTrip(WIRE, TokenValues.class);
        assertInstanceOf(com.example.sdk.models.TypeToken.class, value.getAdditionalProperties().get("key"));
        assertEquals("model", value.getAdditionalProperties().get("key").getWireValue());
    }

    @Test void jsonElementModelDoesNotBecomeAnOpaqueTree() {
        ElementValues value = roundTrip(WIRE, ElementValues.class);
        assertInstanceOf(com.example.sdk.models.JsonElement.class, value.getAdditionalProperties().get("key"));
        assertEquals("model", value.getAdditionalProperties().get("key").getWireValue());
    }

    @Test void jsonNullModelDoesNotBecomeGsonNull() {
        NullValues value = roundTrip(WIRE, NullValues.class);
        assertInstanceOf(com.example.sdk.models.JsonNull.class, value.getAdditionalProperties().get("key"));
        assertEquals("model", value.getAdditionalProperties().get("key").getWireValue());
    }

    @Test void gsonModelDoesNotReflectIntoGsonInternals() {
        GsonValues value = roundTrip(WIRE, GsonValues.class);
        assertInstanceOf(com.example.sdk.models.Gson.class, value.getAdditionalProperties().get("key"));
        assertEquals("model", value.getAdditionalProperties().get("key").getWireValue());
    }

    @Test void nestedContainersKeepAuthoredModelIdentities() {
        NestedValues value = roundTrip("{\"key\":{\"list\":[{\"wire_value\":\"model\"},null]}}", NestedValues.class);
        Object item = value.getAdditionalProperties().get("key").get("list").get(0);
        assertInstanceOf(com.example.sdk.models.JsonElement.class, item);
        assertNull(value.getAdditionalProperties().get("key").get("list").get(1));
    }

    @Test void dictionaryNamesDoNotShadowTheirOwnAdapterHelpers() {
        roundTrip("{\"nil\":null,\"raw\":{\"number\":9007199254740993}}", JsonAdapter.class);
        roundTrip(WIRE, JsonReader.class);
        roundTrip("{\"key\":\"value\"}", TypeAdapter.class);
    }
}
