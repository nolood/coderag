//! Configuration loading and management

use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::Path;

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub jwt: JwtConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub workers: usize,
    pub max_connections: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database_name: String,
    pub max_connections: u32,
    pub connection_timeout: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisConfig {
    pub url: String,
    pub max_connections: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtConfig {
    pub secret: String,
    pub expiration: u64,
    pub refresh_expiration: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub format: LogFormat,
    pub output: LogOutput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogFormat {
    Json,
    Pretty,
    Compact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogOutput {
    Stdout,
    File(String),
}

impl Config {
    /// Load configuration from a file
    pub fn load_config(path: &Path) -> Result<Self, ConfigError> {
        let contents = fs::read_to_string(path)
            .map_err(|e| ConfigError::FileRead(e.to_string()))?;

        let config: Config = toml::from_str(&contents)
            .map_err(|e| ConfigError::ParseError(e.to_string()))?;

        config.validate()?;
        Ok(config)
    }

    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Config {
            server: ServerConfig {
                host: env::var("SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
                port: env::var("SERVER_PORT")
                    .unwrap_or_else(|_| "8080".to_string())
                    .parse()
                    .map_err(|_| ConfigError::InvalidValue("SERVER_PORT".to_string()))?,
                workers: env::var("SERVER_WORKERS")
                    .unwrap_or_else(|_| "4".to_string())
                    .parse()
                    .map_err(|_| ConfigError::InvalidValue("SERVER_WORKERS".to_string()))?,
                max_connections: 1000,
            },
            database: DatabaseConfig {
                host: env::var("DB_HOST")
                    .map_err(|_| ConfigError::MissingEnvVar("DB_HOST".to_string()))?,
                port: env::var("DB_PORT")
                    .unwrap_or_else(|_| "5432".to_string())
                    .parse()
                    .map_err(|_| ConfigError::InvalidValue("DB_PORT".to_string()))?,
                username: env::var("DB_USERNAME")
                    .map_err(|_| ConfigError::MissingEnvVar("DB_USERNAME".to_string()))?,
                password: env::var("DB_PASSWORD")
                    .map_err(|_| ConfigError::MissingEnvVar("DB_PASSWORD".to_string()))?,
                database_name: env::var("DB_NAME")
                    .map_err(|_| ConfigError::MissingEnvVar("DB_NAME".to_string()))?,
                max_connections: 10,
                connection_timeout: 30,
            },
            redis: RedisConfig {
                url: env::var("REDIS_URL")
                    .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string()),
                max_connections: 10,
            },
            jwt: JwtConfig {
                secret: env::var("JWT_SECRET")
                    .map_err(|_| ConfigError::MissingEnvVar("JWT_SECRET".to_string()))?,
                expiration: 3600,
                refresh_expiration: 86400,
            },
            logging: LoggingConfig {
                level: env::var("LOG_LEVEL")
                    .unwrap_or_else(|_| "info".to_string()),
                format: LogFormat::Json,
                output: LogOutput::Stdout,
            },
        })
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.server.port == 0 {
            return Err(ConfigError::InvalidValue("server.port cannot be 0".to_string()));
        }

        if self.server.workers == 0 {
            return Err(ConfigError::InvalidValue("server.workers cannot be 0".to_string()));
        }

        if self.database.max_connections == 0 {
            return Err(ConfigError::InvalidValue("database.max_connections cannot be 0".to_string()));
        }

        if self.jwt.secret.is_empty() {
            return Err(ConfigError::InvalidValue("jwt.secret cannot be empty".to_string()));
        }

        Ok(())
    }

    /// Get database URL
    pub fn database_url(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.database.username,
            self.database.password,
            self.database.host,
            self.database.port,
            self.database.database_name
        )
    }
}

/// Settings builder pattern
pub struct ConfigBuilder {
    config: Config,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: Config::default(),
        }
    }

    pub fn server(mut self, config: ServerConfig) -> Self {
        self.config.server = config;
        self
    }

    pub fn database(mut self, config: DatabaseConfig) -> Self {
        self.config.database = config;
        self
    }

    pub fn redis(mut self, config: RedisConfig) -> Self {
        self.config.redis = config;
        self
    }

    pub fn jwt(mut self, config: JwtConfig) -> Self {
        self.config.jwt = config;
        self
    }

    pub fn logging(mut self, config: LoggingConfig) -> Self {
        self.config.logging = config;
        self
    }

    pub fn build(self) -> Result<Config, ConfigError> {
        self.config.validate()?;
        Ok(self.config)
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 8080,
                workers: 4,
                max_connections: 1000,
            },
            database: DatabaseConfig {
                host: "localhost".to_string(),
                port: 5432,
                username: "postgres".to_string(),
                password: "password".to_string(),
                database_name: "myapp".to_string(),
                max_connections: 10,
                connection_timeout: 30,
            },
            redis: RedisConfig {
                url: "redis://127.0.0.1:6379".to_string(),
                max_connections: 10,
            },
            jwt: JwtConfig {
                secret: "secret".to_string(),
                expiration: 3600,
                refresh_expiration: 86400,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: LogFormat::Json,
                output: LogOutput::Stdout,
            },
        }
    }
}

#[derive(Debug)]
pub enum ConfigError {
    FileRead(String),
    ParseError(String),
    MissingEnvVar(String),
    InvalidValue(String),
}