mod constants;
mod handler;
mod utils;

use handler::create_echo_handler;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Runtime already initializes tracing, so we don't need to do it here
    tracing::info!("Starting Hello Lambda function");

    // Create handler with port information
    let handler = create_echo_handler();

    // Start the Lambda runtime with our echo handler
    lambda_runtime::run(handler).await?;

    Ok(())
}