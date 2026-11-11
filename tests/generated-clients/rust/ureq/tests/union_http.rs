use rust_union_first_match::apis::{GetChoiceError, GetChoiceResponse, SelectionApi};
use rust_union_first_match::runtime::client::Client;

#[path = "../../support/one_response_server.rs"]
mod server;

#[path = "../../support/union_http.rs"]
mod cases;

async fn request(status: &str, body: &'static str) -> Result<GetChoiceResponse, GetChoiceError> {
    let url = server::spawn_one_response_server(status, body);
    let client = Client::new(&url);
    SelectionApi::new(&client).get_choice()
}
