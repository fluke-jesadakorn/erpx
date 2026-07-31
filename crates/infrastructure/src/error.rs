use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error(
        "optimistic concurrency conflict for {aggregate_type}/{aggregate_id}: expected {expected:?}, actual {actual:?}"
    )]
    OptimisticConcurrency {
        aggregate_type: String,
        aggregate_id: Uuid,
        expected: Option<u64>,
        actual: Option<u64>,
    },
    #[error("idempotency key {key} was already used with a different request")]
    IdempotencyConflict { key: String },
    #[error("persisted {entity} {id} was not found")]
    NotFound { entity: &'static str, id: Uuid },
    #[error("worker {worker_id} does not own outbox event {event_id}")]
    ClaimLost { worker_id: Uuid, event_id: Uuid },
    #[error("invalid persisted value: {0}")]
    InvalidData(String),
    #[error("invalid persistence configuration: {0}")]
    Configuration(&'static str),
}
