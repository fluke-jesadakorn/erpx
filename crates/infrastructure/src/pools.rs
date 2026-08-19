use std::time::Duration;

use sqlx::{PgPool, postgres::PgPoolOptions};

#[derive(Debug, Clone)]
pub struct ControlPlaneDatabaseConfig {
    pub write_url: String,
    pub read_url: String,
}

impl ControlPlaneDatabaseConfig {
    pub fn from_env() -> Result<Self, &'static str> {
        let write_url = std::env::var("ERPX_CONTROL_WRITE_DATABASE_URL")
            .map_err(|_| "ERPX_CONTROL_WRITE_DATABASE_URL is required")?;
        let read_url = std::env::var("ERPX_CONTROL_READ_DATABASE_URL")
            .map_err(|_| "ERPX_CONTROL_READ_DATABASE_URL is required")?;
        if write_url == read_url {
            return Err("control write and read database URLs must be distinct");
        }
        Ok(Self {
            write_url,
            read_url,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ControlPlanePoolSettings {
    pub write_max_connections: u32,
    pub read_max_connections: u32,
    pub acquire_timeout: Duration,
}

impl Default for ControlPlanePoolSettings {
    fn default() -> Self {
        Self {
            write_max_connections: 20,
            read_max_connections: 8,
            acquire_timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ControlPlanePools {
    pub write: PgPool,
    pub read: PgPool,
}

impl ControlPlanePools {
    pub async fn connect(config: &ControlPlaneDatabaseConfig) -> Result<Self, sqlx::Error> {
        Self::connect_with_settings(config, ControlPlanePoolSettings::default()).await
    }

    pub async fn connect_with_settings(
        config: &ControlPlaneDatabaseConfig,
        settings: ControlPlanePoolSettings,
    ) -> Result<Self, sqlx::Error> {
        let write = PgPoolOptions::new()
            .max_connections(settings.write_max_connections)
            .acquire_timeout(settings.acquire_timeout)
            .connect(&config.write_url)
            .await?;
        let read = PgPoolOptions::new()
            .max_connections(settings.read_max_connections)
            .acquire_timeout(settings.acquire_timeout)
            .connect(&config.read_url)
            .await?;
        Ok(Self { write, read })
    }

    /// Connect the worker's always-used write pool eagerly while keeping its
    /// control-read pool lazy. ERPX runs one bounded process per worker kind;
    /// eagerly reserving an otherwise-unused read connection in every process
    /// can exhaust the database before holding projection routes are opened.
    /// Handlers that need the read side still establish and validate that
    /// connection on their first query.
    pub async fn connect_worker(
        config: &ControlPlaneDatabaseConfig,
        settings: ControlPlanePoolSettings,
    ) -> Result<Self, sqlx::Error> {
        let write = PgPoolOptions::new()
            .max_connections(settings.write_max_connections)
            .acquire_timeout(settings.acquire_timeout)
            .connect(&config.write_url)
            .await?;
        let read = PgPoolOptions::new()
            .max_connections(settings.read_max_connections)
            .acquire_timeout(settings.acquire_timeout)
            .connect_lazy(&config.read_url)?;
        Ok(Self { write, read })
    }
}
