import static org.junit.jupiter.api.Assertions.assertThrows;

import com.example.sdk.models.AmbiguousValues;
import com.google.gson.Gson;
import com.google.gson.JsonParseException;
import org.junit.jupiter.api.Test;

class AdditionalPropertiesTest extends AdditionalPropertiesContracts {
    @Test void extensionUnionRetainsStrictAmbiguityPolicy() {
        assertThrows(JsonParseException.class, () -> new Gson().fromJson("{\"value\":2}", AmbiguousValues.class));
    }
}
