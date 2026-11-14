import com.example.sdk.models.*;
import com.google.gson.Gson;
import com.google.gson.JsonParser;
import java.util.List;
import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

class UnionSelectionTest {
    private final Gson gson = new Gson();
    private static final String PAYLOAD_A = "{\"label\":\"weather\",\"payload_schema\":{\"type\":\"object\"}}";
    private static final String PAYLOAD_B = "{\"kind\":\"variant_b\",\"label\":\"payload_b\",\"limit\":1}";
    private static final String NUMERIC = "{\"revision\":2,\"second_value\":\"two\"}";

    private <T> void roundTrip(String wire, Class<T> type) {
        T value = gson.fromJson(wire, type);
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(value, type));
    }

    @Test void directVariantMembersPreserveWire() {
        roundTrip(PAYLOAD_A, PayloadA.class);
        roundTrip(PAYLOAD_B, PayloadB.class);
    }

    @Test void typedUnionRequestIsJsonSerializable() {
        VariantRequest request = new VariantRequest(List.of(
            new VariantSet.VariantPayloadA(gson.fromJson(PAYLOAD_A, PayloadA.class)),
            new VariantSet.VariantPayloadB(gson.fromJson(PAYLOAD_B, PayloadB.class))
        ));
        assertEquals(JsonParser.parseString("{\"choices\":[" + PAYLOAD_A + "," + PAYLOAD_B + "]}"), gson.toJsonTree(request));
    }

    @Test void inboundVariantRequestConstructsUnionVariants() {
        String wire = "{\"choices\":[" + PAYLOAD_A + "," + PAYLOAD_B + "]}";
        VariantRequest request = gson.fromJson(wire, VariantRequest.class);
        assertInstanceOf(VariantSet.VariantPayloadA.class, request.getChoices().get(0));
        assertInstanceOf(VariantSet.VariantPayloadB.class, request.getChoices().get(1));
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(request));
    }

    @Test void directNumericMemberPreservesPayload() { roundTrip(NUMERIC, NumericTwo.class); }

    @Test void disjointRequiredFieldControlSelectsSecondMember() {
        RequiredChoice value = gson.fromJson(NUMERIC, RequiredChoice.class);
        assertInstanceOf(RequiredChoice.VariantRequiredTwo.class, value);
        roundTrip(NUMERIC, RequiredChoice.class);
    }

    @Test void disjointEnumConstraintSelectsSecondMember() {
        Choice value = gson.fromJson(NUMERIC, Choice.class);
        assertInstanceOf(Choice.VariantNumericTwo.class, value);
        roundTrip(NUMERIC, Choice.class);
    }

    @Test void nestedUnionSelectionPreservesScalarArrayAndMapPayloads() {
        String wire = "{\"choice\":" + NUMERIC + ",\"choice_list\":[" + NUMERIC + "],\"choice_map\":{\"keep_key\":" + NUMERIC + "}}";
        Envelope value = gson.fromJson(wire, Envelope.class);
        assertInstanceOf(Choice.VariantNumericTwo.class, value.getChoice());
        assertInstanceOf(Choice.VariantNumericTwo.class, value.getChoiceList().get(0));
        assertInstanceOf(Choice.VariantNumericTwo.class, value.getChoiceMap().get("keep_key"));
        roundTrip(wire, Envelope.class);
    }
}
