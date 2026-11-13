import static org.junit.jupiter.api.Assertions.*;

import com.example.sdk.models.*;
import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.google.gson.FieldNamingPolicy;
import com.google.gson.JsonSyntaxException;
import com.google.gson.reflect.TypeToken;
import com.google.gson.stream.JsonWriter;
import com.google.gson.stream.JsonReader;
import com.google.gson.TypeAdapter;
import com.google.gson.JsonParseException;
import com.example.sdk.apis.DefaultApi;
import com.example.sdk.apis.EchoCompositionBadRequest;
import com.example.sdk.apis.EchoCompositionException;
import com.example.sdk.runtime.ApiClient;
import java.io.IOException;
import java.io.StringWriter;
import java.util.Map;
import com.google.gson.JsonParser;
import org.junit.jupiter.api.Test;
import okhttp3.MediaType;
import okhttp3.OkHttpClient;
import okhttp3.Protocol;
import okhttp3.Response;
import okhttp3.ResponseBody;

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

    @Test void openObjectAndMapKeepFlatOpaqueValues() {
        String wire = "{\"known_field\":\"fixed\",\"knownField\":\"shadow\",\"extra_key\":{\"nil\":null}}";
        assertWire(wire, gson.fromJson(wire, OpenAllOf.class));
    }

    @Test void ordinaryMixedObjectCodecRemainsTheControl() {
        assertWire("{\"known_field\":\"fixed\"}", new MixedMap("fixed"));
    }

    @Test void siblingKeysDoNotRemainInExtensionStorageAfterEditing() {
        Pair value = gson.fromJson("{\"known_field\":\"fixed\",\"other_field\":7,\"nil\":null}", Pair.class);
        assertEquals(7, value.getOther().getOtherField());
        assertFalse(value.getOpaqueMap().getAdditionalProperties().containsKey("other_field"));
        assertWire("{\"known_field\":\"fixed\",\"other_field\":7,\"nil\":null}", value);
        assertWire("{\"known_field\":\"fixed\",\"other_field\":8,\"nil\":null}", new Pair(value.getOpaqueMap(), new Other(8)));
    }

    @Test void nestedAndRepeatedCompositionsKeepTheirMemberEntrypoints() {
        String wire = "{\"known_field\":\"fixed\",\"other_field\":7,\"nil\":null}";
        Nested value = gson.fromJson(wire, Nested.class);
        assertEquals(7, value.getPair().getOther().getOtherField());
        assertWire(wire, value);
        Repeated repeated = gson.fromJson("{\"known_field\":\"fixed\"}", Repeated.class);
        assertEquals(repeated.getOpaqueMap().getKnownField(), repeated.getOpaqueMap2().getKnownField());
        assertThrows(IllegalArgumentException.class, () -> gson.toJson(new Repeated(new OpaqueMap("one"), new OpaqueMap("two"))));
    }

    @Test void opaqueMapMembersPreserveNullsLargeNumbersAndVerbatimKeys() {
        String wire = "{\"known_field\":\"fixed\",\"big\":9007199254740993,\"nil\":null,\"nested_key\":[null,{\"yes\":true}]}";
        assertWire(wire, gson.fromJson(wire, OpenAllOf.class));
        Dictionary dictionary = gson.fromJson(wire, Dictionary.class);
        assertWire(wire, dictionary);
        assertEquals("9007199254740993", dictionary.getMapStringObject().get("big").toString());
    }

    @Test void typedMapMembersRetainNativeStreamAndTreeCoercionDifferences() {
        String wire = "{\"value\":1.7}";
        var type = new TypeToken<Map<String, Integer>>() {}.getType();
        assertThrows(JsonSyntaxException.class, () -> gson.fromJson(wire, type));
        assertThrows(JsonSyntaxException.class, () -> gson.fromJson(wire, IntDictionary.class));
        Map<String, Integer> control = gson.fromJson(JsonParser.parseString(wire), type);
        assertEquals(control.get("value"), gson.fromJson(JsonParser.parseString(wire), IntDictionary.class).getMapStringInteger().get("value"));
    }

    @Test void projectionRetainsNestedDuplicateMergeAndEarlierDecodeErrors() {
        String wire = "{\"part\":{\"left\":\"kept\"},\"part\":{\"right\":\"also\"}}";
        Container direct = gson.fromJson(wire, Container.class);
        ContainerAlias value = gson.fromJson(wire, ContainerAlias.class);
        assertEquals(direct.getPart().getLeft(), value.getContainer().getPart().getLeft());
        assertEquals(direct.getPart().getRight(), value.getContainer().getPart().getRight());
        String invalid = "{\"other_field\":{},\"other_field\":7}";
        assertThrows(JsonSyntaxException.class, () -> gson.fromJson(invalid, Other.class));
        assertThrows(JsonSyntaxException.class, () -> gson.fromJson(invalid, Pair.class));
    }

    @Test void configuredNamingAndWriterNullPolicyStayLocalToMembers() throws Exception {
        Gson configured = new GsonBuilder().setFieldNamingPolicy(FieldNamingPolicy.UPPER_CAMEL_CASE).create();
        String wire = "{\"CamelName\":\"kept\",\"other_field\":7,\"nil\":null}";
        ConfiguredPair value = configured.fromJson(wire, ConfiguredPair.class);
        assertEquals("kept", value.getConfigured().getCamelName());
        assertFalse(value.getConfigured().getAdditionalProperties().containsKey("other_field"));
        assertEquals(JsonParser.parseString(wire), JsonParser.parseString(configured.toJson(value)));
        StringWriter buffer = new StringWriter();
        JsonWriter writer = new JsonWriter(buffer);
        writer.setSerializeNulls(false);
        configured.getAdapter(ConfiguredPair.class).write(writer, value);
        assertFalse(writer.getSerializeNulls());
    }

    @Test void rawFallbacksPreserveNullAndUnsupportedJsonWithoutWrapperKeys() {
        for (java.lang.Class<?> type : new java.lang.Class<?>[]{Nullable.class, Cycle.class, Scalar.class, Incompatible.class, AliasComposition.class}) {
            for (String wire : new String[]{"null", "{\"big\":9007199254740993,\"nil\":null}"}) {
                assertWire(wire, gson.fromJson(wire, type));
            }
        }
    }

    @Test void allOfMembersInsideUntaggedUnionsUseFlatMatcherFacts() {
        String wire = "{\"known_field\":\"fixed\",\"other_field\":7}";
        assertInstanceOf(Choice.VariantPair.class, gson.fromJson(wire, Choice.class));
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(gson.fromJson(wire, Choice.class), Choice.class));
    }

    @Test void authoredHelperTypesAndJavaReservedMemberNamesDoNotBreakCompilation() {
        assertWire("{\"label\":\"kept\"}", new ClassComposition(new com.example.sdk.models.Class("kept")));
        assertNotNull(gson.fromJson("{\"label\":\"kept\"}", ClassComposition.class).getClass_());
        assertWire("{\"label\":\"kept\"}", new HelperComposition(new com.example.sdk.models.Gson("kept")));
    }

    @Test void configuredMemberAdaptersAreReusedWithoutReplacingOrdinaryCodecs() {
        TypeAdapter<NestedValue> adapter = new TypeAdapter<>() {
            @Override public NestedValue read(JsonReader reader) throws IOException { return new NestedValue(reader.nextString()); }
            @Override public void write(JsonWriter writer, NestedValue value) throws IOException { writer.value(value.getWireValue()); }
        };
        Gson configured = new GsonBuilder().registerTypeAdapter(NestedValue.class, adapter).create();
        String wire = "{\"known_field\":\"fixed\",\"extra\":\"custom\"}";
        MixedAlias value = configured.fromJson(wire, MixedAlias.class);
        assertEquals("custom", value.getMixedMap().getAdditionalProperties().get("extra").getWireValue());
        assertEquals(JsonParser.parseString(wire), configured.toJsonTree(value));
    }

    @Test void configuredNumericOutputAndNamedNullOmissionMatchTheOrdinaryMember() {
        Gson configured = new GsonBuilder().serializeSpecialFloatingPointValues().create();
        Numeric member = new Numeric(Double.NaN, null);
        assertEquals(configured.toJson(member), configured.toJson(new NumericAlias(member)));
        assertEquals("{\"amount\":NaN}", configured.toJson(new NumericAlias(member)));
    }

    @Test void nestedContainerAndBinaryMapValuesKeepNativeCodecsAndOpaquePrecision() {
        String nested = "{\"key\":[null,{\"big\":9007199254740993,\"nil\":null}]}";
        assertWire(nested, gson.fromJson(nested, NestedDictionary.class));
        String binary = "{\"bytes\":[0,127,-128],\"nil\":null}";
        assertWire(binary, gson.fromJson(binary, BinaryDictionary.class));
    }

    @Test void recursiveCompositionFollowsObjectFieldsWithoutFlatteningTheRecursion() {
        String wire = "{\"label\":\"root\",\"next\":{\"label\":\"leaf\"}}";
        Recursive value = gson.fromJson(wire, Recursive.class);
        assertEquals("leaf", value.getTreeData().getNext().getTreeData().getLabel());
        assertWire(wire, value);
    }

    @Test void writerNullPolicyIsRestoredEvenWhenOpaqueOutputFails() throws IOException {
        Dictionary value = gson.fromJson("{\"nil\":null}", Dictionary.class);
        JsonWriter writer = new JsonWriter(new StringWriter()) {
            @Override public JsonWriter nullValue() throws IOException { throw new IOException("opaque-null"); }
        };
        writer.setSerializeNulls(false);
        assertThrows(IOException.class, () -> gson.getAdapter(Dictionary.class).write(writer, value));
        assertFalse(writer.getSerializeNulls());
    }

    private DefaultApi api(int status, String reply, String expectedRequest) {
        OkHttpClient transport = new OkHttpClient.Builder().addInterceptor(chain -> {
            assertEquals("/composition", chain.request().url().encodedPath());
            okio.Buffer body = new okio.Buffer();
            chain.request().body().writeTo(body);
            assertEquals(JsonParser.parseString(expectedRequest), JsonParser.parseString(body.readUtf8()));
            return new Response.Builder().request(chain.request()).protocol(Protocol.HTTP_1_1)
                .code(status).message(status == 200 ? "OK" : "Bad Request")
                .header("Content-Type", "application/json")
                .body(ResponseBody.create(reply, MediaType.get("application/json"))).build();
        }).build();
        return new DefaultApi(new ApiClient("https://composition.test", transport, null, Map.of()));
    }

    @Test void httpRequestAndSuccessResponseRetainFlatComposition() throws IOException {
        String wire = "{\"known_field\":\"fixed\",\"other_field\":7,\"nil\":null}";
        Pair response = api(200, wire, wire).echoComposition(gson.fromJson(wire, Pair.class)).getStatus200();
        assertWire(wire, response);
    }

    @Test void lazyHttpErrorBodiesRetainCompositionAndNativeFailures() {
        String request = "{\"known_field\":\"fixed\",\"other_field\":7}";
        String reply = "{\"known_field\":\"fixed\",\"other_field\":8,\"nil\":null}";
        EchoCompositionException error = assertThrows(EchoCompositionException.class,
            () -> api(400, reply, request).echoComposition(new Pair(new OpaqueMap("fixed"), new Other(7))));
        assertWire(reply, assertInstanceOf(EchoCompositionBadRequest.class, error.detail()).body());
        EchoCompositionException invalid = assertThrows(EchoCompositionException.class,
            () -> api(400, "{\"other_field\":{}}", request).echoComposition(new Pair(new OpaqueMap("fixed"), new Other(7))));
        assertThrows(JsonParseException.class, assertInstanceOf(EchoCompositionBadRequest.class, invalid.detail())::body);
    }
}
