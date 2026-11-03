//! Explicit database selections shared by the private API and Files workers.
//! A selection is either a disposable leased development database or a durable
//! private production database. Both expose the same bounded pool operations.
use erpx_server_runtime::public::postgres::{
    DatabaseConfig, DatabasePool, LeasedDevelopmentDatabase, PersistenceError, PoolLimits,
    connect_pool,
};
use std::fmt;

#[derive(Clone)]
pub(crate) enum RuntimeDatabase {
    Leased(LeasedDevelopmentDatabase),
    #[allow(dead_code)]
    Durable {
        configuration: DatabaseConfig,
        identity: (String, String),
    },
}
impl fmt::Debug for RuntimeDatabase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RuntimeDatabase([REDACTED])")
    }
}
impl RuntimeDatabase {
    pub(crate) async fn connect(&self, limits: PoolLimits) -> Result<DatabasePool, PersistenceError> {
        match self {
            Self::Leased(database) => database.connect(limits).await,
            Self::Durable { configuration, .. } => connect_pool(configuration, limits).await,
        }
    }
    #[allow(dead_code)]
    pub(crate) async fn connect_named(
        &self,
        limits: PoolLimits,
        application: &str,
    ) -> Result<DatabasePool, PersistenceError> {
        match self {
            Self::Leased(database) => database.connect_named(limits, application).await,
            Self::Durable { configuration, .. } => {
                connect_pool(&configuration.with_application_name(application)?, limits).await
            }
        }
    }
    /// Durable databases expose the exact configured database/role identity.
    /// Leased development databases keep their disposable lease validation.
    pub(crate) fn durable_identity(&self) -> Option<&(String, String)> {
        match self {
            Self::Leased(_) => None,
            Self::Durable { identity, .. } => Some(identity),
        }
    }
}
