use nested_nullable_api::apis::{GetValuesError, GetValuesResponse, ValuesApi};
use nested_nullable_api::runtime::client::Client;

#[path = "../../support/one_response_server.rs"]
mod server;

#[path = "../../support/nested_nullable.rs"]
mod cases;

async fn request(status: &str, body: &'static str) -> Result<GetValuesResponse, GetValuesError> {
    let url = server::spawn_one_response_server(status, body);
    let client = Client::new(&url);
    ValuesApi::new(&client).get_values()
}
