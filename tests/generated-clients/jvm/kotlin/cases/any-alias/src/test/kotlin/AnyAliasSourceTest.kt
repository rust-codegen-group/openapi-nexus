import com.example.sdk.models.AliasChain
import com.example.sdk.models.AliasListMap
import com.example.sdk.models.AliasMap
import com.example.sdk.models.AnyList
import com.example.sdk.models.Anything
import com.example.sdk.models.MixedAliasMap
import com.example.sdk.models.NestedAliases
import com.example.sdk.models.Ordinary
import com.example.sdk.models.TypedValue
import com.google.gson.Gson
import java.math.BigInteger
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertSame
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

class AnyAliasSourceTest {
    @Test
    fun constructorsCopyAndDestructuringKeepAnyAndListApis() {
        val opaque: Anything = linkedMapOf(
            "nested_null" to null,
            "number" to BigInteger("9007199254740993"),
        )
        val entries: AnyList = listOf(opaque)
        val chained: AliasChain = opaque
        val original = Ordinary(value = opaque, values = entries)
        val duplicate = original.copy()
        val (copiedValue: Any, copiedValues: List<Any>) = duplicate
        val changed = original.copy(values = emptyList())

        assertSame(opaque, copiedValue)
        assertSame(entries, copiedValues)
        assertEquals(original, duplicate)
        assertTrue(changed.values.isEmpty())
        assertSame(opaque, changed.value)

        val nested = NestedAliases(
            chain = chained,
            lists = listOf(entries),
            maps = linkedMapOf("entry" to listOf(chained)),
        )
        assertSame(opaque, nested.chain)
        assertSame(entries, nested.lists.single())
        assertSame(opaque, nested.maps.getValue("entry").single())
    }

    @Test
    fun aliasDictionaryStorageAndTypedSiblingDefaultsRemainSourceCompatible() {
        val opaque: Any = linkedMapOf("number" to BigInteger("9007199254740993"))
        val entries: List<Any> = listOf(opaque)
        val values = AliasMap()
        val lists = AliasListMap()
        val mixed = MixedAliasMap(knownField = "fixed")
        values.additionalProperties["extra"] = opaque
        lists.additionalProperties["extra"] = entries
        mixed.additionalProperties["extra"] = opaque

        assertSame(opaque, values.additionalProperties.getValue("extra"))
        assertSame(entries, lists.additionalProperties.getValue("extra"))
        assertSame(opaque, mixed.additionalProperties.getValue("extra"))
        assertEquals("fixed", mixed.knownField)

        val typed = TypedValue(wireValue = "typed")
        assertNull(typed.optionalNote)
        assertNull(typed.nullableNote)
        assertEquals("{\"wire_value\":\"typed\"}", Gson().toJson(typed))
    }
}
