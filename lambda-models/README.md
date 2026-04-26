# Lambda Models

Core data structures for the Lambda runtime system, providing serializable request and response models for communication between the gateway and Lambda functions.

## Overview

This crate defines the fundamental data types used throughout the Lambda system for request/response handling. It provides consistent serialization/deserialization using serde and includes proper type safety for HTTP methods, status codes, and timing information.

## Models

### LambdaRequest

Represents an incoming request from the gateway to a Lambda function:

```rust
pub struct LambdaRequest {
    pub request_id: Uuid,
    pub method: http::Method,
    pub path: String,
    pub query_parameters: HashMap<String, String>,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub remote_addr: Option<IpAddr>,
    pub timestamp: DateTime<Utc>,
}
```

### LambdaResponse

Represents a response from a Lambda function back to the gateway:

```rust
pub struct LambdaResponse {
    pub status_code: http::StatusCode,
    pub headers: Option<HashMap<String, String>>,
    pub body: String,
    pub request_id: Uuid,
    pub execution_time_ms: Duration,
}
```

## Features

- **Serde Integration**: Full serialization/deserialization support for JSON communication
- **HTTP Types**: Uses standard `http` crate types for methods and status codes
- **Duration Handling**: Automatic millisecond serialization for execution timing using `serde_with`
- **UUID Support**: Built-in request ID tracking for distributed tracing
- **Type Safety**: Strong typing prevents common errors in request/response handling

## Dependencies

- `serde`: Serialization framework
- `serde_with`: Enhanced serialization for Duration types
- `http`: Standard HTTP types
- `uuid`: Unique identifier generation
- `chrono`: Date and time handling

## Usage

Add to your `Cargo.toml`:
```rust
[dependencies]
lambda-models = { path = "../lambda-models" }
```

Import and use:
```rust
use lambda_models::{LambdaRequest, LambdaResponse};

// Create a response
let response = LambdaResponse {
    status_code: http::StatusCode::OK,
    headers: None,
    body: "Hello World".to_string(),
    request_id: request.request_id,
    execution_time_ms: std::time::Duration::from_millis(150),
};
```

## JSON Serialization

Both models serialize cleanly to JSON for network communication:

```json
{
  "request_id": "123e4567-e89b-12d3-a456-426614174000",
  "method": "GET",
  "path": "/api/users",
  "query_parameters": {"limit": "10"},
  "headers": {"content-type": "application/json"},
  "body": "",
  "remote_addr": "127.0.0.1",
  "timestamp": "2024-01-15T10:30:00Z"
}
```

This crate provides the foundational types that enable type-safe communication between all components of the Lambda system.