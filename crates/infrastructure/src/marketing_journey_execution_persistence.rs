//! Durable tenant-write persistence for resume-safe Marketing journey cursors.
//!
//! The initial cursor is a durable version-zero seed. Every execution fact
//! after that is committed atomically with the cursor, a complete shared
//! outbox envelope and an exact idempotency receipt.

use chrono::{DateTime, Duration, Utc};
use erpx_domain::contracts::{
    ActorAttribution, ActorKind, CONTRACT_VERSION, CommandContext, EventEnvelope, TenantScope,
};
use erpx_domain::marketing::{MarketingChannel, MarketingPurpose};
use erpx_domain::marketing_crm::AcceptedLeadHandoff;
use erpx_domain::marketing_delivery::{ChannelDispatchRecord, DeliveryDispatch, RetryPolicy};
use erpx_domain::marketing_journey::{
    Journey, JourneyAction, JourneyChannelPayload, JourneyExecutionCursor, JourneyExecutionFact,
    JourneyIntegrationInvocation, JourneyNodeDefinition, JourneyNodeOutcome, JourneyStatus,
    MarketingJourneyError, SegmentChange,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::{collections::BTreeMap, sync::Arc};
use uuid::Uuid;

use crate::PersistenceError;
use crate::finance_persistence::begin_scoped;
use crate::marketing_delivery_persistence::MARKETING_JOURNEY_OWNER_NOTIFICATION_CHANNEL_EVENT;

pub const MARKETING_JOURNEY_EXECUTION_AGGREGATE: &str = "marketing_journey_execution";
pub const MARKETING_JOURNEY_EXECUTION_ADVANCED_EVENT: &str =
    "marketing.journey_execution_advanced.v1";
pub const MARKETING_JOURNEY_EXECUTION_FACT_EVENT: &str = MARKETING_JOURNEY_EXECUTION_ADVANCED_EVENT;
pub const MARKETING_JOURNEY_EXECUTION_STARTED_EVENT: &str =
    "marketing.journey_execution_started.v1";
pub const MAX_MARKETING_JOURNEY_EXECUTION_CLAIM_BATCH_SIZE: u32 = 100;
pub const MIN_MARKETING_JOURNEY_EXECUTION_LEASE_SECONDS: u64 = 1;
pub const MAX_MARKETING_JOURNEY_EXECUTION_LEASE_SECONDS: u64 = 3_600;

type JourneyConversionInput = (Uuid, Option<i64>, Option<String>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingJourneyExecutionClaim {
    pub journey: Journey,
    pub cursor: JourneyExecutionCursor,
    pub claim_id: Uuid,
    pub worker_id: Uuid,
    pub claim_until: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingJourneyProfileContext {
    pub scope: TenantScope,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub attributes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketingJourneyExecutionFailure {
    Retryable,
    Permanent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketingJourneyExecutionMutation {
    Advance {
        run_id: Uuid,
        expected_version: u64,
        outcome: JourneyNodeOutcome,
    },
    Pause {
        run_id: Uuid,
        expected_version: u64,
    },
    Resume {
        run_id: Uuid,
        expected_version: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingJourneyExecutionWriteReceipt {
    pub journey_id: Uuid,
    pub run_id: Uuid,
    pub cursor_version: u64,
    pub idempotency_key: String,
    pub event_id: Uuid,
    pub replayed: bool,
}

struct ReplaySourceEvent {
    aggregate_type: String,
    aggregate_id: Uuid,
    aggregate_version: i64,
    event_type: String,
    idempotency_key: Option<String>,
    payload: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct SqlxMarketingJourneyExecutionStore {
    write_pool: PgPool,
    holding_ref: String,
    control: Option<Arc<crate::SqlxControlPlane>>,
}

impl SqlxMarketingJourneyExecutionStore {
    pub fn new(
        write_pool: PgPool,
        holding_ref: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let holding_ref = holding_ref.into();
        validate_holding_ref(&holding_ref)?;
        Ok(Self {
            write_pool,
            holding_ref,
            control: None,
        })
    }

    /// Construct the execution store with the control-plane boundary needed
    /// to authorize approved-integration journey actions. Existing callers
    /// that only execute internal journey nodes remain valid through `new`.
    pub fn new_with_control(
        write_pool: PgPool,
        holding_ref: impl Into<String>,
        control: Arc<crate::SqlxControlPlane>,
    ) -> Result<Self, PersistenceError> {
        let mut store = Self::new(write_pool, holding_ref)?;
        store.control = Some(control);
        Ok(store)
    }

    /// Re-resolve the active, verified provider-account binding before a
    /// journey cursor can commit an external integration handoff. The worker
    /// performs the same check immediately before HTTP delivery.
    pub async fn ensure_approved_integration(
        &self,
        integration_ref: &str,
        scope: &TenantScope,
    ) -> Result<(), PersistenceError> {
        let holding_id = match scope {
            TenantScope::Platform => {
                return Err(invalid("platform scope cannot invoke a tenant integration"));
            }
            TenantScope::Holding { holding_id }
            | TenantScope::Company { holding_id, .. }
            | TenantScope::Branch { holding_id, .. }
            | TenantScope::Subtree { holding_id, .. } => *holding_id,
        };
        let control = self.control.as_ref().ok_or_else(|| {
            PersistenceError::Configuration(
                "approved integration requires the control-plane provider boundary",
            )
        })?;
        let binding = control
            .resolve_marketing_provider_account_for_callback(integration_ref)
            .await?
            .ok_or_else(|| PersistenceError::NotFound {
                entity: "Active approved Marketing provider account",
                id: Uuid::new_v5(&Uuid::NAMESPACE_URL, integration_ref.as_bytes()),
            })?;
        let has_webhook = binding.channels.iter().any(|channel| channel == "webhook")
            && binding
                .channel_routes
                .iter()
                .any(|route| route.channel == "webhook" && route.endpoint.starts_with("https://"));
        if binding.holding_ref != self.holding_ref
            || binding.holding_id != holding_id
            || !has_webhook
        {
            return Err(invalid(
                "approved integration is not bound to the journey holding",
            ));
        }
        Ok(())
    }

    /// Load the profile facts used by a journey decision from the routed
    /// write database. Branch predicates and channel recipient selection must
    /// never consult a read replica or caller-supplied profile data.
    pub async fn load_profile_context(
        &self,
        profile_id: Uuid,
    ) -> Result<Option<MarketingJourneyProfileContext>, PersistenceError> {
        validate_uuid(profile_id, "Marketing journey profile ID")?;
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let row = sqlx::query(
            "SELECT scope, email, phone, attributes
               FROM erpx_tenant.marketing_profiles
              WHERE holding_ref=$1 AND profile_id=$2 AND erased_at IS NULL
              FOR SHARE",
        )
        .bind(&self.holding_ref)
        .bind(profile_id)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        row.map(|row| {
            Ok(MarketingJourneyProfileContext {
                scope: serde_json::from_value(row.try_get("scope")?)?,
                email: row.try_get("email")?,
                phone: row.try_get("phone")?,
                attributes: serde_json::from_value(row.try_get("attributes")?)?,
            })
        })
        .transpose()
    }

    pub fn holding_ref(&self) -> &str {
        &self.holding_ref
    }

    /// Persist the version-zero cursor seed. Execution facts begin at version
    /// one, so the shared outbox positive-version constraint remains intact.
    pub async fn start_cursor(
        &self,
        journey: &Journey,
        cursor: &JourneyExecutionCursor,
    ) -> Result<(), PersistenceError> {
        journey.validate().map_err(domain_error)?;
        cursor.validate_for_journey(journey).map_err(domain_error)?;
        if journey.status() != JourneyStatus::Running || cursor.version() != 0 {
            return Err(invalid(
                "journey execution cursor must start at version zero on a running journey",
            ));
        }
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let authoritative_journey =
            load_authoritative_journey(&mut tx, &self.holding_ref, journey.id()).await?;
        if authoritative_journey != *journey {
            return Err(invalid(
                "Marketing journey execution start does not match the authoritative journey",
            ));
        }
        let payload = serde_json::to_value(cursor)?;
        let inserted = sqlx::query(
            "INSERT INTO erpx_tenant.marketing_journey_execution_cursors
                (holding_ref, run_id, journey_id, profile_id, scope, state,
                next_node_id, waiting_until, version, cursor, next_attempt_at,
                created_at, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
             ON CONFLICT (holding_ref, run_id) DO NOTHING",
        )
        .bind(&self.holding_ref)
        .bind(cursor.run_id())
        .bind(cursor.journey_id())
        .bind(cursor.profile_id())
        .bind(serde_json::to_value(cursor.scope())?)
        .bind(state_text(cursor.state()))
        .bind(cursor.next_node_id())
        .bind(cursor.waiting_until())
        .bind(0_i64)
        .bind(payload)
        .bind(cursor.updated_at())
        .bind(cursor.updated_at())
        .bind(cursor.updated_at())
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if inserted != 1 {
            return Err(invalid("Marketing journey execution run already exists"));
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn load_cursor(
        &self,
        run_id: Uuid,
    ) -> Result<Option<JourneyExecutionCursor>, PersistenceError> {
        if run_id.is_nil() {
            return Err(invalid("Marketing journey execution run ID cannot be nil"));
        }
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let row = sqlx::query(
            "SELECT cursor FROM erpx_tenant.marketing_journey_execution_cursors
              WHERE holding_ref=$1 AND run_id=$2",
        )
        .bind(&self.holding_ref)
        .bind(run_id)
        .fetch_optional(&mut *tx)
        .await?;
        let cursor = row
            .map(|row| serde_json::from_value(row.get("cursor")))
            .transpose()?;
        tx.commit().await?;
        Ok(cursor)
    }

    pub async fn advance_cursor(
        &self,
        journey: &Journey,
        run_id: Uuid,
        expected_version: u64,
        outcome: JourneyNodeOutcome,
        at: DateTime<Utc>,
        context: &CommandContext,
    ) -> Result<MarketingJourneyExecutionWriteReceipt, PersistenceError> {
        let request_hash = request_hash("advance", run_id, expected_version, &outcome, context)?;
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let mut cursor = load_for_update(&mut tx, &self.holding_ref, run_id).await?;
        let authoritative_journey =
            load_authoritative_journey(&mut tx, &self.holding_ref, cursor.journey_id()).await?;
        if authoritative_journey != *journey {
            return Err(invalid(
                "Marketing journey execution command does not match the authoritative journey",
            ));
        }
        validate_context(context, cursor.scope(), Some(expected_version))?;
        cursor
            .validate_for_journey(&authoritative_journey)
            .map_err(domain_error)?;
        if let Some(receipt) = find_replay(
            &mut tx,
            &self.holding_ref,
            context.idempotency_key.as_str(),
            &request_hash,
            run_id,
        )
        .await?
        {
            tx.commit().await?;
            return Ok(receipt);
        }
        let fact = cursor
            .advance(&authoritative_journey, expected_version, outcome, at)
            .map_err(domain_error)?;
        let event = build_event(context, &cursor, &fact)?;
        update_cursor(&mut tx, &self.holding_ref, &cursor, expected_version).await?;
        append_event(&mut tx, &self.holding_ref, &event).await?;
        let receipt = make_receipt(&event, cursor.journey_id(), cursor.version(), false);
        record_idempotency(
            &mut tx,
            &self.holding_ref,
            context.idempotency_key.as_str(),
            run_id,
            &request_hash,
            &receipt,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    pub async fn pause_cursor(
        &self,
        journey: &Journey,
        run_id: Uuid,
        expected_version: u64,
        at: DateTime<Utc>,
        context: &CommandContext,
    ) -> Result<MarketingJourneyExecutionWriteReceipt, PersistenceError> {
        self.control_cursor(journey, run_id, expected_version, at, context, true)
            .await
    }

    pub async fn resume_cursor(
        &self,
        journey: &Journey,
        run_id: Uuid,
        expected_version: u64,
        at: DateTime<Utc>,
        context: &CommandContext,
    ) -> Result<MarketingJourneyExecutionWriteReceipt, PersistenceError> {
        self.control_cursor(journey, run_id, expected_version, at, context, false)
            .await
    }

    async fn control_cursor(
        &self,
        journey: &Journey,
        run_id: Uuid,
        expected_version: u64,
        at: DateTime<Utc>,
        context: &CommandContext,
        pause: bool,
    ) -> Result<MarketingJourneyExecutionWriteReceipt, PersistenceError> {
        let operation = if pause { "pause" } else { "resume" };
        let request_hash = request_hash(operation, run_id, expected_version, &(), context)?;
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let mut cursor = load_for_update(&mut tx, &self.holding_ref, run_id).await?;
        let authoritative_journey =
            load_authoritative_journey(&mut tx, &self.holding_ref, cursor.journey_id()).await?;
        if authoritative_journey != *journey {
            return Err(invalid(
                "Marketing journey execution control command does not match the authoritative journey",
            ));
        }
        validate_context(context, cursor.scope(), Some(expected_version))?;
        cursor
            .validate_for_journey(&authoritative_journey)
            .map_err(domain_error)?;
        if let Some(receipt) = find_replay(
            &mut tx,
            &self.holding_ref,
            context.idempotency_key.as_str(),
            &request_hash,
            run_id,
        )
        .await?
        {
            tx.commit().await?;
            return Ok(receipt);
        }
        let fact = if pause {
            cursor.pause(expected_version, at)
        } else {
            cursor.resume(expected_version, at)
        }
        .map_err(domain_error)?;
        let event = build_event(context, &cursor, &fact)?;
        update_cursor(&mut tx, &self.holding_ref, &cursor, expected_version).await?;
        append_event(&mut tx, &self.holding_ref, &event).await?;
        let receipt = make_receipt(&event, cursor.journey_id(), cursor.version(), false);
        record_idempotency(
            &mut tx,
            &self.holding_ref,
            context.idempotency_key.as_str(),
            run_id,
            &request_hash,
            &receipt,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    /// Claim due cursors with PostgreSQL row locks. A claim is a short-lived
    /// lease; expired claims are eligible for another worker.
    pub async fn claim_due(
        &self,
        worker_id: Uuid,
        now: DateTime<Utc>,
        limit: u32,
        lease: Duration,
    ) -> Result<Vec<MarketingJourneyExecutionClaim>, PersistenceError> {
        validate_uuid(worker_id, "Marketing journey execution worker ID")?;
        if limit == 0 || limit > MAX_MARKETING_JOURNEY_EXECUTION_CLAIM_BATCH_SIZE {
            return Err(invalid(
                "Marketing journey execution claim batch is outside its bound",
            ));
        }
        if lease < Duration::seconds(MIN_MARKETING_JOURNEY_EXECUTION_LEASE_SECONDS as i64)
            || lease > Duration::seconds(MAX_MARKETING_JOURNEY_EXECUTION_LEASE_SECONDS as i64)
        {
            return Err(invalid(
                "Marketing journey execution lease is outside its bound",
            ));
        }
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let rows = sqlx::query(
            "SELECT c.run_id, c.journey_id, c.cursor
               FROM erpx_tenant.marketing_journey_execution_cursors AS c
               JOIN erpx_tenant.marketing_journeys AS j
                 ON j.holding_ref = c.holding_ref
                AND j.journey_id = c.journey_id
              WHERE c.holding_ref=$1
                AND j.status='running'
                AND c.state IN ('ready', 'running', 'waiting')
                AND c.next_attempt_at <= $2
                AND (c.waiting_until IS NULL OR c.waiting_until <= $2)
                AND c.dead_lettered_at IS NULL
                AND (c.claim_until IS NULL OR c.claim_until <= $2)
              ORDER BY c.next_attempt_at, c.run_id
              LIMIT $3
              FOR UPDATE OF c, j SKIP LOCKED",
        )
        .bind(&self.holding_ref)
        .bind(now)
        .bind(i64::from(limit))
        .fetch_all(&mut *tx)
        .await?;
        let claim_until = now + lease;
        let mut claims = Vec::with_capacity(rows.len());
        for row in rows {
            let run_id: Uuid = row.get("run_id");
            let journey_id: Uuid = row.get("journey_id");
            let cursor: JourneyExecutionCursor = serde_json::from_value(row.get("cursor"))?;
            let journey_row = sqlx::query(
                "SELECT journey FROM erpx_tenant.marketing_journeys
                  WHERE holding_ref=$1 AND journey_id=$2",
            )
            .bind(&self.holding_ref)
            .bind(journey_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(PersistenceError::NotFound {
                entity: "Marketing journey",
                id: journey_id,
            })?;
            let journey: Journey = serde_json::from_value(journey_row.get("journey"))?;
            if cursor.run_id() != run_id || cursor.journey_id() != journey_id {
                return Err(invalid(
                    "Marketing journey execution cursor identity mismatch",
                ));
            }
            cursor
                .validate_for_journey(&journey)
                .map_err(domain_error)?;
            let claim_id = Uuid::new_v4();
            sqlx::query(
                "UPDATE erpx_tenant.marketing_journey_execution_cursors
                    SET current_claim_id=$3, claimed_by=$4, claim_until=$5,
                        attempts=attempts+1, updated_at=updated_at
                  WHERE holding_ref=$1 AND run_id=$2",
            )
            .bind(&self.holding_ref)
            .bind(run_id)
            .bind(claim_id)
            .bind(worker_id)
            .bind(claim_until)
            .execute(&mut *tx)
            .await?;
            claims.push(MarketingJourneyExecutionClaim {
                journey,
                cursor,
                claim_id,
                worker_id,
                claim_until,
            });
        }
        tx.commit().await?;
        Ok(claims)
    }

    /// Apply a worker claim atomically. The deterministic command context
    /// makes an expired lease resumed by another worker an exact replay.
    pub async fn apply_claim(
        &self,
        claim: &MarketingJourneyExecutionClaim,
        worker_id: Uuid,
        outcome: JourneyNodeOutcome,
        at: DateTime<Utc>,
    ) -> Result<MarketingJourneyExecutionWriteReceipt, PersistenceError> {
        validate_uuid(worker_id, "Marketing journey execution worker ID")?;
        if claim.worker_id != worker_id || at >= claim.claim_until {
            return Err(PersistenceError::ClaimLost {
                worker_id,
                event_id: claim.cursor.run_id(),
            });
        }
        let context = worker_context(claim, worker_id)?;
        let request_hash = request_hash(
            "advance",
            claim.cursor.run_id(),
            claim.cursor.version(),
            &outcome,
            &context,
        )?;
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let row = sqlx::query(
            "SELECT cursor, current_claim_id, claimed_by, claim_until
               FROM erpx_tenant.marketing_journey_execution_cursors
              WHERE holding_ref=$1 AND run_id=$2 FOR UPDATE",
        )
        .bind(&self.holding_ref)
        .bind(claim.cursor.run_id())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "Marketing journey execution cursor",
            id: claim.cursor.run_id(),
        })?;
        let mut cursor: JourneyExecutionCursor = serde_json::from_value(row.get("cursor"))?;
        let journey_row = sqlx::query(
            "SELECT journey FROM erpx_tenant.marketing_journeys
              WHERE holding_ref=$1 AND journey_id=$2 FOR SHARE",
        )
        .bind(&self.holding_ref)
        .bind(cursor.journey_id())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "Marketing journey",
            id: cursor.journey_id(),
        })?;
        let authoritative_journey: Journey = serde_json::from_value(journey_row.get("journey"))?;
        cursor
            .validate_for_journey(&authoritative_journey)
            .map_err(domain_error)?;
        if let Some(receipt) = find_replay(
            &mut tx,
            &self.holding_ref,
            context.idempotency_key.as_str(),
            &request_hash,
            cursor.run_id(),
        )
        .await?
        {
            // Older deployments persisted the internal owner handoff without
            // the downstream delivery aggregate. Replays are therefore also
            // a bounded repair point: the immutable notification facts and
            // the deterministic delivery reservation are healed atomically,
            // while an existing delivery remains an exact idempotent replay.
            let mut replay_cursor = claim.cursor.clone();
            let replay_matches_current = replay_cursor
                .advance(
                    &authoritative_journey,
                    claim.cursor.version(),
                    outcome,
                    claim.cursor.updated_at(),
                )
                .is_ok()
                && replay_cursor.version() == cursor.version()
                && replay_cursor.journey_id() == cursor.journey_id()
                && replay_cursor.run_id() == cursor.run_id()
                && replay_cursor.profile_id() == cursor.profile_id()
                && replay_cursor.scope() == cursor.scope()
                && replay_cursor.state() == cursor.state()
                && replay_cursor.next_node_id() == cursor.next_node_id()
                && replay_cursor.waiting_until() == cursor.waiting_until();
            if replay_matches_current
                && let Some((notification_ref, owner_ref)) =
                    journey_owner_notification(claim, &authoritative_journey)?
            {
                let notification_at =
                    crate::marketing_persistence::apply_journey_owner_notification_in_transaction(
                        &mut tx,
                        &self.holding_ref,
                        claim.cursor.scope(),
                        notification_ref,
                        claim.cursor.journey_id(),
                        claim.cursor.run_id(),
                        claim.cursor.profile_id(),
                        owner_ref.clone(),
                        at,
                    )
                    .await?;
                let delivery = journey_owner_notification_delivery(
                    claim,
                    authoritative_journey.id(),
                    notification_ref,
                    &owner_ref,
                    notification_at,
                )?;
                crate::marketing_delivery_persistence::enqueue_in_transaction(
                    &mut tx,
                    &self.holding_ref,
                    &delivery,
                )
                .await?;
            }
            tx.commit().await?;
            return Ok(receipt);
        }
        validate_context(&context, cursor.scope(), Some(claim.cursor.version()))?;
        if cursor != claim.cursor {
            return Err(PersistenceError::ClaimLost {
                worker_id,
                event_id: claim.cursor.run_id(),
            });
        }
        let current_claim: Option<Uuid> = row.get("current_claim_id");
        let claimed_by: Option<Uuid> = row.get("claimed_by");
        let claim_until: Option<DateTime<Utc>> = row.get("claim_until");
        if current_claim != Some(claim.claim_id)
            || claimed_by != Some(worker_id)
            || claim_until.is_none_or(|until| at >= until)
        {
            return Err(PersistenceError::ClaimLost {
                worker_id,
                event_id: claim.cursor.run_id(),
            });
        }
        let integration_invocation =
            journey_integration_invocation(claim, &authoritative_journey, at)?;
        if let Some(invocation) = integration_invocation.as_ref() {
            self.ensure_approved_integration(&invocation.integration_ref, &invocation.scope)
                .await?;
        }
        let fact = cursor
            .advance(&authoritative_journey, cursor.version(), outcome, at)
            .map_err(domain_error)?;
        let crm_handoff = journey_crm_handoff(claim, &authoritative_journey, &context, at)?;
        if let Some(handoff) = crm_handoff.as_ref() {
            crate::marketing_crm_persistence::SqlxMarketingCrmStore::persist_handoff_in_transaction(
                &mut tx,
                &self.holding_ref,
                handoff,
                &context,
                None,
            )
            .await?;
        }
        if let Some(dispatch) = journey_channel_delivery(
            &mut tx,
            &self.holding_ref,
            claim,
            &authoritative_journey,
            at,
        )
        .await?
        {
            crate::marketing_delivery_persistence::enqueue_in_transaction(
                &mut tx,
                &self.holding_ref,
                &dispatch,
            )
            .await?;
        }
        if let Some((audience_id, change)) = journey_segment_change(claim, &authoritative_journey)?
        {
            crate::marketing_persistence::apply_journey_segment_change_in_transaction(
                &mut tx,
                &self.holding_ref,
                claim.cursor.scope(),
                audience_id,
                claim.cursor.profile_id(),
                change,
                at,
            )
            .await?;
        }
        if let Some((tag_ref, add)) = journey_profile_tag_action(claim, &authoritative_journey)? {
            crate::marketing_persistence::apply_journey_profile_tag_in_transaction(
                &mut tx,
                &self.holding_ref,
                claim.cursor.scope(),
                claim.cursor.profile_id(),
                &tag_ref,
                add,
                at,
            )
            .await?;
        }
        if let Some((conversion_ref, revenue_minor, currency)) =
            journey_conversion(claim, &authoritative_journey)?
        {
            crate::marketing_persistence::apply_journey_conversion_in_transaction(
                &mut tx,
                &self.holding_ref,
                claim.cursor.scope(),
                conversion_ref,
                claim.cursor.journey_id(),
                claim.cursor.run_id(),
                claim.cursor.profile_id(),
                revenue_minor,
                currency,
                at,
            )
            .await?;
        }
        if let Some((notification_ref, owner_ref)) =
            journey_owner_notification(claim, &authoritative_journey)?
        {
            let notification_at =
                crate::marketing_persistence::apply_journey_owner_notification_in_transaction(
                    &mut tx,
                    &self.holding_ref,
                    claim.cursor.scope(),
                    notification_ref,
                    claim.cursor.journey_id(),
                    claim.cursor.run_id(),
                    claim.cursor.profile_id(),
                    owner_ref.clone(),
                    at,
                )
                .await?;
            let delivery = journey_owner_notification_delivery(
                claim,
                authoritative_journey.id(),
                notification_ref,
                &owner_ref,
                notification_at,
            )?;
            crate::marketing_delivery_persistence::enqueue_in_transaction(
                &mut tx,
                &self.holding_ref,
                &delivery,
            )
            .await?;
        }
        if let Some(invocation) = integration_invocation.as_ref() {
            crate::marketing_journey_integration_persistence::SqlxMarketingJourneyIntegrationStore::enqueue_in_transaction(
                &mut tx,
                &self.holding_ref,
                invocation,
                &context,
            )
            .await?;
        }
        let event = build_event(&context, &cursor, &fact)?;
        update_cursor(
            &mut tx,
            &self.holding_ref,
            &cursor,
            fact.expected_cursor_version,
        )
        .await?;
        append_event(&mut tx, &self.holding_ref, &event).await?;
        let receipt = make_receipt(&event, cursor.journey_id(), cursor.version(), false);
        record_idempotency(
            &mut tx,
            &self.holding_ref,
            context.idempotency_key.as_str(),
            cursor.run_id(),
            &request_hash,
            &receipt,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    /// Release a claim after a decision failure. Permanent failures are
    /// retained as dead letters; retryable failures become due after a bounded
    /// delay.
    pub async fn release_claim(
        &self,
        claim: &MarketingJourneyExecutionClaim,
        worker_id: Uuid,
        at: DateTime<Utc>,
        failure: MarketingJourneyExecutionFailure,
    ) -> Result<(), PersistenceError> {
        validate_uuid(worker_id, "Marketing journey execution worker ID")?;
        if claim.worker_id != worker_id || at >= claim.claim_until {
            return Err(PersistenceError::ClaimLost {
                worker_id,
                event_id: claim.cursor.run_id(),
            });
        }
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let error = match failure {
            MarketingJourneyExecutionFailure::Retryable => "retryable_execution_failure",
            MarketingJourneyExecutionFailure::Permanent => "permanent_execution_failure",
        };
        let next_attempt_at = match failure {
            MarketingJourneyExecutionFailure::Retryable => at + Duration::seconds(30),
            MarketingJourneyExecutionFailure::Permanent => at,
        };
        let dead_lettered_at = match failure {
            MarketingJourneyExecutionFailure::Retryable => None,
            MarketingJourneyExecutionFailure::Permanent => Some(at),
        };
        let affected = sqlx::query(
            "UPDATE erpx_tenant.marketing_journey_execution_cursors
                SET current_claim_id=NULL, claimed_by=NULL, claim_until=NULL,
                    next_attempt_at=$5, last_error=$6, dead_lettered_at=$7
              WHERE holding_ref=$1 AND run_id=$2 AND current_claim_id=$3
                AND claimed_by=$4 AND claim_until > $8",
        )
        .bind(&self.holding_ref)
        .bind(claim.cursor.run_id())
        .bind(claim.claim_id)
        .bind(worker_id)
        .bind(next_attempt_at)
        .bind(error)
        .bind(dead_lettered_at)
        .bind(at)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if affected != 1 {
            return Err(PersistenceError::ClaimLost {
                worker_id,
                event_id: claim.cursor.run_id(),
            });
        }
        tx.commit().await?;
        Ok(())
    }
}

async fn load_for_update(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    run_id: Uuid,
) -> Result<JourneyExecutionCursor, PersistenceError> {
    let row = sqlx::query(
        "SELECT cursor FROM erpx_tenant.marketing_journey_execution_cursors
          WHERE holding_ref=$1 AND run_id=$2 FOR UPDATE",
    )
    .bind(holding_ref)
    .bind(run_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(PersistenceError::NotFound {
        entity: "Marketing journey execution cursor",
        id: run_id,
    })?;
    Ok(serde_json::from_value(row.get("cursor"))?)
}

async fn load_authoritative_journey(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    journey_id: Uuid,
) -> Result<Journey, PersistenceError> {
    let row = sqlx::query(
        "SELECT journey
           FROM erpx_tenant.marketing_journeys
          WHERE holding_ref=$1 AND journey_id=$2 FOR SHARE",
    )
    .bind(holding_ref)
    .bind(journey_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(PersistenceError::NotFound {
        entity: "Marketing journey",
        id: journey_id,
    })?;
    Ok(serde_json::from_value(row.get("journey"))?)
}

async fn update_cursor(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    cursor: &JourneyExecutionCursor,
    expected_version: u64,
) -> Result<(), PersistenceError> {
    let affected = sqlx::query(
        "UPDATE erpx_tenant.marketing_journey_execution_cursors
            SET state=$3, next_node_id=$4, waiting_until=$5, version=$6,
                cursor=$7, next_attempt_at=$8, current_claim_id=NULL,
                claimed_by=NULL, claim_until=NULL, last_error=NULL, updated_at=$9
          WHERE holding_ref=$1 AND run_id=$2 AND version=$10",
    )
    .bind(holding_ref)
    .bind(cursor.run_id())
    .bind(state_text(cursor.state()))
    .bind(cursor.next_node_id())
    .bind(cursor.waiting_until())
    .bind(as_i64(cursor.version(), "execution cursor version")?)
    .bind(serde_json::to_value(cursor)?)
    .bind(cursor.updated_at())
    .bind(cursor.updated_at())
    .bind(as_i64(
        expected_version,
        "expected execution cursor version",
    )?)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    if affected != 1 {
        return Err(PersistenceError::OptimisticConcurrency {
            aggregate_type: MARKETING_JOURNEY_EXECUTION_AGGREGATE.into(),
            aggregate_id: cursor.run_id(),
            expected: Some(expected_version),
            actual: None,
        });
    }
    Ok(())
}

fn build_event(
    context: &CommandContext,
    cursor: &JourneyExecutionCursor,
    fact: &JourneyExecutionFact,
) -> Result<EventEnvelope, PersistenceError> {
    Ok(EventEnvelope {
        contract_version: context.contract_version,
        event_id: Uuid::new_v4(),
        event_type: MARKETING_JOURNEY_EXECUTION_ADVANCED_EVENT.into(),
        event_version: 1,
        occurred_at: fact.occurred_at,
        aggregate_type: MARKETING_JOURNEY_EXECUTION_AGGREGATE.into(),
        aggregate_id: cursor.run_id(),
        aggregate_version: fact.cursor_version,
        tenant_scope: cursor.scope().clone(),
        actor: context.actor.clone(),
        correlation_id: context.correlation_id,
        causation_id: context.causation_id,
        idempotency_key: Some(context.idempotency_key.clone()),
        payload: serde_json::json!({ "cursor": cursor, "fact": fact }),
    })
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
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)",
    )
    .bind(event.event_id)
    .bind(&event.aggregate_type)
    .bind(event.aggregate_id)
    .bind(holding_ref)
    .bind(as_i64(event.aggregate_version, "execution event version")?)
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

async fn find_replay(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    idempotency_key: &str,
    request_hash: &str,
    run_id: Uuid,
) -> Result<Option<MarketingJourneyExecutionWriteReceipt>, PersistenceError> {
    let row = sqlx::query(
        "SELECT request_hash, run_id, response
           FROM erpx_tenant.marketing_journey_execution_idempotency
          WHERE holding_ref=$1 AND idempotency_key=$2 FOR UPDATE",
    )
    .bind(holding_ref)
    .bind(idempotency_key)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(row) = row else { return Ok(None) };
    if row.get::<String, _>("request_hash") != request_hash
        || row.get::<Uuid, _>("run_id") != run_id
    {
        return Err(PersistenceError::IdempotencyConflict {
            key: idempotency_key.into(),
        });
    }
    let mut receipt: MarketingJourneyExecutionWriteReceipt =
        serde_json::from_value(row.get("response"))?;
    let source = sqlx::query(
        "SELECT aggregate_type, aggregate_id, aggregate_version, event_type,
                idempotency_key, payload
           FROM erpx_tenant.outbox_events
          WHERE holding_ref=$1 AND event_id=$2",
    )
    .bind(holding_ref)
    .bind(receipt.event_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| invalid("Marketing journey execution replay source event is missing"))?;
    let source = ReplaySourceEvent {
        aggregate_type: source.get("aggregate_type"),
        aggregate_id: source.get("aggregate_id"),
        aggregate_version: source.get("aggregate_version"),
        event_type: source.get("event_type"),
        idempotency_key: source.get("idempotency_key"),
        payload: source.get("payload"),
    };
    validate_replay_source_event(&source, idempotency_key, &receipt)?;
    receipt.replayed = true;
    Ok(Some(receipt))
}

fn validate_replay_source_event(
    source: &ReplaySourceEvent,
    expected_idempotency_key: &str,
    receipt: &MarketingJourneyExecutionWriteReceipt,
) -> Result<(), PersistenceError> {
    let source_version = as_u64(source.aggregate_version, "execution replay source version")?;
    let source_key_matches = source.idempotency_key.as_deref() == Some(expected_idempotency_key);
    if source.aggregate_type != MARKETING_JOURNEY_EXECUTION_AGGREGATE
        || source.aggregate_id != receipt.run_id
        || source_version != receipt.cursor_version
        || source.event_type != MARKETING_JOURNEY_EXECUTION_ADVANCED_EVENT
        || !source_key_matches
    {
        return Err(invalid(
            "Marketing journey execution replay source event identity is invalid",
        ));
    }
    let cursor_payload =
        source.payload.get("cursor").cloned().ok_or_else(|| {
            invalid("Marketing journey execution replay source cursor is missing")
        })?;
    let cursor: JourneyExecutionCursor = serde_json::from_value(cursor_payload)?;
    if cursor.run_id() != receipt.run_id
        || cursor.journey_id() != receipt.journey_id
        || cursor.version() != receipt.cursor_version
    {
        return Err(invalid(
            "Marketing journey execution replay source cursor identity is invalid",
        ));
    }
    Ok(())
}

async fn record_idempotency(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    idempotency_key: &str,
    run_id: Uuid,
    request_hash: &str,
    receipt: &MarketingJourneyExecutionWriteReceipt,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_tenant.marketing_journey_execution_idempotency
            (holding_ref, idempotency_key, run_id, request_hash, version, event_id, response)
         VALUES ($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(holding_ref)
    .bind(idempotency_key)
    .bind(run_id)
    .bind(request_hash)
    .bind(as_i64(receipt.cursor_version, "execution receipt version")?)
    .bind(receipt.event_id)
    .bind(serde_json::to_value(receipt)?)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn request_hash<T: Serialize>(
    operation: &str,
    run_id: Uuid,
    expected_version: u64,
    command: &T,
    context: &CommandContext,
) -> Result<String, PersistenceError> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(&(
        operation,
        run_id,
        expected_version,
        command,
        context,
    ))?)))
}

fn validate_context(
    context: &CommandContext,
    cursor_scope: &TenantScope,
    expected: Option<u64>,
) -> Result<(), PersistenceError> {
    context
        .validate()
        .map_err(|error| invalid(format!("invalid Marketing execution context: {error}")))?;
    if context.contract_version != CONTRACT_VERSION
        || matches!(context.tenant_scope, TenantScope::Platform)
        || !context.tenant_scope.contains(cursor_scope)
        || context.expected_aggregate_version != expected
        || context.idempotency_key.trim().is_empty()
        || context.idempotency_key.len() > 255
        || context.idempotency_key.chars().any(char::is_control)
    {
        return Err(invalid("Marketing execution context is outside its bound"));
    }
    Ok(())
}

fn worker_context(
    claim: &MarketingJourneyExecutionClaim,
    _worker_id: Uuid,
) -> Result<CommandContext, PersistenceError> {
    let idempotency_key = format!(
        "marketing-journey-execution:{}:{}",
        claim.cursor.run_id(),
        claim.cursor.version()
    );
    let mut context = CommandContext::try_new(
        idempotency_key.clone(),
        ActorAttribution {
            principal_ref: "worker:marketing_journey_execution".into(),
            kind: ActorKind::SystemWorker,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        claim.cursor.scope().clone(),
        "marketing_journey_execution",
    )
    .map_err(|error| invalid(format!("invalid worker execution context: {error}")))?;
    context.command_id = deterministic_uuid(&idempotency_key);
    context.correlation_id = deterministic_uuid(&format!(
        "marketing-journey-correlation:{}",
        claim.cursor.run_id()
    ));
    context.audit.correlation_id = context.correlation_id;
    context.expected_aggregate_version = Some(claim.cursor.version());
    Ok(context)
}

/// Build the internal CRM acceptance fact for a journey CRM node. The
/// deterministic identity and worker command context make a retried claim an
/// exact replay; the caller persists it in the same transaction as the cursor
/// and execution outbox event.
fn journey_crm_handoff(
    claim: &MarketingJourneyExecutionClaim,
    journey: &Journey,
    context: &CommandContext,
    at: DateTime<Utc>,
) -> Result<Option<AcceptedLeadHandoff>, PersistenceError> {
    let Some(node_id) = claim.cursor.next_node_id() else {
        return Ok(None);
    };
    let node = journey
        .nodes()
        .iter()
        .find(|node| node.id == node_id)
        .ok_or_else(|| invalid("Marketing journey CRM node is missing"))?;
    let JourneyNodeDefinition::CrmHandoff {
        source,
        account,
        contact,
    } = &node.definition
    else {
        return Ok(None);
    };
    let handoff_id = deterministic_uuid(&format!(
        "marketing-journey-crm-handoff:{}:{}",
        claim.cursor.run_id(),
        node_id
    ));
    AcceptedLeadHandoff::new(
        handoff_id,
        claim.cursor.scope().clone(),
        (**source).clone(),
        account.clone(),
        contact.clone(),
        at,
        context,
    )
    .map(Some)
    .map_err(|error| invalid(format!("Marketing journey CRM handoff is invalid: {error}")))
}

/// Build the durable delivery aggregate for a supported journey channel.
/// The caller owns the cursor transaction, so the idempotency reservation and
/// the journey advance share one commit boundary.
async fn journey_channel_delivery(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    claim: &MarketingJourneyExecutionClaim,
    journey: &Journey,
    at: DateTime<Utc>,
) -> Result<Option<DeliveryDispatch>, PersistenceError> {
    let Some(node_id) = claim.cursor.next_node_id() else {
        return Ok(None);
    };
    let node = journey
        .nodes()
        .iter()
        .find(|node| node.id == node_id)
        .ok_or_else(|| invalid("Marketing journey channel node is missing"))?;
    let JourneyNodeDefinition::Channel { channel, payload } = &node.definition else {
        return Ok(None);
    };
    let profile_row = sqlx::query(
        "SELECT scope, email, phone, attributes
           FROM erpx_tenant.marketing_profiles
          WHERE holding_ref=$1 AND profile_id=$2 AND erased_at IS NULL
          FOR SHARE",
    )
    .bind(holding_ref)
    .bind(claim.cursor.profile_id())
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(PersistenceError::NotFound {
        entity: "Marketing journey profile",
        id: claim.cursor.profile_id(),
    })?;
    let profile_scope: TenantScope = serde_json::from_value(profile_row.try_get("scope")?)?;
    if !claim.cursor.scope().contains(&profile_scope) {
        return Err(invalid(
            "Marketing journey profile scope is outside the execution scope",
        ));
    }
    let email: Option<String> = profile_row.try_get("email")?;
    let phone: Option<String> = profile_row.try_get("phone")?;
    let attributes: BTreeMap<String, String> =
        serde_json::from_value(profile_row.try_get("attributes")?)?;
    let (record, logical_suffix) = match payload {
        JourneyChannelPayload::Email {
            template_ref,
            subject,
        } => (
            ChannelDispatchRecord::Email {
                recipient: required_contact(email, "email")?,
                template_ref: template_ref.clone(),
            },
            format!("email:{template_ref}:{subject}"),
        ),
        JourneyChannelPayload::Sms { template_ref } => (
            ChannelDispatchRecord::Sms {
                phone: required_contact(phone, "phone")?,
                template_ref: template_ref.clone(),
            },
            format!("sms:{template_ref}"),
        ),
        JourneyChannelPayload::Social {
            network,
            content_ref,
        } => (
            ChannelDispatchRecord::Social {
                destination_ref: required_contact(
                    attributes.get(&format!("social:{network}")).cloned(),
                    "social destination",
                )?,
                network: network.clone(),
                content_ref: content_ref.clone(),
            },
            format!("social:{network}:{content_ref}"),
        ),
        JourneyChannelPayload::Push { template_ref } => (
            ChannelDispatchRecord::Push {
                device_ref: required_contact(
                    attributes.get("push:device_ref").cloned(),
                    "push device",
                )?,
                template_ref: template_ref.clone(),
            },
            format!("push:{template_ref}"),
        ),
        JourneyChannelPayload::Webhook {
            event_type,
            payload_ref,
        } => (
            ChannelDispatchRecord::Webhook {
                destination_ref: required_contact(
                    attributes.get("webhook:destination_ref").cloned(),
                    "webhook destination",
                )?,
                event_type: event_type.clone(),
                payload_ref: payload_ref.clone(),
            },
            format!("webhook:{event_type}:{payload_ref}"),
        ),
    };
    let dispatch = DeliveryDispatch::new(
        deterministic_uuid(&format!(
            "marketing-journey-delivery:{}:{}",
            claim.cursor.run_id(),
            node_id
        )),
        claim.cursor.scope().clone(),
        claim.cursor.profile_id(),
        journey.id(),
        MarketingPurpose::LeadNurture,
        *channel,
        record,
        format!(
            "journey:{}:{}:{logical_suffix}",
            claim.cursor.run_id(),
            node_id
        ),
        1,
        at,
        at + Duration::days(30),
        RetryPolicy::default(),
    )
    .map_err(|error| invalid(format!("Marketing journey delivery is invalid: {error}")))?;
    Ok(Some(dispatch))
}

fn required_contact(
    value: Option<String>,
    field: &'static str,
) -> Result<String, PersistenceError> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| invalid(format!("Marketing journey profile {field} is missing")))
}

fn journey_segment_change(
    claim: &MarketingJourneyExecutionClaim,
    journey: &Journey,
) -> Result<Option<(Uuid, SegmentChange)>, PersistenceError> {
    let Some(node_id) = claim.cursor.next_node_id() else {
        return Ok(None);
    };
    let node = journey
        .nodes()
        .iter()
        .find(|node| node.id == node_id)
        .ok_or_else(|| invalid("Marketing journey segment node is missing"))?;
    match &node.definition {
        JourneyNodeDefinition::SegmentChange { segment_id, change } => {
            Ok(Some((*segment_id, *change)))
        }
        _ => Ok(None),
    }
}

fn journey_profile_tag_action(
    claim: &MarketingJourneyExecutionClaim,
    journey: &Journey,
) -> Result<Option<(String, bool)>, PersistenceError> {
    let Some(node_id) = claim.cursor.next_node_id() else {
        return Ok(None);
    };
    let node = journey
        .nodes()
        .iter()
        .find(|node| node.id == node_id)
        .ok_or_else(|| invalid("Marketing journey action node is missing"))?;
    match &node.definition {
        JourneyNodeDefinition::Action {
            action: JourneyAction::AddProfileTag { tag_ref },
        } => Ok(Some((tag_ref.clone(), true))),
        JourneyNodeDefinition::Action {
            action: JourneyAction::RemoveProfileTag { tag_ref },
        } => Ok(Some((tag_ref.clone(), false))),
        _ => Ok(None),
    }
}

fn journey_conversion(
    claim: &MarketingJourneyExecutionClaim,
    journey: &Journey,
) -> Result<Option<JourneyConversionInput>, PersistenceError> {
    let Some(node_id) = claim.cursor.next_node_id() else {
        return Ok(None);
    };
    let node = journey
        .nodes()
        .iter()
        .find(|node| node.id == node_id)
        .ok_or_else(|| invalid("Marketing journey conversion node is missing"))?;
    match &node.definition {
        JourneyNodeDefinition::Conversion {
            conversion_ref,
            revenue_minor,
            currency,
        } => Ok(Some((*conversion_ref, *revenue_minor, currency.clone()))),
        _ => Ok(None),
    }
}

fn journey_owner_notification(
    claim: &MarketingJourneyExecutionClaim,
    journey: &Journey,
) -> Result<Option<(Uuid, String)>, PersistenceError> {
    let Some(node_id) = claim.cursor.next_node_id() else {
        return Ok(None);
    };
    let node = journey
        .nodes()
        .iter()
        .find(|node| node.id == node_id)
        .ok_or_else(|| invalid("Marketing journey owner notification node is missing"))?;
    let JourneyNodeDefinition::Action {
        action: JourneyAction::NotifyOwner { owner_ref },
    } = &node.definition
    else {
        return Ok(None);
    };
    let notification_ref = deterministic_uuid(&format!(
        "marketing-journey-owner-notification:{}:{}",
        claim.cursor.run_id(),
        node_id
    ));
    Ok(Some((notification_ref, owner_ref.clone())))
}

/// Translate the immutable `NotifyOwner` handoff into the existing durable
/// delivery aggregate. Owner notifications are internal workflow messages,
/// so the Webhook record carries only an opaque owner reference and the
/// notification reference; the configured provider account remains the only
/// component allowed to resolve an endpoint or credential.
fn journey_owner_notification_delivery(
    claim: &MarketingJourneyExecutionClaim,
    journey_id: Uuid,
    notification_ref: Uuid,
    owner_ref: &str,
    at: DateTime<Utc>,
) -> Result<DeliveryDispatch, PersistenceError> {
    if notification_ref.is_nil() || owner_ref.trim().is_empty() {
        return Err(invalid(
            "Marketing journey owner notification delivery identity is invalid",
        ));
    }
    let payload_ref = notification_ref.to_string();
    DeliveryDispatch::new(
        journey_owner_notification_delivery_id(notification_ref),
        claim.cursor.scope().clone(),
        claim.cursor.profile_id(),
        journey_id,
        MarketingPurpose::Transactional,
        MarketingChannel::Webhook,
        ChannelDispatchRecord::Webhook {
            destination_ref: owner_ref.to_owned(),
            event_type: MARKETING_JOURNEY_OWNER_NOTIFICATION_CHANNEL_EVENT.into(),
            payload_ref,
        },
        format!("journey-owner-notification:{notification_ref}"),
        1,
        at,
        at + Duration::days(30),
        RetryPolicy::default(),
    )
    .map_err(|error| {
        invalid(format!(
            "Marketing owner notification delivery is invalid: {error}"
        ))
    })
}

pub(crate) fn journey_owner_notification_delivery_id(notification_ref: Uuid) -> Uuid {
    deterministic_uuid(&format!(
        "marketing-journey-owner-notification-delivery:{notification_ref}"
    ))
}

fn journey_integration_invocation(
    claim: &MarketingJourneyExecutionClaim,
    journey: &Journey,
    at: DateTime<Utc>,
) -> Result<Option<JourneyIntegrationInvocation>, PersistenceError> {
    let Some(node_id) = claim.cursor.next_node_id() else {
        return Ok(None);
    };
    let node = journey
        .nodes()
        .iter()
        .find(|node| node.id == node_id)
        .ok_or_else(|| invalid("Marketing journey integration node is missing"))?;
    let JourneyNodeDefinition::Action {
        action: JourneyAction::InvokeApprovedIntegration { integration_ref },
    } = &node.definition
    else {
        return Ok(None);
    };
    let invocation_id = deterministic_uuid(&format!(
        "marketing-journey-integration:{}:{}",
        claim.cursor.run_id(),
        node_id
    ));
    let idempotency_key = format!(
        "marketing-journey-integration:{}:{}",
        claim.cursor.run_id(),
        node_id
    );
    JourneyIntegrationInvocation::new(
        invocation_id,
        integration_ref.clone(),
        claim.cursor.journey_id(),
        claim.cursor.run_id(),
        node_id,
        claim.cursor.profile_id(),
        claim.cursor.scope().clone(),
        idempotency_key,
        at,
    )
    .map(Some)
    .map_err(|error| invalid(format!("Marketing journey integration is invalid: {error}")))
}

fn make_receipt(
    event: &EventEnvelope,
    journey_id: Uuid,
    cursor_version: u64,
    replayed: bool,
) -> MarketingJourneyExecutionWriteReceipt {
    MarketingJourneyExecutionWriteReceipt {
        journey_id,
        run_id: event.aggregate_id,
        cursor_version,
        idempotency_key: event.idempotency_key.clone().unwrap_or_default(),
        event_id: event.event_id,
        replayed,
    }
}

fn state_text(state: erpx_domain::marketing_journey::JourneyExecutionState) -> &'static str {
    match state {
        erpx_domain::marketing_journey::JourneyExecutionState::Ready => "ready",
        erpx_domain::marketing_journey::JourneyExecutionState::Running => "running",
        erpx_domain::marketing_journey::JourneyExecutionState::Waiting => "waiting",
        erpx_domain::marketing_journey::JourneyExecutionState::Paused => "paused",
        erpx_domain::marketing_journey::JourneyExecutionState::Completed => "completed",
    }
}

fn validate_holding_ref(value: &str) -> Result<(), PersistenceError> {
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

fn validate_uuid(value: Uuid, field: &str) -> Result<(), PersistenceError> {
    if value.is_nil() {
        return Err(invalid(format!("{field} cannot be nil")));
    }
    Ok(())
}

fn deterministic_uuid(seed: &str) -> Uuid {
    let digest = Sha256::digest(seed.as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn invalid(message: impl Into<String>) -> PersistenceError {
    PersistenceError::InvalidData(message.into())
}

fn domain_error(error: MarketingJourneyError) -> PersistenceError {
    invalid(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_domain::marketing_journey::JourneyTrigger;

    #[test]
    fn execution_state_mapping_and_zero_seed_contract_are_bounded() {
        assert_eq!(
            state_text(erpx_domain::marketing_journey::JourneyExecutionState::Ready),
            "ready"
        );
        assert_eq!(
            state_text(erpx_domain::marketing_journey::JourneyExecutionState::Completed),
            "completed"
        );
        assert_eq!(as_i64(0, "cursor version").expect("zero seed"), 0);
    }

    #[test]
    fn replay_receipts_require_the_exact_authoritative_source_event() {
        let journey_id = Uuid::from_u128(1);
        let run_id = Uuid::from_u128(2);
        let holding_id = Uuid::from_u128(3);
        let receipt = MarketingJourneyExecutionWriteReceipt {
            journey_id,
            run_id,
            cursor_version: 1,
            idempotency_key: "worker-key".into(),
            event_id: Uuid::from_u128(4),
            replayed: false,
        };
        let cursor = serde_json::json!({
            "journey_id": journey_id,
            "run_id": run_id,
            "profile_id": Uuid::from_u128(5),
            "scope": serde_json::to_value(TenantScope::Holding { holding_id })
                .expect("scope"),
            "state": "running",
            "next_node_id": Uuid::from_u128(6),
            "waiting_until": null,
            "version": 1,
            "updated_at": Utc::now(),
        });
        let payload = serde_json::json!({ "cursor": cursor });
        let source = ReplaySourceEvent {
            aggregate_type: MARKETING_JOURNEY_EXECUTION_AGGREGATE.into(),
            aggregate_id: run_id,
            aggregate_version: 1,
            event_type: MARKETING_JOURNEY_EXECUTION_ADVANCED_EVENT.into(),
            idempotency_key: Some("worker-key".into()),
            payload: payload.clone(),
        };

        assert!(validate_replay_source_event(&source, "worker-key", &receipt,).is_ok());
        let mut mismatched = source;
        mismatched.aggregate_version = 2;
        assert!(validate_replay_source_event(&mismatched, "worker-key", &receipt,).is_err());
    }

    #[test]
    fn notify_owner_delivery_is_deterministic_and_provider_safe() {
        let scope = TenantScope::Holding {
            holding_id: Uuid::from_u128(3),
        };
        let node_id = Uuid::from_u128(6);
        let at = Utc::now();
        let journey = Journey::new(
            Uuid::from_u128(7),
            scope,
            "owner-notification",
            vec![
                erpx_domain::marketing_journey::JourneyNode::new(
                    node_id,
                    0,
                    None,
                    JourneyNodeDefinition::Trigger {
                        trigger: JourneyTrigger::Enrollment,
                    },
                )
                .expect("node"),
            ],
            at,
        )
        .expect("journey");
        let mut serialized = serde_json::to_value(journey).expect("journey JSON");
        serialized["status"] = serde_json::json!("running");
        let journey: Journey = serde_json::from_value(serialized).expect("running journey");
        let cursor =
            JourneyExecutionCursor::start(&journey, Uuid::from_u128(8), Uuid::from_u128(9), at)
                .expect("cursor");
        let claim = MarketingJourneyExecutionClaim {
            journey: journey.clone(),
            cursor,
            claim_id: Uuid::from_u128(10),
            worker_id: Uuid::from_u128(11),
            claim_until: at + Duration::minutes(1),
        };
        let notification_ref = Uuid::from_u128(12);
        let first = journey_owner_notification_delivery(
            &claim,
            journey.id(),
            notification_ref,
            "operations-owner",
            at,
        )
        .expect("delivery");
        let second = journey_owner_notification_delivery(
            &claim,
            journey.id(),
            notification_ref,
            "operations-owner",
            at,
        )
        .expect("replayed delivery");
        assert_eq!(first, second);
        assert_eq!(first.channel, MarketingChannel::Webhook);
        assert_eq!(first.purpose, MarketingPurpose::Transactional);
        assert_eq!(
            first.logical_message_ref,
            format!("journey-owner-notification:{notification_ref}")
        );
        assert!(matches!(
            first.record,
            ChannelDispatchRecord::Webhook { ref event_type, ref payload_ref, .. }
                if event_type == MARKETING_JOURNEY_OWNER_NOTIFICATION_CHANNEL_EVENT
                    && payload_ref == &notification_ref.to_string()
        ));
    }
}
