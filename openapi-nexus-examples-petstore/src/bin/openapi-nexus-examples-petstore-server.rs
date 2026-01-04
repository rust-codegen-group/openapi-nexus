//! Petstore API example using Axum and utoipa

use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::Method,
    routing::{delete, get, post, put},
};
use tower::ServiceBuilder;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use utoipa::OpenApi;

use openapi_nexus_examples_petstore::{
    ApiDoc, add_pet, create_user, create_users_with_list_input, delete_order, delete_pet,
    delete_user, find_pets_by_status, find_pets_by_tags, get_inventory, get_order_by_id,
    get_pet_by_id, get_user_by_name, login_user, logout_user, place_order, update_pet,
    update_pet_with_form, update_user, upload_file,
};

/// Create the OpenAPI specification
#[utoipa::path(
    get,
    path = "/openapi.json",
    responses(
        (status = 200, description = "OpenAPI specification", content_type = "application/json")
    )
)]
async fn openapi_spec() -> String {
    ApiDoc::openapi().to_json().unwrap()
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "openapi_nexus_petstore_example=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Create CORS layer
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers(Any)
        .allow_origin(Any);

    // Create the router
    let app = Router::new()
        // Pet endpoints
        .route("/pet", put(update_pet))
        .route("/pet", post(add_pet))
        .route("/pet/findByStatus", get(find_pets_by_status))
        .route("/pet/findByTags", get(find_pets_by_tags))
        .route("/pet/:petId", get(get_pet_by_id))
        .route("/pet/:petId", post(update_pet_with_form))
        .route("/pet/:petId", delete(delete_pet))
        .route("/pet/:petId/uploadImage", post(upload_file))
        // Store endpoints
        .route("/store/inventory", get(get_inventory))
        .route("/store/order", post(place_order))
        .route("/store/order/:orderId", get(get_order_by_id))
        .route("/store/order/:orderId", delete(delete_order))
        // User endpoints
        .route("/user", post(create_user))
        .route("/user/createWithList", post(create_users_with_list_input))
        .route("/user/login", get(login_user))
        .route("/user/logout", get(logout_user))
        .route("/user/:username", get(get_user_by_name))
        .route("/user/:username", put(update_user))
        .route("/user/:username", delete(delete_user))
        // OpenAPI spec endpoint
        .route("/openapi.json", get(openapi_spec))
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(cors)
                .layer(DefaultBodyLimit::max(1024 * 1024 * 10)), // 10MB limit
        );

    // Start the server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("🚀 Petstore API server running on http://localhost:3000");
    println!("📚 Swagger UI available at http://localhost:3000/swagger-ui");
    println!("📋 OpenAPI spec available at http://localhost:3000/openapi.json");

    axum::serve(listener, app).await.unwrap();
}
