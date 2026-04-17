use std::time::Duration;

use crate::fixtures::*;
use crate::setup::*;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn test_invalid_json_request() {
    let config = TestConfig::default();
    let runtime = setup_runtime_sync(config, echo_handler).await;

    let response = runtime.client()
        .post(&format!("{}/invoke", runtime.base_url))
        .body("invalid json")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 500);
    let body: serde_json::Value = response.json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("Failed to parse LambdaRequest JSON"));

    runtime.shutdown().await;
}

#[tokio::test]
#[serial]
async fn test_not_found_endpoint() {
    let config = TestConfig::default();
    let runtime = setup_runtime_sync(config, echo_handler).await;

    // The gateway should only ever make requests to /invoke and /health, so any other path should return 404
    let response = runtime.client()
        .get(&format!("{}/unknown", runtime.base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["error"], "Not Found");

    runtime.shutdown().await;
}

#[tokio::test]
#[serial]
async fn test_wrong_method_for_invoke() {
    let config = TestConfig::default();
    let runtime = setup_runtime_sync(config, echo_handler).await;

    // The /invoke endpoint should only accept POST requests, so a GET request should return 404
    let response = runtime.client()
        .get(&format!("{}/invoke", runtime.base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 404);

    runtime.shutdown().await;
}

#[tokio::test]
#[serial]
async fn test_panicking_handler_crashes_runtime() {
    let config = TestConfig::default();

    // Start runtime with panicking handler
    let runtime_handle = tokio::spawn(async move {
        lambda_runtime::run_sync_with_port(config.port, panicking_handler).await
    });

    // Wait for runtime to start
    wait_for_runtime_ready(config.port, config.timeout).await
        .expect("Runtime failed to start");

    let client = reqwest::Client::new();
    let request = create_test_request();

    // Send request that will cause panic
    let response_result = client
        .post(&format!("http://127.0.0.1:{}/invoke", config.port))
        .json(&request)
        .send()
        .await;

    // The request might fail due to connection being dropped
    // or we might get a connection error
    match response_result {
        Ok(response) => {
            // If we get a response, it should be an error
            assert!(response.status().is_server_error());
        }
        Err(_) => {
            // Connection error is also acceptable - runtime crashed
        }
    }

    // Runtime task should have panicked and finished
    let runtime_result = tokio::time::timeout(Duration::from_secs(1), runtime_handle).await;
    assert!(runtime_result.is_ok()); // Task completed (crashed)

    // The result should be an error due to panic
    let task_result = runtime_result.unwrap();
    assert!(task_result.is_err()); // Task panicked
}