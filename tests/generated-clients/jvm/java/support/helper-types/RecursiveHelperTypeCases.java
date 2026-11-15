import com.example.sdk.models.*;
import com.google.gson.Gson;
import com.google.gson.JsonParser;
import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

abstract class RecursiveHelperTypeCases {
    private final Gson gson = new Gson();

    private <T> T roundTrip(String wire, Class<T> type) {
        T value = gson.fromJson(wire, type);
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(value, type));
        return value;
    }

    @Test void primitiveNumericDelegatesDoNotResolveToAuthoredModels() {
        IntegerChoice.VariantInteger integer = assertInstanceOf(
            IntegerChoice.VariantInteger.class, roundTrip("7", IntegerChoice.class));
        assertEquals(7, integer.value0());

        LongChoice.VariantInteger longValue = assertInstanceOf(
            LongChoice.VariantInteger.class, roundTrip("9007199254740993", LongChoice.class));
        assertEquals(9007199254740993L, longValue.value0());

        FloatChoice.VariantNumber floatValue = assertInstanceOf(
            FloatChoice.VariantNumber.class, roundTrip("1.25", FloatChoice.class));
        assertEquals(1.25f, floatValue.value0());

        DoubleChoice.VariantNumber doubleValue = assertInstanceOf(
            DoubleChoice.VariantNumber.class, roundTrip("2.5", DoubleChoice.class));
        assertEquals(2.5, doubleValue.value0());
    }

    @Test void primitiveListsAndMapsKeepBoxedIntegerValues() {
        PrimitiveContainers.VariantArray array = assertInstanceOf(
            PrimitiveContainers.VariantArray.class, roundTrip("[1,2]", PrimitiveContainers.class));
        java.util.List<java.lang.Integer> list = array.value0();
        assertEquals(java.util.List.of(1, 2), list);
        assertInstanceOf(java.lang.Integer.class, list.get(0));

        PrimitiveContainers.VariantMap dictionary = assertInstanceOf(
            PrimitiveContainers.VariantMap.class, roundTrip("{\"count\":7}", PrimitiveContainers.class));
        java.util.Map<String, java.lang.Integer> map = dictionary.value0();
        assertEquals(java.util.Map.of("count", 7), map);
        assertInstanceOf(java.lang.Integer.class, map.get("count"));
    }

    @Test void nestedBooleanContainersKeepBoxedValues() {
        String wire = "[{\"enabled\":true,\"disabled\":false}]";
        NestedBooleans.VariantArray array = assertInstanceOf(
            NestedBooleans.VariantArray.class, roundTrip(wire, NestedBooleans.class));
        java.util.List<java.util.Map<String, java.lang.Boolean>> values = array.value0();
        assertEquals(java.util.List.of(java.util.Map.of("enabled", true, "disabled", false)), values);
        assertInstanceOf(java.lang.Boolean.class, values.get(0).get("enabled"));
    }

    @Test void namedIntegerMembersRetainTheirAuthoredIdentity() {
        NamedIntegerChoice.VariantInteger named = assertInstanceOf(
            NamedIntegerChoice.VariantInteger.class,
            roundTrip("{\"value\":\"named-model\"}", NamedIntegerChoice.class));
        com.example.sdk.models.Integer integer = named.value0();
        assertEquals("named-model", integer.getValue());

        String wire = "[{\"value\":\"first-model\"},{\"value\":\"second-model\"}]";
        NamedIntegerListChoice.VariantArray array = assertInstanceOf(
            NamedIntegerListChoice.VariantArray.class, roundTrip(wire, NamedIntegerListChoice.class));
        java.util.List<com.example.sdk.models.Integer> integers = array.value0();
        assertEquals(2, integers.size());
        assertEquals("first-model", integers.get(0).getValue());
        assertEquals("second-model", integers.get(1).getValue());
    }

    @Test void recursiveCachesDoNotResolveBoxedTypesToGeneratedModels() {
        String wire = "{\"children\":[{\"children\":[\"leaf\"]}]}";
        Tree.VariantBranch value = assertInstanceOf(Tree.VariantBranch.class, roundTrip(wire, Tree.class));
        assertInstanceOf(Tree.VariantBranch.class, value.value0().getChildren().get(0));
        assertEquals("boolean-model", new com.example.sdk.models.Boolean("boolean-model").getValue());
        assertEquals("integer-model", new com.example.sdk.models.Integer("integer-model").getValue());
        assertEquals("long-model", new com.example.sdk.models.Long("long-model").getValue());
        assertEquals("float-model", new com.example.sdk.models.Float("float-model").getValue());
        assertEquals("double-model", new com.example.sdk.models.Double("double-model").getValue());
    }
}
