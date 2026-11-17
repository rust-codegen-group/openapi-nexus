import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.example.sdk.models.Choice;
import com.example.sdk.models.CollisionMap;
import com.example.sdk.models.CollisionMapAdditionalProperty;
import com.example.sdk.models.CollisionMapAdditionalProperty2;
import com.example.sdk.models.Inline;
import com.example.sdk.models.InlineAdditionalProperty;
import com.example.sdk.models.InlineOneOf;
import com.example.sdk.models.InlineOneOfAdditionalProperty;
import com.example.sdk.models.Mixed;
import com.example.sdk.models.MixedAdditionalProperty;
import com.example.sdk.models.MixedTypeInline;
import com.example.sdk.models.MixedTypeRef;
import com.example.sdk.models.NestedArray;
import com.example.sdk.models.NestedArrayAdditionalPropertyItemAdditionalProperty;
import com.example.sdk.models.NestedMap;
import com.example.sdk.models.NestedMapAdditionalPropertyAdditionalProperty;
import com.example.sdk.models.Referenced;
import com.example.sdk.models.SelectionInline;
import com.example.sdk.models.SelectionRef;
import com.google.gson.Gson;
import com.google.gson.JsonParseException;
import com.google.gson.JsonParser;
import java.util.List;
import java.util.Map;
import org.junit.jupiter.api.Test;

class InlineAdditionalUnionTest {
    private static final String VALUES = "{\"integer_key\":7,\"string_key\":\"7\"}";
    private final Gson gson = new Gson();

