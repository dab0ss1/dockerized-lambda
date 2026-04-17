use lambda_models::{LambdaRequest, LambdaResponse};
use http::StatusCode;
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;
use chrono::Utc;

/// Create a basic test LambdaRequest
pub fn create_test_request() -> LambdaRequest {
    create_request_with_path_method("/test", http::Method::GET)
}

/// Create a test request with custom path and method
pub fn create_request_with_path_method(path: &str, method: http::Method) -> LambdaRequest {
    create_custom_request(path, method, "")
}

/// Create a test request with body
pub fn create_request_with_body(body: &str) -> LambdaRequest {
    create_custom_request("/test", http::Method::POST, body)
}

/// Create a test request with custom path and method
pub fn create_custom_request(path: &str, method: http::Method, body: &str) -> LambdaRequest {
    LambdaRequest {
        request_id: Uuid::new_v4(),
        method,
        path: path.to_string(),
        query_parameters: HashMap::new(),
        headers: HashMap::new(),
        body: body.to_string(),
        remote_addr: Some("127.0.0.1".parse().unwrap()),
        timestamp: Utc::now(),
    }
}

/// Simple echo handler for testing
pub fn echo_handler(req: LambdaRequest) -> LambdaResponse {
    LambdaResponse {
        status_code: StatusCode::OK,
        headers: Some([("content-type".to_string(), "application/json".to_string())].into()),
        body: serde_json::json!({
            "echo": {
                "method": req.method.to_string(),
                "path": req.path,
                "body": req.body,
                "headers": req.headers
            }
        }).to_string(),
        request_id: req.request_id,
        execution_time_ms: Duration::from_millis(0),
    }
}

/// Handler that returns different responses based on path
pub fn routing_handler(req: LambdaRequest) -> LambdaResponse {
    let (status, body) = match req.path.as_str() {
        "/hello" => (StatusCode::OK, serde_json::json!({"message": "Hello World!"})),
        "/error" => (StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({"error": "Something went wrong"})),
        "/slow" => {
            std::thread::sleep(Duration::from_millis(100));
            (StatusCode::OK, serde_json::json!({"message": "Slow response"}))
        }
        _ => (StatusCode::NOT_FOUND, serde_json::json!({"error": "Not Found"})),
    };

    LambdaResponse {
        status_code: status,
        headers: Some([("content-type".to_string(), "application/json".to_string())].into()),
        body: body.to_string(),
        request_id: req.request_id,
        execution_time_ms: Duration::from_millis(0),
    }
}

/// Handler that panics (for error testing)
pub fn panicking_handler(_req: LambdaRequest) -> LambdaResponse {
    panic!("Test panic");
}

/// Async handler for testing async functionality
pub async fn async_echo_handler(req: LambdaRequest) -> LambdaResponse {
    // Simulate async work
    tokio::time::sleep(Duration::from_millis(10)).await;

    LambdaResponse {
        status_code: StatusCode::OK,
        headers: None,
        body: serde_json::json!({
            "echo": {
                "method": req.method.to_string(),
                "path": req.path,
                "body": req.body,
                "headers": req.headers
            }
        }).to_string(),
        request_id: req.request_id,
        execution_time_ms: Duration::from_millis(0),
    }
}