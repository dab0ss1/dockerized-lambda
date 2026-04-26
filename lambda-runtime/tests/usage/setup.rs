use lambda_runtime::{LambdaRequest, LambdaResponse};
use std::time::Duration;
use tokio::time::timeout;

/// Test configuration for runtime instances
pub struct TestConfig {
    pub port: u16,
    pub timeout: Duration,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            port: get_free_port(),
            timeout: Duration::from_secs(5),
        }
    }
}

/// Get a free port for testing
/// Returns a free port number by binding to port 0 and letting the OS assign an available port
pub fn get_free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Setup runtime with a test handler
pub async fn setup_runtime<F, Fut>(
    config: TestConfig,
    handler: F,
) -> RuntimeHandle
where
    F: Fn(LambdaRequest) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = LambdaResponse> + Send + 'static,
{
    // Start runtime in background
    let runtime_handle = tokio::spawn(async move {
        lambda_runtime::run_with_port(config.port, handler).await
    });

    // Wait for runtime to start
    wait_for_runtime_ready(config.port, config.timeout).await
        .expect("Runtime failed to start");

    RuntimeHandle {
        handle: runtime_handle,
        base_url: format!("http://127.0.0.1:{}", config.port),
    }
}

/// Setup runtime with sync handler
pub async fn setup_runtime_sync<F>(
    config: TestConfig,
    handler: F,
) -> RuntimeHandle
where
    F: Fn(LambdaRequest) -> LambdaResponse + Send + Sync + 'static,
{
    let runtime_handle = tokio::spawn(async move {
        lambda_runtime::run_sync_with_port(config.port, handler).await
    });

    wait_for_runtime_ready(config.port, config.timeout).await
        .expect("Runtime failed to start");

    RuntimeHandle {
        handle: runtime_handle,
        base_url: format!("http://127.0.0.1:{}", config.port),
    }
}

/// Wait for runtime to be ready
pub async fn wait_for_runtime_ready(port: u16, timeout_duration: Duration) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/health", port);

    timeout(timeout_duration, async {
        loop {
            match client.get(&url).send().await {
                Ok(response) if response.status().is_success() => break,
                _ => tokio::time::sleep(Duration::from_millis(50)).await,
            }
        }
    }).await?;

    Ok(())
}

/// Handle for managing test runtime
pub struct RuntimeHandle {
    pub handle: tokio::task::JoinHandle<Result<(), lambda_runtime::RuntimeError>>,
    pub base_url: String,
}

impl RuntimeHandle {
    /// Get HTTP client for making requests
    pub fn client(&self) -> reqwest::Client {
        reqwest::Client::new()
    }

    /// Make a request to the invoke endpoint
    pub async fn invoke(&self, request: &LambdaRequest) -> Result<reqwest::Response, reqwest::Error> {
        self.client()
            .post(&format!("{}/invoke", self.base_url))
            .json(request)
            .send()
            .await
    }

    /// Make a health check request
    pub async fn health(&self) -> Result<reqwest::Response, reqwest::Error> {
        self.client()
            .get(&format!("{}/health", self.base_url))
            .send()
            .await
    }

    /// Shutdown the runtime
    pub async fn shutdown(self) {
        self.handle.abort();
        let _ = self.handle.await;
    }
}