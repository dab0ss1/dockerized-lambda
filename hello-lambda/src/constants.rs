use lazy_static::lazy_static;

lazy_static! {
    /// Lambda port from environment variable, parsed once at startup
    pub static ref LAMBDA_PORT: u16 = get_port();
}

/// Get the Lambda port (convenience function)
pub fn get_port() -> u16 {
    std::env::var("LAMBDA_PORT")
            .expect("LAMBDA_PORT environment variable must be set by the gateway")
            .parse::<u16>()
            .expect("LAMBDA_PORT must be a valid port number")
}
