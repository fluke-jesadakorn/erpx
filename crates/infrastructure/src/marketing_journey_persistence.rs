//! Authoritative holding-write persistence for the Marketing Journey aggregate.
//!
//! Journey state, the complete shared event envelope, and the idempotency
//! receipt are committed in one transaction. The routed holding setting and
//! forced RLS remain active for every statement in that transaction.

use chrono::Utc;
use erpx_domain::contracts::{AuthorityScope, CONTRACT_VERSION, CommandContext, EventEnvelope};
use erpx_domain::marketing_journey::{
    Journey, JourneyStatus, JourneyTransitionCommand, MarketingJourneyError,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::PersistenceError;
use crate::finance_persistence::begin_scoped;

pub const MARKETING_JOURNEY_AGGREGATE: &str = "marketing_journey";
pub const MARKETING_JOURNEY_CREATED_EVENT: &str = "marketing.journey_created.v1";
pub const MARKETING_JOURNEY_STATUS_CHANGED_EVENT: &str = "marketing.journey_status_changed.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingJourneyWriteReceipt {
    pub journey_id: Uuid,
    pub version: u64,
    pub idempotency_key: String,
    pub event_id: Uuid,
    pub replayed: bool,
}

#[derive(Debug, Clone)]
pub struct SqlxMarketingJourneyStore {
    write_pool: PgPool,
    data_profile_key: String,
}

impl SqlxMarketingJourneyStore {
    pub fn new(
        write_pool: PgPool,
        data_profile_key: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let data_profile_key = data_profile_key.into();
        validate_data_profile_key(&data_profile_key)?;
        Ok(Self {
            write_pool,
            data_profile_key,
        })
    }

    pub fn data_profile_key(&self) -> &str {
        &self.data_profile_key
    }

