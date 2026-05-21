use tokio::sync::mpsc;
use uuid::Uuid;

pub struct HostLease {
    pub container_id: String,
    pub port: u16,

    // Fields used to release the leased host back to the pool
    lease_id: Uuid,
    return_tx: mpsc::Sender<Uuid>,
}

impl HostLease {
    pub fn new(
        container_id: String,
        port: u16,
        lease_id: Uuid,
        return_tx: mpsc::Sender<Uuid>,
    ) -> Self {
        Self {
            container_id,
            port,
            lease_id,
            return_tx,
        }
    }

    /// Release the host lease immediately
    pub fn release_now(self) {
        // Do nothing - just consume self
        // Drop will handle sending the lease_id
    }
}

impl Drop for HostLease {
    #[tracing::instrument(name = "DropHostLease", skip_all)]
    fn drop(&mut self) {
        // Use try_send for synchronous sending in Drop
        if let Err(e) = self.return_tx.try_send(self.lease_id.clone()) {
            tracing::error!("Failed to return lease {} on drop: {}", self.lease_id, e);
        } else {
            tracing::info!("Lease {} returned to pool on drop", self.lease_id);
        }
    }
}