import com.example.sdk.models.MixedAlias
import com.example.sdk.models.MixedMap
import com.example.sdk.models.OpenAllOf
import com.example.sdk.models.OpaqueMap
import com.google.gson.JsonNull
import com.google.gson.JsonParser
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertSame
import org.junit.jupiter.api.Test

class AllOfCompositionSourceTest {
    @Test
    fun dataClassConstructorsCopyAndDestructuringRetainComposedMembers() {
        val member = MixedMap(knownField = "fixed")
        val original = MixedAlias(mixedMap = member)
        val duplicate = original.copy()
        val (copiedMember) = duplicate

        assertSame(member, original.mixedMap)
        assertSame(member, copiedMember)
        assertEquals(original, duplicate)
    }

    @Test
    fun openCompositionKeepsMemberPropertiesAndOpaqueEntries() {
        val extras = linkedMapOf(
            "nil" to JsonNull.INSTANCE,
            "big" to JsonParser.parseString("9007199254740993"),
        )
        val member = OpaqueMap(knownField = "fixed")
        val value = OpenAllOf(opaqueMap = member, mapStringAny = extras)
        val (copiedMember, copiedExtras) = value

        assertSame(member, copiedMember)
        assertSame(extras, copiedExtras)
        assertEquals("fixed", value.opaqueMap.knownField)
    }
}
