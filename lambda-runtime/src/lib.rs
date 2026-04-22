//! Lambda Runtime - A lightweight runtime for executing Lambda functions in containers
//!
//! This crate provides the runtime environment for Lambda functions, handling HTTP requests,
//! routing, timing, and error handling automatically.

pub mod server;
pub mod handler;
pub mod error;
pub mod trace;
pub mod headers;

// Re-export models for convenience
pub use lambda_models::{LambdaRequest, LambdaResponse};

// Re-export main types
pub use server::LambdaServer;
pub use handler::{LambdaHandler, HandlerFn};
pub use error::{RuntimeError, RuntimeResult};

use std::future::Future;

const LAMBDA_PORT_ENV: &str = "LAMBDA_PORT";

/// Main entry point for Lambda functions
///
/// This function starts the Lambda runtime server and handles all incoming requests.
/// The provided handler function will be called for each `/invoke` request from the gateay.
///
/// # Arguments
/// * `handler` - A function that takes a `LambdaRequest` and returns a `LambdaResponse`
///
/// # Example
/// ```no_run
/// use lambda_runtime::{run, LambdaRequest, LambdaResponse};
/// use http::StatusCode;
/// use std::time::Duration;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     lambda_runtime::run(|req| async move {
///         LambdaResponse {
///             status_code: StatusCode::OK,
///             headers: None,
///             body: format!("Hello from {}!", req.path),
///             request_id: req.request_id,
///             execution_time_ms: Duration::from_millis(0),
///         }
///     }).await?;
///    Ok(())
/// }
/// ```
pub async fn run<F, Fut>(handler: F) -> RuntimeResult<()>
where
    // F is a function that:
    F: Fn(LambdaRequest) -> Fut  // - Takes LambdaRequest, returns Fut
       + Send                    // - Can be moved between threads
       + Sync                    // - Can be shared between threads (called concurrently)
       + 'static,                // - Lives for the entire program duration
    // Fut is a future that:
    Fut: Future<Output = LambdaResponse>  // - Resolves to LambdaResponse
         + Send                           // - Can be moved between threads
         + 'static,                       // - Lives for the entire program duration
{
    // Get port from environment variable set by gateway
    let port = std::env::var(LAMBDA_PORT_ENV)
        .expect("LAMBDA_PORT environment variable must be set by the gateway")
        .parse::<u16>()
        .expect("LAMBDA_PORT must be a valid port number");

    run_with_port(port, handler).await
}

/// Convenience function for synchronous handlers
///
/// This function allows you to use synchronous handler functions with the async runtime.
/// It automatically wraps your sync function to make it compatible with the async system.
///
/// # Arguments
/// * `sync_handler` - A synchronous function that takes `LambdaRequest` and returns `LambdaResponse`
///
/// # Example
/// ```no_run
/// use lambda_runtime::{run, LambdaRequest, LambdaResponse};
/// use http::StatusCode;
/// use std::time::Duration;
///
/// fn my_handler(req: LambdaRequest) -> LambdaResponse {
///     LambdaResponse {
///         status_code: StatusCode::OK,
///         headers: None,
///         body: format!("Hello from {}!", req.path),
///         request_id: req.request_id,
///         execution_time_ms: Duration::from_millis(0),
///     }
/// }
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     lambda_runtime::run_sync(my_handler).await?;
///     Ok(())
/// }
/// ```
pub async fn run_sync<F>(sync_handler: F) -> RuntimeResult<()>
where
    F: Fn(LambdaRequest) -> LambdaResponse + Send + Sync + 'static,
{
    // Convert the synchronous handler into an async-compatible handler
    // by wrapping it in a closure that returns a ready future
    run(move |req| {
        // Call the synchronous handler function immediately
        // This executes synchronously and returns LambdaResponse
        let response = sync_handler(req);

        // Wrap the response in a future that's immediately ready
        // This satisfies the async function signature requirement
        // without actually doing any async work
        //
        // Note: This is not truly asynchronous, but it allows sync handlers
        // to be used in the async runtime.
        //
        // This is equivalent to using `async { response }` in our case becuase
        // the value is already computed and we just need to return it as a future.
        std::future::ready(response)
    }).await
}

// Convenience functions for testing with custom ports
pub async fn run_sync_with_port<F>(port: u16, sync_handler: F) -> RuntimeResult<()>
where
    F: Fn(LambdaRequest) -> LambdaResponse + Send + Sync + 'static,
{
    // Convert the synchronous handler into an async-compatible handler
    // by wrapping it in a closure that returns a ready future
    run_with_port(port, move |req| {
        // Call the synchronous handler function immediately
        // This executes synchronously and returns LambdaResponse
        let response = sync_handler(req);

        // Wrap the response in a future that's immediately ready
        // This satisfies the async function signature requirement
        // without actually doing any async work
        //
        // Note: This is not truly asynchronous, but it allows sync handlers
        // to be used in the async runtime.
        //
        // This is equivalent to using `async { response }` in our case becuase
        // the value is already computed and we just need to return it as a future.
        std::future::ready(response)
    }).await
}

// Convenience functions for testing with custom ports
pub async fn run_with_port<F, Fut>(port: u16, handler: F) -> RuntimeResult<()>
where
    F: Fn(LambdaRequest) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = LambdaResponse> + Send + 'static,
{
    // Initialize tracing
    let _ = crate::trace::init_tracing().expect("Failed to initialize tracing");

    tracing::info!("Starting Lambda runtime on port {}", port);

    // Create and start server
    let server = LambdaServer::new(port, handler);
    server.start().await
}