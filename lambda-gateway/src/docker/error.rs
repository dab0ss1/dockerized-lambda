use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum DockerError {
    #[error("Docker API error: {0}")]
    Api(#[from] bollard::errors::Error),

    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Timeout error")]
    Timeout,

    #[error("Container not found: {0}")]
    ContainerNotFound(String),

    #[error("Binary not found: {0}")]
    BinaryNotFound(PathBuf),
}