use super::{
    pool::{DatabasePool, SessionLease},
    unit_of_work::{PgTransactionError, PgWorkFuture},
};
use sqlx::{Connection, PgConnection};

/// Retain Control authorization locks until an inner Holding unit of work finishes.
/// This callback is NEVER replayed: a Holding commit cannot be undone or repeated
/// because a later Control commit failed. Only the inner Holding unit retries.
/// Composition supplies read/authorization operations; Control mutations use their own UoW.
#[derive(Clone)]
pub struct PgRequestLease {
    pool: DatabasePool,
}
impl PgRequestLease {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
    pub async fn run<T: Send + 'static, E: Send + 'static, F>(
        &self,
        action: F,
    ) -> Result<T, PgTransactionError<E>>
    where
        F: for<'c> FnOnce(&'c mut PgConnection) -> PgWorkFuture<'c, T, E> + Send,
    {
        let mut lease = SessionLease::acquire(&self.pool).await?;
        let mut transaction = lease
            .connection()
            .begin_with("BEGIN ISOLATION LEVEL READ COMMITTED")
            .await?;
        let result = match action(&mut transaction).await {
            Ok(value) => {
                transaction.commit().await?;
                Ok(value)
            }
            Err(error) => {
                transaction.rollback().await?;
                Err(error)
            }
        };
        lease.dirty = false;
        result
    }
}
