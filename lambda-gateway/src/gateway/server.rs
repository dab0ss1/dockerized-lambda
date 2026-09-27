use hyper::{
    Method, Request, Response, StatusCode, body::Incoming, header, server::conn::http1, service::service_fn
};
use hyper_util::{client::legacy::Client, rt::{TokioExecutor, TokioIo}};
use http_body_util::{BodyExt, Full, combinators::BoxBody};
use tokio::net::TcpListener;
use std::{sync::Arc, time::Instant};
use std::time::Duration;
use std::convert::Infallible;
use bytes::Bytes;

use crate::{pool::{HostLease, HostPoolManager, PoolError}, trace};
use super::{convert::http_to_lambda_request, error::GatewayError};

pub struct GatewayServer {
    pool_manager: Arc<HostPoolManager>,
    request_timeout: Duration,
    client: Client<hyper_util::client::legacy::connect::HttpConnector, Full<Bytes>>,
}

impl GatewayServer {
    pub fn new(pool_manager: Arc<HostPoolManager>, request_timeout: Duration) -> Self {
        let client = Client::builder(TokioExecutor::new())

            .build_http();

        Self {
            pool_manager,
            request_timeout,
            client,
        }
    }

    /// Start the gateway HTTP server
    #[tracing::instrument(name = "Start", skip_all)]
    pub async fn start(&self, bind_addr: &str) -> Result<(), GatewayError> {
        let listener = TcpListener::bind(bind_addr).await
            .map_err(|e| GatewayError::Internal(format!("Failed to bind to {}: {}", bind_addr, e)))?;

        tracing::info!("Gateway server listening on {}", bind_addr);

        loop {
            let (stream, addr) = listener.accept().await
                .map_err(|e| GatewayError::Internal(format!("Failed to accept connection: {}", e)))?;

            let io = TokioIo::new(stream);
            let server = Arc::new(self.clone());

            tokio::spawn(async move {
                let service = service_fn(move |req| {
                    let server = Arc::clone(&server);
                    async move { server.handle_request(req).await }
                });

                if let Err(err) = http1::Builder::new()
                    .serve_connection(io, service)
                    .await
                {
                    tracing::error!("Error serving connection from {}: {}", addr, err);
                }
            });
        }
    }

    #[tracing::instrument(name = "HandleRequest", skip_all)]
    async fn handle_request(
        &self,
        req: Request<Incoming>,
    ) -> Result<Response<BoxBody<Bytes, hyper::Error>>, Infallible> {
        match self.forward_to_lambda(req).await {
            Ok(response) => Ok(response),
            Err(e) => Ok(self.error_response(e)),
        }
    }

    #[tracing::instrument(name = "ForwardToLambda", skip_all)]
    async fn forward_to_lambda(
        &self,
        req: Request<Incoming>,
    ) -> Result<Response<BoxBody<Bytes, hyper::Error>>, GatewayError> {
        let start_time = Instant::now();

        // Convert HTTP request to LambdaRequest (buffers body)
        let lambda_request = http_to_lambda_request(req).await?;

        let span = trace::make_span_with_request_id(
            lambda_request.request_id,
            lambda_request.method.as_str(),
            &lambda_request.path
        );
        let _enter = span.enter();

        // Log request start
        trace::on_request_start(lambda_request.request_id);

        // Serialize LambdaRequest to JSON
        let body_bytes = serde_json::to_vec(&lambda_request)
            .map_err(GatewayError::Json)?;

        // Lease a host from the pool
        let host_lease = self.lease_host_with_retry().await?;

        tracing::info!(
            request_id = %lambda_request.request_id,
            container_id = %host_lease.container_id,
            port = host_lease.port,
            "Leased host for request"
        );

        // Create request to container
        let container_url = format!("http://127.0.0.1:{}/invoke", host_lease.port);

        tracing::info!(
            request_id = %lambda_request.request_id,
            "Creating Hyper request"
        );

        let hyper_request = Request::builder()
            .method(Method::POST)
            .uri(&container_url)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Full::new(Bytes::from(body_bytes)))
            .map_err(|e| GatewayError::Internal(format!("Failed to build request: {}", e)))?;

        tracing::info!(
            request_id = %lambda_request.request_id,
            "Making Hyper request"
        );

