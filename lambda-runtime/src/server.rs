use crate::{RuntimeError, RuntimeResult, LambdaHandler};
use http::{StatusCode, header};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use http_body_util::Full;
use hyper::body::Bytes;
use std::future::Future;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::net::TcpListener;

// Use 0.0.0.0 (all interfaces) instead of 127.0.0.1 (localhost)
// so the service can accept traffic routed from the Docker host.
const IP_ADDRESS: Ipv4Addr = Ipv4Addr::new(0, 0, 0, 0);

/// Lambda server that handles HTTP requests and routes them to user handlers
pub struct LambdaServer<F> {
    port: u16,
    handler: Arc<F>,
}

impl<F, Fut> LambdaServer<F>
where
    F: Fn(lambda_models::LambdaRequest) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = lambda_models::LambdaResponse> + Send + 'static,
{
    /// Create a new Lambda server
    pub fn new(port: u16, handler: F) -> Self {
        Self {
            port,
            handler: Arc::new(handler),
        }
    }

    /// Start the server and listen for requests
    pub async fn start(self) -> RuntimeResult<()> {
        let addr = SocketAddr::from((IP_ADDRESS, self.port));
        let listener = TcpListener::bind(addr).await
            .map_err(|e| RuntimeError::ServerStart(format!("Failed to bind to {}: {}", addr, e)))?;

        tracing::info!("Lambda runtime listening on {}", addr);

        loop {
            // Waiting for new TCP connection (blocks until new connection is established)
            let (stream, remote_addr) = listener.accept().await
                .map_err(|e| RuntimeError::ServerStart(format!("TCP listen failed to accept connection: {}", e)))?;

            tracing::info!("Processing request from {}", remote_addr);

            let handler = Arc::clone(&self.handler);

            // If you wanted to handle multiple requests concurrently, you could spawn a new task here for each connection.
            // I am not doing that as I am trying to mimic the single-threaded nature of the Lambda runtime, but it would
            // be a simple change to add concurrency if desired. You would just need to wrap the connection handling code
            // below in a `tokio::spawn(async move { ... })` block to run it in the background.

            // TokioIo is a compatibility wrapper that bridges Tokio's async I/O traits with Hyper's expected I/O traits
            // Tokio provides [AsyncRead + AsyncWrite] and Hyper expects [hyper::rt::Read + hyper::rt::Write]
            let io = TokioIo::new(stream);

            // Create service for this connection that routes requests to our handler
            let service = service_fn(move |req| {
                let handler = Arc::clone(&handler);
                handle_request(req, handler)
            });

            // Serve the connection using Hyper's HTTP/1.1 server implementation. This will read requests from the connection,
            // pass them to our service, and write responses back to the connection. It runs until the connection is closed or an error occurs.
            // Using Http1 instead of Http2 since I do not need any of the features of Http2 and it is simpler to implement.
            if let Err(err) = http1::Builder::new()
                .keep_alive(false)
                .serve_connection(io, service)
                .await
            {
                tracing::error!("Error serving connection: {:?}", err);
            }

            tracing::info!("Ready for new gateway connection");
        }
    }
}

/// Handle individual HTTP requests
async fn handle_request<F, Fut>(
    req: Request<hyper::body::Incoming>,
    handler: Arc<F>,
) -> Result<Response<Full<Bytes>>, hyper::Error>
where
    F: Fn(lambda_models::LambdaRequest) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = lambda_models::LambdaResponse> + Send + 'static,
{
    tracing::info!("Received Method: [{:?}] and Path: [{:?}]",
        req.method(),
        req.uri().path()
    );

    let response = match (req.method().as_str(), req.uri().path()) {
        ("GET", "/health") => handle_health().await,
        ("POST", "/invoke") => {
            // Gateway always sends POST /invoke with LambdaRequest JSON
            let lambda_handler = LambdaHandler::new(Arc::clone(&handler));
            lambda_handler.handle_invoke(req).await
        }
        _ => handle_not_found().await,
    };

    tracing::info!("Request completed");

    Ok(response)
}

/// Handle health check requests
// Default health check is provided for use by the gateway to verify
// the runtime is alive and responsive. It returns a simple JSON
// response indicating health status.
async fn handle_health() -> Response<Full<Bytes>> {
    tracing::debug!("Health check requested");

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Full::new(Bytes::from(r#"{"status":"healthy"}"#)))
        .unwrap()
}

/// Handle 404 Not Found
async fn handle_not_found() -> Response<Full<Bytes>> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Full::new(Bytes::from(r#"{"error":"Not Found"}"#)))
        .unwrap()
}

#[cfg(test)]
mod helper_tests {
    use std::time::Duration;

    use super::*;
    use http::StatusCode;
    use http_body_util::BodyExt;
    use lambda_models::{LambdaRequest, LambdaResponse};
    use uuid::Uuid;

    #[test]
    fn test_lambda_server_with_different_ports() {
        let handler = |_req: LambdaRequest| async {
            LambdaResponse {
                status_code: StatusCode::OK,
                headers: None,
                body: "test".to_string(),
                request_id: Uuid::new_v4(),
                execution_time_ms: Duration::from_millis(100),
            }
        };

        let server1 = LambdaServer::new(3000, handler);
        let server2 = LambdaServer::new(4000, handler);

        assert_eq!(server1.port, 3000);
        assert_eq!(server2.port, 4000);
    }

    #[tokio::test]
    async fn test_handle_health() {
        let response = handle_health().await;

        assert_eq!(response.status(), 200);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE.as_str()).unwrap(),
            "application/json"
        );

        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
        assert_eq!(body_str, r#"{"status":"healthy"}"#);
    }

    #[tokio::test]
    async fn test_handle_not_found() {
        let response = handle_not_found().await;

        assert_eq!(response.status(), 404);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE.as_str()).unwrap(),
            "application/json"
        );

        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
        assert_eq!(body_str, r#"{"error":"Not Found"}"#);
    }
}