    /// Creates the initial Draft aggregate and publishes its version-one
    /// source event atomically.
    pub async fn create_journey(
        &self,
        journey: &Journey,
        context: &CommandContext,
    ) -> Result<MarketingJourneyWriteReceipt, PersistenceError> {
        validate_command_context(context, journey, None)?;
        if journey.status() != JourneyStatus::Draft {
            return Err(invalid("a new Marketing journey must be Draft"));
        }
        if journey.version() != 1 {
            return Err(invalid("a new Marketing journey must start at version one"));
        }
        let request_hash = create_request_hash(journey, context)?;
        let mut tx = begin_scoped(&self.write_pool, &self.data_profile_key).await?;
        lock_journey(&mut tx, &self.data_profile_key, journey.id()).await?;
        if let Some(receipt) = find_replay(
            &mut tx,
            &self.data_profile_key,
            "create",
            &context.idempotency_key,
            &request_hash,
            journey.id(),
        )
        .await?
        {
            tx.commit().await?;
            return Ok(receipt);
        }
        if journey_exists(&mut tx, &self.data_profile_key, journey.id()).await? {
            return Err(invalid("Marketing journey already exists"));
        }
        let payload = serde_json::to_value(journey)?;
        let event = build_event(context, journey, MARKETING_JOURNEY_CREATED_EVENT, payload)?;
        insert_journey(&mut tx, &self.data_profile_key, journey, context).await?;
        append_event(&mut tx, &self.data_profile_key, &event).await?;
        let receipt = make_receipt(&event, journey.version(), &context.idempotency_key, false);
        record_idempotency(
            &mut tx,
            &self.data_profile_key,
            "create",
            &request_hash,
            &receipt,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    /// Applies one lifecycle command against the authoritative aggregate and
    /// commits the resulting normalized state, source event, and receipt.
    pub async fn transition_journey(
        &self,
        command: &JourneyTransitionCommand,
    ) -> Result<MarketingJourneyWriteReceipt, PersistenceError> {
        command.validate().map_err(marketing_journey_error)?;
        let mut tx = begin_scoped(&self.write_pool, &self.data_profile_key).await?;
        lock_journey(&mut tx, &self.data_profile_key, command.journey_id).await?;
        let row = sqlx::query(
            "SELECT journey FROM erpx_holding.marketing_journeys
              WHERE data_profile_key=$1 AND journey_id=$2 FOR UPDATE",
        )
        .bind(&self.data_profile_key)
        .bind(command.journey_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            return Err(PersistenceError::NotFound {
                entity: "Marketing journey",
                id: command.journey_id,
            });
        };
        let mut journey: Journey = serde_json::from_value(row.get("journey"))?;
        validate_command_context(
            &command.context,
            &journey,
            command.context.expected_aggregate_version,
        )?;
        let request_hash = transition_request_hash(command)?;
        if let Some(receipt) = find_replay(
            &mut tx,
            &self.data_profile_key,
            "transition",
            &command.context.idempotency_key,
            &request_hash,
            command.journey_id,
        )
        .await?
        {
            tx.commit().await?;
            return Ok(receipt);
        }
        let fact = journey
            .transition(command)
            .map_err(marketing_journey_error)?;
        let payload = serde_json::json!({
            "journey": serde_json::to_value(&journey)?,
            "transition": fact,
        });
        let event = build_event(
            &command.context,
            &journey,
            MARKETING_JOURNEY_STATUS_CHANGED_EVENT,
            payload,
        )?;
        update_journey(&mut tx, &self.data_profile_key, &journey, &command.context).await?;
        append_event(&mut tx, &self.data_profile_key, &event).await?;
        let receipt = make_receipt(
            &event,
            journey.version(),
            &command.context.idempotency_key,
            false,
        );
        record_idempotency(
            &mut tx,
            &self.data_profile_key,
            "transition",
            &request_hash,
            &receipt,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    pub async fn load_journey(
        &self,
        journey_id: Uuid,
    ) -> Result<Option<Journey>, PersistenceError> {
        if journey_id.is_nil() {
            return Err(invalid("Marketing journey ID cannot be nil"));
        }
        let mut tx = begin_scoped(&self.write_pool, &self.data_profile_key).await?;
        let row = sqlx::query(
            "SELECT journey FROM erpx_holding.marketing_journeys
              WHERE data_profile_key=$1 AND journey_id=$2",
        )
        .bind(&self.data_profile_key)
        .bind(journey_id)
        .fetch_optional(&mut *tx)
        .await?;
        let result = row
            .map(|row| serde_json::from_value(row.get("journey")))
            .transpose()?;
        tx.commit().await?;
        Ok(result)
    }
}

async fn lock_journey(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    journey_id: Uuid,
) -> Result<(), PersistenceError> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!(
            "erpx.marketing-journey:{data_profile_key}:{journey_id}"
        ))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn journey_exists(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    journey_id: Uuid,
) -> Result<bool, PersistenceError> {
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM erpx_holding.marketing_journeys
                         WHERE data_profile_key=$1 AND journey_id=$2)",
    )
    .bind(data_profile_key)
    .bind(journey_id)
    .fetch_one(&mut **tx)
    .await?)
}

