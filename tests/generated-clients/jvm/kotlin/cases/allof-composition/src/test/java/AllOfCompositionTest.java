import static org.junit.jupiter.api.Assertions.*;

import com.example.sdk.models.MixedAlias;
import com.example.sdk.models.MixedMap;
import com.example.sdk.models.NestedValue;
import com.example.sdk.models.OpenAllOf;
import com.example.sdk.models.OpaqueMap;
import com.google.gson.Gson;
import com.google.gson.JsonParser;
import com.google.gson.stream.JsonWriter;
import java.io.IOException;
import java.io.StringWriter;
import org.junit.jupiter.api.Test;

class AllOfCompositionTest {
    private final Gson gson = new Gson();

    private void assertWire(String expected, Object value) {
        assertEquals(JsonParser.parseString(expected), JsonParser.parseString(gson.toJson(value)));
    }

    @Test void constructedMemberKeepsItsExistingConstructorAndGetterButWritesFlatJson() {
        MixedMap member = new MixedMap("fixed");
        MixedAlias value = new MixedAlias(member);
        assertSame(member, value.getMixedMap());
        assertWire("{\"known_field\":\"fixed\"}", value);
    }

    @Test void flatAliasInputRetainsTypedExtensionValues() {
        String wire = "{\"known_field\":\"fixed\",\"extra_key\":{\"wire_value\":\"nested\"}}";
        MixedAlias value = gson.fromJson(wire, MixedAlias.class);
        assertNotNull(value.getMixedMap());
        assertEquals("nested", value.getMixedMap().getAdditionalProperties().get("extra_key").getWireValue());
        assertWire(wire, value);
    }

    @Test void typedExtensionNullsDoNotForceOrdinaryNestedFieldNulls() {
        MixedMap member = new MixedMap("fixed");
        member.getAdditionalProperties().put("model", gson.fromJson("{}", NestedValue.class));
        member.getAdditionalProperties().put("nil", null);
        assertWire("{\"known_field\":\"fixed\",\"model\":{},\"nil\":null}", new MixedAlias(member));
    }

    @Test void openObjectAndMapKeepFlatOpaqueValues() {
        String wire = "{\"known_field\":\"fixed\",\"knownField\":\"shadow\",\"extra_key\":{\"nil\":null}}";
        OpenAllOf value = gson.fromJson(wire, OpenAllOf.class);
        assertEquals(JsonParser.parseString(wire), JsonParser.parseString(gson.toJson(value)));
    }

    @Test void ordinaryMixedObjectCodecRemainsTheControl() {
        assertWire("{\"known_field\":\"fixed\"}", new MixedMap("fixed"));
    }

    @Test void opaqueValuesPreserveNullsLargeNumbersAndVerbatimKeys() {
        String wire = "{\"known_field\":\"fixed\",\"big\":9007199254740993,\"nil\":null,\"nested_key\":[null,{\"yes\":true}]}";
        assertWire(wire, gson.fromJson(wire, OpenAllOf.class));
    }

    @Test void composedRootNullRemainsKotlinNull() {
        assertNull(gson.fromJson("null", MixedAlias.class));
        assertEquals("null", gson.toJson(null, MixedAlias.class));
    }

    @Test void writerNullPolicyIsRestoredWhenOpaqueOutputFails() throws IOException {
        OpenAllOf value = gson.fromJson("{\"known_field\":\"fixed\",\"nil\":null}", OpenAllOf.class);
        JsonWriter writer = new JsonWriter(new StringWriter()) {
            @Override public JsonWriter nullValue() throws IOException { throw new IOException("opaque-null"); }
        };
        writer.setSerializeNulls(false);
        assertThrows(IOException.class, () -> gson.getAdapter(OpenAllOf.class).write(writer, value));
        assertFalse(writer.getSerializeNulls());
    }
}
