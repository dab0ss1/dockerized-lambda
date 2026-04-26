use crate::fixtures::*;
use crate::setup::*;
use serial_test::serial;
use lambda_models::LambdaRequest;


#[tokio::test]
#[serial]
async fn test_sync_handler() {
    let config: TestConfig = TestConfig::default();
    let runtime: RuntimeHandle = setup_runtime_sync(config, echo_handler).await;

    let request: LambdaRequest = create_test_request();
    let response = runtime.invoke(&request).await.unwrap();

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["echo"]["method"], "GET");
    assert_eq!(body["echo"]["path"], "/test");
    assert_eq!(body["echo"]["body"], "");
    assert_eq!(body["echo"]["headers"], serde_json::json!({}));

    runtime.shutdown().await;
}

#[tokio::test]
#[serial]
async fn test_async_handler() {
    let config = TestConfig::default();
    let runtime = setup_runtime(config, async_echo_handler).await;

    let request = create_request_with_body("async test");
    let response = runtime.invoke(&request).await.unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["echo"]["method"], "POST");
    assert_eq!(body["echo"]["path"], "/test");
    assert_eq!(body["echo"]["body"], "async test");
    assert_eq!(body["echo"]["headers"], serde_json::json!({}));

    runtime.shutdown().await;
}

#[tokio::test]
#[serial]
async fn test_multiple_requests() {
    let config = TestConfig::default();
    let runtime = setup_runtime_sync(config, routing_handler).await;

    // Test multiple different endpoints
    let requests = vec![
        create_request_with_path_method("/hello", http::Method::GET),
        create_request_with_path_method("/error", http::Method::POST),
        create_request_with_path_method("/unknown", http::Method::GET),
    ];

    for request in requests {
        let response = runtime.invoke(&request).await.unwrap();
        assert!(response.status().as_u16() > 0); // Got some response
    }

    runtime.shutdown().await;
}