async fn insert_journey(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    journey: &Journey,
    context: &CommandContext,
) -> Result<(), PersistenceError> {
    let payload = serde_json::to_value(journey)?;
    sqlx::query(
        "INSERT INTO erpx_holding.marketing_journeys
            (data_profile_key, journey_id, scope, name, status, nodes, journey, version,
             last_command_id, last_idempotency_key, actor, correlation_id,
             created_at, updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
    )
    .bind(data_profile_key)
    .bind(journey.id())
    .bind(serde_json::to_value(journey.scope())?)
    .bind(journey.name())
    .bind(status_text(journey.status()))
    .bind(serde_json::to_value(journey.nodes())?)
    .bind(payload)
    .bind(as_i64(journey.version(), "Marketing journey version")?)
    .bind(context.command_id)
    .bind(&context.idempotency_key)
    .bind(serde_json::to_value(&context.actor)?)
    .bind(context.correlation_id)
    .bind(journey.created_at())
    .bind(journey.updated_at())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn update_journey(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    journey: &Journey,
    context: &CommandContext,
) -> Result<(), PersistenceError> {
    let payload = serde_json::to_value(journey)?;
    let expected = context
        .expected_aggregate_version
        .ok_or_else(|| invalid("Marketing journey expected version is required"))?;
    let affected = sqlx::query(
        "UPDATE erpx_holding.marketing_journeys
            SET scope=$3, name=$4, status=$5, nodes=$6, journey=$7, version=$8,
                last_command_id=$9, last_idempotency_key=$10, actor=$11,
                correlation_id=$12, updated_at=$13
          WHERE data_profile_key=$1 AND journey_id=$2 AND version=$14",
    )
    .bind(data_profile_key)
    .bind(journey.id())
    .bind(serde_json::to_value(journey.scope())?)
    .bind(journey.name())
    .bind(status_text(journey.status()))
    .bind(serde_json::to_value(journey.nodes())?)
    .bind(payload)
    .bind(as_i64(journey.version(), "Marketing journey version")?)
    .bind(context.command_id)
    .bind(&context.idempotency_key)
    .bind(serde_json::to_value(&context.actor)?)
    .bind(context.correlation_id)
    .bind(journey.updated_at())
    .bind(as_i64(expected, "Marketing journey expected version")?)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    if affected != 1 {
        return Err(PersistenceError::OptimisticConcurrency {
            aggregate_type: MARKETING_JOURNEY_AGGREGATE.into(),
            aggregate_id: journey.id(),
            expected: Some(expected),
            actual: None,
        });
    }
    Ok(())
}

fn build_event(
    context: &CommandContext,
    journey: &Journey,
    event_type: &str,
    payload: Value,
) -> Result<EventEnvelope, PersistenceError> {
    Ok(EventEnvelope {
        contract_version: context.contract_version,
        event_id: Uuid::new_v4(),
        event_type: event_type.into(),
        event_version: 1,
        occurred_at: Utc::now(),
        aggregate_type: MARKETING_JOURNEY_AGGREGATE.into(),
        aggregate_id: journey.id(),
        aggregate_version: journey.version(),
        active_scope: journey.scope().clone(),
        actor: context.actor.clone(),
        correlation_id: context.correlation_id,
        causation_id: context.causation_id,
        idempotency_key: Some(context.idempotency_key.clone()),
        payload,
    })
}

async fn append_event(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    event: &EventEnvelope,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_holding.outbox_events
            (event_id, aggregate_type, aggregate_id, data_profile_key,
             aggregate_version, event_type, payload, occurred_at,
             contract_version, event_version, active_scope, actor,
             correlation_id, causation_id, idempotency_key)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)",
    )
    .bind(event.event_id)
    .bind(&event.aggregate_type)
    .bind(event.aggregate_id)
    .bind(data_profile_key)
    .bind(as_i64(
        event.aggregate_version,
        "Marketing journey event version",
    )?)
    .bind(&event.event_type)
    .bind(&event.payload)
    .bind(event.occurred_at)
    .bind(i32::from(event.contract_version))
    .bind(i32::from(event.event_version))
    .bind(serde_json::to_value(&event.active_scope)?)
    .bind(serde_json::to_value(&event.actor)?)
    .bind(event.correlation_id)
    .bind(event.causation_id)
    .bind(&event.idempotency_key)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn find_replay(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    operation: &str,
    idempotency_key: &str,
    request_hash: &str,
    journey_id: Uuid,
) -> Result<Option<MarketingJourneyWriteReceipt>, PersistenceError> {
    let row = sqlx::query(
        "SELECT operation, request_hash, journey_id, version, event_id, response
           FROM erpx_holding.marketing_journey_command_idempotency
          WHERE data_profile_key=$1 AND idempotency_key=$2 FOR UPDATE",
    )
    .bind(data_profile_key)
    .bind(idempotency_key)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(row) = row else { return Ok(None) };
    if row.get::<String, _>("operation") != operation
        || row.get::<String, _>("request_hash") != request_hash
        || row.get::<Uuid, _>("journey_id") != journey_id
    {
        return Err(PersistenceError::IdempotencyConflict {
            key: idempotency_key.into(),
        });
    }
    let stored_version: u64 = as_u64(
        row.get::<i64, _>("version"),
        "Marketing journey replay version",
    )?;
    let stored_event_id: Uuid = row.get("event_id");
    let mut receipt: MarketingJourneyWriteReceipt = serde_json::from_value(row.get("response"))?;
    if receipt.journey_id != journey_id
        || receipt.version != stored_version
        || receipt.event_id != stored_event_id
        || receipt.idempotency_key != idempotency_key
        || receipt.version == 0
        || receipt.event_id.is_nil()
    {
        return Err(invalid(
            "Marketing journey idempotency receipt is inconsistent",
        ));
    }
    let expected_event_type = match operation {
        "create" => MARKETING_JOURNEY_CREATED_EVENT,
        "transition" => MARKETING_JOURNEY_STATUS_CHANGED_EVENT,
        _ => {
            return Err(invalid(
                "Marketing journey idempotency operation is invalid",
            ));
        }
    };
    let source_event = sqlx::query(
        "SELECT aggregate_type, aggregate_id, aggregate_version, event_type,
                contract_version, event_version, idempotency_key, payload
         FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND event_id=$2
         FOR SHARE",
    )
    .bind(data_profile_key)
    .bind(stored_event_id)
    .fetch_optional(&mut **tx)
    .await?;
    let journey_exists = journey_exists(tx, data_profile_key, journey_id).await?;
    let source_event_matches = source_event.is_some_and(|event| {
        event.get::<String, _>("aggregate_type") == MARKETING_JOURNEY_AGGREGATE
            && event.get::<Uuid, _>("aggregate_id") == journey_id
            && event.get::<i64, _>("aggregate_version") == stored_version as i64
            && event.get::<String, _>("event_type") == expected_event_type
            && event.get::<i32, _>("contract_version") == i32::from(CONTRACT_VERSION)
            && event.get::<i32, _>("event_version") == 1
            && event.get::<Option<String>, _>("idempotency_key").as_deref() == Some(idempotency_key)
            && !event.get::<Value, _>("payload").is_null()
    });
    validate_replay_evidence(journey_exists, source_event_matches)?;
    receipt.replayed = true;
    Ok(Some(receipt))
}

async fn record_idempotency(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    operation: &str,
    request_hash: &str,
    receipt: &MarketingJourneyWriteReceipt,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_holding.marketing_journey_command_idempotency
            (data_profile_key, idempotency_key, operation, request_hash,
             journey_id, version, event_id, response)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(data_profile_key)
    .bind(&receipt.idempotency_key)
    .bind(operation)
    .bind(request_hash)
    .bind(receipt.journey_id)
    .bind(as_i64(
        receipt.version,
        "Marketing journey receipt version",
    )?)
    .bind(receipt.event_id)
    .bind(serde_json::to_value(receipt)?)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn create_request_hash(
    journey: &Journey,
    context: &CommandContext,
) -> Result<String, PersistenceError> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(&(
        "create", journey, context,
    ))?)))
}

