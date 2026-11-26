import com.example.sdk.models.Union;
import com.google.gson.Gson;
import com.google.gson.JsonParser;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;

public final class AuthoredTagRoundTripTest {
    @Test
    void roundTripsAbsentAndBothAlternateLabels() {
        var gson = new Gson();
        for (var json : new String[] {
            "{\"payload_a\":\"a\",\"payload_a_schema\":{}}",
            "{\"kind\":\"variant_a\",\"payload_a\":\"a\",\"payload_a_schema\":{}}",
            "{\"kind\":\"variant_b_1\",\"payload_b\":3}",
            "{\"kind\":\"variant_b_2\",\"payload_b\":3}"
        }) {
            var model = gson.fromJson(json, Union.class);
            assertEquals(JsonParser.parseString(json), JsonParser.parseString(gson.toJson(model, Union.class)));
        }
    }
}
