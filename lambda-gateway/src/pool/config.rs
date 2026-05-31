use std::{ops::Range, time::Duration};
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    /// Maximum number of container instances to maintain
    pub max_instances: usize,

    /// Minimum number of container instances to keep warm
    pub min_instances: usize,

    /// How long to keep idle containers alive before terminating them
    pub instance_ttl: Duration,

    /// How often to perform reap on containers
    pub reap_check_interval: Duration,

    /// How often to perform host scaling check on containers
    pub host_scaling_check_interval: Duration,

    /// Timeout for container startup
    pub lambda_startup_timeout: Duration,

    /// Path to the Lambda function binary
    pub binary_path: PathBuf,

    /// Name of the Lambda function binary
    pub binary_name: String,

    /// Port range for container allocation
    pub port_range: Range<u16>,

    /// Container resource limits
    pub container_limits: ContainerLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerLimits {
    /// Memory limit in MB
    pub memory_mb: u64,

    /// CPU limit (1.0 = 1 CPU core)
    pub cpu_limit: f64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_instances: 10,
            min_instances: 1,
            instance_ttl: Duration::from_secs(300), // 5 minutes
            reap_check_interval: Duration::from_secs(30),
            host_scaling_check_interval: Duration::from_secs(60),
            lambda_startup_timeout: Duration::from_secs(30),
            binary_path: PathBuf::from("./functions"),
            binary_name: "hello-lambda".to_string(),
            port_range: Range {
                start: 8000,
                end: 9000,
            },
            container_limits: ContainerLimits {
                memory_mb: 128,
                cpu_limit: 0.5,
            },
        }
    }
}

impl PoolConfig {
    /// Validate the configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_instances == 0 {
            return Err(ConfigError::InvalidValue("max_instances must be > 0".to_string()));
        }

        if self.min_instances > self.max_instances {
            return Err(ConfigError::InvalidValue("min_instances cannot exceed max_instances".to_string()));
        }

        if self.port_range.start >= self.port_range.end {
            return Err(ConfigError::InvalidValue("port_range.start must be < port_range.end".to_string()));
        }

        if !self.binary_path.exists() {
            return Err(ConfigError::InvalidPath(format!("Binary path does not exist: {:?}", self.binary_path)));
        }

        let binary_full_path = self.binary_path.join(&self.binary_name);
        if !binary_full_path.exists() {
            return Err(ConfigError::InvalidPath(format!("Binary not found: {:?}", binary_full_path)));
        }

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Invalid configuration value: {0}")]
    InvalidValue(String),

    #[error("Invalid path: {0}")]
    InvalidPath(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = PoolConfig::default();
        assert_eq!(config.max_instances, 10);
        assert_eq!(config.min_instances, 1);
        assert_eq!(config.port_range.start, 8000);
        assert_eq!(config.port_range.end, 9000);
    }

    #[test]
    fn test_config_validation_max_instances() {
        let mut config = PoolConfig::default();
        config.max_instances = 0;

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validation_min_max_instances() {
        let mut config = PoolConfig::default();
        config.min_instances = 5;
        config.max_instances = 3;

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validation_port_range() {
        let mut config = PoolConfig::default();
        config.port_range.start = 9000;
        config.port_range.end = 8000;

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validation_with_valid_binary() {
        let temp_dir = TempDir::new().unwrap();
        let binary_path = temp_dir.path().to_path_buf();
        let binary_file = binary_path.join("test-binary");

        // Create the binary file
        fs::write(&binary_file, "fake binary").unwrap();

        let config = PoolConfig {
            binary_path,
            binary_name: "test-binary".to_string(),
            ..Default::default()
        };

        assert!(config.validate().is_ok());
    }
}