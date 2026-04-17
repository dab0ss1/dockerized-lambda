use crate::fixtures::*;
use crate::setup::*;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn test_health_endpoint() {
    let config = TestConfig::default();
    let runtime = setup_runtime_sync(config, echo_handler).await;

    let response = runtime.health().await.unwrap();

    assert_eq!(response.status(), 200);
    assert_eq!(response.headers().get("content-type").unwrap(), "application/json");

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["status"], "healthy");

    runtime.shutdown().await;
}

#[tokio::test]
#[serial]
async fn test_health_check_multiple_times() {
    let config = TestConfig::default();
    let runtime = setup_runtime_sync(config, echo_handler).await;

    // Health check should work multiple times
    for _ in 0..3 {
        let response = runtime.health().await.unwrap();
        assert_eq!(response.status(), 200);
    }

    runtime.shutdown().await;
}