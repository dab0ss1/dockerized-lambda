use crate::fixtures::*;
use crate::setup::*;
use chrono::Utc;
use http::header;
use lambda_models::LambdaRequest;
use lambda_runtime::headers::CustomHeader;
use serial_test::serial;
use uuid::Uuid;

#[tokio::test]
#[serial]
async fn test_complete_request_response_mapping() {
    let config = TestConfig::default();
    let runtime = setup_runtime(config, async_echo_handler).await;

    // Create request directly with all the values we want to test
    let request = LambdaRequest {
        request_id: Uuid::new_v4(),
        method: http::Method::POST,
        path: "/api/test".to_string(),
        query_parameters: [
            ("param1".to_string(), "value1".to_string()),
            ("limit".to_string(), "10".to_string()),
        ].into(),
        headers: [
            ("x-custom-header".to_string(), "test-value".to_string()),
            ("authorization".to_string(), "Bearer token123".to_string()),
        ].into(),
        body: r#"{"test": "data"}"#.to_string(),
        remote_addr: Some("127.0.0.1".parse().unwrap()),
        timestamp: Utc::now(),
    };

    // Store original values for comparison
    let original_request_id = request.request_id;
    let original_method = request.method.clone();
    let original_path = request.path.clone();
    let original_body = request.body.clone();
    let original_headers = request.headers.clone();

    let response = runtime.invoke(&request).await.unwrap();

    // Test HTTP response attributes
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers().get(header::CONTENT_TYPE.as_str()).unwrap(), "application/json");

    println!("Response Headers: {:?}", response.headers());

    // Test metadata headers
    assert_eq!(
        response.headers().get(CustomHeader::LambdaRequestId.as_ref()).unwrap(),
        original_request_id.to_string().as_str()
    );
    let execution_time_header = response.headers().get(CustomHeader::LambdaExecutionTimeMs.as_ref()).unwrap();
    let execution_time_ms: u64 = execution_time_header.to_str().unwrap().parse().unwrap();
    assert!(execution_time_ms > 10); // Should be greater than 10ms for async handler

    // Parse the response body (which contains the echo JSON)
    let echo_body: serde_json::Value = response.json().await.unwrap();

    // Test echoed request parameters
    assert_eq!(echo_body["echo"]["method"], original_method.to_string());
    assert_eq!(echo_body["echo"]["path"], original_path);
    assert_eq!(echo_body["echo"]["body"], original_body);

    // Validate that all original headers are preserved in the echo
    assert_eq!(echo_body["echo"]["headers"].as_object().unwrap().len(), 2);
    for (key, value) in &original_headers {
        assert_eq!(echo_body["echo"]["headers"][key], value.as_str());
    }

    runtime.shutdown().await;
}