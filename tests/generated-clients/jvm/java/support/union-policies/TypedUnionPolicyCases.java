import com.example.sdk.models.*;
import com.example.sdk.apis.*;
import com.google.gson.*;
import java.nio.charset.StandardCharsets;
import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

abstract class TypedUnionPolicyCases {
    protected abstract boolean strict();
    private final Gson gson = new Gson();
    private static final String FINITE = "{\"revision\":2,\"values\":[2,2.5],\"flags\":{\"keep_key\":true},\"second_value\":\"two\"}";

    private <T> T read(String wire, Class<T> type) {
        return gson.fromJson(wire, type);
    }

    private void roundTrip(String wire, Class<?> type) {
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(read(wire, type), type));
    }

    @Test void finiteScalarArrayAndMapFactsSelectTheSecondMember() {
        assertInstanceOf(FiniteChoice.VariantFiniteTwo.class, read(FINITE, FiniteChoice.class));
        roundTrip(FINITE, FiniteChoice.class);
    }

    @Test void unmatchedValuesAndCoercedScalarKindsAreRejected() {
        for (String wire : new String[] {
            FINITE.replace("\"revision\":2", "\"revision\":3"),
            FINITE.replace("\"revision\":2", "\"revision\":\"2\""),
            FINITE.replace("[2,2.5]", "[1.5]"),
            FINITE.replace("true", "false"),
            "{\"revision\":2}", "42", "[]"
        }) {
            assertThrows(JsonParseException.class, () -> read(wire, FiniteChoice.class), wire);
        }
    }

    @Test void containerFactsDistinguishMembersWithoutAScalarTag() {
        String wire = "{\"values\":[2,2],\"second_value\":\"two\"}";
        assertInstanceOf(ContainerChoice.VariantContainerTwo.class, read(wire, ContainerChoice.class));
        roundTrip(wire, ContainerChoice.class);
        assertThrows(JsonParseException.class, () -> read("{\"values\":[3]}", ContainerChoice.class));
    }

    @Test void largeIntegersRemainDisjointAboveDoublePrecision() {
        String wire = "{\"revision\":9007199254740993,\"second_value\":\"two\"}";
        LargeChoice.VariantLargeTwo value = assertInstanceOf(LargeChoice.VariantLargeTwo.class, read(wire, LargeChoice.class));
        assertEquals(9007199254740993L, value.value0().getRevision());
        roundTrip(wire, LargeChoice.class);
    }

    @Test void zeroExponentsDoNotIntroduceMatcherScaleLimits() {
        for (String wire : new String[] {"0e10000", "0e-10000", "-0.000e10000", "0e2147483648", "0e-2147483649"}) {
            double ordinary = gson.fromJson(JsonParser.parseString(wire), Double.class);
            NumericZero.VariantNumber value = assertInstanceOf(NumericZero.VariantNumber.class, read(wire, NumericZero.class));
            assertEquals(ordinary, value.value0(), wire);
        }
    }

    @Test void unmatchedExponentsReachOpaqueFallbackWithoutRounding() {
        for (String wire : new String[] {"1e10000", "1e-10000", "1e2147483648", "1e-2147483649", "12500000000000000001e-19"}) {
            NumericFinite.VariantAny value = assertInstanceOf(NumericFinite.VariantAny.class, read(wire, NumericFinite.class));
            assertEquals(wire, value.value0().getAsString());
        }
        for (String wire : new String[] {"1.25", "125e-2", "0.0125000e2", "12500000000000000000e-19"}) {
            assertInstanceOf(NumericFinite.VariantNumber.class, read(wire, NumericFinite.class));
        }
    }

    @Test void integerSelectionDoesNotHideTheNativeDelegateLimit() {
        String wire = "0e10000";
        JsonParseException ordinary = assertThrows(JsonParseException.class, () -> gson.fromJson(JsonParser.parseString(wire), Long.class));
        JsonParseException selected = assertThrows(JsonParseException.class, () -> read(wire, IntegerFallback.class));
        assertEquals(ordinary.getMessage(), selected.getMessage());
        for (String fractional : new String[] {"1e-10000", "1e-2147483649"}) {
            assertInstanceOf(IntegerFallback.VariantAny.class, read(fractional, IntegerFallback.class));
        }
    }

    @Test void binarySelectionRejectsOutOfRangeExponentsWithoutParserFailures() {
        for (String wire : new String[] {"[1e10000]", "[-1e10000]", "[1e-10000]", "[1e2147483648]"}) {
            JsonParseException error = assertThrows(JsonParseException.class, () -> read(wire, BinaryChoice.class));
            assertTrue(error.getMessage().contains("unknown BinaryChoice union value"));
        }
        String wire = "[0e10000]";
        JsonParseException ordinary = assertThrows(JsonParseException.class, () -> gson.fromJson(JsonParser.parseString(wire), byte[].class));
        JsonParseException selected = assertThrows(JsonParseException.class, () -> read(wire, BinaryChoice.class));
        assertEquals(ordinary.getMessage(), selected.getMessage());
    }

    @Test void scalarFiniteFactsDoNotWidenTheirJsonKinds() {
        assertInstanceOf(ScalarChoice.VariantInteger.class, read("1", ScalarChoice.class));
        assertInstanceOf(ScalarChoice.VariantNumber.class, read("2.5", ScalarChoice.class));
        assertInstanceOf(ScalarChoice.VariantBoolean.class, read("true", ScalarChoice.class));
        for (String wire : new String[] {"2", "false", "\"1\""}) {
            assertThrows(JsonParseException.class, () -> read(wire, ScalarChoice.class));
        }
    }

    @Test void finiteNullPermissionsStayAtTheirAuthoredBoundary() {
        assertInstanceOf(NullableFields.VariantIncludedNull.class, read("{\"revision\":null,\"second_value\":\"two\"}", NullableFields.class));
        assertInstanceOf(NullableFields.VariantExcludedNull.class, read("{\"revision\":1}", NullableFields.class));
    }

    @Test void typedRootNullHandlingRetainsTheExistingJavaApi() {
        assertNull(read("null", FiniteChoice.class));
        assertSame(NullableChoice.VariantNull.INSTANCE, read("null", NullableChoice.class));
        roundTrip("null", NullableChoice.class);
    }

    @Test void concreteMembersPrecedeOpaqueFallbacks() {
        assertInstanceOf(OpaqueFirst.VariantFiniteTwo.class, read(FINITE, OpaqueFirst.class));
        roundTrip(FINITE, OpaqueFirst.class);
        assertInstanceOf(OpaqueFirst.VariantAny.class, read("42", OpaqueFirst.class));
        roundTrip("42", OpaqueFirst.class);
    }

    @Test void ambiguityIsCountedBeforeTheSelectedDelegateRuns() {
        String wire = "{\"wire_name\":\"detail\",\"first_value\":\"one\"}";
        if (strict()) {
            JsonParseException error = assertThrows(JsonParseException.class, () -> read(wire, Ambiguous.class));
            assertTrue(error.getMessage().contains("ambiguous untagged union Ambiguous"));
        } else {
            assertInstanceOf(Ambiguous.VariantAlpha.class, read(wire, Ambiguous.class));
            roundTrip(wire, Ambiguous.class);
        }
    }

    @Test void aliasWrappersRetainTheirOwnConversionBoundary() {
        String wire = "{\"value\":7}";
        AliasChoice.VariantNumberAlias value = assertInstanceOf(AliasChoice.VariantNumberAlias.class, read(wire, AliasChoice.class));
        assertEquals(7L, value.value0().getValue());
        roundTrip(wire, AliasChoice.class);
        assertInstanceOf(AliasChoice.VariantAlpha.class, read("{\"wire_name\":\"detail\"}", AliasChoice.class));
    }

    @Test void nestedStrictDelegateErrorsDoNotFallBackToOpaqueJson() {
        String wire = "{\"value\":{\"wire_name\":\"detail\"}}";
        if (strict()) {
            JsonParseException error = assertThrows(JsonParseException.class, () -> read(wire, OpenNested.class));
            assertTrue(error.getMessage().contains("ambiguous untagged union Ambiguous"));
        } else {
            assertInstanceOf(OpenNested.VariantAmbiguousAlias.class, read(wire, OpenNested.class));
            roundTrip(wire, OpenNested.class);
        }
    }

    @Test void intersectionMembersKeepTheirExistingWrappedWireRepresentation() {
        String wire = "{\"leftPart\":{\"left_value\":\"left\"},\"rightPart\":{\"right_value\":2}}";
        assertInstanceOf(WrappedChoice.VariantWrappedPair.class, read(wire, WrappedChoice.class));
        roundTrip(wire, WrappedChoice.class);
        assertInstanceOf(WrappedChoice.VariantAlpha.class, read("{\"wire_name\":\"detail\"}", WrappedChoice.class));
    }

    @Test void deepRecursivePredicatesReuseBothSuccessfulAndFailedChecks() {
        JsonObject wire = JsonParser.parseString("{\"revision\":2,\"children\":[],\"second_value\":\"leaf\"}").getAsJsonObject();
        JsonObject invalid = JsonParser.parseString("{\"revision\":3,\"children\":[]}").getAsJsonObject();
        for (int i = 0; i < 24; i++) {
            JsonObject parent = new JsonObject();
            JsonArray children = new JsonArray();
            children.add(wire);
            parent.add("children", children);
            parent.addProperty("revision", 2);
            parent.addProperty("second_value", "branch");
            wire = parent;
            JsonObject badParent = new JsonObject();
            JsonArray badChildren = new JsonArray();
            badChildren.add(invalid);
            badParent.add("children", badChildren);
            badParent.addProperty("revision", 2);
            invalid = badParent;
        }
        assertInstanceOf(Tree.VariantBranchTwo.class, gson.fromJson(wire, Tree.class));
        roundTrip(wire.toString(), Tree.class);
        JsonObject rejected = invalid;
        assertThrows(JsonParseException.class, () -> gson.fromJson(rejected, Tree.class));
    }

    @Test void recursiveSchemaIdentityIsNotItsPascalCaseJavaName() {
        assertInstanceOf(RecursiveChoice.VariantString.class, read("\"safe\"", RecursiveChoice.class));
        roundTrip("\"safe\"", RecursiveChoice.class);
    }

    @Test void binaryMembersMatchGsonsNativeByteArrayRepresentation() {
        assertInstanceOf(BinaryChoice.VariantBinary.class, read("[-128,0,127]", BinaryChoice.class));
        roundTrip("[-128,0,127]", BinaryChoice.class);
        assertInstanceOf(BinaryChoice.VariantString.class, read("\"text\"", BinaryChoice.class));
        assertThrows(JsonParseException.class, () -> read("[256]", BinaryChoice.class));
    }

    @Test void nativeFormatBoundsDoNotBecomeSelectionValidators() {
        String wire = "{\"identifier\":2147483648,\"second_value\":\"two\"}";
        // The existing adapter delegates through Gson's JsonElement path,
        // whose integer overflow behavior differs from its streaming reader.
        int nativeValue = gson.fromJson(JsonParser.parseString(wire), Narrow.class).getIdentifier();
        if (strict()) {
            JsonParseException error = assertThrows(JsonParseException.class, () -> read(wire, FormatChoice.class));
            assertTrue(error.getMessage().contains("ambiguous untagged union FormatChoice"));
        } else {
            FormatChoice.VariantNarrow value = assertInstanceOf(FormatChoice.VariantNarrow.class, read(wire, FormatChoice.class));
            assertEquals(nativeValue, value.value0().getIdentifier());
        }
        FormatFallback.VariantNarrow value = assertInstanceOf(FormatFallback.VariantNarrow.class, read(wire, FormatFallback.class));
        assertEquals(nativeValue, value.value0().getIdentifier());
    }

    @Test void containingModelsUseTheSameSelectionInFieldsListsAndMaps() {
        String wire = "{\"caption\":\"envelope\",\"choice\":" + FINITE + ",\"choices\":[" + FINITE + "],\"by_name\":{\"keep_key\":" + FINITE + "}}";
        ChoiceEnvelope value = read(wire, ChoiceEnvelope.class);
        assertInstanceOf(FiniteChoice.VariantFiniteTwo.class, value.getChoice());
        assertInstanceOf(FiniteChoice.VariantFiniteTwo.class, value.getChoices().get(0));
        assertInstanceOf(FiniteChoice.VariantFiniteTwo.class, value.getByName().get("keep_key"));
        roundTrip(wire, ChoiceEnvelope.class);
    }

    @Test void successfulHttpResponsesUseTheSelectedTypedMember() throws Exception {
        GetChoiceResponse response = UnionHttpTestSupport.api(200, FINITE).getChoice();
        assertEquals(200, response.getStatusCode());
        assertInstanceOf(FiniteChoice.VariantFiniteTwo.class, response.getStatus200());
        assertEquals(JsonParser.parseString(FINITE), gson.toJsonTree(response.getStatus200(), FiniteChoice.class));
    }

    @Test void typedHttpErrorsPreservePayloadsAndRawBytes() {
        GetChoiceException error = assertThrows(GetChoiceException.class, () -> UnionHttpTestSupport.api(400, FINITE).getChoice());
        GetChoiceBadRequest detail = assertInstanceOf(GetChoiceBadRequest.class, error.detail());
        assertInstanceOf(FiniteChoice.VariantFiniteTwo.class, detail.body());
        assertArrayEquals(FINITE.getBytes(StandardCharsets.UTF_8), error.rawBody());
        assertArrayEquals(error.rawBody(), detail.rawBody());
    }

    @Test void unmatchedHttpSuccessBodiesPropagateSelectionErrors() {
        String wire = FINITE.replace("\"revision\":2", "\"revision\":3");
        assertThrows(JsonParseException.class, () -> UnionHttpTestSupport.api(200, wire).getChoice());
    }

    @Test void unmatchedHttpErrorBodiesRemainLazyAndKeepTheirDiagnostics() {
        String wire = FINITE.replace("\"revision\":2", "\"revision\":3");
        GetChoiceException error = assertThrows(GetChoiceException.class, () -> UnionHttpTestSupport.api(400, wire).getChoice());
        GetChoiceBadRequest detail = assertInstanceOf(GetChoiceBadRequest.class, error.detail());
        assertThrows(JsonParseException.class, detail::body);
        assertThrows(JsonParseException.class, detail::body);
        assertArrayEquals(wire.getBytes(StandardCharsets.UTF_8), error.rawBody());
    }
}
