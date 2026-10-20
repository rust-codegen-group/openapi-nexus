import com.example.sdk.models.AdjacentlyTaggedEnum;
import com.example.sdk.models.ExternallyTaggedEnum;
import com.example.sdk.models.VariantA;
import com.google.gson.Gson;
import com.google.gson.JsonParser;
import static org.junit.jupiter.api.Assertions.*;

public final class TaggedShapesTest {
    @org.junit.jupiter.api.Test
    void adjacentRoundTripRetainsTheCompletePayload() {
        Gson gson = new Gson();
        VariantA payload = new VariantA("value", 7);
        AdjacentlyTaggedEnum value = new AdjacentlyTaggedEnum.VariantAdjacentlyTaggedEnumVariantA(payload);
        String json = gson.toJson(value, AdjacentlyTaggedEnum.class);
        assertEquals(JsonParser.parseString("{\"ty\":\"ADJACENTLY_TAGGED_ENUM_VARIANT_A\",\"data\":{\"field1\":\"value\",\"field2\":7}}"), JsonParser.parseString(json));
        var decoded = assertInstanceOf(AdjacentlyTaggedEnum.VariantAdjacentlyTaggedEnumVariantA.class, gson.fromJson(json, AdjacentlyTaggedEnum.class));
        assertEquals("value", decoded.getValue0().getField1());
        assertEquals(7, decoded.getValue0().getField2());
    }

    @org.junit.jupiter.api.Test
    void externalRoundTripRetainsTheCompletePayload() {
        Gson gson = new Gson();
        ExternallyTaggedEnum value = new ExternallyTaggedEnum.VariantExternallyTaggedEnumVariantA(new VariantA("value", 7));
        String json = gson.toJson(value, ExternallyTaggedEnum.class);
        assertEquals(JsonParser.parseString("{\"EXTERNALLY_TAGGED_ENUM_VARIANT_A\":{\"field1\":\"value\",\"field2\":7}}"), JsonParser.parseString(json));
        var decoded = assertInstanceOf(ExternallyTaggedEnum.VariantExternallyTaggedEnumVariantA.class, gson.fromJson(json, ExternallyTaggedEnum.class));
        assertEquals("value", decoded.getValue0().getField1());
        assertEquals(7, decoded.getValue0().getField2());
    }
}
