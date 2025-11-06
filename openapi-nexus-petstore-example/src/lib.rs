//! Petstore API example using Axum and utoipa

pub mod handlers;
pub mod models;
pub mod openapi;

// Re-export commonly used items
pub use handlers::{
    add_pet, create_user, create_users_with_list_input, delete_order, delete_pet, delete_user,
    find_pets_by_status, find_pets_by_tags, get_inventory, get_order_by_id, get_pet_by_id,
    get_user_by_name, login_user, logout_user, place_order, update_pet, update_pet_with_form,
    update_user, upload_file,
};
pub use models::{
    Category, ErrorResponse, Order, OrderStatus, Pet, PetStatus, Tag, UploadResponse, User,
};
pub use openapi::ApiDoc;
