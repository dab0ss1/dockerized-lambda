use lambda_models::{LambdaRequest, LambdaResponse};
use crate::utils::{should_panic, should_error, get_random_delay};
use http::StatusCode;
use serde_json::json;
use std::time::Duration;
use tokio::time::sleep;
use crate::constants::LAMBDA_PORT;

/// Create an echo handler with port information
pub fn create_echo_handler() -> impl Fn(LambdaRequest) -> std::pin::Pin<Box<dyn std::future::Future<Output = LambdaResponse> + Send>> + Send + Sync + 'static {
    move |req| {
        Box::pin(echo_handler_impl(req))
    }
}

/// Async echo handler implementation with simulated behaviors
async fn echo_handler_impl(req: LambdaRequest) -> LambdaResponse {
    let port = LAMBDA_PORT.to_owned();
    let request_id = req.request_id;

    tracing::info!(
        request_id = %request_id,
        method = %req.method,
        path = %req.path,
        port = port,
        "Processing request"
    );

    // simulating a panic
    if should_panic() {
        tracing::error!(request_id = %request_id, port = port, "Handler is about to panic!");
        panic!("Simulated panic in hello-lambda handler");
    }

    // simulating an error response
    if should_error() {
        tracing::warn!(request_id = %request_id, port = port, "Returning simulated server error");
        return create_error_response(request_id, port);
    }

    // simulating variable processing time
    let delay = get_random_delay();
    tracing::info!(
        request_id = %request_id,
        port = port,
        delay_ms = delay.as_millis(),
        "Simulating work with delay"
    );

    // Simulate work being done
    sleep(delay).await;

    tracing::info!(request_id = %request_id, port = port, "Request processed successfully");
    create_success_response(req, delay, port)
}

/// Create a successful echo response with metadata
fn create_success_response(req: LambdaRequest, processing_delay: Duration, port: u16) -> LambdaResponse {
    let response_body = json!({
        "status": "success",
        "echo": {
            "request_id": req.request_id,
            "method": req.method.to_string(),
            "path": req.path,
            "query_parameters": req.query_parameters,
            "headers": req.headers,
            "body": req.body,
            "remote_addr": req.remote_addr,
            "timestamp": req.timestamp,
        },
        "metadata": {
            "processing_delay_ms": processing_delay.as_millis(),
            "handler_version": "1.0.0",
            "simulated_work": true
        },
        "server_info": {
            "function_name": "hello-lambda",
            "runtime": "custom-rust-runtime",
            "port": port,
            "response_time": chrono::Utc::now()
        }
    });

    LambdaResponse {
        status_code: StatusCode::OK,
        headers: Some([
            ("content-type".to_string(), "application/json".to_string()),
            ("x-function-name".to_string(), "hello-lambda".to_string()),
            ("x-lambda-port".to_string(), port.to_string()),
            ("x-processing-delay-ms".to_string(), processing_delay.as_millis().to_string()),
            ("x-handler-version".to_string(), "1.0.0".to_string()),
        ].into()),
        body: response_body.to_string(),
        request_id: req.request_id,
        execution_time_ms: Duration::from_millis(0), // Runtime will override this
    }
}

/// Create an error response for simulated server errors
fn create_error_response(request_id: uuid::Uuid, port: u16) -> LambdaResponse {
    let error_body = json!({
        "status": "error",
        "error": {
            "code": "INTERNAL_SERVER_ERROR",
            "message": "Simulated server error occurred",
            "details": "This is a randomly generated error for testing purposes",
            "request_id": request_id,
            "timestamp": chrono::Utc::now()
        },
        "metadata": {
            "error_simulation": true,
            "handler_version": "1.0.0"
        },
        "server_info": {
            "function_name": "hello-lambda",
            "runtime": "custom-rust-runtime",
            "port": port
        }
    });

    LambdaResponse {
        status_code: StatusCode::INTERNAL_SERVER_ERROR,
        headers: Some([
            ("content-type".to_string(), "application/json".to_string()),
            ("x-function-name".to_string(), "hello-lambda".to_string()),
            ("x-lambda-port".to_string(), port.to_string()),
            ("x-error-type".to_string(), "simulated".to_string()),
            ("x-handler-version".to_string(), "1.0.0".to_string()),
        ].into()),
        body: error_body.to_string(),
        request_id,
        execution_time_ms: Duration::from_millis(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lambda_models::LambdaRequest;
    use std::collections::HashMap;

    fn create_test_request() -> LambdaRequest {
        LambdaRequest {
            request_id: uuid::Uuid::new_v4(),
            method: http::Method::GET,
            path: "/test".to_string(),
            query_parameters: HashMap::new(),
            headers: [("user-agent".to_string(), "test-client".to_string())].into(),
            body: "test body".to_string(),
            remote_addr: Some("127.0.0.1".parse().unwrap()),
            timestamp: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_create_success_response() {
        let req = create_test_request();
        let delay = Duration::from_millis(1500);
        let port = 8081;
        let response = create_success_response(req.clone(), delay, port);

        assert_eq!(response.status_code, StatusCode::OK);
        assert_eq!(response.request_id, req.request_id);

        let body: serde_json::Value = serde_json::from_str(&response.body).unwrap();
        assert_eq!(body["status"], "success");
        assert_eq!(body["echo"]["method"], "GET");
        assert_eq!(body["echo"]["path"], "/test");
        assert_eq!(body["metadata"]["processing_delay_ms"], 1500);
        assert_eq!(body["server_info"]["port"], 8081);

        // Test headers
        assert_eq!(response.headers.as_ref().unwrap().get("x-lambda-port").unwrap(), "8081");
    }

    #[tokio::test]
    async fn test_create_error_response() {
        let request_id = uuid::Uuid::new_v4();
        let port = 8082;
        let response = create_error_response(request_id, port);

        assert_eq!(response.status_code, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(response.request_id, request_id);

        let body: serde_json::Value = serde_json::from_str(&response.body).unwrap();
        assert_eq!(body["status"], "error");
        assert_eq!(body["error"]["code"], "INTERNAL_SERVER_ERROR");
        assert_eq!(body["server_info"]["port"], 8082);

        // Test headers
        assert_eq!(response.headers.as_ref().unwrap().get("x-lambda-port").unwrap(), "8082");
    }
}