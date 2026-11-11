import com.example.sdk.models.*;
import com.google.gson.Gson;
import com.google.gson.JsonParser;
import java.util.List;
import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

class UnionSelectionTest {
    private final Gson gson = new Gson();
    private static final String CLIENT = "{\"name\":\"weather\",\"input_schema\":{\"type\":\"object\"}}";
    private static final String SEARCH = "{\"type\":\"web_search_20250305\",\"name\":\"web_search\",\"max_uses\":1}";
    private static final String NUMERIC = "{\"revision\":2,\"second_value\":\"two\"}";

    private <T> void roundTrip(String wire, Class<T> type) {
        T value = gson.fromJson(wire, type);
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(value, type));
    }

    @Test void directToolMembersPreserveWire() {
        roundTrip(CLIENT, ClientTool.class);
        roundTrip(SEARCH, SearchTool.class);
    }

    @Test void typedUnionRequestIsJsonSerializable() {
        Request request = new Request(List.of(
            new Tool.VariantClientTool(gson.fromJson(CLIENT, ClientTool.class)),
            new Tool.VariantSearchTool(gson.fromJson(SEARCH, SearchTool.class))
        ));
        assertEquals(JsonParser.parseString("{\"tools\":[" + CLIENT + "," + SEARCH + "]}"), gson.toJsonTree(request));
    }

    @Test void inboundToolRequestConstructsUnionVariants() {
        String wire = "{\"tools\":[" + CLIENT + "," + SEARCH + "]}";
        Request request = gson.fromJson(wire, Request.class);
        assertInstanceOf(Tool.VariantClientTool.class, request.getTools().get(0));
        assertInstanceOf(Tool.VariantSearchTool.class, request.getTools().get(1));
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