fn transition_request_hash(command: &JourneyTransitionCommand) -> Result<String, PersistenceError> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(&(
        "transition",
        command,
    ))?)))
}

fn validate_command_context(
    context: &CommandContext,
    journey: &Journey,
    expected: Option<u64>,
) -> Result<(), PersistenceError> {
    context.validate().map_err(|error| {
        invalid(format!(
            "invalid Marketing journey command context: {error}"
        ))
    })?;
    if context.contract_version != CONTRACT_VERSION {
        return Err(invalid("unsupported Marketing journey contract version"));
    }
    journey.validate().map_err(marketing_journey_error)?;
    if matches!(context.active_scope, AuthorityScope::Platform)
        || !context.active_scope.contains(journey.scope())
    {
        return Err(invalid(
            "Marketing journey command scope does not contain the journey",
        ));
    }
    if context.expected_aggregate_version != expected {
        return Err(invalid(
            "Marketing journey expected version binding is invalid",
        ));
    }
    if context.actor.principal_ref.trim().is_empty()
        || context.actor.principal_ref.len() > 255
        || context.actor.principal_ref.chars().any(char::is_control)
    {
        return Err(invalid("Marketing journey actor reference is invalid"));
    }
    if context.idempotency_key.trim().is_empty()
        || context.idempotency_key.len() > 255
        || context.idempotency_key.chars().any(char::is_control)
        || context.audit.purpose.trim().is_empty()
        || context.audit.purpose.len() > 255
        || context.audit.purpose.chars().any(char::is_control)
        || context
            .actor
            .represented_principal_ref
            .as_ref()
            .is_some_and(|value| value.len() > 255 || value.chars().any(char::is_control))
    {
        return Err(invalid("Marketing journey command metadata is invalid"));
    }
    Ok(())
}

