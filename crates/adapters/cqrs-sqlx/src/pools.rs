use std::time::Duration;

use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgPoolOptions};
use uuid::Uuid;

use crate::CqrsSqlxError;

pub const REQUIRED_SCHEMA_REVISION: i16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabasePlane {
    Control,
    Holding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseRole {
    ControlWrite,
    ControlRead,
    HoldingWrite,
    HoldingRead,
}

impl DatabaseRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ControlWrite => "control_write",
            Self::ControlRead => "control_read",
            Self::HoldingWrite => "holding_write",
            Self::HoldingRead => "holding_read",
        }
    }

    pub const fn plane(self) -> DatabasePlane {
        match self {
            Self::ControlWrite | Self::ControlRead => DatabasePlane::Control,
            Self::HoldingWrite | Self::HoldingRead => DatabasePlane::Holding,
        }
    }

    const fn is_write(self) -> bool {
        matches!(self, Self::ControlWrite | Self::HoldingWrite)
    }

    const fn read_role(self) -> Option<Self> {
        match self {
            Self::ControlWrite => Some(Self::ControlRead),
            Self::HoldingWrite => Some(Self::HoldingRead),
            Self::ControlRead | Self::HoldingRead => None,
        }
    }

    fn parse(value: &str) -> Result<Self, CqrsSqlxError> {
        match value {
            "control_write" => Ok(Self::ControlWrite),
            "control_read" => Ok(Self::ControlRead),
            "holding_write" => Ok(Self::HoldingWrite),
            "holding_read" => Ok(Self::HoldingRead),
            _ => Err(CqrsSqlxError::DatabaseRoleMismatch),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseManifest {
    pub schema_revision: i16,
    pub role: DatabaseRole,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CqrsDatabaseConfig {
    pub write_url: String,
    pub projection_url: String,
    pub read_url: String,
    /// Required when the manifest identifies a holding data plane. Control
    /// planes must leave this absent so a control pool cannot accidentally
    /// acquire holding authority.
    pub holding_id: Option<Uuid>,
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
        if self
            .holding_id
            .is_some_and(|holding_id| holding_id.is_nil())
        {
            return Err(CqrsSqlxError::InvalidInput(
                "holding database binding must not be nil",
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PoolAuthority {
    Control,
    Holding(Uuid),
}

#[derive(Debug, Clone)]
pub struct WritePool {
    pool: PgPool,
    authority: PoolAuthority,
}

/// Capability type used by projectors to update read models.
#[derive(Debug, Clone)]
pub struct ProjectionPool {
    pool: PgPool,
    authority: PoolAuthority,
}

/// Capability type for queries and read-your-writes checks.
#[derive(Debug, Clone)]
pub struct ReadPool {
    pool: PgPool,
    authority: PoolAuthority,
}

impl WritePool {
    pub fn from_control_pg_pool(pool: PgPool) -> Self {
        Self {
            pool,
            authority: PoolAuthority::Control,
        }
    }

    pub fn for_holding(pool: PgPool, holding_id: Uuid) -> Result<Self, CqrsSqlxError> {
        Ok(Self {
            pool,
            authority: holding_authority(holding_id)?,
        })
    }

    pub(crate) const fn holding_id(&self) -> Option<Uuid> {
        match self.authority {
            PoolAuthority::Control => None,
            PoolAuthority::Holding(holding_id) => Some(holding_id),
        }
    }

    /// Begin a command transaction with the immutable holding authority bound
    /// and verified before any caller-owned context guard executes.
    pub async fn begin(&self) -> Result<Transaction<'_, Postgres>, CqrsSqlxError> {
        begin_bound(&self.pool, self.authority).await
    }
}

impl ProjectionPool {
    pub fn from_control_pg_pool(pool: PgPool) -> Self {
        Self {
            pool,
            authority: PoolAuthority::Control,
        }
    }

    pub fn for_holding(pool: PgPool, holding_id: Uuid) -> Result<Self, CqrsSqlxError> {
        Ok(Self {
            pool,
            authority: holding_authority(holding_id)?,
        })
    }

    pub(crate) const fn holding_id(&self) -> Option<Uuid> {
        match self.authority {
            PoolAuthority::Control => None,
            PoolAuthority::Holding(holding_id) => Some(holding_id),
        }
    }

    /// Begin a projection transaction with the immutable holding authority
    /// bound and verified before any projector-owned statement executes.
    pub async fn begin(&self) -> Result<Transaction<'_, Postgres>, CqrsSqlxError> {
        begin_bound(&self.pool, self.authority).await
    }
}

impl ReadPool {
    pub fn from_control_pg_pool(pool: PgPool) -> Self {
        Self {
            pool,
            authority: PoolAuthority::Control,
        }
    }

    pub fn for_holding(pool: PgPool, holding_id: Uuid) -> Result<Self, CqrsSqlxError> {
        Ok(Self {
            pool,
            authority: holding_authority(holding_id)?,
        })
    }

    pub(crate) const fn holding_id(&self) -> Option<Uuid> {
        match self.authority {
            PoolAuthority::Control => None,
            PoolAuthority::Holding(holding_id) => Some(holding_id),
        }
    }

    /// Begin a read transaction that is bound to this immutable pool
    /// authority. Holding adapters must execute through this boundary so the
    /// transaction-local holding setting is installed and read back before the
    /// first projection query.
    pub async fn begin(&self) -> Result<Transaction<'_, Postgres>, CqrsSqlxError> {
        begin_bound(&self.pool, self.authority).await
    }
}

fn holding_authority(holding_id: Uuid) -> Result<PoolAuthority, CqrsSqlxError> {
    if holding_id.is_nil() {
        return Err(CqrsSqlxError::InvalidInput(
            "holding database binding must not be nil",
        ));
    }
    Ok(PoolAuthority::Holding(holding_id))
}

async fn begin_bound(
    pool: &PgPool,
    authority: PoolAuthority,
) -> Result<Transaction<'_, Postgres>, CqrsSqlxError> {
    let mut transaction = pool.begin().await?;
    if let PoolAuthority::Holding(holding_id) = authority {
        let expected = holding_id.to_string();
        let actual: String = sqlx::query_scalar("SELECT set_config('app.holding_id',$1,true)")
            .bind(&expected)
            .fetch_one(&mut *transaction)
            .await?;
        if actual != expected {
            return Err(CqrsSqlxError::DatabasePlaneMismatch);
        }
    }
    Ok(transaction)
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
        let write_manifest = verify_manifest(&write).await?;
        if !write_manifest.role.is_write() {
            return Err(CqrsSqlxError::DatabaseRoleMismatch);
        }
        let expected_read_role = write_manifest
            .role
            .read_role()
            .ok_or(CqrsSqlxError::DatabaseRoleMismatch)?;
        let projection_manifest = verify_manifest(&projection).await?;
        let read_manifest = verify_manifest(&read).await?;
        if projection_manifest.role != expected_read_role
            || read_manifest.role != expected_read_role
        {
            return Err(CqrsSqlxError::DatabasePlaneMismatch);
        }
        let (write, projection, read) = match write_manifest.role.plane() {
            DatabasePlane::Control => {
                if config.holding_id.is_some() {
                    return Err(CqrsSqlxError::DatabasePlaneMismatch);
                }
                (
                    WritePool::from_control_pg_pool(write),
                    ProjectionPool::from_control_pg_pool(projection),
                    ReadPool::from_control_pg_pool(read),
                )
            }
            DatabasePlane::Holding => {
                let holding_id = config
                    .holding_id
                    .ok_or(CqrsSqlxError::DatabasePlaneMismatch)?;
                (
                    WritePool::for_holding(write, holding_id)?,
                    ProjectionPool::for_holding(projection, holding_id)?,
                    ReadPool::for_holding(read, holding_id)?,
                )
            }
        };
        Ok(Self {
            write,
            projection,
            read,
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

/// Read and validate the non-secret schema-generation identity before a raw
/// pool is wrapped in a CQRS capability. Composition roots that must bridge
/// legacy `PgPool` ownership can call this before `from_pg_pool`.
pub async fn verify_database_role(
    pool: &PgPool,
    expected: DatabaseRole,
) -> Result<DatabaseManifest, CqrsSqlxError> {
    let manifest = verify_manifest(pool).await?;
    if manifest.role != expected {
        return Err(CqrsSqlxError::DatabaseRoleMismatch);
    }
    Ok(manifest)
}

async fn verify_manifest(pool: &PgPool) -> Result<DatabaseManifest, CqrsSqlxError> {
    let row = sqlx::query(
        "SELECT schema_revision, database_role
         FROM erpx_meta.schema_manifest WHERE singleton = TRUE",
    )
    .fetch_optional(pool)
    .await
    .map_err(|error| match error {
        sqlx::Error::Database(database)
            if database.code().as_deref() == Some("42P01")
                || database.code().as_deref() == Some("3F000") =>
        {
            CqrsSqlxError::MissingSchemaManifest
        }
        other => CqrsSqlxError::Database(other),
    })?
    .ok_or(CqrsSqlxError::MissingSchemaManifest)?;
    let generation: i16 = row.try_get("schema_revision")?;
    if generation != REQUIRED_SCHEMA_REVISION {
        return Err(CqrsSqlxError::UnsupportedSchemaRevision);
    }
    let role: String = row.try_get("database_role")?;
    Ok(DatabaseManifest {
        schema_revision: generation,
        role: DatabaseRole::parse(&role)?,
    })
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
            holding_id: None,
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
            holding_id: None,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn database_roles_form_two_closed_write_read_planes() {
        for (write, read, plane) in [
            (
                DatabaseRole::ControlWrite,
                DatabaseRole::ControlRead,
                DatabasePlane::Control,
            ),
            (
                DatabaseRole::HoldingWrite,
                DatabaseRole::HoldingRead,
                DatabasePlane::Holding,
            ),
        ] {
            assert!(write.is_write());
            assert!(!read.is_write());
            assert_eq!(write.read_role(), Some(read));
            assert_eq!(write.plane(), plane);
            assert_eq!(read.plane(), plane);
            assert_eq!(DatabaseRole::parse(write.as_str()).unwrap(), write);
            assert_eq!(DatabaseRole::parse(read.as_str()).unwrap(), read);
        }
        assert!(DatabaseRole::parse("legacy_write").is_err());
    }

    #[tokio::test]
    async fn holding_capabilities_reject_nil_authority_without_connecting() {
        let pool = PgPool::connect_lazy("postgres://localhost/holding").unwrap();
        assert!(WritePool::for_holding(pool.clone(), Uuid::nil()).is_err());
        assert!(ProjectionPool::for_holding(pool.clone(), Uuid::nil()).is_err());
        assert!(ReadPool::for_holding(pool, Uuid::nil()).is_err());
    }
}
