use std::fmt;

/// Result type for runtime operations
pub type RuntimeResult<T> = Result<T, RuntimeError>;

/// Errors that can occur in the Lambda runtime
#[derive(Debug)]
pub enum RuntimeError {
    /// Server failed to start
    ServerStart(String),
    /// Failed to parse incoming request
    RequestParsing(String),
    /// Failed to build response
    ResponseBuilding(String),
    /// User handler panicked or failed
    HandlerError(String),
}

impl std::error::Error for RuntimeError {}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeError::ServerStart(msg) => write!(f, "Server start error: {}", msg),
            RuntimeError::RequestParsing(msg) => write!(f, "Request parsing error: {}", msg),
            RuntimeError::ResponseBuilding(msg) => write!(f, "Response building error: {}", msg),
            RuntimeError::HandlerError(msg) => write!(f, "Handler error: {}", msg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_start_error_display() {
        let error = RuntimeError::ServerStart("Port 8080 already in use".to_string());
        assert_eq!(error.to_string(), "Server start error: Port 8080 already in use");
    }

    #[test]
    fn test_request_parsing_error_display() {
        let error = RuntimeError::RequestParsing("Invalid JSON format".to_string());
        assert_eq!(error.to_string(), "Request parsing error: Invalid JSON format");
    }

    #[test]
    fn test_response_building_error_display() {
        let error = RuntimeError::ResponseBuilding("Invalid status code".to_string());
        assert_eq!(error.to_string(), "Response building error: Invalid status code");
    }

    #[test]
    fn test_handler_error_display() {
        let error = RuntimeError::HandlerError("User function panicked".to_string());
        assert_eq!(error.to_string(), "Handler error: User function panicked");
    }
}