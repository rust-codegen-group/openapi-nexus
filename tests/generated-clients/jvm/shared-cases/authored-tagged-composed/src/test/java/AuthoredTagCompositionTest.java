import com.example.sdk.models.AmbiguousTagged;
import com.example.sdk.models.ComposedTagged;
import com.example.sdk.models.InlineTagged;
import com.example.sdk.models.NestedOuter;
import com.example.sdk.models.NestedTagged;
import com.example.sdk.models.NullableTagged;
import com.google.gson.Gson;
import com.google.gson.JsonParseException;
import com.google.gson.JsonParser;
import com.google.gson.JsonPrimitive;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

public final class AuthoredTagCompositionTest {
    private final Gson gson = new Gson();

    @Test
    void firstMatchUsesAuthoredMemberOrder() {
        assertInstanceOf(AmbiguousTagged.VariantAlpha.class,
            roundTrip(AmbiguousTagged.class, "{\"label\":\"both\"}"));
        assertInstanceOf(AmbiguousTagged.VariantBeta.class,
            roundTrip(AmbiguousTagged.class, "{\"kind\":\"beta_alt\",\"label\":\"both\"}"));
    }

    @Test
    void routesOneOrMultipleTagReadersAndWritesOnlyOneTag() {
        assertInstanceOf(ComposedTagged.VariantAlpha.class,
            roundTrip(ComposedTagged.class, "{\"kind\":\"alpha\",\"label\":\"x\",\"count\":3}"));
        var input = "{\"kind\":\"alpha_alt\",\"label\":\"x\",\"count\":3}";
        assertInstanceOf(ComposedTagged.VariantAlphaAlt.class, roundTrip(ComposedTagged.class, input));
        assertInstanceOf(NestedTagged.VariantComposedTagged.class, roundTrip(NestedTagged.class, input));
        assertInstanceOf(NestedOuter.VariantAlpha.class,
            roundTrip(NestedOuter.class, "{\"kind\":\"alpha_alt\",\"label\":\"x\"}"));

        var conflicting = new ComposedTagged.VariantAlphaAlt(
            JsonParser.parseString(input).getAsJsonObject(), new JsonPrimitive("alpha"));
        assertTrue(assertThrows(JsonParseException.class,
            () -> gson.toJson(conflicting, ComposedTagged.class)).getMessage().contains("conflicting"));
    }

    @Test
    void validatesFullAuthoredObjectBeforeProjectionAndRetainsNull() {
        assertInstanceOf(ComposedTagged.VariantBeta.class,
            roundTrip(ComposedTagged.class, "{\"kind\":null,\"label\":\"x\",\"extension\":7}"));
        assertInstanceOf(NullableTagged.VariantAlpha.class,
            roundTrip(NullableTagged.class, "{\"kind\":\"alpha_alt\",\"label\":\"x\",\"nullable_count\":null}"));
        assertThrows(JsonParseException.class,
            () -> gson.fromJson("{\"kind\":null,\"label\":\"x\",\"extension\":\"bad\"}", ComposedTagged.class));
        assertThrows(JsonParseException.class,
            () -> gson.fromJson("{\"kind\":\"alpha\",\"count\":3}", ComposedTagged.class));
    }

    @Test
    void promotedInlineUnionKeepsItsCommonGate() {
        assertInstanceOf(InlineTagged.VariantInlineTaggedMember1.class,
            roundTrip(InlineTagged.class, "{\"scope\":true,\"label\":\"alpha\"}"));
        assertThrows(JsonParseException.class,
            () -> gson.fromJson("{\"label\":\"alpha\"}", InlineTagged.class));
    }

    private <T> T roundTrip(Class<T> type, String json) {
        var parsed = gson.fromJson(json, type);
        assertEquals(JsonParser.parseString(json), JsonParser.parseString(gson.toJson(parsed, type)));
        return parsed;
    }
}
