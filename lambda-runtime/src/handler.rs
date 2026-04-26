use crate::headers::CustomHeader;
use crate::trace::{make_span_with_request_id, on_request_end, on_request_start};
use crate::{RuntimeError, RuntimeResult};
use tracing::Instrument;
use http::{StatusCode, header};
use lambda_models::{LambdaRequest, LambdaResponse};
use hyper::{Request, Response};
use http_body_util::{Full, BodyExt};
use hyper::body::Bytes;
use std::future::Future;
use std::sync::Arc;
use std::time::Instant;

/// Type alias for handler functions
pub type HandlerFn<Fut> = dyn Fn(LambdaRequest) -> Fut + Send + Sync + 'static;

/// Handles Lambda function invocations
pub struct LambdaHandler<F> {
    user_handler: Arc<F>,
}

impl<F, Fut> LambdaHandler<F>
where
    F: Fn(LambdaRequest) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = LambdaResponse> + Send + 'static,
{
    /// Create a new handler wrapper
    pub fn new(user_handler: Arc<F>) -> Self {
        Self { user_handler }
    }

    /// Handle an invoke request
    pub async fn handle_invoke(
        &self,
        req: Request<hyper::body::Incoming>,
    ) -> Response<Full<Bytes>> {
        match self.process_invoke(req).await {
            Ok(response) => response,
            Err(e) => {
                tracing::error!("Error processing invoke request: {:?}", e);
                self.error_response(e)
            }
        }
    }

    /// Process the invoke request
    async fn process_invoke(
        &self,
        req: Request<hyper::body::Incoming>,
    ) -> RuntimeResult<Response<Full<Bytes>>> {
        // Parse the incoming request
        let lambda_request = self.parse_request(req).await?;
        let request_id = lambda_request.request_id;

        // Create span with the request ID from gateway
        let span = make_span_with_request_id(
            request_id,
            lambda_request.method.as_str(),
            &lambda_request.path
        );

        // Using tracing spans correctly when dealing with async code.
        // Please reference this documentation for more infromation:
        // https://docs.rs/tracing/latest/tracing/span/struct.Span.html#method.enter
        async move {
            on_request_start(request_id);

            // Call user handler
            let handler_start = Instant::now();
            let mut lambda_response = (self.user_handler)(lambda_request).await;
            let handler_duration = handler_start.elapsed();

            // Override timing and request ID
            lambda_response.execution_time_ms = handler_duration;
            lambda_response.request_id = request_id;

            on_request_end(request_id, handler_duration, lambda_response.status_code.as_u16());

            // Convert to HTTP response
            self.build_http_response(lambda_response)
        }.instrument(span).await
    }

    /// Parse HTTP request into LambdaRequest (sent by gateway as JSON)
    async fn parse_request(
        &self,
        req: Request<hyper::body::Incoming>,
    ) -> RuntimeResult<LambdaRequest> {
        let (_parts, body) = req.into_parts();

        // Read body containing LambdaRequest JSON from gateway
        let body_bytes = body.collect().await
            .map_err(|e| RuntimeError::RequestParsing(format!("Failed to read body: {}", e)))?
            .to_bytes();

        let body_string = String::from_utf8(body_bytes.to_vec())
            .map_err(|e| RuntimeError::RequestParsing(format!("Body is not valid UTF-8: {}", e)))?;

        // Parse the LambdaRequest JSON sent by gateway
        let lambda_request: LambdaRequest = serde_json::from_str(&body_string)
            .map_err(|e| RuntimeError::RequestParsing(format!("Failed to parse LambdaRequest JSON: {}", e)))?;

        // Gateway already created the request with proper UUID, headers, etc.
        Ok(lambda_request)
    }

    /// Build HTTP response from LambdaResponse
    fn build_http_response(&self, lambda_response: LambdaResponse) -> RuntimeResult<Response<Full<Bytes>>> {
        tracing::debug!("Building HTTP response for LambdaResponse: {:?}", lambda_response);
        let mut builder = Response::builder()
            .status(lambda_response.status_code.as_u16())
            .header(CustomHeader::LambdaRequestId.as_ref(), lambda_response.request_id.to_string())
            .header(CustomHeader::LambdaExecutionTimeMs.as_ref(), lambda_response.execution_time_ms.as_millis().to_string());

        let mut has_content_type = false;

        // Add headers if provided
        if let Some(headers) = lambda_response.headers {
            for (name, value) in headers {
                if name.to_lowercase() == header::CONTENT_TYPE.as_str() {
                    has_content_type = true;
                }
                builder = builder.header(name, value);
            }
        }

        // Add default content-type if not specified
        if !has_content_type {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
        }

        tracing::debug!("Response builder after adding headers: {:?}", builder);

        let response = builder
            .body(Full::new(Bytes::from(lambda_response.body)))
            .map_err(|e| RuntimeError::ResponseBuilding(format!("Failed to build response: {}", e)))?;

        Ok(response)
    }

    /// Create error response
    fn error_response(&self, error: RuntimeError) -> Response<Full<Bytes>> {
        let error_body = serde_json::json!({
            "error": error.to_string(),
        });

        Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Full::new(Bytes::from(error_body.to_string())))
            .unwrap()
    }
}