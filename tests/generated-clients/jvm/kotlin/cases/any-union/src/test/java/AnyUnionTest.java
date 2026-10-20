import com.example.sdk.models.Config;
import com.google.gson.Gson;
import com.google.gson.JsonParser;
import static org.junit.jupiter.api.Assertions.*;

public final class AnyUnionTest {
    private final Gson gson = new Gson();

    @org.junit.jupiter.api.Test
    void stringPrefersItsSpecificVariantOverAny() {
        Config.VariantString value = assertInstanceOf(Config.VariantString.class, gson.fromJson("\"specific\"", Config.class));
        assertEquals("specific", value.getValue0());
    }

    @org.junit.jupiter.api.Test
    void numberPrefersItsSpecificVariantOverAny() {
        Config.VariantNumber value = assertInstanceOf(Config.VariantNumber.class, gson.fromJson("7.5", Config.class));
        assertEquals(7.5, value.getValue0());
    }

    @org.junit.jupiter.api.Test
    void unmatchedValuesUseAnyWithoutLosingTheirPayload() {
        for (String json : new String[] {"true", "{\"key\":1}"}) {
            Config.VariantAny value = assertInstanceOf(Config.VariantAny.class, gson.fromJson(json, Config.class));
            assertEquals(JsonParser.parseString(json), value.getValue0());
            assertEquals(JsonParser.parseString(json), JsonParser.parseString(gson.toJson(value, Config.class)));
        }
    }
}
