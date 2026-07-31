use async_trait::async_trait;
use erpx_application::{ApplicationError, CommandMutation, CommandReceipt, CommandUnitOfWork};
use serde_json::Value;
use sqlx::Row;

use crate::{ControlPlaneWriteTransaction, EventMetadata, PersistenceError, SqlxControlPlane};

/// Named adapter marker for composition roots that want to state explicitly
/// that the SQLx control plane is the application command unit of work.
pub type CommandUnitOfWorkAdapter = SqlxControlPlane;

fn app_error(error: PersistenceError) -> ApplicationError {
    ApplicationError::Repository(error.to_string())
}

#[async_trait]
impl CommandUnitOfWork for SqlxControlPlane {
    async fn execute_command(
        &self,
        mutation: CommandMutation,
    ) -> Result<CommandReceipt, ApplicationError> {
        validate_mutation(&mutation).map_err(app_error)?;
        let mut transaction = self.begin_write().await.map_err(app_error)?;
        let reservation = transaction
            .reserve_idempotency(
                &mutation.idempotency_key,
                &mutation.request_hash,
                Some(mutation.command_id),
            )
            .await
            .map_err(app_error)?;

        match reservation.state {
            crate::IdempotencyState::Completed { .. } => {
                let receipt = transaction
                    .read_command_receipt(&mutation.idempotency_key)
                    .await
                    .map_err(app_error)?
                    .ok_or_else(|| {
                        ApplicationError::Repository(
                            "completed idempotency record has no command receipt".into(),
                        )
                    })?;
                if let Err(error) = validate_replayed_receipt(
                    &receipt,
                    &mutation.aggregate_type,
                    mutation.aggregate_id,
                    mutation.event.aggregate_version,
                    None,
                ) {
                    let _ = transaction.rollback().await;
                    return Err(app_error(error));
                }
                transaction.commit().await.map_err(app_error)?;
                return Ok(CommandReceipt {
                    replayed: true,
                    ..receipt
                });
            }
            crate::IdempotencyState::InProgress { .. } => {
                let _ = transaction.rollback().await;
                return Err(ApplicationError::Conflict(
                    "command idempotency key is already in progress".into(),
                ));
            }
            crate::IdempotencyState::Reserved => {}
        }

        let aggregate_version = transaction
            .upsert_aggregate_snapshot(
                &mutation.aggregate_type,
                mutation.aggregate_id,
                mutation.aggregate_state.clone(),
                mutation.expected_aggregate_version,
            )
            .await
            .map_err(app_error)?;
        if aggregate_version != mutation.event.aggregate_version {
            let _ = transaction.rollback().await;
            return Err(ApplicationError::Conflict(format!(
                "event aggregate version {} does not match persisted version {}",
                mutation.event.aggregate_version, aggregate_version
            )));
        }

        transaction
            .append_outbox_event(
                &mutation.event,
                &EventMetadata {
                    tenant_scope: mutation.tenant_scope.clone(),
                    actor: mutation.actor.clone(),
                    correlation_id: Some(mutation.audit.correlation_id),
                    causation_id: mutation.causation_id,
                    idempotency_key: Some(mutation.idempotency_key.clone()),
                    event_version: mutation.event_version,
                },
            )
            .await
            .map_err(app_error)?;
        transaction
            .append_audit_event(&mutation.audit)
            .await
            .map_err(app_error)?;

        let receipt = CommandReceipt {
            command_id: mutation.command_id,
            aggregate_type: mutation.aggregate_type,
            aggregate_id: mutation.aggregate_id,
            aggregate_version,
            event_id: mutation.event.id,
            response: mutation.response,
            replayed: false,
        };
        transaction
            .complete_command(&mutation.idempotency_key, &receipt)
            .await
            .map_err(app_error)?;
        transaction.commit().await.map_err(app_error)?;
        Ok(receipt)
    }
}

