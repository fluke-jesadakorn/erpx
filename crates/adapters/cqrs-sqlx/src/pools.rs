use std::time::Duration;

use sqlx::{PgPool, postgres::PgPoolOptions};

use crate::CqrsSqlxError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CqrsDatabaseConfig {
    pub write_url: String,
    pub projection_url: String,
    pub read_url: String,
}

impl CqrsDatabaseConfig {
    pub fn validate(&self) -> Result<(), CqrsSqlxError> {
        if self.write_url.trim().is_empty()
            || self.projection_url.trim().is_empty()
            || self.read_url.trim().is_empty()
        {
            return Err(CqrsSqlxError::InvalidInput(
                "write, projection, and read database URLs are required",
            ));
        }
        if self.write_url == self.read_url || self.write_url == self.projection_url {
            return Err(CqrsSqlxError::InvalidInput(
                "command database must be physically separate from the read database",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CqrsPoolSettings {
    pub write_max_connections: u32,
    pub projection_max_connections: u32,
    pub read_max_connections: u32,
    pub acquire_timeout: Duration,
}

impl Default for CqrsPoolSettings {
    fn default() -> Self {
        Self {
            write_max_connections: 20,
            projection_max_connections: 8,
            read_max_connections: 12,
            acquire_timeout: Duration::from_secs(5),
        }
    }
}

/// Capability type for aggregate, receipt, audit, and outbox writes.
#[derive(Debug, Clone)]
pub struct WritePool(PgPool);

/// Capability type used by projectors to update read models.
#[derive(Debug, Clone)]
pub struct ProjectionPool(PgPool);

/// Capability type for queries and read-your-writes checks.
#[derive(Debug, Clone)]
pub struct ReadPool(PgPool);

impl WritePool {
    pub fn from_pg_pool(pool: PgPool) -> Self {
        Self(pool)
    }

    pub(crate) fn inner(&self) -> &PgPool {
        &self.0
    }
}

impl ProjectionPool {
    pub fn from_pg_pool(pool: PgPool) -> Self {
        Self(pool)
    }

    pub(crate) fn inner(&self) -> &PgPool {
        &self.0
    }
}

impl ReadPool {
    pub fn from_pg_pool(pool: PgPool) -> Self {
        Self(pool)
    }

    pub(crate) fn inner(&self) -> &PgPool {
        &self.0
    }

    /// Grants a context read adapter access to its typed projection tables.
    /// The capability cannot be constructed from `WritePool`, preserving the
    /// compile-time boundary that prevents query fallback to aggregate state.
    pub fn as_pg_pool(&self) -> &PgPool {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct CqrsPools {
    write: WritePool,
    projection: ProjectionPool,
    read: ReadPool,
}

impl CqrsPools {
    pub async fn connect(config: &CqrsDatabaseConfig) -> Result<Self, CqrsSqlxError> {
        Self::connect_with_settings(config, CqrsPoolSettings::default()).await
    }

    pub async fn connect_with_settings(
        config: &CqrsDatabaseConfig,
        settings: CqrsPoolSettings,
    ) -> Result<Self, CqrsSqlxError> {
        config.validate()?;
        let write = PgPoolOptions::new()
            .max_connections(settings.write_max_connections)
            .acquire_timeout(settings.acquire_timeout)
            .connect(&config.write_url)
            .await?;
        let projection = PgPoolOptions::new()
            .max_connections(settings.projection_max_connections)
            .acquire_timeout(settings.acquire_timeout)
            .connect(&config.projection_url)
            .await?;
        let read = PgPoolOptions::new()
            .max_connections(settings.read_max_connections)
            .acquire_timeout(settings.acquire_timeout)
            .connect(&config.read_url)
            .await?;
        Ok(Self {
            write: WritePool(write),
            projection: ProjectionPool(projection),
            read: ReadPool(read),
        })
    }

    pub fn write(&self) -> WritePool {
        self.write.clone()
    }

    pub fn projection(&self) -> ProjectionPool {
        self.projection.clone()
    }

    pub fn read(&self) -> ReadPool {
        self.read.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_and_read_databases_must_be_distinct() {
        let config = CqrsDatabaseConfig {
            write_url: "postgres://same".into(),
            projection_url: "postgres://read-writer".into(),
            read_url: "postgres://same".into(),
        };
        assert!(matches!(
            config.validate(),
            Err(CqrsSqlxError::InvalidInput(_))
        ));
    }

    #[test]
    fn projection_writer_may_target_the_read_database() {
        let config = CqrsDatabaseConfig {
            write_url: "postgres://write".into(),
            projection_url: "postgres://read".into(),
            read_url: "postgres://read".into(),
        };
        assert!(config.validate().is_ok());
    }
}
