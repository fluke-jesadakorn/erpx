use thiserror::Error;

#[derive(Debug, Error)]
pub enum CqrsSqlxError {
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
    #[error("persisted JSON contract is invalid")]
    Serialization(#[from] serde_json::Error),
    #[error("invalid CQRS persistence input: {0}")]
    InvalidInput(&'static str),
    #[error("idempotency key was previously used for a different request")]
    IdempotencyConflict,
    #[error("command id was previously used with a different idempotency key")]
    CommandReplayConflict,
    #[error(
        "aggregate version conflict for {aggregate_type}/{aggregate_id}: expected {expected:?}, actual {actual:?}"
    )]
    OptimisticConcurrency {
        aggregate_type: String,
        aggregate_id: uuid::Uuid,
        expected: Option<u64>,
        actual: Option<u64>,
    },
    #[error("numeric database value is outside the supported range")]
    NumericRange,
    #[error("projection generation is not available")]
    MissingGeneration,
    #[error("projection generation changed before activation")]
    GenerationSwitchConflict,
    #[error("projection position is occupied by a different event")]
    ProjectionPositionConflict,
    #[error("projection event arrived behind its checkpoint")]
    ProjectionOutOfOrder,
    #[error("context command guard rejected the atomic mutation: {0}")]
    GuardRejected(&'static str),
    #[error("CQRS database is missing its schema-generation manifest")]
    MissingSchemaManifest,
    #[error("CQRS database schema generation is unsupported")]
    UnsupportedSchemaGeneration,
    #[error("CQRS database role is not valid for this capability")]
    DatabaseRoleMismatch,
    #[error("CQRS write and read databases do not belong to the same plane")]
    DatabasePlaneMismatch,
    #[error("outbox delivery claim is missing, expired, or owned by another worker")]
    OutboxClaimLost,
    #[error("projection application failed: {0}")]
    Projection(#[from] ProjectionFailure),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct ProjectionFailure {
    message: String,
}

impl ProjectionFailure {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}
