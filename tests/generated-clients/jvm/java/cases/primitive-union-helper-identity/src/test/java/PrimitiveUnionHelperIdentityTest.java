import com.example.sdk.models.Choice;
import com.google.gson.Gson;
import com.google.gson.JsonParser;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;

class PrimitiveUnionHelperIdentityTest {
    private final Gson gson = new Gson();

    @Test
    void primitiveUnionUsesJavaRuntimeIntegerBesideAuthoredModel() {
        Choice value = gson.fromJson("7", Choice.class);
        Choice.VariantInteger variant = assertInstanceOf(Choice.VariantInteger.class, value);

        assertEquals(7, variant.value0());
        assertEquals(JsonParser.parseString("7"), gson.toJsonTree(value, Choice.class));
    }

    @Test
    void authoredIntegerModelRemainsUsable() {
        com.example.sdk.models.Integer value =
                gson.fromJson("{\"label\":\"authored\"}", com.example.sdk.models.Integer.class);

        assertEquals("authored", value.getLabel());
        assertEquals(
                JsonParser.parseString("{\"label\":\"authored\"}"),
                gson.toJsonTree(value, com.example.sdk.models.Integer.class));
    }
}
