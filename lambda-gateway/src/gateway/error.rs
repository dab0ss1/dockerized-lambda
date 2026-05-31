use crate::pool::PoolError;
use crate::docker::DockerError;

#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("Pool error: {0}")]
    Pool(#[from] PoolError),

    #[error("Docker error: {0}")]
    Docker(#[from] DockerError),

    #[error("Request conversion error: {0}")]
    Convert(#[from] ConvertError),

    #[error("HTTP client error: {0}")]
    HttpClient(#[from] hyper_util::client::legacy::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] hyper::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("UTF-8 conversion error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),

    #[error("Invalid header value: {0}")]
    InvalidHeader(#[from] hyper::header::InvalidHeaderValue),

    #[error("No available hosts")]
    NoAvailableHosts,

    #[error("Request timeout")]
    Timeout,

    #[error("Internal server error: {0}")]
    Internal(String),

    #[error("Gateway is shutting down")]
    ShuttingDown,
}

#[derive(Debug, thiserror::Error)]
pub enum ConvertError {
    #[error("Failed to read request body: {0}")]
    BodyRead(String),

    #[error("Invalid UTF-8 in request body")]
    InvalidUtf8,

    #[error("Invalid header value: {0}")]
    InvalidHeader(String),

    #[error("Missing required header: {0}")]
    MissingHeader(String),

    #[error("Invalid URI: {0}")]
    InvalidUri(String),
}

// Convert GatewayError to HTTP response
impl GatewayError {
    pub fn status_code(&self) -> hyper::StatusCode {
        use hyper::StatusCode;

        match self {
            GatewayError::Pool(_) => StatusCode::SERVICE_UNAVAILABLE,
            GatewayError::Docker(_) => StatusCode::INTERNAL_SERVER_ERROR,
            GatewayError::Convert(_) => StatusCode::BAD_REQUEST,
            GatewayError::HttpClient(_) => StatusCode::BAD_GATEWAY,
            GatewayError::Http(_) => StatusCode::INTERNAL_SERVER_ERROR,
            GatewayError::Json(_) => StatusCode::BAD_REQUEST,
            GatewayError::Utf8(_) => StatusCode::BAD_REQUEST,
            GatewayError::InvalidHeader(_) => StatusCode::BAD_REQUEST,
            GatewayError::NoAvailableHosts => StatusCode::SERVICE_UNAVAILABLE,
            GatewayError::Timeout => StatusCode::GATEWAY_TIMEOUT,
            GatewayError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            GatewayError::ShuttingDown => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    pub fn error_message(&self) -> String {
        match self {
            GatewayError::Pool(_) => "Service temporarily unavailable".to_string(),
            GatewayError::Docker(_) => "Internal server error".to_string(),
            GatewayError::Convert(e) => format!("Invalid request: {}", e),
            GatewayError::NoAvailableHosts => "No available Lambda instances".to_string(),
            GatewayError::Timeout => "Request timeout".to_string(),
            GatewayError::ShuttingDown => "Gateway is shutting down".to_string(),
            _ => "Internal server error".to_string(),
        }
    }
}