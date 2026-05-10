use uuid::Uuid;

use crate::{docker::DockerError, port::PortError};

#[derive(Debug, thiserror::Error)]
pub enum PoolError {
    #[error("Invalid lease: {0}")]
    InvalidLease(Uuid),

    #[error("Container not ready")]
    ContainerNotReady,

    #[error("Docker error: {0}")]
    Docker(#[from] DockerError),

    #[error("No available ports")]
    NoAvailablePorts,

    #[error("Port allocation error: {0}")]
    PortAllocation(#[from] PortError),

    #[error("Max instances reached")]
    MaxInstancesReached,

    #[error("Gateway is shutting down")]
    ShuttingDown,
}