mod docker;
mod data_structure;
mod gateway;
mod pool;
mod port;
mod trace;

use std::ops::Range;
use std::{sync::Arc, time::Duration};
use std::path::PathBuf;

use pool::{HostPoolManager, PoolConfig};
use gateway::start_server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing using your module
    trace::init_tracing()?;

    tracing::info!("Starting Lambda Gateway");

    // Load configuration (using defaults for now, can be made configurable later)
    let gateway_config = GatewayConfig::default();

    // Validate configuration
    gateway_config.validate()?;

    // Create pool configuration
    let pool_config = PoolConfig {
        max_instances: gateway_config.max_instances,
        min_instances: gateway_config.min_instances,
        instance_ttl: gateway_config.instance_ttl,
        reap_check_interval: gateway_config.reap_check_interval,
        host_scaling_check_interval: gateway_config.host_scaling_check_interval,
        lambda_startup_timeout: gateway_config.lambda_startup_timeout,
        binary_path: gateway_config.binary_path.clone(),
        binary_name: gateway_config.binary_name.clone(),
        port_range: gateway_config.port_range.clone(),
        container_limits: gateway_config.container_limits.clone(),
    };

    // Create host pool manager and start background tasks
    let pool_manager = HostPoolManager::new(pool_config);
    tracing::info!("Host pool manager initialized with background tasks started");

    // Setup graceful shutdown
    let shutdown_signal = setup_shutdown_signal();

    // Start the HTTP server
    let bind_addr = format!("{}:{}", gateway_config.host, gateway_config.port);
    tracing::info!("Starting HTTP server on {}", bind_addr);

    let pool_manager_clone = Arc::clone(&pool_manager);
    tokio::select! {
        result = start_server(&bind_addr, pool_manager_clone, gateway_config.request_timeout) => {
            if let Err(e) = result {
                tracing::error!("Server error: {}", e);
                return Err(e.into());
            }
        }
        _ = shutdown_signal => {
            tracing::info!("Shutdown signal received, stopping server...");
        }
    }

    // Cleanup all containers before exiting
    tracing::info!("Cleaning up all containers...");
    if let Err(e) = pool_manager.shutdown_all_containers().await {
        tracing::error!("Error during container cleanup: {}", e);
    } else {
        tracing::info!("All containers cleaned up successfully");
    }

    tracing::info!("Lambda Gateway stopped");
    Ok(())
}

#[derive(Debug, Clone)]
struct GatewayConfig {
    pub host: String,
    pub port: u16,
    pub request_timeout: Duration,
    pub max_instances: usize,
    pub min_instances: usize,
    pub instance_ttl: Duration,
    pub reap_check_interval: Duration,
    pub host_scaling_check_interval: Duration,
    pub lambda_startup_timeout: Duration,
    pub binary_path: PathBuf,
    pub binary_name: String,
    pub port_range: Range<u16>,
    pub container_limits: pool::ContainerLimits,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            request_timeout: Duration::from_secs(8),
            max_instances: 10,
            min_instances: 1,
            instance_ttl: Duration::from_secs(30),
            reap_check_interval: Duration::from_secs(20),
            host_scaling_check_interval: Duration::from_secs(30),
            lambda_startup_timeout: Duration::from_secs(1),
            binary_path: PathBuf::from("./functions"),
            binary_name: "hello-lambda".to_string(),
            port_range: Range {
                start: 8081,
                end: 9000,
            },
            container_limits: pool::ContainerLimits {
                memory_mb: 128,
                cpu_limit: 0.5,
            },
        }
    }
}

impl GatewayConfig {
    fn validate(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.max_instances == 0 {
            return Err("max_instances must be > 0".into());
        }

        if self.min_instances > self.max_instances {
            return Err("min_instances cannot exceed max_instances".into());
        }

        if self.port_range.start >= self.port_range.end {
            return Err("port_range.start must be < port_range.end".into());
        }

        if !self.binary_path.exists() {
            return Err(format!("Binary path does not exist: {:?}", self.binary_path).into());
        }

        let binary_full_path = self.binary_path.join(&self.binary_name);
        if !binary_full_path.exists() {
            return Err(format!("Binary not found: {:?}", binary_full_path).into());
        }

        Ok(())
    }
}

async fn setup_shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Received Ctrl+C");
        },
        _ = terminate => {
            tracing::info!("Received SIGTERM");
        },
    }
}