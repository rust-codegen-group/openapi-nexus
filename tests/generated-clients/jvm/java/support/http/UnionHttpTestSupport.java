import com.example.sdk.apis.SelectionApi;
import com.example.sdk.runtime.ApiClient;
import java.util.Map;
import okhttp3.*;
import static org.junit.jupiter.api.Assertions.*;

final class UnionHttpTestSupport {
    static SelectionApi api(int status, String wire) {
        OkHttpClient transport = new OkHttpClient.Builder().addInterceptor(chain -> {
            assertEquals("/choice", chain.request().url().encodedPath());
            return new Response.Builder().request(chain.request()).protocol(Protocol.HTTP_1_1)
                .code(status).message(status == 200 ? "OK" : "Bad Request")
                .header("Content-Type", "application/json")
                .body(ResponseBody.create(wire, MediaType.get("application/json"))).build();
        }).build();
        return new SelectionApi(new ApiClient("https://union.test", transport, null, Map.of()));
    }
}
