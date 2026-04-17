use crate::fixtures::*;
use crate::setup::*;
use serial_test::serial;
use std::time::Instant;

#[tokio::test]
#[serial]
async fn test_response_timing() {
    let config = TestConfig::default();
    let runtime = setup_runtime_sync(config, routing_handler).await;

    let request = create_request_with_path_method("/slow", http::Method::GET);

    let start = Instant::now();
    let response = runtime.invoke(&request).await.unwrap();
    let elapsed = start.elapsed();

    assert_eq!(response.status(), 200);
    assert!(elapsed >= std::time::Duration::from_millis(90)); // Should take at least 100ms
    assert!(elapsed <= std::time::Duration::from_millis(120)); // Should take at most 120ms

    runtime.shutdown().await;
}