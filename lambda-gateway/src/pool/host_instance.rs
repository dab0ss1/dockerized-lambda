use tokio::sync::mpsc;
use crate::port::AllocatedPort;
use std::time::Duration;

#[derive(Debug)]
pub struct HostInstance {
    pub container_id: String,
    pub allocated_port: AllocatedPort,
    pub created_at: std::time::Instant,
    pub health_check_tested: bool,

    // used for cleanup on drop
    cleanup_sender: mpsc::Sender<String>,
}

impl HostInstance {
    pub fn new(
        container_id: String,
        allocated_port: AllocatedPort,
        cleanup_sender: mpsc::Sender<String>,
    ) -> Self {
        Self {
            container_id,
            allocated_port,
            created_at: std::time::Instant::now(),
            health_check_tested: false,
            cleanup_sender,
        }
    }

    /// Check if this host has exceeded TTL
    pub fn is_expired(&self, ttl: Duration) -> bool {
        std::time::Instant::now().duration_since(self.created_at) > ttl
    }

    /// Manually stop the container (optional - will happen automatically on drop)
    pub async fn stop_now(self) {
        // Do nothing - just consume self for drop to run
    }
}

impl Drop for HostInstance {
    #[tracing::instrument(name = "DropHostInstance", skip_all)]
    fn drop(&mut self) {
        tracing::info!("Dropping HostInstance, sending container {} for cleanup", self.container_id);

        // Send container ID to cleanup channel
        if let Err(e) = self.cleanup_sender.try_send(self.container_id.clone()) {
            tracing::error!("Failed to send container {} for cleanup: {}", self.container_id, e);
        } else {
            tracing::info!("Container {} sent for cleanup on drop", self.container_id);
        }
    }
}