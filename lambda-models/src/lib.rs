use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DurationMilliSeconds};
use std::collections::HashMap;
use std::net::IpAddr;
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LambdaRequest {
    /// Unique identifier for tracking this specific request through the system
    pub request_id: Uuid,
    /// HTTP verb (GET, POST, PUT, DELETE, etc.) indicating the type of operation
    #[serde(with = "http_serde::method")]
    pub method: http::Method,
    /// URL path portion (e.g., "/api/users/123") without query parameters
    pub path: String,
    /// URL query string parameters as key-value pairs (e.g., "?limit=10&offset=20")
    pub query_parameters: HashMap<String, String>,
    /// HTTP headers as key-value pairs (e.g., "Content-Type", "Authorization")
    pub headers: HashMap<String, String>,
    /// Request body content as a string (JSON, form data, plain text, etc.)
    pub body: String,
    /// Client's IP address if available, None if unknown or behind proxy
    pub remote_addr: Option<IpAddr>,
    /// When this request was received/created in UTC timezone
    pub timestamp: DateTime<Utc>,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LambdaResponse {
    /// HTTP status code (200, 404, 500, etc.) indicating request outcome
    #[serde(with = "http_serde::status_code")]
    pub status_code: http::StatusCode,
    /// Optional response headers as key-value pairs (e.g., "Content-Type", "Cache-Control")
    pub headers: Option<HashMap<String, String>>,
    /// Response body content as a string (JSON, HTML, plain text, etc.)
    pub body: String,
    /// Matching request ID for correlating this response with the original request
    pub request_id: Uuid,
    /// Duration tracking how long the request took to process
    #[serde_as(as = "DurationMilliSeconds<u64>")]
    pub execution_time_ms: std::time::Duration,
}