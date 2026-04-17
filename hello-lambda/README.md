# Hello Lambda

A testing Lambda function designed to simulate realistic serverless behavior patterns for development and testing purposes.

## Overview

This is a demonstration Lambda function that implements various runtime behaviors to test the lambda-runtime system under different conditions. It provides a comprehensive testing environment for validating error handling, performance monitoring, and system resilience.

## Features

- **Async Request Processing**: Handles requests asynchronously with simulated work delays
- **Behavioral Simulation**: Randomly exhibits different behaviors including:
  - Successful responses with processing delays
  - Server errors (500 responses)
  - Panic conditions for testing crash recovery
- **Rich Response Metadata**: Returns detailed information about request processing including:
  - Processing delays and timing information
  - Server information (port, function name, version)
  - Complete request echo for debugging
- **Structured Logging**: Comprehensive logging with request IDs for tracing
- **Custom Headers**: Includes debugging headers for monitoring and observability

## Configuration

The function reads its configuration from environment variables:
- `LAMBDA_PORT`: Port number for the runtime (set by the gateway)

Behavioral probabilities and timing parameters can be adjusted in the `utils.rs` module.

## Building

```bash
# Development build
cargo build

# Release build
cargo build --release

# Linux deployment build
cargo build --release --target x86_64-unknown-linux-musl
```

## Running
```bash
# Set required environment variable and run
LAMBDA_PORT=8080 cargo run

# Or run the compiled binary
LAMBDA_PORT=8080 ./target/release/hello-lambda
```

## Testing
```bash
cargo test
```

## Response Examples

### Successful Response
```json
{
  "status": "success",
  "echo": {
    "request_id": "123e4567-e89b-12d3-a456-426614174000",
    "method": "GET",
    "path": "/api/test",
    "headers": {...},
    "body": "request content"
  },
  "metadata": {
    "processing_delay_ms": 1847,
    "handler_version": "1.0.0",
    "simulated_work": true
  },
  "server_info": {
    "function_name": "hello-lambda",
    "runtime": "custom-rust-runtime",
    "port": 8080,
    "response_time": "2024-01-15T10:30:00Z"
  }
}
```

### Error Response
```json
{
  "status": "error",
  "error": {
    "code": "INTERNAL_SERVER_ERROR",
    "message": "Simulated server error occurred",
    "request_id": "123e4567-e89b-12d3-a456-426614174000"
  },
  "server_info": {
    "function_name": "hello-lambda",
    "port": 8080
  }
}
```

## Usage with Gateway

This function is designed to work with the lambda-gateway system:

1. Build the binary for your target platform
2. Deploy to the gateway's functions directory
3. Configure the gateway to route requests to this function
4. Monitor logs and responses to observe different behavioral patterns

## Purpose

This crate serves as a comprehensive test fixture for:

- Lambda runtime error handling and recovery
- Performance monitoring and timing validation
- Request/response serialization testing
- System resilience under various failure conditions
- Load testing and behavioral analysis

The randomized behaviors help ensure that monitoring, logging, and error handling systems work correctly across different scenarios that might occur in production environments.