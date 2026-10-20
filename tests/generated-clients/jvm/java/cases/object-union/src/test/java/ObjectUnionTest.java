import com.example.sdk.models.OneOfObjects;
import com.google.gson.Gson;
import static org.junit.jupiter.api.Assertions.*;

public final class ObjectUnionTest {
    @org.junit.jupiter.api.Test
    void selectsMatchingRequiredFields() {
        Gson gson = new Gson();
        OneOfObjects value = gson.fromJson("{\"user\":{\"id\":1}}", OneOfObjects.class);
        assertInstanceOf(OneOfObjects.VariantNestedObject.class, value);
    }
}
