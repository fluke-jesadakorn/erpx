//! Durable provider-account handoffs emitted by Marketing journey actions.
//!
//! The journey cursor and the initial invocation outbox event are committed
//! in one tenant-write transaction. Provider calls happen only in the
//! separately claimed invocation worker, after the worker re-resolves the
//! active, verified holding-bound provider account from the control plane.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};
use erpx_domain::contracts::{ActorAttribution, ActorKind, CommandContext, EventEnvelope};
use erpx_domain::marketing_journey::JourneyIntegrationInvocation;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{PersistenceError, finance_persistence::begin_scoped};

pub const MARKETING_JOURNEY_INTEGRATION_AGGREGATE: &str = "marketing_journey_integration";
pub const MARKETING_JOURNEY_INTEGRATION_INVOKED_EVENT: &str =
    "marketing.journey_integration_invoked.v1";
pub const MARKETING_JOURNEY_INTEGRATION_SUCCEEDED_EVENT: &str =
    "marketing.journey_integration_succeeded.v1";
pub const MARKETING_JOURNEY_INTEGRATION_RETRY_EVENT: &str =
    "marketing.journey_integration_retry_scheduled.v1";
pub const MARKETING_JOURNEY_INTEGRATION_FAILED_EVENT: &str =
    "marketing.journey_integration_failed.v1";
