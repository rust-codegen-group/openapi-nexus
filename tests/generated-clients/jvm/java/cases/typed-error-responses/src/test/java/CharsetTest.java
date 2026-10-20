import com.example.sdk.apis.CreateResourceServerError;
import com.example.sdk.apis.CreateResourceException;
import java.nio.charset.StandardCharsets;
import okhttp3.Headers;

public final class CharsetTest {
    @org.junit.jupiter.api.Test
    void preservesCharsetAndHeaders() {
        byte[] raw = "{\"message\":\"caf\u00e9\",\"retryable\":true}".getBytes(StandardCharsets.ISO_8859_1);
        Headers headers = Headers.of("Content-Type", "application/json; charset=iso-8859-1", "Retry-After", "120");
        CreateResourceServerError detail = new CreateResourceServerError(503, headers, raw);
        String message = detail.body().getMessage();
        if (!"caf\u00e9".equals(message)) {
            throw new AssertionError("expected charset-aware message, got: " + message);
        }
        CreateResourceException error = new CreateResourceException(503, "Service Unavailable", message, headers, raw, detail);
        if (!"120".equals(error.getRetryAfterHeader())) {
            throw new AssertionError("expected Retry-After header");
        }
    }
}
