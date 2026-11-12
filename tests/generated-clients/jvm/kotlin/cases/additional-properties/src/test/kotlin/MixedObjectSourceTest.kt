import com.example.sdk.models.MixedMap
import com.example.sdk.models.NestedValue
import com.example.sdk.models.OptionalKnown
import com.example.sdk.models.StorageCollision
import com.google.gson.Gson
import org.junit.jupiter.api.Assertions.*
import org.junit.jupiter.api.Test

class MixedObjectSourceTest {
    @Test
    fun namedArgumentsDefaultCopyAndDestructuringRetainExtensions() {
        val original = MixedMap(knownField = "fixed")
        original.additionalProperties["extra"] = NestedValue(wireValue = "nested")
        val duplicate = original.copy()
        val changed = original.copy(knownField = "changed")
        val (known, extras) = duplicate
        assertEquals("fixed", known)
        assertSame(original.additionalProperties, extras)
        assertEquals(original, duplicate)
        assertEquals("changed", changed.knownField)
        assertSame(extras, changed.additionalProperties)
        assertEquals(Gson().toJsonTree(original), Gson().toJsonTree(duplicate))
    }

    @Test
    fun defaultAndAuthoredStorageArgumentsKeepTheirSourceNames() {
        assertTrue(OptionalKnown().additionalProperties.isEmpty())
        assertTrue(OptionalKnown(knownField = null).additionalProperties.isEmpty())
        val collision = StorageCollision(additionalProperties = "authored")
        collision.additionalProperties2["extra"] = NestedValue(wireValue = "nested")
        assertEquals("authored", collision.copy().additionalProperties)
        assertSame(collision.additionalProperties2, collision.copy().additionalProperties2)
        val supplied = MixedMap(
            knownField = "fixed",
            additionalProperties = linkedMapOf("extra" to NestedValue(wireValue = "nested")),
        )
        assertEquals("nested", supplied.additionalProperties["extra"]?.wireValue)
    }
}
