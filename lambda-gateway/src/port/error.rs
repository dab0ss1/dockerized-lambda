
// src/pool/port_error.rs
#[derive(Debug, thiserror::Error)]
pub enum PortError {
    #[error("No available ports in range")]
    NoAvailablePorts,
}