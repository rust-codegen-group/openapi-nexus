//! Petstore API models

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Pet model
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Pet {
    /// Pet ID
    pub id: Option<i64>,
    /// Pet name
    pub name: String,
    /// Pet category
    pub category: Option<Category>,
    /// Photo URLs
    pub photo_urls: Vec<String>,
    /// Pet tags
    pub tags: Option<Vec<Tag>>,
    /// Pet status in the store
    pub status: Option<PetStatus>,
}

/// Pet status enum
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum PetStatus {
    Available,
    Pending,
    Sold,
}

/// Category model
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Category {
    /// Category ID
    pub id: Option<i64>,
    /// Category name
    pub name: Option<String>,
}

/// Tag model
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Tag {
    /// Tag ID
    pub id: Option<i64>,
    /// Tag name
    pub name: Option<String>,
}

/// Order model
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Order {
    /// Order ID
    pub id: Option<i64>,
    /// Pet ID
    pub pet_id: Option<i64>,
    /// Quantity
    pub quantity: Option<i32>,
    /// Ship date
    pub ship_date: Option<DateTime<Utc>>,
    /// Order status
    pub status: Option<OrderStatus>,
    /// Complete flag
    pub complete: Option<bool>,
}

/// Order status enum
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum OrderStatus {
    Placed,
    Approved,
    Delivered,
}

/// User model
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct User {
    /// User ID
    pub id: Option<i64>,
    /// Username
    pub username: Option<String>,
    /// First name
    pub first_name: Option<String>,
    /// Last name
    pub last_name: Option<String>,
    /// Email
    pub email: Option<String>,
    /// Password
    pub password: Option<String>,
    /// Phone
    pub phone: Option<String>,
    /// User status
    pub user_status: Option<i32>,
}

/// Upload response model
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UploadResponse {
    /// Response code
    pub code: Option<i32>,
    /// Response type
    pub r#type: Option<String>,
    /// Response message
    pub message: Option<String>,
}

/// Error response model
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErrorResponse {
    /// Error code
    pub code: i32,
    /// Error message
    pub message: String,
}
