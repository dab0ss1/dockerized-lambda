use std::{collections::{HashMap, VecDeque}, sync::{Arc, atomic::{AtomicBool, Ordering}}, time::Duration};

use tokio::sync::{Mutex, mpsc};
use uuid::Uuid;

use crate::{docker::{DockerConfig, DockerManager}, pool::{HostInstance, HostLease, PoolConfig, PoolError}, port::PortManager};

struct HostPoolManagerState {
    available_hosts: VecDeque<HostInstance>,
    leased_hosts: HashMap<Uuid, HostInstance>,
}

pub struct HostPoolManager {
    state: Mutex<HostPoolManagerState>,

    config: PoolConfig,

    docker_manager: Arc<DockerManager>,
    port_manager: Arc<PortManager>,

    lease_release_sender: mpsc::Sender<Uuid>,
    cleanup_sender: mpsc::Sender<String>,

    // SAFETY: This can be held outside state since it is only used in the shutdown process
    worker_handles: Mutex<Vec<tokio::task::JoinHandle<()>>>,

    // SAFETY: This can be held outside state since when set to true we should not lease hosts anymore.
    // This is only checked at the beginning of a lease, otherwise in the shutdown process we wait for
    // all active / leased hosts to finish handling their requests.
    shutdown_flag: AtomicBool,
}

impl HostPoolManager {
    pub fn new(config: PoolConfig) -> Arc<Self> {
        let (lease_release_sender, lease_release_receiver) = mpsc::channel(1000);
        let (cleanup_sender, cleanup_receiver) = mpsc::channel(1000);

        let port_manager = PortManager::new(config.port_range.clone());

        // Create Docker manager
        let docker_config = DockerConfig {
            binary_path: config.binary_path.clone(),
            binary_name: config.binary_name.clone(),
            memory_limit_mb: config.container_limits.memory_mb,
            cpu_limit: config.container_limits.cpu_limit,
        };

        let docker_manager = DockerManager::new(docker_config)
            .expect("Docker Manager required for gateway to be created.");
        tracing::info!("Docker manager initialized");

        let manager = Arc::new(Self {
            state: Mutex::new(HostPoolManagerState {
                available_hosts: VecDeque::new(),
                leased_hosts: HashMap::new(),
            }),
            docker_manager,
            config,
            port_manager,
            lease_release_sender,
            cleanup_sender,
            worker_handles: Mutex::new(Vec::new()),
            shutdown_flag: AtomicBool::new(false),
        });

        let mut handles = Vec::new();

        // 1. Return processor task - processes returned host leases
        let return_manager = Arc::clone(&manager);
        handles.push(tokio::spawn(async move {
            return_manager.process_host_lease_returns(lease_release_receiver).await;
        }));

        // 2. Start cleanup background task
        let cleanup_manager = Arc::clone(&manager);
        handles.push(tokio::spawn(async move {
            cleanup_manager.handle_container_cleanup(cleanup_receiver).await;
        }));

        // 4. TTL reaper task - removes expired hosts from available pool
        let reaper_manager = Arc::clone(&manager);
        handles.push(tokio::spawn(async move {
            let mut interval = tokio::time::interval(reaper_manager.config.reap_check_interval);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                interval.tick().await;
                if let Err(e) = reaper_manager.reap_expired_hosts().await {
                    tracing::error!("TTL reaper task failed: {}", e);
                }
            }
        }));

