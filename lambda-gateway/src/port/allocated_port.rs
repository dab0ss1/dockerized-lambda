use tokio::sync::mpsc::{self, error::TrySendError};

pub struct AllocatedPort {
    port: u16,
    return_tx: mpsc::Sender<u16>,
}

impl AllocatedPort {
    pub(crate) fn new(port: u16, return_tx: mpsc::Sender<u16>) -> Self {
        Self { port, return_tx }
    }

    /// Get the port number
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Release the port immediately
    pub fn release_now(self) {
        // Drop will handle the return
    }
}

impl Drop for AllocatedPort {
    #[tracing::instrument(name = "DropAllocatedPort", skip_all)]
    fn drop(&mut self) {
        match self.return_tx.try_send(self.port) {
            Ok(()) => {
                tracing::info!("Port {} notified for return to pool", self.port);
            }
            Err(TrySendError::Full(_)) => {
                tracing::warn!("Port return channel full, port {} may leak", self.port);
            }
            Err(TrySendError::Closed(_)) => {
                // Port Manager is being destroyed
                tracing::debug!("Port manager shut down, port {} not returned", self.port);
            }
        }
    }
}

// Implement common traits for convenience
impl std::fmt::Debug for AllocatedPort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AllocatedPort")
            .field("port", &self.port)
            .finish()
    }
}

impl std::fmt::Display for AllocatedPort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.port)
    }
}