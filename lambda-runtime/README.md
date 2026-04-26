# Lambda Runtime

A lightweight, high-performance runtime for executing Lambda functions in containerized environments. This runtime provides the execution environment and HTTP server infrastructure needed to run user-defined Lambda functions.

## Overview

The Lambda Runtime acts as the bridge between the gateway and user Lambda functions. It handles HTTP communication, request parsing, response building, timing measurement, and structured logging while providing a simple async interface for function authors.

## Features

- **HTTP Server**: Built-in HTTP/1.1 server with keep-alive support
- **Request Handling**: Automatic parsing of `LambdaRequest` JSON from gateway
- **Response Building**: Converts `LambdaResponse` objects to proper HTTP responses
- **Timing Measurement**: Automatic execution time tracking and reporting
- **Structured Logging**: Request ID correlation and distributed tracing support
- **Error Handling**: Graceful error recovery with proper HTTP error responses
- **Health Checks**: Built-in `/health` endpoint for container orchestration
- **Async Support**: Full async/await support for user functions

## API

### Main Entry Points

```rust
// For async handlers
pub async fn run<F, Fut>(handler: F) -> RuntimeResult<()>

// For sync handlers
pub async fn run_sync<F>(handler: F) -> RuntimeResult<()>
```

### Handler Function Signatures
```rust
// Async handler
async fn my_handler(req: LambdaRequest) -> LambdaResponse { ... }

// Sync handler
fn my_handler(req: LambdaRequest) -> LambdaResponse { ... }
```

## Usage

Add to your Cargo.toml:
```rust
[dependencies]
lambda-runtime = { path = "../lambda-runtime" }
lambda-models = { path = "../lambda-models" }
```

Create a Lambda function:
```rust
use lambda_runtime::{run_sync, LambdaRequest, LambdaResponse};
use http::StatusCode;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    lambda_runtime::run_sync(|req| {
        LambdaResponse {
            status_code: StatusCode::OK,
            headers: None,
            body: format!("Hello from {}!", req.path),
            request_id: req.request_id,
            execution_time_ms: std::time::Duration::from_millis(0),
        }
    }).await?;

    Ok(())
}
```

## Environment Variables

- `LAMBDA_PORT`: Port number for the HTTP server (set by gateway)

## Endpoints

- `GET /health`: Health check endpoint returning `{"status":"healthy"}`
- `POST /invoke`: Function invocation endpoint (accepts `LambdaRequest` JSON)

## Architecture

The runtime follows a simple request/response flow:

1. **Gateway** sends `POST /invoke` with `LambdaRequest` JSON
2. **Runtime** deserializes request and calls user handler
3. **User Handler** processes request and returns `LambdaResponse`
4. **Runtime** converts response to HTTP and adds metadata headers
5. **Gateway** receives HTTP response and forwards to client

## Response Headers

The runtime automatically adds metadata headers:

- `x-lambda-request-id`: Request correlation ID
- `x-lambda-execution-time-ms`: Function execution time in milliseconds
- `content-type`: Set to `application/json` if not specified by handler

## Error Handling

- **Parse Errors**: Returns 500 with JSON error details
- **Handler Panics**: Runtime may crash (depending on panic handling)
- **Invalid Requests**: Returns appropriate HTTP error codes

## Testing

The runtime includes comprehensive unit and integration tests:
```bash
# Run all tests
cargo test

# Run integration tests only
cargo test --test '*' -- --test-threads=1
```

## Dependencies
- `tokio`: Async runtime
- `hyper`: HTTP server implementation
- `serde`: JSON serialization
- `tracing`: Structured logging
- `lambda-models`: Request/response types

This runtime provides the foundation for building scalable, observable Lambda functions with minimal boilerplate and maximum performance.