        // Forward request to container with timeout
        let response = tokio::time::timeout(
            self.request_timeout,
            self.client.request(hyper_request)
        ).await
        .map_err(|e| {
            tracing::error!(
                request_id = %lambda_request.request_id,
                error = %e,
                "Request to container timed out"
            );
            GatewayError::Timeout
        })?
        .map_err(|e| {
            tracing::error!(
                request_id = %lambda_request.request_id,
                error = %e,
                "Failed to send request to container"
            );
            GatewayError::HttpClient(e)
        })?;

        // Return host to pool immediately after request completes
        drop(host_lease);

        tracing::info!(
            request_id = %lambda_request.request_id,
            status = response.status().as_u16(),
            "Received response from Lambda"
        );

        // Buffer the response body
        let (parts, body) = response.into_parts();
        let body_bytes = body.collect().await
            .map_err(GatewayError::Http)?
            .to_bytes();

        // Create response with buffered body
        let response = Response::from_parts(
            parts,
            BoxBody::new(Full::new(body_bytes).map_err(|never| match never {}))
        );

        // Log request completion
        let latency = start_time.elapsed();
        trace::on_request_end(lambda_request.request_id, latency, response.status().as_u16());

        // Host is automatically returned to pool when it goes out of scope
        Ok(response)
    }

    #[tracing::instrument(name = "LeaseHostWithRetry", skip_all)]
    async fn lease_host_with_retry(&self) -> Result<HostLease, GatewayError> {
        let max_attempts = 3;
        let mut last_error = None;

        for attempt in 1..=max_attempts {
            match self.pool_manager.lease_host().await {
                Ok(host) => {
                    tracing::info!("Successfully leased host on attempt {}", attempt);
                    return Ok(host);
                }
                Err(PoolError::ShuttingDown) => {
                    // Don't retry shutdown
                    return Err(GatewayError::ShuttingDown);
                }
                Err(PoolError::MaxInstancesReached) => {
                    // Don't retry capacity limits
                    return Err(GatewayError::Pool(PoolError::MaxInstancesReached));
                }
                Err(PoolError::ContainerNotReady) => {
                    tracing::warn!("Lease attempt {} failed: {}", attempt, PoolError::ContainerNotReady);
                    last_error = Some(PoolError::ContainerNotReady);

                    if attempt < max_attempts {
                        // Small backoff between retries
                        let delay = Duration::from_millis(50 * attempt as u64);
                        tokio::time::sleep(delay).await;
                    }
                }
                Err(e) => return Err(GatewayError::Pool(e)),
            }
        }

        Err(GatewayError::Pool(last_error.unwrap()))
    }

    #[tracing::instrument(name = "ErrorResponse", skip_all)]
    fn error_response(&self, error: GatewayError) -> Response<BoxBody<Bytes, hyper::Error>> {
        let status = error.status_code();
        let message = error.error_message();

        tracing::error!(
            error = %error,
            status = status.as_u16(),
            message = %message,
            "Gateway request failed"
    );

        let body = serde_json::json!({
            "error": {
                "status": status.as_u16(),
                "message": message,
                "timestamp": chrono::Utc::now()
            }
        });

        let body_bytes = serde_json::to_vec(&body)
            .unwrap_or_else(|_| b"Internal server error".to_vec());

        Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, "application/json")
            .body(BoxBody::new(Full::new(Bytes::from(body_bytes)).map_err(|never| match never {})))
            .unwrap_or_else(|_| {
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(BoxBody::new(Full::new(Bytes::from("Internal server error")).map_err(|never| match never {})))
                    .unwrap()
            })
    }
}

impl Clone for GatewayServer {
    fn clone(&self) -> Self {
        Self {
            pool_manager: Arc::clone(&self.pool_manager),
            request_timeout: self.request_timeout,

            // According to the documentation:
            // `Client` is cheap to clone and cloning is the recommended way to share a `Client`.
            // The underlying connection pool will be reused.
            client: self.client.clone(),
        }
    }
}

/// Start the gateway server (convenience function)
#[tracing::instrument(name = "StartServer", skip_all)]
pub async fn start_server(
    bind_addr: &str,
    pool_manager: Arc<HostPoolManager>,
    request_timeout: Duration,
) -> Result<(), GatewayError> {
    let server = GatewayServer::new(pool_manager, request_timeout);
    server.start(bind_addr).await
}