fn validate_mutation(mutation: &CommandMutation) -> Result<(), PersistenceError> {
    if mutation.command_id.is_nil() {
        return Err(PersistenceError::InvalidData(
            "command ID cannot be nil".into(),
        ));
    }
    if mutation.aggregate_type.is_empty()
        || mutation.aggregate_type != mutation.event.aggregate_type
        || mutation.aggregate_id != mutation.event.aggregate_id
    {
        return Err(PersistenceError::InvalidData(
            "command and event aggregate identity must match".into(),
        ));
    }
    if mutation.event.id.is_nil() {
        return Err(PersistenceError::InvalidData(
            "event ID cannot be nil".into(),
        ));
    }
    if mutation.event_version == 0 {
        return Err(PersistenceError::InvalidData(
            "event version must be positive".into(),
        ));
    }
    Ok(())
}

fn validate_replayed_receipt(
    receipt: &CommandReceipt,
    aggregate_type: &str,
    aggregate_id: uuid::Uuid,
    aggregate_version: u64,
    event_id: Option<uuid::Uuid>,
) -> Result<(), PersistenceError> {
    if receipt.command_id.is_nil()
        || receipt.aggregate_id.is_nil()
        || receipt.event_id.is_nil()
        || receipt.aggregate_version == 0
    {
        return Err(PersistenceError::InvalidData(
            "completed command receipt has invalid source identity".into(),
        ));
    }
    if receipt.aggregate_type != aggregate_type
        || receipt.aggregate_id != aggregate_id
        || receipt.aggregate_version != aggregate_version
        || event_id.is_some_and(|expected| receipt.event_id != expected)
    {
        return Err(PersistenceError::InvalidData(
            "completed command receipt does not match the replay source".into(),
        ));
    }
    Ok(())
}

impl<'a> ControlPlaneWriteTransaction<'a> {
    pub async fn append_audit_event(
        &mut self,
        event: &erpx_application::AuditEvent,
    ) -> Result<(), PersistenceError> {
        let payload = serde_json::to_value(AuditEventPayload::from(event))?;
        sqlx::query(
            "INSERT INTO erpx_control.audit_events
                (id, actor_ref, customer_account_id, action, purpose, correlation_id, payload)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(uuid::Uuid::new_v4())
        .bind(&event.actor_ref)
        .bind(event.customer_account_id)
        .bind(&event.action)
        .bind(&event.purpose)
        .bind(event.correlation_id)
        .bind(payload)
        .execute(&mut *self.tx)
        .await?;
        Ok(())
    }

    pub async fn complete_command(
        &mut self,
        key: &str,
        receipt: &CommandReceipt,
    ) -> Result<(), PersistenceError> {
        let serialized = serde_json::to_value(receipt)?;
        let result = sqlx::query(
            "UPDATE erpx_control.idempotency_keys
             SET status = 'completed', response = $2, completed_at = now(),
                 aggregate_type = $3, aggregate_id = $4, aggregate_version = $5, event_id = $6
             WHERE idempotency_key = $1 AND status = 'in_progress'",
        )
        .bind(key)
        .bind(serialized)
        .bind(&receipt.aggregate_type)
        .bind(receipt.aggregate_id)
        .bind(i64::try_from(receipt.aggregate_version).map_err(|_| {
            PersistenceError::InvalidData("aggregate version exceeds database range".into())
        })?)
        .bind(receipt.event_id)
        .execute(&mut *self.tx)
        .await?;
        if result.rows_affected() != 1 {
            return Err(PersistenceError::InvalidData(
                "command idempotency key is not in progress".into(),
            ));
        }
        Ok(())
    }

    pub async fn read_command_receipt(
        &mut self,
        key: &str,
    ) -> Result<Option<CommandReceipt>, PersistenceError> {
        let row = sqlx::query(
            "SELECT command_id, response, aggregate_type, aggregate_id,
                    aggregate_version, event_id
             FROM erpx_control.idempotency_keys
             WHERE idempotency_key = $1 AND status = 'completed' FOR UPDATE",
        )
        .bind(key)
        .fetch_optional(&mut *self.tx)
        .await?;
        row.map(|row| {
            let response: Value = row.try_get("response")?;
            let receipt: CommandReceipt = serde_json::from_value(response)?;
            let stored_command_id: Option<uuid::Uuid> = row.try_get("command_id")?;
            let stored_aggregate_type: String = row.try_get("aggregate_type")?;
            let stored_aggregate_id: uuid::Uuid = row.try_get("aggregate_id")?;
            let stored_aggregate_version: i64 = row.try_get("aggregate_version")?;
            let stored_event_id: uuid::Uuid = row.try_get("event_id")?;
            let stored_aggregate_version =
                u64::try_from(stored_aggregate_version).map_err(|_| {
                    PersistenceError::InvalidData(
                        "completed command receipt has an invalid aggregate version".into(),
                    )
                })?;
            if stored_command_id != Some(receipt.command_id)
                || stored_aggregate_type != receipt.aggregate_type
                || stored_aggregate_id != receipt.aggregate_id
                || stored_aggregate_version != receipt.aggregate_version
                || stored_event_id != receipt.event_id
            {
                return Err(PersistenceError::InvalidData(
                    "completed command receipt does not match its idempotency source".into(),
                ));
            }
            Ok(receipt)
        })
        .transpose()
    }
}

#[derive(Debug, serde::Serialize)]
struct AuditEventPayload<'a> {
    actor_ref: &'a str,
    customer_account_id: Option<uuid::Uuid>,
    action: &'a str,
    purpose: &'a str,
    correlation_id: uuid::Uuid,
}

