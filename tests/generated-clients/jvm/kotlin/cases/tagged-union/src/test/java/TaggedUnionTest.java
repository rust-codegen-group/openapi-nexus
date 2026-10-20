import com.example.sdk.models.Setup;
import com.google.gson.Gson;
import com.google.gson.JsonParser;
import static org.junit.jupiter.api.Assertions.*;

public final class TaggedUnionTest {
    @org.junit.jupiter.api.Test
    void preservesDiscriminatorAndBusinessFields() {
        Gson gson = new Gson();
        String json = "{\"kind\":\"SETUP_QUICK\",\"name\":\"x\",\"engine\":\"e\"}";
        Setup.VariantSetupQuick setup = assertInstanceOf(Setup.VariantSetupQuick.class, gson.fromJson(json, Setup.class));
        assertEquals("x", setup.getValue0().getName());
        assertEquals("e", setup.getValue0().getEngine());
        assertEquals(JsonParser.parseString(json), JsonParser.parseString(gson.toJson(setup, Setup.class)));
    }
}