pub const MAX_MARKETING_JOURNEY_INTEGRATION_CLAIM_BATCH_SIZE: u32 = 100;
pub const MAX_MARKETING_JOURNEY_INTEGRATION_PROJECTION_PAGE_SIZE: u32 = 100;
pub const MIN_MARKETING_JOURNEY_INTEGRATION_LEASE_SECONDS: u64 = 1;
pub const MAX_MARKETING_JOURNEY_INTEGRATION_LEASE_SECONDS: u64 = 3_600;
const MAX_RECONCILIATION_ROWS: i64 = 10_000;
pub const MAX_MARKETING_JOURNEY_INTEGRATION_REBUILD_EVENTS: u64 = 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingJourneyIntegrationClaim {
    pub invocation: JourneyIntegrationInvocation,
    pub attempt: u32,
    pub claim_id: Uuid,
    pub worker_id: Uuid,
    pub claim_until: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarketingJourneyIntegrationProviderResult {
    Accepted { provider_reference: String },
    Retryable { reason: String },
    Permanent { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingJourneyIntegrationWriteReceipt {
    pub invocation_id: Uuid,
    pub version: u64,
    pub event_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingJourneyIntegrationReadRow {
    pub invocation: JourneyIntegrationInvocation,
    pub state: String,
    pub attempt_count: u32,
    pub version: u64,
    pub provider_reference: Option<String>,
    pub last_error: Option<String>,
    pub next_attempt_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub source_version: u64,
    pub source_event_id: Uuid,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MarketingJourneyIntegrationProjectionReconciliation {
    pub source_invocation_count: u64,
    pub source_event_count: u64,
    pub read_invocation_count: u64,
    pub missing_invocation_count: u64,
    pub stale_invocation_count: u64,
    pub ahead_invocation_count: u64,
    pub orphan_read_count: u64,
    pub payload_mismatch_count: u64,
    pub healthy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingJourneyIntegrationProjectionRebuildPreparation {
    pub holding_ref: String,
    pub rows_cleared: u64,
    pub checkpoints_cleared: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InvocationSnapshot {
    invocation: JourneyIntegrationInvocation,
    state: String,
    attempt_count: u32,
    version: u64,
    provider_reference: Option<String>,
    last_error: Option<String>,
    next_attempt_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct SqlxMarketingJourneyIntegrationStore {
    write_pool: PgPool,
    read_pool: PgPool,
    holding_ref: String,
}

impl SqlxMarketingJourneyIntegrationStore {
    pub fn new(
        write_pool: PgPool,
        read_pool: PgPool,
        holding_ref: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let holding_ref = holding_ref.into();
        validate_holding_ref(&holding_ref)?;
        Ok(Self {
            write_pool,
            read_pool,
            holding_ref,
        })
    }

    pub fn holding_ref(&self) -> &str {
        &self.holding_ref
    }

    /// Queue an invocation as a standalone command boundary. Journey
    /// execution uses the transaction-scoped variant below so the cursor and
    /// handoff share one commit; this entry point is used by reconciliation
    /// and SQLx acceptance tooling.
    pub async fn enqueue(
        &self,
        invocation: &JourneyIntegrationInvocation,
        context: &CommandContext,
    ) -> Result<(), PersistenceError> {
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        Self::enqueue_in_transaction(&mut tx, &self.holding_ref, invocation, context).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Queue the profile-redacted invocation inside the journey cursor
    /// transaction. Replaying the same action reuses the same invocation and
    /// outbox identity, never creating a second provider handoff.
    pub(crate) async fn enqueue_in_transaction(
        tx: &mut Transaction<'_, Postgres>,
        holding_ref: &str,
        invocation: &JourneyIntegrationInvocation,
        context: &CommandContext,
    ) -> Result<(), PersistenceError> {
        invocation
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        validate_holding_ref(holding_ref)?;
        context
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        if !context.tenant_scope.contains(&invocation.scope) {
            return Err(PersistenceError::InvalidData(
                "integration invocation scope is outside command scope".into(),
            ));
        }
        let snapshot = InvocationSnapshot {
            invocation: invocation.clone(),
            state: "queued".into(),
            attempt_count: 0,
            version: 1,
            provider_reference: None,
            last_error: None,
            next_attempt_at: invocation.occurred_at,
            completed_at: None,
        };
        let payload = serde_json::to_value(&snapshot)?;
        let inserted = sqlx::query(
            "INSERT INTO erpx_tenant.marketing_journey_integration_invocations
                (holding_ref, invocation_id, integration_ref, journey_id, run_id,
                 node_id, profile_id, scope, idempotency_key, state, attempt_count,
                 version, provider_reference, last_error, next_attempt_at,
                 claim_id, claimed_by, claim_until, completed_at, payload,
                 created_at, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,
                     NULL,NULL,NULL,$16,$17,$18,$18)
             ON CONFLICT (holding_ref, invocation_id) DO NOTHING",
        )
        .bind(holding_ref)
        .bind(invocation.invocation_id)
        .bind(&invocation.integration_ref)
        .bind(invocation.journey_id)
        .bind(invocation.run_id)
        .bind(invocation.node_id)
        .bind(invocation.profile_id)
        .bind(serde_json::to_value(&invocation.scope)?)
        .bind(&invocation.idempotency_key)
        .bind("queued")
        .bind(0_i32)
        .bind(1_i64)
        .bind(Option::<String>::None)
        .bind(Option::<String>::None)
        .bind(invocation.occurred_at)
        .bind(Option::<DateTime<Utc>>::None)
        .bind(&payload)
        .bind(invocation.occurred_at)
        .execute(&mut **tx)
        .await?
        .rows_affected();
        if inserted == 0 {
            let existing = sqlx::query(
                "SELECT integration_ref, journey_id, run_id, node_id, profile_id,
                        idempotency_key
                   FROM erpx_tenant.marketing_journey_integration_invocations
                  WHERE holding_ref=$1 AND invocation_id=$2
                  FOR UPDATE",
            )
            .bind(holding_ref)
            .bind(invocation.invocation_id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| invalid("integration invocation disappeared during replay"))?;
            if existing.get::<String, _>("integration_ref") != invocation.integration_ref
                || existing.get::<Uuid, _>("journey_id") != invocation.journey_id
                || existing.get::<Uuid, _>("run_id") != invocation.run_id
                || existing.get::<Uuid, _>("node_id") != invocation.node_id
                || existing.get::<Uuid, _>("profile_id") != invocation.profile_id
                || existing.get::<String, _>("idempotency_key") != invocation.idempotency_key
            {
                return Err(PersistenceError::IdempotencyConflict {
                    key: invocation.idempotency_key.clone(),
                });
            }
        }
        let event_id = deterministic_event_id(invocation.invocation_id, 1);
        let event = EventEnvelope {
            contract_version: context.contract_version,
            event_id,
            event_type: MARKETING_JOURNEY_INTEGRATION_INVOKED_EVENT.into(),
            event_version: 1,
            occurred_at: invocation.occurred_at,
            aggregate_type: MARKETING_JOURNEY_INTEGRATION_AGGREGATE.into(),
            aggregate_id: invocation.invocation_id,
            aggregate_version: 1,
            tenant_scope: invocation.scope.clone(),
            actor: context.actor.clone(),
            correlation_id: context.correlation_id,
            causation_id: context.causation_id,
            idempotency_key: Some(invocation.idempotency_key.clone()),
            payload: serde_json::json!({ "aggregate": payload }),
        };
        append_event(tx, holding_ref, &event).await?;
        Ok(())
    }

    pub async fn claim_due(
        &self,
        worker_id: Uuid,
        now: DateTime<Utc>,
        limit: u32,
        lease: Duration,
    ) -> Result<Vec<MarketingJourneyIntegrationClaim>, PersistenceError> {
        validate_uuid(worker_id, "integration worker ID")?;
        if limit == 0 || limit > MAX_MARKETING_JOURNEY_INTEGRATION_CLAIM_BATCH_SIZE {
            return Err(invalid("integration claim batch is outside its bound"));
        }
        if lease < Duration::seconds(MIN_MARKETING_JOURNEY_INTEGRATION_LEASE_SECONDS as i64)
            || lease > Duration::seconds(MAX_MARKETING_JOURNEY_INTEGRATION_LEASE_SECONDS as i64)
        {
            return Err(invalid("integration claim lease is outside its bound"));
        }
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        sqlx::query(
            "UPDATE erpx_tenant.marketing_journey_integration_invocations
                SET state='retry_scheduled', claim_id=NULL, claimed_by=NULL,
                    claim_until=NULL, next_attempt_at=$2, updated_at=$2
              WHERE holding_ref=$1 AND state='sending'
                AND claim_until IS NOT NULL AND claim_until <= $2",
        )
        .bind(&self.holding_ref)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        let rows = sqlx::query(
            "SELECT invocation_id, integration_ref, journey_id, run_id, node_id,
                    profile_id, scope, idempotency_key, attempt_count, claim_id,
                    claimed_by, claim_until, created_at
               FROM erpx_tenant.marketing_journey_integration_invocations
              WHERE holding_ref=$1
                AND state IN ('queued', 'retry_scheduled')
                AND next_attempt_at <= $2
                AND (claim_until IS NULL OR claim_until <= $2)
              ORDER BY next_attempt_at, invocation_id
              LIMIT $3
              FOR UPDATE SKIP LOCKED",
        )
        .bind(&self.holding_ref)
        .bind(now)
        .bind(i64::from(limit))
        .fetch_all(&mut *tx)
        .await?;
        let mut claims = Vec::with_capacity(rows.len());
        for row in rows {
            let invocation_id: Uuid = row.try_get("invocation_id")?;
            let attempt_count: i32 = row.try_get("attempt_count")?;
            let attempt = u32::try_from(attempt_count)
                .map_err(|_| invalid("integration attempt count is negative"))?
                .checked_add(1)
                .ok_or_else(|| invalid("integration attempt count overflowed"))?;
            let invocation = JourneyIntegrationInvocation::new(
                invocation_id,
                row.try_get::<String, _>("integration_ref")?,
                row.try_get("journey_id")?,
                row.try_get("run_id")?,
                row.try_get("node_id")?,
                row.try_get("profile_id")?,
                serde_json::from_value(row.try_get("scope")?)?,
                row.try_get::<String, _>("idempotency_key")?,
                row.try_get("created_at")?,
            )
            .map_err(|error| invalid(error.to_string()))?;
            let claim_id = Uuid::new_v4();
            let claim_until = now + lease;
            sqlx::query(
                "UPDATE erpx_tenant.marketing_journey_integration_invocations
                    SET state='sending', attempt_count=$3, claim_id=$4,
                        claimed_by=$5, claim_until=$6, updated_at=$7
                  WHERE holding_ref=$1 AND invocation_id=$2",
            )
            .bind(&self.holding_ref)
            .bind(invocation_id)
            .bind(i32::try_from(attempt).map_err(|_| invalid("attempt exceeds INTEGER"))?)
            .bind(claim_id)
            .bind(worker_id)
            .bind(claim_until)
            .bind(now)
            .execute(&mut *tx)
            .await?;
            claims.push(MarketingJourneyIntegrationClaim {
                invocation,
                attempt,
                claim_id,
                worker_id,
                claim_until,
            });
        }
        tx.commit().await?;
        Ok(claims)
    }

    pub async fn record_result(
        &self,
        worker_id: Uuid,
        claim: &MarketingJourneyIntegrationClaim,
        result: MarketingJourneyIntegrationProviderResult,
        now: DateTime<Utc>,
    ) -> Result<MarketingJourneyIntegrationWriteReceipt, PersistenceError> {
        validate_uuid(worker_id, "integration worker ID")?;
        if claim.worker_id != worker_id || now >= claim.claim_until {
            return Err(PersistenceError::ClaimLost {
                worker_id,
                event_id: claim.invocation.invocation_id,
            });
        }
        let (state, provider_reference, last_error, next_attempt_at, completed_at, event_type) =
            match result {
                MarketingJourneyIntegrationProviderResult::Accepted { provider_reference } => {
                    validate_provider_reference(&provider_reference)?;
                    (
                        "succeeded",
                        Some(provider_reference),
                        None,
                        now,
                        Some(now),
                        MARKETING_JOURNEY_INTEGRATION_SUCCEEDED_EVENT,
                    )
                }
                MarketingJourneyIntegrationProviderResult::Retryable { reason } => {
                    validate_failure_reason(&reason)?;
                    (
                        "retry_scheduled",
                        None,
                        Some(reason),
                        now + Duration::seconds(30),
                        None,
                        MARKETING_JOURNEY_INTEGRATION_RETRY_EVENT,
                    )
                }
                MarketingJourneyIntegrationProviderResult::Permanent { reason } => {
                    validate_failure_reason(&reason)?;
                    (
                        "failed",
                        None,
                        Some(reason),
                        now,
                        Some(now),
                        MARKETING_JOURNEY_INTEGRATION_FAILED_EVENT,
                    )
                }
            };
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let row = sqlx::query(
            "SELECT integration_ref, journey_id, run_id, node_id, profile_id,
                    scope, idempotency_key, attempt_count, version, claim_id,
                    claimed_by, claim_until, provider_reference, last_error,
                    created_at
               FROM erpx_tenant.marketing_journey_integration_invocations
              WHERE holding_ref=$1 AND invocation_id=$2 FOR UPDATE",
        )
        .bind(&self.holding_ref)
        .bind(claim.invocation.invocation_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "Marketing journey integration invocation",
            id: claim.invocation.invocation_id,
        })?;
        let current_claim: Option<Uuid> = row.try_get("claim_id")?;
        let claimed_by: Option<Uuid> = row.try_get("claimed_by")?;
        let claim_until: Option<DateTime<Utc>> = row.try_get("claim_until")?;
        if current_claim != Some(claim.claim_id)
            || claimed_by != Some(worker_id)
            || claim_until.is_none_or(|until| now >= until)
        {
            return Err(PersistenceError::ClaimLost {
                worker_id,
                event_id: claim.invocation.invocation_id,
            });
        }
        let current_version: i64 = row.try_get("version")?;
        let version = u64::try_from(current_version)
            .map_err(|_| invalid("integration version is negative"))?
            .checked_add(1)
            .ok_or_else(|| invalid("integration version overflowed"))?;
        let invocation = JourneyIntegrationInvocation::new(
            claim.invocation.invocation_id,
            row.try_get::<String, _>("integration_ref")?,
            row.try_get("journey_id")?,
            row.try_get("run_id")?,
            row.try_get("node_id")?,
            row.try_get("profile_id")?,
            serde_json::from_value(row.try_get("scope")?)?,
            row.try_get::<String, _>("idempotency_key")?,
            row.try_get("created_at")?,
        )
        .map_err(|error| invalid(error.to_string()))?;
        let snapshot = InvocationSnapshot {
            invocation,
            state: state.into(),
            attempt_count: u32::try_from(row.try_get::<i32, _>("attempt_count")?)
                .map_err(|_| invalid("integration attempt count is negative"))?,
            version,
            provider_reference,
            last_error,
            next_attempt_at,
            completed_at,
        };
        let payload = serde_json::to_value(&snapshot)?;
        let affected = sqlx::query(
            "UPDATE erpx_tenant.marketing_journey_integration_invocations
                SET state=$3, version=$4, provider_reference=$5, last_error=$6,
                    next_attempt_at=$7, claim_id=NULL, claimed_by=NULL,
                    claim_until=NULL, completed_at=$8, payload=$9, updated_at=$10
              WHERE holding_ref=$1 AND invocation_id=$2 AND version=$11",
        )
        .bind(&self.holding_ref)
        .bind(claim.invocation.invocation_id)
        .bind(&snapshot.state)
        .bind(i64::try_from(version).map_err(|_| invalid("version exceeds BIGINT"))?)
        .bind(&snapshot.provider_reference)
        .bind(&snapshot.last_error)
        .bind(snapshot.next_attempt_at)
        .bind(snapshot.completed_at)
        .bind(&payload)
        .bind(now)
        .bind(current_version)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if affected != 1 {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: MARKETING_JOURNEY_INTEGRATION_AGGREGATE.into(),
                aggregate_id: claim.invocation.invocation_id,
                expected: Some(version - 1),
                actual: None,
            });
        }
        let context = worker_context(&snapshot.invocation)?;
        let event_id = deterministic_event_id(snapshot.invocation.invocation_id, version);
        let event = EventEnvelope {
            contract_version: context.contract_version,
            event_id,
            event_type: event_type.into(),
            event_version: 1,
            occurred_at: now,
            aggregate_type: MARKETING_JOURNEY_INTEGRATION_AGGREGATE.into(),
            aggregate_id: snapshot.invocation.invocation_id,
            aggregate_version: version,
            tenant_scope: snapshot.invocation.scope.clone(),
            actor: context.actor,
            correlation_id: context.correlation_id,
            causation_id: None,
            idempotency_key: Some(format!(
                "{}:{}",
                snapshot.invocation.idempotency_key, version
            )),
            payload: serde_json::json!({ "aggregate": payload }),
        };
        append_event(&mut tx, &self.holding_ref, &event).await?;
        tx.commit().await?;
        Ok(MarketingJourneyIntegrationWriteReceipt {
            invocation_id: snapshot.invocation.invocation_id,
            version,
            event_id,
        })
    }

    pub async fn project_pending_read_model(
        &self,
        page_size: u32,
    ) -> Result<u64, PersistenceError> {
        if page_size == 0 || page_size > MAX_MARKETING_JOURNEY_INTEGRATION_PROJECTION_PAGE_SIZE {
            return Err(invalid("integration projection page is outside its bound"));
        }
        let mut read_tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let checkpoint = sqlx::query(
            "SELECT last_occurred_at, last_event_id
               FROM erpx_tenant.marketing_journey_integration_projection_checkpoints
              WHERE holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .fetch_optional(&mut *read_tx)
        .await?;
        let (last_at, last_id) = if let Some(row) = checkpoint {
            (
                row.try_get("last_occurred_at")?,
                row.try_get("last_event_id")?,
            )
        } else {
            (
                DateTime::<Utc>::from_timestamp(0, 0).expect("Unix epoch is valid"),
                Uuid::nil(),
            )
        };
        read_tx.commit().await?;

        let mut source_tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let events = sqlx::query(
            "SELECT event_id, event_type, aggregate_id, aggregate_version,
                    occurred_at, payload
               FROM erpx_tenant.outbox_events
              WHERE holding_ref=$1 AND aggregate_type=$2
                AND (occurred_at > $3 OR (occurred_at = $3 AND event_id > $4))
              ORDER BY occurred_at, event_id
              LIMIT $5",
        )
        .bind(&self.holding_ref)
        .bind(MARKETING_JOURNEY_INTEGRATION_AGGREGATE)
        .bind(last_at)
        .bind(last_id)
        .bind(i64::from(page_size))
        .fetch_all(&mut *source_tx)
        .await?;
        source_tx.commit().await?;
        if events.is_empty() {
            return Ok(0);
        }
        let mut read_tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let mut projected = 0_u64;
        let mut cursor_at = last_at;
        let mut cursor_id = last_id;
        for event in events {
            let event_id: Uuid = event.try_get("event_id")?;
            let event_type: String = event.try_get("event_type")?;
            let aggregate_id: Uuid = event.try_get("aggregate_id")?;
            let source_version: i64 = event.try_get("aggregate_version")?;
            let occurred_at: DateTime<Utc> = event.try_get("occurred_at")?;
            let payload: Value = event.try_get("payload")?;
            let source_version = u64::try_from(source_version)
                .map_err(|_| invalid("integration source version is negative"))?;
            validate_integration_event(
                event_id,
                &event_type,
                aggregate_id,
                source_version,
                &payload,
            )?;
            let snapshot: InvocationSnapshot = serde_json::from_value(
                payload
                    .get("aggregate")
                    .cloned()
                    .ok_or_else(|| invalid("integration source aggregate is missing"))?,
            )?;
            let current: Option<i64> = sqlx::query_scalar(
                "SELECT source_version
                   FROM erpx_tenant.marketing_journey_integration_read_rows
                  WHERE holding_ref=$1 AND invocation_id=$2 FOR UPDATE",
            )
            .bind(&self.holding_ref)
            .bind(aggregate_id)
            .fetch_optional(&mut *read_tx)
            .await?;
            if current.is_some_and(|version| version >= source_version as i64) {
                if current == Some(source_version as i64) {
                    let existing_payload: Value = sqlx::query_scalar(
                        "SELECT payload
                           FROM erpx_tenant.marketing_journey_integration_read_rows
                          WHERE holding_ref=$1 AND invocation_id=$2",
                    )
                    .bind(&self.holding_ref)
                    .bind(aggregate_id)
                    .fetch_one(&mut *read_tx)
                    .await?;
                    if existing_payload != serde_json::to_value(&snapshot)? {
                        return Err(invalid(
                            "integration projection same-version payload conflict",
                        ));
                    }
                }
                cursor_at = occurred_at;
                cursor_id = event_id;
                continue;
            }
            if current.is_some_and(|version| version + 1 != source_version as i64)
                || current.is_none() && source_version != 1
            {
                return Err(invalid("integration projection version is not contiguous"));
            }
            sqlx::query(
                "INSERT INTO erpx_tenant.marketing_journey_integration_read_rows
                    (holding_ref, invocation_id, integration_ref, journey_id, run_id,
                     node_id, profile_id, scope, idempotency_key, state, attempt_count,
                     version, provider_reference, last_error, next_attempt_at,
                     completed_at, source_version, source_event_id, payload, updated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20)
                 ON CONFLICT (holding_ref, invocation_id) DO UPDATE SET
                    integration_ref=EXCLUDED.integration_ref, journey_id=EXCLUDED.journey_id,
                    run_id=EXCLUDED.run_id, node_id=EXCLUDED.node_id, profile_id=EXCLUDED.profile_id,
                    scope=EXCLUDED.scope, idempotency_key=EXCLUDED.idempotency_key,
                    state=EXCLUDED.state, attempt_count=EXCLUDED.attempt_count,
                    version=EXCLUDED.version, provider_reference=EXCLUDED.provider_reference,
                    last_error=EXCLUDED.last_error, next_attempt_at=EXCLUDED.next_attempt_at,
                    completed_at=EXCLUDED.completed_at, source_version=EXCLUDED.source_version,
                    source_event_id=EXCLUDED.source_event_id, payload=EXCLUDED.payload,
                    updated_at=EXCLUDED.updated_at",
            )
            .bind(&self.holding_ref)
            .bind(snapshot.invocation.invocation_id)
            .bind(&snapshot.invocation.integration_ref)
            .bind(snapshot.invocation.journey_id)
            .bind(snapshot.invocation.run_id)
            .bind(snapshot.invocation.node_id)
            .bind(snapshot.invocation.profile_id)
            .bind(serde_json::to_value(&snapshot.invocation.scope)?)
            .bind(&snapshot.invocation.idempotency_key)
            .bind(&snapshot.state)
            .bind(i32::try_from(snapshot.attempt_count).map_err(|_| invalid("attempt exceeds INTEGER"))?)
            .bind(i64::try_from(snapshot.version).map_err(|_| invalid("version exceeds BIGINT"))?)
            .bind(&snapshot.provider_reference)
            .bind(&snapshot.last_error)
            .bind(snapshot.next_attempt_at)
            .bind(snapshot.completed_at)
            .bind(i64::try_from(source_version).map_err(|_| invalid("version exceeds BIGINT"))?)
            .bind(event_id)
            .bind(serde_json::to_value(&snapshot)?)
            .bind(occurred_at)
            .execute(&mut *read_tx)
            .await?;
            projected = projected.saturating_add(1);
            cursor_at = occurred_at;
            cursor_id = event_id;
        }
        sqlx::query(
            "INSERT INTO erpx_tenant.marketing_journey_integration_projection_checkpoints
                (holding_ref, last_occurred_at, last_event_id, updated_at)
             VALUES ($1,$2,$3,now())
             ON CONFLICT (holding_ref) DO UPDATE SET
                last_occurred_at=EXCLUDED.last_occurred_at,
                last_event_id=EXCLUDED.last_event_id, updated_at=now()",
        )
        .bind(&self.holding_ref)
        .bind(cursor_at)
        .bind(cursor_id)
        .execute(&mut *read_tx)
        .await?;
        read_tx.commit().await?;
        Ok(projected)
    }

    /// Atomically rebuild the disposable invocation read model from the
    /// immutable tenant outbox. The source transaction holds a table fence
    /// for the whole replacement, while the read transaction keeps the old
    /// projection committed until every event and checkpoint has been
    /// replayed successfully. Any malformed event, source movement, bound
    /// violation, or read-side failure therefore rolls back the replacement
    /// and preserves the previous read model.
    pub async fn rebuild_projection_with_source_fence(
        &self,
        expected_source_event_count: Option<u64>,
        page_size: u32,
    ) -> Result<(MarketingJourneyIntegrationProjectionRebuildPreparation, u64), PersistenceError>
    {
        if page_size == 0 || page_size > MAX_MARKETING_JOURNEY_INTEGRATION_PROJECTION_PAGE_SIZE {
            return Err(invalid("integration rebuild page is outside its bound"));
        }
        let mut source_tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        sqlx::query("LOCK TABLE erpx_tenant.outbox_events IN SHARE MODE")
            .execute(&mut *source_tx)
            .await?;
        let actual_source_event_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM erpx_tenant.outbox_events
              WHERE holding_ref=$1 AND aggregate_type=$2",
        )
        .bind(&self.holding_ref)
        .bind(MARKETING_JOURNEY_INTEGRATION_AGGREGATE)
        .fetch_one(&mut *source_tx)
        .await?;
        let actual_source_event_count = u64::try_from(actual_source_event_count)
            .map_err(|_| invalid("integration source event count is negative"))?;
        if expected_source_event_count.is_some_and(|expected| expected != actual_source_event_count)
        {
            return Err(invalid("integration source fence changed before rebuild"));
        }
        if actual_source_event_count > MAX_MARKETING_JOURNEY_INTEGRATION_REBUILD_EVENTS {
            return Err(invalid("integration rebuild source-event bound exceeded"));
        }

        let mut read_tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        lock_integration_projection(&mut read_tx, &self.holding_ref).await?;
        let rows_cleared = sqlx::query(
            "DELETE FROM erpx_tenant.marketing_journey_integration_read_rows
              WHERE holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .execute(&mut *read_tx)
        .await?
        .rows_affected();
        let checkpoints_cleared = sqlx::query(
            "DELETE FROM erpx_tenant.marketing_journey_integration_projection_checkpoints
              WHERE holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .execute(&mut *read_tx)
        .await?
        .rows_affected();
        let preparation = MarketingJourneyIntegrationProjectionRebuildPreparation {
            holding_ref: self.holding_ref.clone(),
            rows_cleared,
            checkpoints_cleared,
        };

        let epoch = DateTime::<Utc>::from_timestamp(0, 0).expect("Unix epoch is valid");
        let mut cursor_at = epoch;
        let mut cursor_id = Uuid::nil();
        let mut events_replayed = 0_u64;
        loop {
            let events = sqlx::query(
                "SELECT event_id, event_type, aggregate_id, aggregate_version,
                        occurred_at, payload
                   FROM erpx_tenant.outbox_events
                  WHERE holding_ref=$1 AND aggregate_type=$2
                    AND (occurred_at > $3 OR (occurred_at = $3 AND event_id > $4))
                  ORDER BY occurred_at, event_id
                  LIMIT $5",
            )
            .bind(&self.holding_ref)
            .bind(MARKETING_JOURNEY_INTEGRATION_AGGREGATE)
            .bind(cursor_at)
            .bind(cursor_id)
            .bind(i64::from(page_size))
            .fetch_all(&mut *source_tx)
            .await?;
            if events.is_empty() {
                break;
            }
            for event in &events {
                let event_id: Uuid = event.try_get("event_id")?;
                let event_type: String = event.try_get("event_type")?;
                let aggregate_id: Uuid = event.try_get("aggregate_id")?;
                let source_version =
                    u64::try_from(event.try_get::<i64, _>("aggregate_version")?)
                        .map_err(|_| invalid("integration source version is negative"))?;
                let occurred_at: DateTime<Utc> = event.try_get("occurred_at")?;
                let payload: Value = event.try_get("payload")?;
                if apply_integration_event(
                    &mut read_tx,
                    &self.holding_ref,
                    IntegrationOutboxEvent {
                        event_id,
                        event_type: &event_type,
                        aggregate_id,
                        source_version,
                        occurred_at,
                        payload: &payload,
                    },
                )
                .await?
                {
                    events_replayed = events_replayed.saturating_add(1);
                }
                cursor_at = occurred_at;
                cursor_id = event_id;
            }
            if events.len() < usize::try_from(page_size).map_err(|_| invalid("page size"))? {
                break;
            }
        }

        let final_source_event_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM erpx_tenant.outbox_events
              WHERE holding_ref=$1 AND aggregate_type=$2",
        )
        .bind(&self.holding_ref)
        .bind(MARKETING_JOURNEY_INTEGRATION_AGGREGATE)
        .fetch_one(&mut *source_tx)
        .await?;
        let final_source_event_count = u64::try_from(final_source_event_count)
            .map_err(|_| invalid("integration final source event count is negative"))?;
        if final_source_event_count != actual_source_event_count {
            return Err(invalid("integration source fence changed during rebuild"));
        }
        if actual_source_event_count > 0 {
            sqlx::query(
                "INSERT INTO erpx_tenant.marketing_journey_integration_projection_checkpoints
                    (holding_ref, last_occurred_at, last_event_id, updated_at)
                 VALUES ($1,$2,$3,now())
                 ON CONFLICT (holding_ref) DO UPDATE SET
                    last_occurred_at=EXCLUDED.last_occurred_at,
                    last_event_id=EXCLUDED.last_event_id, updated_at=now()",
            )
            .bind(&self.holding_ref)
            .bind(cursor_at)
            .bind(cursor_id)
            .execute(&mut *read_tx)
            .await?;
        }
        read_tx.commit().await?;
        source_tx.commit().await?;
        Ok((preparation, events_replayed))
    }

    pub async fn read_page(
        &self,
        after_id: Option<Uuid>,
        page_size: u32,
    ) -> Result<Vec<MarketingJourneyIntegrationReadRow>, PersistenceError> {
        if page_size == 0 || page_size > MAX_MARKETING_JOURNEY_INTEGRATION_PROJECTION_PAGE_SIZE {
            return Err(invalid("integration read page is outside its bound"));
        }
        let mut tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let rows = sqlx::query(
            "SELECT invocation_id, integration_ref, journey_id, run_id, node_id,
                    profile_id, scope, idempotency_key, state, attempt_count, version,
                    provider_reference, last_error, next_attempt_at, completed_at,
                    source_version, source_event_id, payload
               FROM erpx_tenant.marketing_journey_integration_read_rows
              WHERE holding_ref=$1 AND ($2::uuid IS NULL OR invocation_id>$2)
              ORDER BY invocation_id
              LIMIT $3",
        )
        .bind(&self.holding_ref)
        .bind(after_id)
        .bind(i64::from(page_size))
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        rows.into_iter().map(read_row).collect()
    }

    pub async fn reconcile_projection(
        &self,
    ) -> Result<MarketingJourneyIntegrationProjectionReconciliation, PersistenceError> {
        let mut source_tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        sqlx::query("LOCK TABLE erpx_tenant.outbox_events IN SHARE MODE")
            .execute(&mut *source_tx)
            .await?;
        let source_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM erpx_tenant.marketing_journey_integration_invocations
              WHERE holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .fetch_one(&mut *source_tx)
        .await?;
        let event_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM erpx_tenant.outbox_events
              WHERE holding_ref=$1 AND aggregate_type=$2",
        )
        .bind(&self.holding_ref)
        .bind(MARKETING_JOURNEY_INTEGRATION_AGGREGATE)
        .fetch_one(&mut *source_tx)
        .await?;
        let source_count_u64 = u64::try_from(source_count).map_err(|_| invalid("source count"))?;
        let event_count_u64 = u64::try_from(event_count).map_err(|_| invalid("event count"))?;
        if source_count_u64 > MAX_RECONCILIATION_ROWS as u64
            || event_count_u64 > MAX_RECONCILIATION_ROWS as u64
        {
            return Err(invalid("integration reconciliation bound exceeded"));
        }
        let source_rows = sqlx::query(
            "SELECT invocation_id, version, payload
               FROM erpx_tenant.marketing_journey_integration_invocations
              WHERE holding_ref=$1 ORDER BY invocation_id",
        )
        .bind(&self.holding_ref)
        .fetch_all(&mut *source_tx)
        .await?;
        let event_rows = sqlx::query(
            "SELECT event_id, event_type, aggregate_id, aggregate_version, payload
               FROM erpx_tenant.outbox_events
              WHERE holding_ref=$1 AND aggregate_type=$2
              ORDER BY aggregate_id, aggregate_version, event_id",
        )
        .bind(&self.holding_ref)
        .bind(MARKETING_JOURNEY_INTEGRATION_AGGREGATE)
        .fetch_all(&mut *source_tx)
        .await?;
        let mut source_by_id = BTreeMap::new();
        for row in &source_rows {
            let id: Uuid = row.try_get("invocation_id")?;
            let version = u64::try_from(row.try_get::<i64, _>("version")?)
                .map_err(|_| invalid("integration source version is negative"))?;
            let payload: Value = row.try_get("payload")?;
            let snapshot: InvocationSnapshot = serde_json::from_value(payload.clone())?;
            if snapshot.invocation.invocation_id != id || snapshot.version != version {
                return Err(invalid("integration source snapshot identity is invalid"));
            }
            source_by_id.insert(id, (version, payload));
        }
        let mut event_versions: BTreeMap<Uuid, Vec<u64>> = BTreeMap::new();
        let mut latest_event_payloads = BTreeMap::new();
        for row in &event_rows {
            let event_id: Uuid = row.try_get("event_id")?;
            let event_type: String = row.try_get("event_type")?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            let source_version = u64::try_from(row.try_get::<i64, _>("aggregate_version")?)
                .map_err(|_| invalid("integration source version is negative"))?;
            let payload: Value = row.try_get("payload")?;
            validate_integration_event(
                event_id,
                &event_type,
                aggregate_id,
                source_version,
                &payload,
            )?;
            if !source_by_id.contains_key(&aggregate_id) {
                return Err(invalid("integration outbox event has no source invocation"));
            }
            event_versions
                .entry(aggregate_id)
                .or_default()
                .push(source_version);
            latest_event_payloads.insert(
                (aggregate_id, source_version),
                payload
                    .get("aggregate")
                    .cloned()
                    .ok_or_else(|| invalid("integration source aggregate is missing"))?,
            );
        }
        let mut expected_event_count = 0_u64;
        for (id, (version, payload)) in &source_by_id {
            let versions = event_versions
                .get(id)
                .ok_or_else(|| invalid("integration source invocation has no outbox history"))?;
            let expected_versions = (1..=*version).collect::<Vec<_>>();
            if versions != &expected_versions
                || latest_event_payloads.get(&(*id, *version)) != Some(payload)
            {
                return Err(invalid("integration outbox history is not contiguous"));
            }
            expected_event_count = expected_event_count
                .checked_add(*version)
                .ok_or_else(|| invalid("integration event count overflowed"))?;
        }
        if expected_event_count != event_count_u64 {
            return Err(invalid("integration source and outbox counts disagree"));
        }
        source_tx.commit().await?;
        let mut read_tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let read_rows = sqlx::query(
            "SELECT invocation_id, source_version, payload
               FROM erpx_tenant.marketing_journey_integration_read_rows
              WHERE holding_ref=$1 ORDER BY invocation_id LIMIT $2",
        )
        .bind(&self.holding_ref)
        .bind(MAX_RECONCILIATION_ROWS + 1)
        .fetch_all(&mut *read_tx)
        .await?;
        read_tx.commit().await?;
        if read_rows.len()
            > usize::try_from(MAX_RECONCILIATION_ROWS)
                .map_err(|_| invalid("reconciliation row bound"))?
        {
            return Err(invalid("integration read reconciliation bound exceeded"));
        }
        let read_count = u64::try_from(read_rows.len()).map_err(|_| invalid("read count"))?;
        let mut read_by_id = BTreeMap::new();
        for row in read_rows {
            let id: Uuid = row.try_get("invocation_id")?;
            let read_version = u64::try_from(row.try_get::<i64, _>("source_version")?)
                .map_err(|_| invalid("integration read version is negative"))?;
            let payload: Value = row.try_get("payload")?;
            let snapshot: InvocationSnapshot = serde_json::from_value(payload.clone())?;
            if snapshot.invocation.invocation_id != id || snapshot.version != read_version {
                return Err(invalid("integration read snapshot identity is invalid"));
            }
            read_by_id.insert(id, (read_version, payload));
        }
        let mut missing = 0_u64;
        let mut stale = 0_u64;
        let mut ahead = 0_u64;
        let mut mismatch = 0_u64;
        for row in source_rows {
            let id: Uuid = row.try_get("invocation_id")?;
            let version = u64::try_from(row.try_get::<i64, _>("version")?)
                .map_err(|_| invalid("integration source version is negative"))?;
            let payload: Value = row.try_get("payload")?;
            match read_by_id.remove(&id) {
                None => missing = missing.saturating_add(1),
                Some((read_version, read_payload)) => {
                    if read_version < version {
                        stale = stale.saturating_add(1);
                    }
                    if read_version > version {
                        ahead = ahead.saturating_add(1);
                    }
                    if read_version == version && read_payload != payload {
                        mismatch = mismatch.saturating_add(1);
                    }
                }
            }
        }
        let orphan = u64::try_from(read_by_id.len()).map_err(|_| invalid("orphan count"))?;
        let healthy = missing == 0
            && stale == 0
            && ahead == 0
            && orphan == 0
            && mismatch == 0
            && read_count == source_count_u64
            && event_count_u64 == expected_event_count;
        Ok(MarketingJourneyIntegrationProjectionReconciliation {
            source_invocation_count: source_count_u64,
            source_event_count: event_count_u64,
            read_invocation_count: read_count,
            missing_invocation_count: missing,
            stale_invocation_count: stale,
            ahead_invocation_count: ahead,
            orphan_read_count: orphan,
            payload_mismatch_count: mismatch,
            healthy,
        })
    }
}

struct IntegrationOutboxEvent<'a> {
    event_id: Uuid,
    event_type: &'a str,
    aggregate_id: Uuid,
    source_version: u64,
    occurred_at: DateTime<Utc>,
    payload: &'a Value,
}

async fn apply_integration_event(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    event: IntegrationOutboxEvent<'_>,
) -> Result<bool, PersistenceError> {
    validate_integration_event(
        event.event_id,
        event.event_type,
        event.aggregate_id,
        event.source_version,
        event.payload,
    )?;
    let snapshot: InvocationSnapshot = serde_json::from_value(
        event
            .payload
            .get("aggregate")
            .cloned()
            .ok_or_else(|| invalid("integration source aggregate is missing"))?,
    )?;
    let current: Option<i64> = sqlx::query_scalar(
        "SELECT source_version
           FROM erpx_tenant.marketing_journey_integration_read_rows
          WHERE holding_ref=$1 AND invocation_id=$2 FOR UPDATE",
    )
    .bind(holding_ref)
    .bind(event.aggregate_id)
    .fetch_optional(&mut **tx)
    .await?;
    if current.is_some_and(|version| version >= event.source_version as i64) {
        if current == Some(event.source_version as i64) {
            let existing_payload: Option<Value> = sqlx::query_scalar(
                "SELECT payload
                   FROM erpx_tenant.marketing_journey_integration_read_rows
                  WHERE holding_ref=$1 AND invocation_id=$2",
            )
            .bind(holding_ref)
            .bind(event.aggregate_id)
            .fetch_optional(&mut **tx)
            .await?;
            if existing_payload.is_some_and(|existing| existing != serde_json::json!(snapshot)) {
                return Err(invalid(
                    "integration projection same-version payload conflict",
                ));
            }
        }
        return Ok(false);
    }
    if current.is_some_and(|version| version + 1 != event.source_version as i64)
        || current.is_none() && event.source_version != 1
    {
        return Err(invalid("integration projection version is not contiguous"));
    }
    let snapshot_payload = serde_json::to_value(&snapshot)?;
    sqlx::query(
        "INSERT INTO erpx_tenant.marketing_journey_integration_read_rows
            (holding_ref, invocation_id, integration_ref, journey_id, run_id,
             node_id, profile_id, scope, idempotency_key, state, attempt_count,
             version, provider_reference, last_error, next_attempt_at,
             completed_at, source_version, source_event_id, payload, updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20)
         ON CONFLICT (holding_ref, invocation_id) DO UPDATE SET
            integration_ref=EXCLUDED.integration_ref, journey_id=EXCLUDED.journey_id,
            run_id=EXCLUDED.run_id, node_id=EXCLUDED.node_id, profile_id=EXCLUDED.profile_id,
            scope=EXCLUDED.scope, idempotency_key=EXCLUDED.idempotency_key,
            state=EXCLUDED.state, attempt_count=EXCLUDED.attempt_count,
            version=EXCLUDED.version, provider_reference=EXCLUDED.provider_reference,
            last_error=EXCLUDED.last_error, next_attempt_at=EXCLUDED.next_attempt_at,
            completed_at=EXCLUDED.completed_at, source_version=EXCLUDED.source_version,
            source_event_id=EXCLUDED.source_event_id, payload=EXCLUDED.payload,
            updated_at=EXCLUDED.updated_at",
    )
    .bind(holding_ref)
    .bind(snapshot.invocation.invocation_id)
    .bind(&snapshot.invocation.integration_ref)
    .bind(snapshot.invocation.journey_id)
    .bind(snapshot.invocation.run_id)
    .bind(snapshot.invocation.node_id)
    .bind(snapshot.invocation.profile_id)
    .bind(serde_json::to_value(&snapshot.invocation.scope)?)
    .bind(&snapshot.invocation.idempotency_key)
    .bind(&snapshot.state)
    .bind(i32::try_from(snapshot.attempt_count).map_err(|_| invalid("attempt exceeds INTEGER"))?)
    .bind(i64::try_from(snapshot.version).map_err(|_| invalid("version exceeds BIGINT"))?)
    .bind(&snapshot.provider_reference)
    .bind(&snapshot.last_error)
    .bind(snapshot.next_attempt_at)
    .bind(snapshot.completed_at)
    .bind(i64::try_from(event.source_version).map_err(|_| invalid("version exceeds BIGINT"))?)
    .bind(event.event_id)
    .bind(snapshot_payload)
    .bind(event.occurred_at)
    .execute(&mut **tx)
    .await?;
    Ok(true)
}

fn validate_integration_event(
    event_id: Uuid,
    event_type: &str,
    aggregate_id: Uuid,
    source_version: u64,
    payload: &Value,
) -> Result<(), PersistenceError> {
    validate_uuid(event_id, "integration event ID")?;
    validate_uuid(aggregate_id, "integration aggregate ID")?;
    if source_version == 0 {
        return Err(invalid("integration source version must be positive"));
    }
    let allowed = matches!(
        event_type,
        MARKETING_JOURNEY_INTEGRATION_INVOKED_EVENT
            | MARKETING_JOURNEY_INTEGRATION_SUCCEEDED_EVENT
            | MARKETING_JOURNEY_INTEGRATION_RETRY_EVENT
            | MARKETING_JOURNEY_INTEGRATION_FAILED_EVENT
    );
    if !allowed {
        return Err(invalid("unknown integration source event"));
    }
    let snapshot: InvocationSnapshot = serde_json::from_value(
        payload
            .get("aggregate")
            .cloned()
            .ok_or_else(|| invalid("integration source aggregate is missing"))?,
    )?;
    if snapshot.invocation.invocation_id != aggregate_id || snapshot.version != source_version {
        return Err(invalid("integration source identity is invalid"));
    }
    if source_version == 1 && event_type != MARKETING_JOURNEY_INTEGRATION_INVOKED_EVENT {
        return Err(invalid(
            "integration invocation must start with an invoked event",
        ));
    }
    if source_version > 1 && event_type == MARKETING_JOURNEY_INTEGRATION_INVOKED_EVENT {
        return Err(invalid("integration invoked event cannot repeat"));
    }
    Ok(())
}

async fn lock_integration_projection(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
) -> Result<(), PersistenceError> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!(
            "marketing-journey-integration:projection:{holding_ref}"
        ))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn append_event(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    event: &EventEnvelope,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_tenant.outbox_events
            (event_id, aggregate_type, aggregate_id, holding_ref,
             aggregate_version, event_type, payload, occurred_at,
             contract_version, event_version, tenant_scope, actor,
             correlation_id, causation_id, idempotency_key)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
         ON CONFLICT (event_id) DO NOTHING",
    )
    .bind(event.event_id)
    .bind(&event.aggregate_type)
    .bind(event.aggregate_id)
    .bind(holding_ref)
    .bind(i64::try_from(event.aggregate_version).map_err(|_| invalid("event version"))?)
    .bind(&event.event_type)
    .bind(&event.payload)
    .bind(event.occurred_at)
    .bind(i32::from(event.contract_version))
    .bind(i32::from(event.event_version))
    .bind(serde_json::to_value(&event.tenant_scope)?)
    .bind(serde_json::to_value(&event.actor)?)
    .bind(event.correlation_id)
    .bind(event.causation_id)
    .bind(&event.idempotency_key)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn read_row(
    row: sqlx::postgres::PgRow,
) -> Result<MarketingJourneyIntegrationReadRow, PersistenceError> {
    let payload: Value = row.try_get("payload")?;
    let snapshot: InvocationSnapshot = serde_json::from_value(payload.clone())?;
    Ok(MarketingJourneyIntegrationReadRow {
        invocation: snapshot.invocation,
        state: row.try_get("state")?,
        attempt_count: u32::try_from(row.try_get::<i32, _>("attempt_count")?)
            .map_err(|_| invalid("read attempt count is negative"))?,
        version: u64::try_from(row.try_get::<i64, _>("version")?)
            .map_err(|_| invalid("read version is negative"))?,
        provider_reference: row.try_get("provider_reference")?,
        last_error: row.try_get("last_error")?,
        next_attempt_at: row.try_get("next_attempt_at")?,
        completed_at: row.try_get("completed_at")?,
        source_version: u64::try_from(row.try_get::<i64, _>("source_version")?)
            .map_err(|_| invalid("source version is negative"))?,
        source_event_id: row.try_get("source_event_id")?,
        payload,
    })
}

fn worker_context(
    invocation: &JourneyIntegrationInvocation,
) -> Result<CommandContext, PersistenceError> {
    let mut context = CommandContext::try_new(
        format!(
            "marketing-journey-integration:{}",
            invocation.idempotency_key
        ),
        ActorAttribution {
            principal_ref: "worker:marketing_journey_integration".into(),
            kind: ActorKind::SystemWorker,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        invocation.scope.clone(),
        "marketing_journey_integration",
    )
    .map_err(|error| invalid(error.to_string()))?;
    context.correlation_id = deterministic_event_id(invocation.run_id, 0);
    context.audit.correlation_id = context.correlation_id;
    Ok(context)
}

fn deterministic_event_id(invocation_id: Uuid, version: u64) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("erpx-marketing-journey-integration:{invocation_id}:{version}").as_bytes(),
    )
}

fn validate_holding_ref(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
        return Err(PersistenceError::Configuration(
            "holding reference must be bounded and non-empty",
        ));
    }
    Ok(())
}

fn validate_uuid(value: Uuid, field: &str) -> Result<(), PersistenceError> {
    if value.is_nil() {
        return Err(invalid(format!("{field} cannot be nil")));
    }
    Ok(())
}

fn validate_provider_reference(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(invalid("provider reference is invalid"));
    }
    Ok(())
}

fn validate_failure_reason(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty()
        || value.len() > 128
        || value.chars().any(|character| character.is_control())
    {
        return Err(invalid("integration failure reason is invalid"));
    }
    Ok(())
}

fn invalid(message: impl Into<String>) -> PersistenceError {
    PersistenceError::InvalidData(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_domain::contracts::TenantScope;
    use erpx_domain::marketing_journey::JourneyIntegrationInvocation;

    fn invocation() -> JourneyIntegrationInvocation {
        JourneyIntegrationInvocation::new(
            Uuid::from_u128(1),
            "approved-webhook",
            Uuid::from_u128(2),
            Uuid::from_u128(3),
            Uuid::from_u128(4),
            Uuid::from_u128(5),
            TenantScope::Holding {
                holding_id: Uuid::from_u128(6),
            },
            "journey-invocation-1",
            DateTime::<Utc>::from_timestamp(1_700_000_000, 0).expect("timestamp"),
        )
        .expect("valid invocation")
    }

    fn event_payload(version: u64) -> Value {
        serde_json::to_value(serde_json::json!({
            "aggregate": serde_json::to_value(InvocationSnapshot {
                invocation: invocation(),
                state: if version == 1 {
                    "queued".into()
                } else {
                    "succeeded".into()
                },
                attempt_count: if version == 1 { 0 } else { 1 },
                version,
                provider_reference: (version > 1).then(|| "provider-1".to_owned()),
                last_error: None::<String>,
                next_attempt_at: DateTime::<Utc>::from_timestamp(1_700_000_000, 0)
                    .expect("timestamp"),
                completed_at: (version > 1)
                    .then(|| DateTime::<Utc>::from_timestamp(1_700_000_001, 0).expect("timestamp")),
            })
            .expect("snapshot")
        }))
        .expect("event payload")
    }

    #[test]
    fn integration_event_validation_requires_contiguous_lifecycle_start() {
        let payload = event_payload(1);
        assert!(
            validate_integration_event(
                Uuid::from_u128(10),
                MARKETING_JOURNEY_INTEGRATION_INVOKED_EVENT,
                Uuid::from_u128(1),
                1,
                &payload,
            )
            .is_ok()
        );

        assert!(
            validate_integration_event(
                Uuid::from_u128(11),
                MARKETING_JOURNEY_INTEGRATION_SUCCEEDED_EVENT,
                Uuid::from_u128(1),
                1,
                &payload,
            )
            .is_err()
        );
        assert!(
            validate_integration_event(
                Uuid::from_u128(12),
                MARKETING_JOURNEY_INTEGRATION_INVOKED_EVENT,
                Uuid::from_u128(1),
                2,
                &event_payload(2),
            )
            .is_err()
        );
    }

    #[test]
    fn integration_event_validation_binds_identity_and_version() {
        let payload = event_payload(2);
        assert!(
            validate_integration_event(
                Uuid::from_u128(20),
                MARKETING_JOURNEY_INTEGRATION_SUCCEEDED_EVENT,
                Uuid::from_u128(1),
                2,
                &payload,
            )
            .is_ok()
        );
        assert!(
            validate_integration_event(
                Uuid::from_u128(21),
                MARKETING_JOURNEY_INTEGRATION_SUCCEEDED_EVENT,
                Uuid::from_u128(9),
                2,
                &payload,
            )
            .is_err()
        );
        assert!(
            validate_integration_event(
                Uuid::from_u128(22),
                MARKETING_JOURNEY_INTEGRATION_SUCCEEDED_EVENT,
                Uuid::from_u128(1),
                3,
                &payload,
            )
            .is_err()
        );
    }
}
