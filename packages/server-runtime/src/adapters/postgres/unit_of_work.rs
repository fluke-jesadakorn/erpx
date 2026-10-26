use super::{
    PersistenceError,
    pool::{DatabasePool, SessionLease},
};
use sqlx::{Connection, PgConnection};
use std::{future::Future, pin::Pin, time::Duration};

#[derive(Clone, Debug)]
pub struct SessionLock {
    pub namespace: String,
    pub key: String,
}
#[derive(Clone, Debug, Default)]
pub struct TransactionOptions {
    pub session_lock: Option<SessionLock>,
    pub prefer_indexed_reads: bool,
}
#[derive(Debug)]
pub enum PgTransactionError<E> {
    Persistence(PersistenceError),
    Business(E),
}
impl<E> From<sqlx::Error> for PgTransactionError<E> {
    fn from(e: sqlx::Error) -> Self {
        Self::Persistence(e.into())
    }
}
impl<E> From<PersistenceError> for PgTransactionError<E> {
    fn from(e: PersistenceError) -> Self {
        Self::Persistence(e)
    }
}
impl From<erpx_server_shared::public::BusinessError>
    for PgTransactionError<erpx_server_shared::public::BusinessError>
{
    fn from(error: erpx_server_shared::public::BusinessError) -> Self {
        Self::Business(error)
    }
}
pub type PgWorkFuture<'c, T, E> =
    Pin<Box<dyn Future<Output = Result<T, PgTransactionError<E>>> + Send + 'c>>;
#[derive(Clone)]
pub struct PgUnitOfWork {
    pool: DatabasePool,
}
impl PgUnitOfWork {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
    /// The callback may execute four times. External effects belong in a durable outbox.
    /// Returned values cannot borrow this transaction or its leased connection.
    pub async fn run<T: Send + 'static, E: Send + 'static, F>(
        &self,
        options: TransactionOptions,
        mut action: F,
    ) -> Result<T, PgTransactionError<E>>
    where
        F: for<'c> FnMut(&'c mut PgConnection) -> PgWorkFuture<'c, T, E> + Send,
    {
        if options.session_lock.as_ref().is_some_and(|l| {
            l.namespace.is_empty()
                || l.namespace.len() > 100
                || l.key.is_empty()
                || l.key.len() > 200
        }) {
            return Err(PersistenceError::new("INVALID_TRANSACTION_SESSION_LOCK").into());
        }
        for attempt in 0..4 {
            let _turn = match &options.session_lock {
                Some(lock) => Some(self.pool.turn(&lock.namespace, &lock.key).await?),
                None => None,
            };
            let mut lease = SessionLease::acquire(&self.pool).await?;
            if let Some(lock) = &options.session_lock {
                sqlx::query("SELECT pg_advisory_lock(hashtext($1),hashtext($2))")
                    .bind(&lock.namespace)
                    .bind(&lock.key)
                    .execute(lease.connection())
                    .await?;
            }
            let result = async {
                let mut tx = lease
                    .connection()
                    .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
                    .await?;
                if options.prefer_indexed_reads {
                    sqlx::query("SET LOCAL enable_seqscan=off")
                        .execute(&mut *tx)
                        .await?;
                }
                match action(&mut tx).await {
                    Ok(value) => {
                        tx.commit().await?;
                        Ok(value)
                    }
                    Err(error) => {
                        tx.rollback().await?;
                        Err(error)
                    }
                }
            }
            .await;
            let unlocked = match &options.session_lock {
                Some(lock) => {
                    sqlx::query_scalar::<_, bool>(
                        "SELECT pg_advisory_unlock(hashtext($1),hashtext($2))",
                    )
                    .bind(&lock.namespace)
                    .bind(&lock.key)
                    .fetch_one(lease.connection())
                    .await
                }
                None => Ok(true),
            };
            match unlocked {
                Ok(true) => lease.dirty = false,
                _ => return Err(PersistenceError::new("TRANSACTION_SESSION_UNLOCK_FAILED").into()),
            }
            let retry = matches!(&result,Err(PgTransactionError::Persistence(e)) if e.retryable())
                && attempt < 3;
            drop(lease);
            drop(_turn);
            if !retry {
                return result;
            }
            tokio::time::sleep(Duration::from_millis(10 * (1 << attempt))).await;
        }
        unreachable!("the final attempt always returns")
    }
}
