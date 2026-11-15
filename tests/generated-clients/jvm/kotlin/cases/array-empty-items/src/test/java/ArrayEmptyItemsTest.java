import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;

import com.example.sdk.models.EmptyItems;
import com.example.sdk.models.TrueItems;
import com.google.gson.Gson;
import com.google.gson.JsonParser;
import java.util.Map;
import org.junit.jupiter.api.Test;

class ArrayEmptyItemsTest {
    private final Gson gson = new Gson();
    private final String wire = "{\"values\":[{\"wire_value\":\"object\"}]}";

    @Test void emptyAndBooleanItemsAcceptObjectElements() {
        EmptyItems emptyItems = gson.fromJson(wire, EmptyItems.class);
        TrueItems trueItems = gson.fromJson(wire, TrueItems.class);

        assertInstanceOf(Map.class, emptyItems.getValues().get(0));
        assertInstanceOf(Map.class, trueItems.getValues().get(0));
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(emptyItems));
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(trueItems));
    }
}
