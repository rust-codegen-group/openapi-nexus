import com.example.sdk.models.Tree;
import com.google.gson.Gson;
import com.google.gson.JsonParseException;
import com.google.gson.JsonParser;
import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

class RecursiveHelperTypesTest {
    @Test void recursivePredicatesCompileAlongsideModelsNamedBooleanAndInteger() {
        Gson gson = new Gson();
        String wire = "{\"children\":[{\"children\":[\"leaf\"]}]}";
        Tree value = gson.fromJson(wire, Tree.class);
        assertInstanceOf(Tree.VariantBranch.class, value);
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(value, Tree.class));
        assertThrows(JsonParseException.class, () -> gson.fromJson("{\"children\":[42]}", Tree.class));
    }
}
