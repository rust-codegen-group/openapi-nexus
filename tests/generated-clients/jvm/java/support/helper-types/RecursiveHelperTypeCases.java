import com.example.sdk.models.*;
import com.google.gson.Gson;
import com.google.gson.JsonParser;
import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

abstract class RecursiveHelperTypeCases {
    @Test void recursiveCachesDoNotResolveBoxedTypesToGeneratedModels() {
        Gson gson = new Gson();
        String wire = "{\"children\":[{\"children\":[\"leaf\"]}]}";
        Tree.VariantBranch value = assertInstanceOf(Tree.VariantBranch.class, gson.fromJson(wire, Tree.class));
        assertInstanceOf(Tree.VariantBranch.class, value.value0().getChildren().get(0));
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(value, Tree.class));
        assertEquals("boolean-model", new com.example.sdk.models.Boolean("boolean-model").getValue());
        assertEquals("integer-model", new com.example.sdk.models.Integer("integer-model").getValue());
    }
}
