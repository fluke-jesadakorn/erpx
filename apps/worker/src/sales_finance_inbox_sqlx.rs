//! Durable Finance-owned inbox and checkpoint for Sales integration facts.

use erpx_cqrs_sqlx::WritePool;
use erpx_kernel::EventId;
use sqlx::Row;
use thiserror::Error;

use crate::SalesFinanceHandoff;

pub const SALES_FINANCE_CONSUMER_NAME: &str = "finance.sales-facts.v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DurableSalesFinanceOutcome {
    Staged(SalesFinanceHandoff),
    Replay(SalesFinanceHandoff),
    Ignored,
}

#[derive(Debug, Clone)]
pub struct SqlxSalesFinanceInbox {
    pool: WritePool,
}

impl SqlxSalesFinanceInbox {
    pub fn new(pool: WritePool) -> Self {
        Self { pool }
    }

    /// Records a source event and advances its checkpoint atomically. A
    /// duplicate source event returns the exact stored handoff without moving
    /// the checkpoint or creating another Finance command.
    pub async fn record_processed(
        &self,
        global_sequence: u64,
        source_event_id: EventId,
        handoff: Option<&SalesFinanceHandoff>,
    ) -> Result<DurableSalesFinanceOutcome, SalesFinanceInboxError> {
        if global_sequence == 0
            || handoff.is_some_and(|value| value.source_event_id != source_event_id)
        {
            return Err(SalesFinanceInboxError::InvalidInput);
        }
        let sequence =
            i64::try_from(global_sequence).map_err(|_| SalesFinanceInboxError::InvalidInput)?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| SalesFinanceInboxError::Unavailable)?;

        let existing = sqlx::query(
            "SELECT handoff_payload FROM erpx_finance_write.sales_fact_inbox
             WHERE consumer_name=$1 AND source_event_id=$2",
        )
        .bind(SALES_FINANCE_CONSUMER_NAME)
        .bind(source_event_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| SalesFinanceInboxError::Unavailable)?;
        if let Some(row) = existing {
            let stored: SalesFinanceHandoff = serde_json::from_value(
                row.try_get("handoff_payload")
                    .map_err(|_| SalesFinanceInboxError::CorruptState)?,
            )
            .map_err(|_| SalesFinanceInboxError::CorruptState)?;
            if handoff.is_some_and(|expected| expected != &stored) {
                return Err(SalesFinanceInboxError::DuplicateConflict);
            }
            tx.commit()
                .await
                .map_err(|_| SalesFinanceInboxError::Unavailable)?;
            return Ok(DurableSalesFinanceOutcome::Replay(stored));
        }

