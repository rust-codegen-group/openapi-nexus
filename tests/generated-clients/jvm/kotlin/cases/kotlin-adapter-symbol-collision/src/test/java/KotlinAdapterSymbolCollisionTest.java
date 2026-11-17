import static org.junit.jupiter.api.Assertions.assertEquals;

import com.example.sdk.models.Values;
import com.example.sdk.models.ValuesAdditionalProperty;
import com.example.sdk.models.ValuesAdditionalPropertyJsonAdapter;
import com.google.gson.Gson;
import com.google.gson.JsonParser;
import org.junit.jupiter.api.Test;

class KotlinAdapterSymbolCollisionTest {
    private final Gson gson = new Gson();

    @Test
    void unionAdapterAndAuthoredModelUseDistinctSymbols() {
        Values values = new Values();
        values.getAdditionalProperties().put("integer", new ValuesAdditionalProperty.VariantInteger(7));
        values.getAdditionalProperties().put("string", new ValuesAdditionalProperty.VariantString("kept"));

        String wire = "{\"integer\":7,\"string\":\"kept\"}";
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(values, Values.class));
        assertEquals(JsonParser.parseString(wire), JsonParser.parseString(gson.toJson(values, Values.class)));

        ValuesAdditionalPropertyJsonAdapter authored = new ValuesAdditionalPropertyJsonAdapter("kept");
        assertEquals(
            JsonParser.parseString("{\"marker\":\"kept\"}"),
            gson.toJsonTree(authored, ValuesAdditionalPropertyJsonAdapter.class));
    }
}