fn make_receipt(
    event: &EventEnvelope,
    version: u64,
    idempotency_key: &str,
    replayed: bool,
) -> MarketingJourneyWriteReceipt {
    MarketingJourneyWriteReceipt {
        journey_id: event.aggregate_id,
        version,
        idempotency_key: idempotency_key.into(),
        event_id: event.event_id,
        replayed,
    }
}

fn status_text(status: JourneyStatus) -> &'static str {
    match status {
        JourneyStatus::Draft => "draft",
        JourneyStatus::Review => "review",
        JourneyStatus::Approved => "approved",
        JourneyStatus::Scheduled => "scheduled",
        JourneyStatus::Running => "running",
        JourneyStatus::Paused => "paused",
        JourneyStatus::Completed => "completed",
        JourneyStatus::Archived => "archived",
    }
}

fn validate_data_profile_key(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
        return Err(PersistenceError::Configuration(
            "holding reference must be bounded and non-empty",
        ));
    }
    Ok(())
}

fn as_i64(value: u64, field: &str) -> Result<i64, PersistenceError> {
    i64::try_from(value)
        .map_err(|_| PersistenceError::InvalidData(format!("{field} exceeds BIGINT")))
}

fn as_u64(value: i64, field: &str) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| PersistenceError::InvalidData(format!("{field} is negative")))
}

fn invalid(message: impl Into<String>) -> PersistenceError {
    PersistenceError::InvalidData(message.into())
}

fn validate_replay_evidence(
    journey_exists: bool,
    source_event_matches: bool,
) -> Result<(), PersistenceError> {
    if !journey_exists {
        return Err(invalid(
            "Marketing journey replay requires its authoritative journey row",
        ));
    }
    if !source_event_matches {
        return Err(invalid(
            "Marketing journey replay requires a matching authoritative source event",
        ));
    }
    Ok(())
}

fn marketing_journey_error(error: MarketingJourneyError) -> PersistenceError {
    invalid(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_mapping_is_exhaustive_and_positive_versions_are_required() {
        assert_eq!(status_text(JourneyStatus::Draft), "draft");
        assert_eq!(status_text(JourneyStatus::Archived), "archived");
        assert!(as_i64(1, "version").is_ok());
    }

    #[test]
    fn replay_requires_authoritative_journey_and_source_event_evidence() {
        assert!(validate_replay_evidence(false, true).is_err());
        assert!(validate_replay_evidence(true, false).is_err());
        validate_replay_evidence(true, true)
            .expect("journey replay must retain its source row and event evidence");
    }
}
