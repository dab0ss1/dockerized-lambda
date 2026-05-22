use bollard::{
    Docker,
    models::{ContainerCreateBody, ContainerCreateResponse, HostConfig, PortBinding, RestartPolicy, RestartPolicyNameEnum},
    query_parameters::{CreateContainerOptionsBuilder}
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::docker::DockerError;

// — Networking —
const LOCALHOST: &str = "127.0.0.1";
const PROTOCOL_TCP: &str = "tcp";

// — Image —
const CONTAINER_IMAGE: &str = "alpine:latest";

// — Container Naming —
const CONTAINER_NAME_PREFIX: &str = "lambda";
const CONTAINER_RANDOM_ID_LENGTH: usize = 12;

// — Container Filesystem —
const CONTAINER_WORKING_DIR: &str = "/app";
const CONTAINER_BINARY_MOUNT_PATH: &str = "/app/lambda";
const CONTAINER_BINARY_RO_MOUNT_SUFFIX: &str = ":ro";

// — Environment Variables —
const ENV_LAMBDA_PORT_KEY: &str = "LAMBDA_PORT";
const ENV_RUST_LOG: &str = "RUST_LOG=info";

// — Labels —
const LABEL_MANAGED_BY_KEY: &str = "managed-by";
const LABEL_MANAGED_BY_VALUE: &str = "lambda-gateway";
const LABEL_FUNCTION_NAME_KEY: &str = "function-name";
const LABEL_PORT_KEY: &str = "port";

// — Filtering —
const FILTER_KEY_LABEL: &str = "label";
const FILTER_MANAGED_BY_LABEL: &str = "managed-by=lambda-gateway";

// — Health Check —
const HEALTH_CHECK_PATH: &str = "health";
const HEALTH_CHECK_TIMEOUT: u64 = 5;

#[derive(Debug)]
pub struct DockerManager {
    client: Docker,
    config: DockerConfig,
}

#[derive(Debug, Clone)]
pub struct DockerConfig {
    pub binary_path: PathBuf,
    pub binary_name: String,
    // Memory limit in MB
    pub memory_limit_mb: u64,
    // CPU limit (1.0 = 1 CPU core)
    pub cpu_limit: f64,
}

impl DockerConfig {
    /// Get the full path to the binary
    pub fn binary_full_path(&self) -> PathBuf {
        self.binary_path.join(&self.binary_name)
    }
}

impl DockerManager {
    pub fn new(config: DockerConfig) -> Result<Self, DockerError> {
        // Validate binary exists
        let binary_path = config.binary_full_path();
        if !binary_path.exists() {
            return Err(DockerError::BinaryNotFound(binary_path));
        }

        let client = Docker::connect_with_local_defaults()?;
        Ok(Self { client, config })
    }

    #[tracing::instrument(name = "CreatContainer", skip_all)]
    pub async fn create_container(&self, port: u16) -> Result<String, DockerError> {
        // Generate random container name
        let random_id: String = (0..CONTAINER_RANDOM_ID_LENGTH)
            .map(|_| fastrand::alphanumeric())
            .collect();
        let container_name = format!("{}-{}-{}-{}", CONTAINER_NAME_PREFIX, self.config.binary_name, port, random_id);
        let binary_path = self.config.binary_full_path();

        let absolute_binary_path = binary_path.canonicalize()
            .map_err(|_| DockerError::BinaryNotFound(binary_path.clone()))?;

        let host_config = HostConfig {
            // Map container port to host port
            port_bindings: Some({
                let mut bindings = std::collections::HashMap::new();
                bindings.insert(
                    format!("{}/{}", port, PROTOCOL_TCP),
                    Some(vec![PortBinding {
                        host_ip: Some(LOCALHOST.to_string()),
                        host_port: Some(port.to_string()),
                    }])
                );
                bindings
            }),
            // Resource limits
            memory: Some((self.config.memory_limit_mb * 1024 * 1024) as i64),
            nano_cpus: Some((self.config.cpu_limit * 1_000_000_000.0) as i64),
            // Auto-remove container when it stops
            auto_remove: Some(true),
            // Don't restart on failure
            restart_policy: Some(RestartPolicy {
                name: Some(RestartPolicyNameEnum::NO),
                ..Default::default()
            }),
            // Mount binary into container
            binds: Some(vec![
                format!("{}:{}{}", absolute_binary_path.to_string_lossy(), CONTAINER_BINARY_MOUNT_PATH, CONTAINER_BINARY_RO_MOUNT_SUFFIX)
            ]),
            ..Default::default()
        };

        let create_body = ContainerCreateBody {
            // Base Docker image to run the container from
            image: Some(CONTAINER_IMAGE.to_string()),
            // Command to run inside container (overridden by entrypoint below)
            cmd: Some(vec![
                absolute_binary_path.to_string_lossy().to_string() // Path to our Lambda binary
            ]),
            // Environment variables inside the container
            env: Some(vec![
                format!("{}={}", ENV_LAMBDA_PORT_KEY, port),    // Tell Lambda what port to bind to
                ENV_RUST_LOG.to_string(),        // Set logging level
            ]),
            // Declare which ports the container will use (for Docker metadata)
            exposed_ports: Some(vec![
                format!("{}/{}", port, PROTOCOL_TCP)
            ]),
            // Set working directory inside container
            working_dir: Some(CONTAINER_WORKING_DIR.to_string()),
            // Override the default command - run our mounted binary
            entrypoint: Some(vec![CONTAINER_BINARY_MOUNT_PATH.to_string()]),
            // Metadata labels for container identification and management
            labels: Some({
                let mut labels = HashMap::new();
                labels.insert(LABEL_MANAGED_BY_KEY.to_string(), LABEL_MANAGED_BY_VALUE.to_string());     // Who created this
                labels.insert(LABEL_FUNCTION_NAME_KEY.to_string(), self.config.binary_name.clone()); // What function
                labels.insert(LABEL_PORT_KEY.to_string(), port.to_string());                      // What port
                labels
            }),
            host_config: Some(host_config),
            ..Default::default()
        };

        tracing::info!("Calling Docker API to create container...");
        let response: ContainerCreateResponse = self.client
            .create_container(
                Some(CreateContainerOptionsBuilder::default().name(&container_name).build()),
                create_body,
            )
            .await?;
        tracing::info!("Container created with ID: {}", response.id);

        tracing::info!("Starting container {}...", response.id);
        self.client
            .start_container(&response.id, None)
            .await?;
        tracing::info!("Container {} started successfully", response.id);

        tracing::info!(
            container_id = %response.id,
            port = port,
            binary = %self.config.binary_name,
            "Created and started container"
        );

        // Check if container is actually running
        match self.client.inspect_container(&response.id, None).await {
            Ok(info) => {
                tracing::info!("Container {} state: {:?}", response.id, info.state);
            }
            Err(e) => {
                tracing::error!("Failed to inspect container {}: {}", response.id, e);
            }
        }

        Ok(response.id)
    }

    #[tracing::instrument(name = "StopContainer", skip_all)]
    pub async fn stop_container(&self, container_id: &str) -> Result<(), DockerError> {
        // Check if container exists first
        match self.client.inspect_container(container_id, None).await {
            Ok(_) => {
                // Stop container (will be auto-removed due to auto_remove: true)
                self.client
                    .stop_container(container_id, None)
                    .await?;

                tracing::info!(container_id = %container_id, "Stopped container");
                Ok(())
            }
            Err(bollard::errors::Error::DockerResponseServerError { status_code: 404, .. }) => {
                // Container already gone
                tracing::debug!(container_id = %container_id, "Container already removed");
                Ok(())
            }
            Err(e) => Err(DockerError::Api(e)),
        }
    }

    #[tracing::instrument(name = "HealthCheck", skip_all)]
    pub async fn health_check(&self, port: u16) -> Result<bool, DockerError> {
        let client = reqwest::Client::new();
        let url = format!("http://{LOCALHOST}:{port}/{HEALTH_CHECK_PATH}");

        match tokio::time::timeout(
            Duration::from_secs(HEALTH_CHECK_TIMEOUT),
            client.get(&url).send()
        ).await {
            Ok(Ok(response)) => {
                let is_healthy = response.status().is_success();
                tracing::debug!(port = port, healthy = is_healthy, "Health check completed");
                Ok(is_healthy)
            }
            Ok(Err(e)) => {
                tracing::debug!(port = port, error = %e, "Health check failed - connection error");
                Ok(false)
            }
            Err(_) => {
                tracing::debug!(port = port, "Health check failed - timeout");
                Ok(false)
            }
        }
    }

    /// Get container information
    #[tracing::instrument(name = "GetContainerInfo", skip_all)]
    pub async fn get_container_info(&self, container_id: &str) -> Result<bollard::models::ContainerInspectResponse, DockerError> {
        self.client
            .inspect_container(container_id, None)
            .await
            .map_err(DockerError::Api)
    }

    #[tracing::instrument(name = "ListManagedContainers", skip_all)]
    pub async fn list_managed_containers(&self) -> Result<Vec<bollard::models::ContainerSummary>, DockerError> {
        use bollard::query_parameters::ListContainersOptions;
        use std::collections::HashMap;

        let options = Some(ListContainersOptions {
            all: true,
            filters: {
                let mut filters = HashMap::new();
                filters.insert(
                    FILTER_KEY_LABEL.to_string(),
                    vec![FILTER_MANAGED_BY_LABEL.to_string()]
                );
                Some(filters)
            },
            ..Default::default()
        });

        self.client
            .list_containers(options)
            .await
            .map_err(DockerError::Api)
    }

    /// Force cleanup of all managed containers (for shutdown)
    #[tracing::instrument(name = "CleanupAllContainers", skip_all)]
    pub async fn cleanup_all_containers(&self) -> Result<(), DockerError> {
        let containers = self.list_managed_containers().await?;

        for container in containers {
            if let Some(id) = container.id {
                tracing::info!(container_id = %id, "Force stopping container during cleanup");
                let _ = self.stop_container(&id).await; // Ignore errors during cleanup
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::docker::DockerError;

    use super::*;

    /// This test requires Docker to be running on the local machine
    // #[tokio::test]
    // async fn test_docker_manager_creation() {
    //     use tempfile::TempDir;
    //     use std::fs;
    //
    //     let temp_dir = TempDir::new().unwrap();
    //     let binary_path = temp_dir.path().to_path_buf();
    //     let binary_file = binary_path.join("test-lambda");

    //     // Create fake binary
    //     fs::write(&binary_file, "fake binary").unwrap();

    //     let config = DockerConfig {
    //         binary_path,
    //         binary_name: "test-lambda".to_string(),
    //         memory_limit_mb: 128,
    //         cpu_limit: 0.5,
    //     };

    //     let docker_manager = DockerManager::new(config);
    //     assert!(docker_manager.is_ok());
    // }

    #[tokio::test]
    async fn test_docker_manager_missing_binary() {
        let config = DockerConfig {
            binary_path: PathBuf::from("/nonexistent"),
            binary_name: "missing-binary".to_string(),
            memory_limit_mb: 128,
            cpu_limit: 0.5,
        };

        let docker_manager = DockerManager::new(config);
        assert!(docker_manager.is_err());
        assert!(matches!(docker_manager.unwrap_err(), DockerError::BinaryNotFound(_)));
    }
}