        // 5. Min instances ensurer task - maintains minimum number of available hosts
        let scaling_manager = Arc::clone(&manager);
        handles.push(tokio::spawn(async move {
            let mut interval = tokio::time::interval(scaling_manager.config.host_scaling_check_interval);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                interval.tick().await;
                if let Err(e) = scaling_manager.run_scaling_policy().await {
                    tracing::error!("Min instances task failed: {}", e);
                }
            }
        }));

        *manager.worker_handles.try_lock()
            .expect("The lock for worker_handles should never be held somewhere else at this point") = handles;

        tracing::info!("Started all background maintenance tasks");

        manager
    }

    /// Check if shutdown has been initiated
    pub fn is_shutting_down(&self) -> bool {
        self.shutdown_flag.load(Ordering::Acquire)
    }

    // Returns a host that is now leased to the caller -- caller is responsible for releasing
    // the lease when finished. As a safety mechanism leases will be automatically released
    // when the HostLease goes out of scope (dropped), but it is best practice to release as
    // soon as possible to free up resources for other callers.
    #[tracing::instrument(name = "LeaseHost", skip_all)]
    pub async fn lease_host(&self) -> Result<HostLease, PoolError> {
        // Check shutdown flag first
        if self.is_shutting_down() {
            return Err(PoolError::ShuttingDown);
        }

        // Aquire state lock
        let mut state_guard = self.state.lock().await;
        tracing::info!("state_guard locked");

        // Try to get existing host from available pool or create a new one
        let host = match state_guard.available_hosts.pop_front() {
            Some(host) => host,
            None => {
                // Check host capacity limits -- only need to check `leased_hosts` since if there was a host
                // in `available_hosts` the pop_front method would have returned `Some(host)` instead of `None`
                let current_count = state_guard.leased_hosts.len();
                if current_count >= self.config.max_instances {
                    return Err(PoolError::MaxInstancesReached);
                }
                tracing::info!("spawning new host");
                // No available hosts, create new one
                self.spawn_new_host().await?
            }
        };

        tracing::info!("Acquired host instance: {:?}", host);

        let lease_id = Uuid::new_v4();
        let container_id = host.container_id.clone();
        let port = host.allocated_port.port();

        let host_health_check_tested = host.health_check_tested;

        tracing::info!("inserted host {}, into leased hosts with health_check_tested: {}", container_id, host.health_check_tested);
        state_guard.leased_hosts.insert(lease_id, host);
        tracing::info!("leased hosts size: {}", state_guard.leased_hosts.len());
        drop(state_guard); // Release lock after critical section is completed

        // We can wait for host to spin up without holding the state gaurd as the information has
        // already been added to the state data structures
        tracing::info!("host_health_check_tested is {}", host_health_check_tested);
        if !host_health_check_tested {
            let ready_result = self.wait_for_container_ready(&container_id, port).await;
            // Acquire state gaurd after container ready check is completed -- we do not need to block other threads
            // from obtaining the lock while we wait for our host to be tested
            let mut state_guard = self.state.lock().await;
            match ready_result {
                // Host was successfully tested, we need to update the `health_check_tested` value
                Ok(true) => {
                    let active_host = state_guard.leased_hosts.get_mut(&lease_id)
                        // This should never, ever happen, as we just inserted the lease id and host above
                        .ok_or(PoolError::InvalidLease(lease_id))?;
                    active_host.health_check_tested = true;
                    drop(state_guard); // Release lock after critical section is completed
                    tracing::info!("Host health check passed and is ready for leasing");
                }
                // Host failed to response in time for health check, will assume hosts is down and release it
                Ok(false) => {
                    let dead_host = state_guard.leased_hosts.remove(&lease_id);
                    drop(state_guard); // Release lock after critical section is completed
                    drop(dead_host);
                    tracing::error!("Host failed to response to health check in time");
                    return Err(PoolError::ContainerNotReady);
                }
                // Error occured when trying to health check host, will assume hosts is down and release it
                Err(e) => {
                    let dead_host = state_guard.leased_hosts.remove(&lease_id);
                    drop(state_guard); // Release lock after critical section is completed
                    drop(dead_host);
                    tracing::error!("Host failed to response to health check in time with error: {}", e);
                    return Err(e);
                }
            }
        }

        Ok(HostLease::new(
            container_id,
            port,
            lease_id,
            self.lease_release_sender.clone(),
        ))
    }

    // Spawns a new host instance and returns it or an error if we are at max capacity or if creation fails
    #[tracing::instrument(name = "SpawnNewHost", skip_all)]
    async fn spawn_new_host(&self) -> Result<HostInstance, PoolError> {
        // Allocate port (automatically returned when HostInstance is dropped)
        let allocated_port = self.port_manager.allocate_port().await
            .map_err(|e| PoolError::PortAllocation(e))?;

        // Create and start container
        let container_id = self.docker_manager
            .create_container(allocated_port.port())
            .await
            .map_err(|e| PoolError::Docker(e))?;

        Ok(HostInstance::new(
            container_id,
            allocated_port,
            self.cleanup_sender.clone(),
        ))
    }

    #[tracing::instrument(name = "WaitForContainerReady", skip_all)]
    async fn wait_for_container_ready(&self, container_id: &str, port: u16) -> Result<bool, PoolError> {
        let max_attempts = 10;
        let check_interval = Duration::from_millis(50);

        for attempt in 1..=max_attempts {
            match self.docker_manager.health_check(port).await {
                Ok(true) => {
                    tracing::info!("Container {} ready after {} attempts", container_id, attempt);
                    return Ok(true);
                }
                Ok(false) => {
                    tracing::debug!("Health check failed, attempt {}/{}", attempt, max_attempts);
                }
                Err(e) => {
                    tracing::debug!("Health check error, attempt {}/{}: {}", attempt, max_attempts, e);
                    if attempt == max_attempts {
                        tracing::info!("Health check error'ed on final attempt {}/{}: {}", attempt, max_attempts, e);
                        return Err(PoolError::Docker(e));
                    }
                }
            }

            if attempt < max_attempts {
                tracing::info!("Sleeping for {:?}, on attempt {} / {}", check_interval, attempt, max_attempts);
                tokio::time::sleep(check_interval).await;
            }
        }

        Ok(false) // All attempts failed but no error
    }

    // Only called by asynchronous background task -- should never be called otherwise
    // This function is for cleaning up leased out hosts (host lease structs on drop call this channel)
    #[tracing::instrument(name = "ProcessHostLeaseReturns", skip_all)]
    async fn process_host_lease_returns(&self, mut lease_release_receiver: mpsc::Receiver<Uuid>) {
        tracing::info!("Host return background process starting up");
        while let Some(lease_id) = lease_release_receiver.recv().await {
            tracing::info!("Received release request for host with lease_id: {}", lease_id);
            if let Err(e) = self.return_host_internal(lease_id).await {
                tracing::error!("Failed to return host {}: {}", lease_id, e);
            }
        }
        tracing::info!("Host return background process shutting down");
    }

    // Method for returning host - only meant to be called by background task running host lease returns
    #[tracing::instrument(name = "ReturnHostInternal", skip_all)]
    async fn return_host_internal(&self, lease_id: Uuid) -> Result<(), PoolError> {
        let mut state_guard = self.state.lock().await;

        // It is safe to remove from leased_hosts here because only the background task processes returns,
        // and hosts are only leased out through `available_hosts`.
        let host_opt: Option<HostInstance> = state_guard.leased_hosts.remove(&lease_id);

        if let Some(host) = host_opt {
            // Check if host has exceeded TTL
            if host.is_expired(self.config.instance_ttl) {
                drop(state_guard); // Release lock after critical section is completed
                tracing::info!(
                    "Host {} exceeded TTL on return, dropping instead of returning to pool",
                    host.container_id
                );
                // Drop the host - docker container and port will be cleaned up automatically
                drop(host);
            } else {
                // confirm container is still up
                let container_id = host.container_id.clone();
                if self.docker_manager.get_container_info(&container_id).await.is_err() {
                    tracing::info!("docker container is no longer up for container id: {} -- cleaning up", container_id);
                    // Drop the host - all resources will be cleaned up automatically
                    drop(host);
                } else {
                    // Host is still fresh, return to available pool
                    state_guard.available_hosts.push_back(host);
                    drop(state_guard); // Release lock after critical section is completed
                    tracing::info!("Host {} returned to pool for lease {}", container_id, lease_id);
                }
            }
            Ok(())
        } else {
            tracing::warn!("Attempted to return non-existent lease: {}", lease_id);
            Err(PoolError::InvalidLease(lease_id))
        }
    }

    // Background task to handle container cleanup
    // This function is for cleaning up host instances (host instance structs on drop call this channel)
    // This is required to clean up the docker container
    #[tracing::instrument(name = "HandleContainerCleanup", skip_all)]
    async fn handle_container_cleanup(&self, mut receiver: mpsc::Receiver<String>) {
        while let Some(container_id) = receiver.recv().await {
            tracing::info!("Cleaning up container: {}", container_id);

            // Stop and remove container
            if let Err(e) = self.docker_manager.stop_container(&container_id).await {
                tracing::error!("Failed to stop container {}: {}", container_id, e);
            } else {
                tracing::info!("Container {} stopped successfully", container_id);
            }
        }
    }

    // Background task to handle cleaning up expried hosts
    #[tracing::instrument(name = "ReapExpiredHosts", skip_all)]
    async fn reap_expired_hosts(&self) -> Result<(), PoolError> {
        // Only check available hosts (not leased ones) - leased hosts will be reaped when they are returned and found to be expired, so no need to check them here
        let expired_hosts = {
            let mut state_guard = self.state.lock().await;

            let (expired, still_valid): (VecDeque<_>, VecDeque<_>) = state_guard.available_hosts
                .drain(..)
                .partition(|host| host.is_expired(self.config.instance_ttl));

            tracing::info!("Still valid hosts: {:?}", still_valid);

            state_guard.available_hosts = still_valid;  // set available_hosts to what is actually valid

            drop(state_guard); // Release lock after critical section is completed
            expired
        };

        // Drop expired hosts - container and port cleanup happens automatically on drop
        for host in expired_hosts {
            tracing::info!(
                "Reaping expired host: {} (age: {:?})",
                host.container_id,
                std::time::Instant::now().duration_since(host.created_at)
            );
            // Container and port automatically cleaned up when host is dropped
            drop(host);
        }

        Ok(())
    }

    // Background task to handle minimum host scaling policy
    // This method will spin up new hosts and add them to the pool but will not health check them.
    // Health check before hand off is handled by the host leasing method
    #[tracing::instrument(name = "RunScalingPolicy", skip_all)]
    async fn run_scaling_policy(&self) -> Result<(), PoolError> {
        let mut state_guard = self.state.lock().await;

        // Check capacity and create new host
        let total_host_count = state_guard.leased_hosts.len() + state_guard.available_hosts.len();

        if total_host_count < self.config.min_instances {
            let needed = self.config.min_instances - total_host_count;
            tracing::info!("Need to spawn {} hosts to meet min_instances", needed);

            for _ in 0..needed {
                match self.spawn_new_host().await {
                    Ok(host) => {
                        state_guard.available_hosts.push_back(host);
                        tracing::info!("Spawned new host to meet min_instances");
                    }
                    Err(e) => {
                        tracing::error!("Failed to spawn host for min_instances: {}", e);
                        // Continue trying to spawn others
                    }
                }
            }
        }
        drop(state_guard); // Release lock after critical section is completed

        Ok(())
    }

    /// Graceful shutdown with request draining
    #[tracing::instrument(name = "ShutdownAllContainers", skip_all)]
    pub async fn shutdown_all_containers(&self) -> Result<(), PoolError> {
        tracing::info!("Initiating graceful shutdown...");

        // 1. Set shutdown flag to reject new requests
        self.shutdown_flag.store(true, Ordering::Release);
        tracing::info!("Shutdown flag set - rejecting new requests");

        // 2. Wait for all in-flight requests to complete
        self.wait_for_requests_to_drain().await;

        // 3. Cancel background tasks
        let mut worker_handles_gaurd = self.worker_handles.lock().await;
        for handle in worker_handles_gaurd.drain(..) {
            handle.abort();
        }
        drop(worker_handles_gaurd); // Release lock after critical section is completed
        tracing::info!("Background worker tasks aborted");

        // 4. Now safely collect all hosts
        let all_hosts = self.collect_all_hosts().await;

        tracing::info!("Stopping {} containers...", all_hosts.len());

        // 5. Stop all containers
        for host in all_hosts {
            tracing::info!("Stopping container: {}", host.container_id);
            // Drop will handle container cleanup
        }

        // 6. Final cleanup for any missed containers
        if let Err(e) = self.docker_manager.cleanup_all_containers().await {
            tracing::error!("Error during Docker cleanup: {}", e);
        }

        tracing::info!("Graceful shutdown complete");
        Ok(())
    }

    /// Wait for all leased hosts to be returned
    #[tracing::instrument(name = "WaitForRequestsToDrain", skip_all)]
    async fn wait_for_requests_to_drain(&self) {
        let mut check_interval = tokio::time::interval(Duration::from_millis(100));
        let shutdown_timeout = Duration::from_secs(30); // Max wait time
        let start_time = std::time::Instant::now();

        loop {
            check_interval.tick().await;

            // Acquire the state just to see how many leased hosts are left
            let state_guard = self.state.lock().await;
            let leased_count = state_guard.leased_hosts.len();
            drop(state_guard); // Release lock after critical section is completed

            if leased_count == 0 {
                tracing::info!("All requests drained successfully");
                break;
            }

            if start_time.elapsed() > shutdown_timeout {
                tracing::warn!("Shutdown timeout reached with {} requests still in-flight.
                    Some docker instances might not be shut down correctly!", leased_count);
                break;
            }

            tracing::info!("Waiting for {} in-flight requests to complete...", leased_count);
        }
    }

    /// Safely collect all hosts after draining
    async fn collect_all_hosts(&self) -> Vec<HostInstance> {
        let mut state_guard = self.state.lock().await;

        // Pre-allocate with exact capacity to avoid reallocations
        let total_capacity = state_guard.available_hosts.len() + state_guard.leased_hosts.len();
        let mut all_hosts = Vec::with_capacity(total_capacity);

        // Drain both collections
        all_hosts.extend(state_guard.available_hosts.drain(..));
        all_hosts.extend(state_guard.leased_hosts.drain().map(|(_, host)| host));

        drop(state_guard); // Release lock after critical section is completed

        all_hosts
    }
}