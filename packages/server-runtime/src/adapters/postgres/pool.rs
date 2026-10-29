use super::{PersistenceError, config::DatabaseConfig};
use sqlx::{PgPool, Postgres, pool::PoolConnection, postgres::PgPoolOptions};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

#[derive(Clone, Copy, Debug)]
pub struct PoolLimits {
    pub max_connections: u32,
    pub acquire_timeout: Duration,
}
impl Default for PoolLimits {
    fn default() -> Self {
        Self {
            max_connections: 5,
            acquire_timeout: Duration::from_secs(10),
        }
    }
}
type GateMap = HashMap<(String, String), Weak<AsyncMutex<()>>>;
#[derive(Clone)]
pub struct DatabasePool {
    raw: PgPool,
    gates: Arc<Mutex<GateMap>>,
}
impl DatabasePool {
    pub fn raw(&self) -> &PgPool {
        &self.raw
    }
    pub async fn close(&self) {
        self.raw.close().await;
    }
    pub(crate) async fn turn(
        &self,
        namespace: &str,
        key: &str,
    ) -> Result<OwnedMutexGuard<()>, PersistenceError> {
        let gate = {
            let mut gates = self
                .gates
                .lock()
                .map_err(|_| PersistenceError::new("TRANSACTION_LOCK_UNAVAILABLE"))?;
            gates.retain(|_, gate| gate.strong_count() != 0);
            let slot = gates
                .entry((namespace.to_owned(), key.to_owned()))
                .or_default();
            if let Some(gate) = slot.upgrade() {
                gate
            } else {
                let gate = Arc::new(AsyncMutex::new(()));
                *slot = Arc::downgrade(&gate);
                gate
            }
        };
        tokio::time::timeout(Duration::from_secs(10), gate.lock_owned())
            .await
            .map_err(|_| PersistenceError::new("TRANSACTION_LOCK_TIMEOUT"))
    }
}
pub async fn connect_pool(
    config: &DatabaseConfig,
    limits: PoolLimits,
) -> Result<DatabasePool, PersistenceError> {
    if limits.max_connections == 0
        || limits.max_connections > 64
        || limits.acquire_timeout.is_zero()
    {
        return Err(PersistenceError::new("INVALID_POOL_LIMITS"));
    }
    let raw = PgPoolOptions::new()
        .max_connections(limits.max_connections)
        .acquire_timeout(limits.acquire_timeout)
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::raw_sql(
                    "SET TIME ZONE 'UTC'; SET statement_timeout = 30000; SET lock_timeout = 10000",
                )
                .execute(connection)
                .await?;
                Ok(())
            })
        })
        .connect_with(config.options.clone())
        .await?;
    Ok(DatabasePool {
        raw,
        gates: Arc::new(Mutex::new(HashMap::new())),
    })
}
/// A cancelled query may have acquired a session lock on the server already.
/// Discard until the complete transaction and explicit unlock have been observed.
pub(crate) struct SessionLease {
    connection: PoolConnection<Postgres>,
    pub dirty: bool,
}
impl SessionLease {
    pub async fn acquire(pool: &DatabasePool) -> Result<Self, PersistenceError> {
        Ok(Self {
            connection: pool.raw.acquire().await?,
            dirty: true,
        })
    }
    pub fn connection(&mut self) -> &mut sqlx::PgConnection {
        &mut self.connection
    }
}
impl Drop for SessionLease {
    fn drop(&mut self) {
        if self.dirty {
            self.connection.close_on_drop();
        }
    }
}
