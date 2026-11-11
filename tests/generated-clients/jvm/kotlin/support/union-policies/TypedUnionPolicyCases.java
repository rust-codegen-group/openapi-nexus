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

    private <T> T read(String wire, Class<T> type) { return gson.fromJson(wire, type); }

    private void roundTrip(String wire, Class<?> type) {
        assertEquals(JsonParser.parseString(wire), gson.toJsonTree(read(wire, type), type));
    }

    @Test void finiteScalarArrayAndMapFactsSelectTheSecondMember() {
        assertInstanceOf(FiniteChoice.VariantFiniteTwo.class, read(FINITE, FiniteChoice.class));
        roundTrip(FINITE, FiniteChoice.class);
    }

    @Test void unmatchedAndCoercedScalarKindsAreRejected() {
        for (String wire : new String[] {
            FINITE.replace("\"revision\":2", "\"revision\":3"),
            FINITE.replace("\"revision\":2", "\"revision\":\"2\""),
            FINITE.replace("[2,2.5]", "[1.5]"), FINITE.replace("true", "false"),
            "{\"revision\":2}", "42", "[]"
        }) { assertThrows(JsonParseException.class, () -> read(wire, FiniteChoice.class), wire); }
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
        assertEquals(9007199254740993L, value.getValue0().getRevision());
        roundTrip(wire, LargeChoice.class);
    }

    @Test void extremeZeroExponentsPreserveOrdinaryDoubleBehavior() {
        for (String wire : new String[] {"0e10000", "0e-10000", "-0.000e10000", "0e2147483648", "0e-2147483649"}) {
            double ordinary = JsonParser.parseString(wire).getAsDouble();
            NumericZero.VariantNumber selected = assertInstanceOf(NumericZero.VariantNumber.class, read(wire, NumericZero.class));
            assertEquals(Double.doubleToRawLongBits(ordinary), Double.doubleToRawLongBits(selected.getValue0()), wire);
        }
    }

    @Test void fallbackNumbersStayExactAndFiniteNumbersDoNotRoundIntoMatches() {
        for (String wire : new String[] {"1e10000", "1e-10000", "1e2147483648", "1e-2147483649", "12500000000000000001e-19"}) {
            NumericFinite.VariantAny selected = assertInstanceOf(NumericFinite.VariantAny.class, read(wire, NumericFinite.class));
            assertEquals(wire, selected.getValue0().getAsString());
        }
        for (String wire : new String[] {"1.25", "125e-2", "0.0125000e2", "12500000000000000000e-19"}) {
            assertInstanceOf(NumericFinite.VariantNumber.class, read(wire, NumericFinite.class));
        }
    }

    @Test void integerSelectionRetainsTheExistingKotlinScalarDelegate() {
        for (String wire : new String[] {"0e10000", "1e2147483648"}) {
            RuntimeException ordinary = assertThrows(RuntimeException.class, () -> JsonParser.parseString(wire).getAsInt());
            RuntimeException selected = assertThrows(RuntimeException.class, () -> read(wire, IntegerFallback.class));
            assertEquals(ordinary.getClass(), selected.getClass());
            assertEquals(ordinary.getMessage(), selected.getMessage());
        }
        for (String wire : new String[] {"1e-10000", "1e-2147483649"}) {
            assertInstanceOf(IntegerFallback.VariantAny.class, read(wire, IntegerFallback.class));
        }
    }

    @Test void binaryMatchingDoesNotMaskNativeByteErrors() {
        for (String wire : new String[] {"[1e10000]", "[-1e10000]", "[1e-10000]", "[1e2147483648]", "[256]"}) {
            JsonParseException error = assertThrows(JsonParseException.class, () -> read(wire, BinaryChoice.class));
            assertTrue(error.getMessage().contains("unknown BinaryChoice union value"));
        }
        String wire = "[0e10000]";
        JsonParseException ordinary = assertThrows(JsonParseException.class, () -> gson.fromJson(JsonParser.parseString(wire), byte[].class));
        JsonParseException selected = assertThrows(JsonParseException.class, () -> read(wire, BinaryChoice.class));
        assertEquals(ordinary.getMessage(), selected.getMessage());
        assertInstanceOf(BinaryChoice.VariantBinary.class, read("[-128,0,127]", BinaryChoice.class));
        roundTrip("[-128,0,127]", BinaryChoice.class);
        assertInstanceOf(BinaryChoice.VariantString.class, read("\"text\"", BinaryChoice.class));
    }

    @Test void scalarFiniteFactsDoNotWidenTheirKinds() {
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

    @Test void typedRootNullRetainsTheExistingKotlinApi() {
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

    @Test void ambiguityIsResolvedBeforeDelegation() {
        String wire = "{\"wire_name\":\"detail\",\"first_value\":\"one\"}";
        if (strict()) {
            JsonParseException error = assertThrows(JsonParseException.class, () -> read(wire, Ambiguous.class));
            assertTrue(error.getMessage().contains("ambiguous untagged union Ambiguous"));
        } else {
            assertInstanceOf(Ambiguous.VariantAlpha.class, read(wire, Ambiguous.class));
            roundTrip(wire, Ambiguous.class);
        }
    }

    @Test void transparentAliasesKeepTheirOwnConversionBoundary() {
        String wire = "7";
        AliasChoice.VariantNumberAlias value = assertInstanceOf(AliasChoice.VariantNumberAlias.class, read(wire, AliasChoice.class));
        assertEquals(7, value.getValue0());
        roundTrip(wire, AliasChoice.class);
    }

    @Test void nestedStrictErrorsDoNotRetryOpaqueFallback() {
        String wire = "{\"wire_name\":\"detail\"}";
        if (strict()) {
            JsonParseException error = assertThrows(JsonParseException.class, () -> read(wire, OpenNested.class));
            assertTrue(error.getMessage().contains("ambiguous untagged union Ambiguous"));
        } else {
            assertInstanceOf(OpenNested.VariantAmbiguousAlias.class, read(wire, OpenNested.class));
            roundTrip(wire, OpenNested.class);
        }
    }

    @Test void intersectionMembersKeepTheirWrappedWireRepresentation() {
        String wire = "{\"leftPart\":{\"left_value\":\"left\"},\"rightPart\":{\"right_value\":2}}";
        assertInstanceOf(WrappedChoice.VariantWrappedPair.class, read(wire, WrappedChoice.class));
        roundTrip(wire, WrappedChoice.class);
    }

    @Test void recursivePredicatesReuseSuccessfulAndFailedChecks() {
        String valid = "{\"revision\":2,\"children\":[],\"second_value\":\"leaf\"}";
        String invalid = "{\"revision\":3,\"children\":[]}";
        for (int i = 0; i < 24; i++) {
            valid = "{\"children\":[" + valid + "],\"revision\":2,\"second_value\":\"branch\"}";
            invalid = "{\"children\":[" + invalid + "],\"revision\":2}";
        }
        assertInstanceOf(Tree.VariantBranchTwo.class, read(valid, Tree.class));
        roundTrip(valid, Tree.class);
        String rejected = invalid;
        assertThrows(JsonParseException.class, () -> read(rejected, Tree.class));
    }

    @Test void sourceSchemaIdentityIsNotItsPascalCaseClassName() {
        assertInstanceOf(RecursiveChoice.VariantString.class, read("\"safe\"", RecursiveChoice.class));
        roundTrip("\"safe\"", RecursiveChoice.class);
    }

    @Test void nativeIntegerOverflowDoesNotBecomeASelectionValidator() {
        String wire = "{\"identifier\":2147483648,\"second_value\":\"two\"}";
        int ordinary = gson.fromJson(JsonParser.parseString(wire), Narrow.class).getIdentifier();
        if (strict()) {
            assertThrows(JsonParseException.class, () -> read(wire, FormatChoice.class));
        } else {
            FormatChoice.VariantNarrow selected = assertInstanceOf(FormatChoice.VariantNarrow.class, read(wire, FormatChoice.class));
            assertEquals(ordinary, selected.getValue0().getIdentifier());
        }
        FormatFallback.VariantNarrow selected = assertInstanceOf(FormatFallback.VariantNarrow.class, read(wire, FormatFallback.class));
        assertEquals(ordinary, selected.getValue0().getIdentifier());
    }

    @Test void containingFieldsListsAndMapsUseTheSameSelection() {
        String wire = "{\"caption\":\"envelope\",\"choice\":" + FINITE + ",\"choices\":[" + FINITE + "],\"by_name\":{\"keep_key\":" + FINITE + "}}";
        ChoiceEnvelope value = read(wire, ChoiceEnvelope.class);
        assertInstanceOf(FiniteChoice.VariantFiniteTwo.class, value.getChoice());
        assertInstanceOf(FiniteChoice.VariantFiniteTwo.class, value.getChoices().get(0));
        assertInstanceOf(FiniteChoice.VariantFiniteTwo.class, value.getByName().get("keep_key"));
        roundTrip(wire, ChoiceEnvelope.class);
    }

    @Test void successfulHttpResponsesUseTheTypedMember() {
        GetChoiceResponse response = UnionHttpTestSupport.api(200, FINITE).getChoice();
        assertEquals(200, response.getStatusCode());
        assertInstanceOf(FiniteChoice.VariantFiniteTwo.class, response.getStatus200());
        assertEquals(JsonParser.parseString(FINITE), gson.toJsonTree(response.getStatus200(), FiniteChoice.class));
    }

    @Test void typedHttpErrorsKeepTheirPayloadAndRawBytes() {
        GetChoiceException error = assertThrows(GetChoiceException.class, () -> UnionHttpTestSupport.api(400, FINITE).getChoice());
        GetChoiceBadRequest detail = assertInstanceOf(GetChoiceBadRequest.class, error.getDetail());
        assertInstanceOf(FiniteChoice.VariantFiniteTwo.class, detail.body());
        assertArrayEquals(FINITE.getBytes(StandardCharsets.UTF_8), error.rawBody());
        assertArrayEquals(error.rawBody(), detail.rawBody());
    }

    @Test void unmatchedHttpSuccessPropagatesTheSelectionError() {
        String wire = FINITE.replace("\"revision\":2", "\"revision\":3");
        assertThrows(JsonParseException.class, () -> UnionHttpTestSupport.api(200, wire).getChoice());
    }

    @Test void unmatchedHttpErrorsStayLazyWithoutLosingDiagnostics() {
        String wire = FINITE.replace("\"revision\":2", "\"revision\":3");
        GetChoiceException error = assertThrows(GetChoiceException.class, () -> UnionHttpTestSupport.api(400, wire).getChoice());
        GetChoiceBadRequest detail = assertInstanceOf(GetChoiceBadRequest.class, error.getDetail());
        assertThrows(JsonParseException.class, detail::body);
        assertThrows(JsonParseException.class, detail::body);
        assertArrayEquals(wire.getBytes(StandardCharsets.UTF_8), error.rawBody());
    }
}
