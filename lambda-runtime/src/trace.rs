use tracing::{Level, Span};
use uuid::Uuid;
use std::time::Duration;

const LAMBDA_REQUEST_STR: &str = "[LAMBDA REQUEST]";
const LAMBDA_START_STR: &str = "[LAMBDA START]";
const LAMBDA_END_STR: &str = "[LAMBDA END]";

/// Initialize tracing for the Lambda runtime
pub fn init_tracing() -> Result<(), Box<dyn std::error::Error>> {
    // Check if a subscriber is already set
    if tracing::dispatcher::has_been_set() {
        tracing::debug!("Tracing subscriber already set, skipping initialization");
        return Ok(());
    }

    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

    let fmt_layer = fmt::layer().compact();
    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))?;

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .init();

    tracing::info!("Tracing initialized successfully");
    Ok(())
}

/// Create a span with the request ID from the LambdaRequest
pub fn make_span_with_request_id(request_id: Uuid, method: &str, path: &str) -> Span {
    tracing::span!(
        Level::INFO,
        LAMBDA_REQUEST_STR,
        method = method,
        path = path,
        request_id = %request_id,
    )
}

/// Log request start
pub fn on_request_start(request_id: Uuid) {
    tracing::event!(Level::INFO, request_id = %request_id, LAMBDA_START_STR);
}

/// Log request completion
pub fn on_request_end(request_id: Uuid, latency: Duration, status_code: u16) {
    match status_code {
        400..=599 => {
            tracing::event!(
                Level::ERROR,
                request_id = %request_id,
                latency = ?latency,
                status = status_code,
                LAMBDA_END_STR
            )
        }
        _ => {
            tracing::event!(
                Level::INFO,
                request_id = %request_id,
                latency = ?latency,
                status = status_code,
                LAMBDA_END_STR
            )
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_make_span_with_request_id() {
        let _ = crate::trace::init_tracing().expect("Failed to initialize tracing");

        let request_id = Uuid::new_v4();
        let span = make_span_with_request_id(request_id, "GET", "/test");

        let metadata = span.metadata().unwrap();

        // Test that span is created with correct fields
        assert_eq!(metadata.name(), LAMBDA_REQUEST_STR);
        assert_eq!(metadata.level(), &tracing::Level::INFO);

        // Test that the span was created successfully with the expected field names
        let fields = metadata.fields();
        assert!(fields.field("method").is_some());
        assert!(fields.field("path").is_some());
        assert!(fields.field("request_id").is_some());

        // Test that entering the span doesn't panic (validates field values are valid)
        let _enter = span.enter();
    }

    #[test]
    fn test_tracing_functions_dont_panic() {
        let request_id = Uuid::new_v4();

        // No assertions here, just ensure that these functions can be called without panicking
        on_request_start(request_id);
        on_request_end(request_id, Duration::from_millis(100), 200);
        on_request_end(request_id, Duration::from_millis(250), 400);
        on_request_end(request_id, Duration::from_millis(500), 500);
    }
}