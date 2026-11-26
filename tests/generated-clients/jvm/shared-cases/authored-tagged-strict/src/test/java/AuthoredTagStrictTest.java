import com.example.sdk.models.Choice;
import com.example.sdk.models.Envelope;
import com.example.sdk.models.GatedChoice;
import com.example.sdk.models.GatedContainer;
import com.google.gson.Gson;
import com.google.gson.JsonParseException;
import com.google.gson.JsonParser;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

public final class AuthoredTagStrictTest {
    private final Gson gson = new Gson();

    @Test
    void rejectsAmbiguityByMemberNotByLabelCount() {
        var error = assertThrows(JsonParseException.class,
            () -> gson.fromJson("{\"label\":\"shared\"}", Choice.class));
        assertTrue(error.getMessage().contains("ambiguous"));
        roundTrip(Choice.class, "{\"kind\":\"b_alt\",\"label\":\"shared\"}");
        assertThrows(JsonParseException.class,
            () -> gson.fromJson("{\"kind\":\"b_alt\",\"label\":\"alpha\"}", Choice.class));
    }

    @Test
    void checksRecursivePayloadsBeforeDecoding() {
        roundTrip(Envelope.class, "{\"tree\":{\"children\":{\"a\":{\"label\":\"x\",\"extra\":3}}}}");
        assertThrows(JsonParseException.class,
            () -> gson.fromJson("{\"tree\":{\"children\":{\"a\":{\"label\":\"x\",\"extra\":false}}}}", Envelope.class));
    }

    @Test
    void commonConstraintsGateOpaqueBranchesButDoNotPromoteThem() {
        var concrete = "{\"scope\":\"x\",\"other_value\":\"x\"}";
        assertInstanceOf(GatedContainer.VariantOtherPayload.class, gson.fromJson(concrete, GatedContainer.class));
        var opaque = "{\"scope\":\"x\",\"extension\":null}";
        assertInstanceOf(GatedContainer.VariantGatedChoice.class, gson.fromJson(opaque, GatedContainer.class));
        roundTrip(GatedContainer.class, opaque);
        assertThrows(JsonParseException.class, () -> gson.fromJson("{\"extension\":null}", GatedChoice.class));
    }

    private <T> void roundTrip(Class<T> type, String json) {
        assertEquals(JsonParser.parseString(json), JsonParser.parseString(gson.toJson(gson.fromJson(json, type), type)));
    }
}