        let checkpoint: Option<i64> = sqlx::query_scalar(
            "SELECT last_global_sequence FROM erpx_finance_write.sales_fact_checkpoints
             WHERE consumer_name=$1 FOR UPDATE",
        )
        .bind(SALES_FINANCE_CONSUMER_NAME)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| SalesFinanceInboxError::Unavailable)?;
        if checkpoint.is_some_and(|value| sequence <= value) {
            return Err(SalesFinanceInboxError::CheckpointRegression);
        }

        if let Some(handoff) = handoff {
            let payload =
                serde_json::to_value(handoff).map_err(|_| SalesFinanceInboxError::InvalidInput)?;
            let handoff_id = handoff_id(handoff);
            sqlx::query(
                "INSERT INTO erpx_finance_write.sales_fact_inbox
                    (consumer_name,source_event_id,source_event_type,source_aggregate_version,
                     global_sequence,handoff_id,handoff_payload,inbox_status,attempts,updated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,'staged',0,now())",
            )
            .bind(SALES_FINANCE_CONSUMER_NAME)
            .bind(source_event_id.as_uuid())
            .bind(&handoff.source_event_type)
            .bind(
                i64::try_from(handoff.source_aggregate_version)
                    .map_err(|_| SalesFinanceInboxError::InvalidInput)?,
            )
            .bind(sequence)
            .bind(handoff_id)
            .bind(payload)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                if error
                    .as_database_error()
                    .is_some_and(|value| value.code().as_deref() == Some("23505"))
                {
                    SalesFinanceInboxError::DuplicateConflict
                } else {
                    SalesFinanceInboxError::Unavailable
                }
            })?;
        }

        sqlx::query(
            "INSERT INTO erpx_finance_write.sales_fact_checkpoints
                (consumer_name,last_global_sequence,last_source_event_id,updated_at)
             VALUES ($1,$2,$3,now())
             ON CONFLICT (tenant_id,consumer_name) DO UPDATE SET
                last_global_sequence=EXCLUDED.last_global_sequence,
                last_source_event_id=EXCLUDED.last_source_event_id,
                updated_at=EXCLUDED.updated_at",
        )
        .bind(SALES_FINANCE_CONSUMER_NAME)
        .bind(sequence)
        .bind(source_event_id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(|_| SalesFinanceInboxError::Unavailable)?;
        tx.commit()
            .await
            .map_err(|_| SalesFinanceInboxError::Unavailable)?;

        Ok(match handoff {
            Some(value) => DurableSalesFinanceOutcome::Staged(value.clone()),
            None => DurableSalesFinanceOutcome::Ignored,
        })
    }

    pub async fn mark_applied(
        &self,
        source_event_id: EventId,
    ) -> Result<(), SalesFinanceInboxError> {
        self.transition(source_event_id, "applied", None).await
    }

    pub async fn record_failure(
        &self,
        source_event_id: EventId,
        failure_code: &str,
        retryable: bool,
    ) -> Result<(), SalesFinanceInboxError> {
        if failure_code.trim().is_empty()
            || failure_code.len() > 128
            || failure_code.chars().any(char::is_control)
        {
            return Err(SalesFinanceInboxError::InvalidInput);
        }
        self.transition(
            source_event_id,
            if retryable {
                "retryable_failure"
            } else {
                "permanent_failure"
            },
            Some(failure_code),
        )
        .await
    }

    async fn transition(
        &self,
        source_event_id: EventId,
        status: &str,
        failure_code: Option<&str>,
    ) -> Result<(), SalesFinanceInboxError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| SalesFinanceInboxError::Unavailable)?;
        let result = sqlx::query(
            "UPDATE erpx_finance_write.sales_fact_inbox
             SET inbox_status=$3,failure_code=$4,
                 attempts=attempts + CASE WHEN $4::TEXT IS NULL THEN 0 ELSE 1 END,
                 updated_at=now()
             WHERE consumer_name=$1 AND source_event_id=$2",
        )
        .bind(SALES_FINANCE_CONSUMER_NAME)
        .bind(source_event_id.as_uuid())
        .bind(status)
        .bind(failure_code)
        .execute(&mut *tx)
        .await
        .map_err(|_| SalesFinanceInboxError::Unavailable)?;
        if result.rows_affected() != 1 {
            return Err(SalesFinanceInboxError::NotFound);
        }
        tx.commit()
            .await
            .map_err(|_| SalesFinanceInboxError::Unavailable)
    }
}

fn handoff_id(handoff: &SalesFinanceHandoff) -> uuid::Uuid {
    match &handoff.handoff {
        crate::SalesFinanceHandoffKind::CreateRevenueSchedule { schedule_id, .. } => *schedule_id,
        crate::SalesFinanceHandoffKind::PreparePostingPacket { packet_id, .. } => *packet_id,
        crate::SalesFinanceHandoffKind::RecognizeRevenue { recognition_id, .. } => *recognition_id,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SalesFinanceInboxError {
    #[error("Sales-to-Finance inbox input is invalid")]
    InvalidInput,
    #[error("Sales-to-Finance inbox checkpoint moved backwards")]
    CheckpointRegression,
    #[error("Sales-to-Finance duplicate conflicts with retained evidence")]
    DuplicateConflict,
    #[error("Sales-to-Finance inbox row was not found")]
    NotFound,
    #[error("Sales-to-Finance inbox state is corrupt")]
    CorruptState,
    #[error("Sales-to-Finance inbox is unavailable")]
    Unavailable,
}

#[cfg(test)]
mod tests {
    #[test]
    fn persistence_keeps_payload_checkpoint_and_failure_state_together() {
        let source = include_str!("sales_finance_inbox_sqlx.rs");
        assert!(source.contains("FOR UPDATE"));
        assert!(source.contains("handoff_payload"));
        assert!(source.contains("retryable_failure"));
        let ddl = include_str!(
            "../../../crates/migration/migrations_tenant_write/0083_sales_finance_ai_approval_slice.sql"
        );
        assert!(ddl.contains("sales_fact_checkpoints"));
        assert!(ddl.contains("handoff_payload JSONB NOT NULL"));
        assert!(ddl.contains("UNIQUE (tenant_id, source_event_id)"));
    }
}