impl<'a> From<&'a erpx_application::AuditEvent> for AuditEventPayload<'a> {
    fn from(event: &'a erpx_application::AuditEvent) -> Self {
        Self {
            actor_ref: &event.actor_ref,
            customer_account_id: event.customer_account_id,
            action: &event.action,
            purpose: &event.purpose,
            correlation_id: event.correlation_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt() -> CommandReceipt {
        CommandReceipt {
            command_id: uuid::Uuid::from_u128(1),
            aggregate_type: "customer".into(),
            aggregate_id: uuid::Uuid::from_u128(2),
            aggregate_version: 3,
            event_id: uuid::Uuid::from_u128(4),
            response: serde_json::json!({"accepted": true}),
            replayed: false,
        }
    }

    #[test]
    fn completed_receipt_must_match_the_replay_source() {
        let receipt = receipt();
        assert!(
            validate_replayed_receipt(
                &receipt,
                "customer",
                uuid::Uuid::from_u128(2),
                3,
                Some(uuid::Uuid::from_u128(4)),
            )
            .is_ok()
        );

        assert!(
            validate_replayed_receipt(
                &receipt,
                "customer",
                uuid::Uuid::from_u128(9),
                3,
                Some(uuid::Uuid::from_u128(4)),
            )
            .is_err()
        );
        assert!(
            validate_replayed_receipt(
                &receipt,
                "customer",
                uuid::Uuid::from_u128(2),
                4,
                Some(uuid::Uuid::from_u128(4)),
            )
            .is_err()
        );
        assert!(
            validate_replayed_receipt(
                &receipt,
                "customer",
                uuid::Uuid::from_u128(2),
                3,
                Some(uuid::Uuid::from_u128(8)),
            )
            .is_err()
        );
    }

    #[test]
    fn malformed_completed_receipt_fails_closed() {
        let mut receipt = receipt();
        receipt.event_id = uuid::Uuid::nil();
        assert!(
            validate_replayed_receipt(
                &receipt,
                "customer",
                uuid::Uuid::from_u128(2),
                3,
                Some(uuid::Uuid::from_u128(4)),
            )
            .is_err()
        );
    }
}