    private <T> T roundTrip(String wire, Class<T> type) {
        T value = gson.fromJson(wire, type);
        assertNotNull(value);
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(value, type));
        assertEquals(JsonParser.parseString(wire), JsonParser.parseString(gson.toJson(value, type)));
        return value;
    }

    @Test
    void inlineAnyOfValuesDecodeAsTypedIntegerAndStringVariants() {
        Inline value = roundTrip(VALUES, Inline.class);
        Map<String, InlineAdditionalProperty> entries = value.getAdditionalProperties();

        assertEquals(2, entries.size());
        assertEquals(7, assertInstanceOf(
            InlineAdditionalProperty.VariantInteger.class, entries.get("integer_key")).getValue0());
        assertEquals("7", assertInstanceOf(
            InlineAdditionalProperty.VariantString.class, entries.get("string_key")).getValue0());
    }

    @Test
    void inlineOneOfValuesDecodeAsTypedIntegerAndStringVariants() {
        InlineOneOf value = roundTrip(VALUES, InlineOneOf.class);
        Map<String, InlineOneOfAdditionalProperty> entries = value.getAdditionalProperties();

        assertEquals(2, entries.size());
        assertEquals(7, assertInstanceOf(
            InlineOneOfAdditionalProperty.VariantInteger.class, entries.get("integer_key")).getValue0());
        assertEquals("7", assertInstanceOf(
            InlineOneOfAdditionalProperty.VariantString.class, entries.get("string_key")).getValue0());
    }

    @Test
    void inlineAndReferencedDeclarationsHaveEquivalentTypedValues() {
        Referenced referenced = roundTrip(VALUES, Referenced.class);
        Inline inline = roundTrip(VALUES, Inline.class);
        InlineOneOf oneOf = roundTrip(VALUES, InlineOneOf.class);
        Map<String, Choice> control = referenced.getAdditionalProperties();

        int expectedInteger = assertInstanceOf(
            Choice.VariantInteger.class, control.get("integer_key")).getValue0();
        String expectedString = assertInstanceOf(
            Choice.VariantString.class, control.get("string_key")).getValue0();
        assertEquals(7, expectedInteger);
        assertEquals("7", expectedString);
        assertEquals(expectedInteger, assertInstanceOf(
            InlineAdditionalProperty.VariantInteger.class,
            inline.getAdditionalProperties().get("integer_key")).getValue0());
        assertEquals(expectedString, assertInstanceOf(
            InlineAdditionalProperty.VariantString.class,
            inline.getAdditionalProperties().get("string_key")).getValue0());
        assertEquals(expectedInteger, assertInstanceOf(
            InlineOneOfAdditionalProperty.VariantInteger.class,
            oneOf.getAdditionalProperties().get("integer_key")).getValue0());
        assertEquals(expectedString, assertInstanceOf(
            InlineOneOfAdditionalProperty.VariantString.class,
            oneOf.getAdditionalProperties().get("string_key")).getValue0());
    }

    @Test
    void nativeCallersCanPopulateTheTypedPublicDictionaryMaps() {
        Inline inline = new Inline();
        inline.getAdditionalProperties().put("integer_key", new InlineAdditionalProperty.VariantInteger(7));
        inline.getAdditionalProperties().put("string_key", new InlineAdditionalProperty.VariantString("7"));
        assertEquals(JsonParser.parseString(VALUES), gson.toJsonTree(inline, Inline.class));

        InlineOneOf oneOf = new InlineOneOf();
        oneOf.getAdditionalProperties().put("integer_key", new InlineOneOfAdditionalProperty.VariantInteger(7));
        oneOf.getAdditionalProperties().put("string_key", new InlineOneOfAdditionalProperty.VariantString("7"));
        assertEquals(JsonParser.parseString(VALUES), gson.toJsonTree(oneOf, InlineOneOf.class));

        Referenced referenced = new Referenced();
        referenced.getAdditionalProperties().put("integer_key", new Choice.VariantInteger(7));
        referenced.getAdditionalProperties().put("string_key", new Choice.VariantString("7"));
        assertEquals(JsonParser.parseString(VALUES), gson.toJsonTree(referenced, Referenced.class));
    }

    @Test
    void mixedObjectsKeepTheirDeclaredFieldAndTypedAdditionalValues() {
        String wire = "{\"label\":\"fixed\",\"integer_key\":7,\"string_key\":\"7\"}";
        Mixed value = roundTrip(wire, Mixed.class);
        Map<String, MixedAdditionalProperty> entries = value.getAdditionalProperties();

        assertEquals("fixed", value.getLabel());
        assertEquals(2, entries.size());
        assertFalse(entries.containsKey("label"));
        assertEquals(7, assertInstanceOf(
            MixedAdditionalProperty.VariantInteger.class, entries.get("integer_key")).getValue0());
        assertEquals("7", assertInstanceOf(
            MixedAdditionalProperty.VariantString.class, entries.get("string_key")).getValue0());
    }

    @Test
    void nestedDictionaryValuesStayTypedAndPreserveTheirWireShape() {
        String wire = "{\"outer_key\":{\"integer_key\":7,\"string_key\":\"7\"},\"empty_map\":{}}";
        NestedMap value = roundTrip(wire, NestedMap.class);
        Map<String, NestedMapAdditionalPropertyAdditionalProperty> entries =
            value.getAdditionalProperties().get("outer_key");

        assertEquals(2, entries.size());
        assertEquals(7, assertInstanceOf(
            NestedMapAdditionalPropertyAdditionalProperty.VariantInteger.class,
            entries.get("integer_key")).getValue0());
        assertEquals("7", assertInstanceOf(
            NestedMapAdditionalPropertyAdditionalProperty.VariantString.class,
            entries.get("string_key")).getValue0());
        assertTrue(value.getAdditionalProperties().get("empty_map").isEmpty());
    }

    @Test
    void nestedArrayAndMapValuesStayTypedAndPreserveTheirWireShape() {
        String wire = "{\"array_key\":[{\"integer_key\":7},{\"string_key\":\"7\"},{}],\"empty_array\":[]}";
        NestedArray value = roundTrip(wire, NestedArray.class);
        List<Map<String, NestedArrayAdditionalPropertyItemAdditionalProperty>> entries =
            value.getAdditionalProperties().get("array_key");

        assertEquals(3, entries.size());
        assertEquals(7, assertInstanceOf(
            NestedArrayAdditionalPropertyItemAdditionalProperty.VariantInteger.class,
            entries.get(0).get("integer_key")).getValue0());
        assertEquals("7", assertInstanceOf(
            NestedArrayAdditionalPropertyItemAdditionalProperty.VariantString.class,
            entries.get(1).get("string_key")).getValue0());
        assertTrue(entries.get(2).isEmpty());
        assertTrue(value.getAdditionalProperties().get("empty_array").isEmpty());
    }

    @Test
    void unsupportedValueKindsAreRejectedJustLikeTheReferencedControl() {
        for (String wire : new String[] {"{\"value\":true}", "{\"value\":{}}", "{\"value\":[]}"}) {
            assertThrows(JsonParseException.class, () -> gson.fromJson(wire, Inline.class), wire);
            assertThrows(JsonParseException.class, () -> gson.fromJson(wire, InlineOneOf.class), wire);
            assertThrows(JsonParseException.class, () -> gson.fromJson(wire, Referenced.class), wire);
        }
    }

    @Test
    void constrainedArraySelectionsRejectNonNullArraysAndRoundTripNull() {
        String invalid = "{\"value\":[1]}";
        String valid = "{\"value\":null}";
        for (Class<?> type : new Class<?>[] {SelectionInline.class, SelectionRef.class}) {
            assertThrows(JsonParseException.class, () -> gson.fromJson(invalid, type), type.getSimpleName());
            assertEquals(JsonParser.parseString(valid), gson.toJsonTree(gson.fromJson(valid, type), type));
        }
    }

    @Test
    void mixedTypeSetDictionariesPreserveNestedIntegerAndStringValues() {
        String wire = "{\"nested\":{\"integer\":7,\"string\":\"kept\"},\"text\":\"plain\"}";
        for (Class<?> type : new Class<?>[] {MixedTypeInline.class, MixedTypeRef.class}) {
            assertEquals(JsonParser.parseString(wire), JsonParser.parseString(gson.toJson(gson.fromJson(wire, type), type)));
        }
    }

    @Test
    void collisionHelperAndAuthoredModelKeepDistinctPublicIdentities() {
        String wire = "{\"integer\":7,\"string\":\"kept\"}";
        CollisionMap value = roundTrip(wire, CollisionMap.class);
        assertEquals(7, assertInstanceOf(
            CollisionMapAdditionalProperty2.VariantInteger.class,
            value.getAdditionalProperties().get("integer")).getValue0());
        assertEquals("kept", assertInstanceOf(
            CollisionMapAdditionalProperty2.VariantString.class,
            value.getAdditionalProperties().get("string")).getValue0());

        CollisionMap constructed = new CollisionMap();
        constructed.getAdditionalProperties().put("integer", new CollisionMapAdditionalProperty2.VariantInteger(7));
        constructed.getAdditionalProperties().put("string", new CollisionMapAdditionalProperty2.VariantString("kept"));
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(constructed, CollisionMap.class));

        CollisionMapAdditionalProperty authored = new CollisionMapAdditionalProperty("kept");
        assertEquals(JsonParser.parseString("{\"marker\":\"kept\"}"), gson.toJsonTree(authored, CollisionMapAdditionalProperty.class));
    }
}
