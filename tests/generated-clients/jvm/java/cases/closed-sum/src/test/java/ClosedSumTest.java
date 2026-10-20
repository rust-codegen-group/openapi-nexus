import com.example.sdk.models.Choice;
import com.google.gson.Gson;
import static org.junit.jupiter.api.Assertions.*;

public final class ClosedSumTest {
    @org.junit.jupiter.api.Test
    void preservesArraysPrimitivesAndNull() {
        Gson gson = new Gson();
        Choice.VariantArray array = assertInstanceOf(Choice.VariantArray.class, gson.fromJson("[1,2]", Choice.class));
        assertEquals(java.util.List.of(1, 2), array.value0());
        assertEquals("\"ok\"", gson.toJson(new Choice.VariantString("ok"), Choice.class));
        assertInstanceOf(Choice.VariantNull.class, gson.fromJson("null", Choice.class));
        assertEquals("null", gson.toJson(Choice.VariantNull.INSTANCE, Choice.class));
    }
}
