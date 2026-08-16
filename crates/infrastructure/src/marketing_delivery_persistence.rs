//! Tenant-routed authoritative persistence for Marketing delivery dispatches.
//!
//! The repository reserves deterministic delivery keys atomically, rechecks
//! consent and suppression on the write database immediately before a worker
//! attempt, and persists provider results only while the worker lease is
//! owned. Provider calls themselves remain outside this repository.

use chrono::{DateTime, Duration, Utc};
use erpx_domain::marketing::{ConsentRecord, SuppressionRecord};
use erpx_domain::marketing_callback::{
    MarketingProviderCallback, MarketingProviderCallbackOrder, MarketingProviderCallbackRequest,
    MarketingProviderCallbackWatermark, ProviderAuthenticationDecision,
};
use erpx_domain::marketing_delivery::{
    AttemptAuthorization, ConsentRecheckDecision, DeliveryDispatch, DeliveryRecheckDecision,
    DeliveryRecheckResult, DeliveryState, DeliveryTerminalReason, ProviderResult,
    SuppressionRecheckDecision,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::PersistenceError;

pub const MAX_MARKETING_DELIVERY_CLAIM_BATCH_SIZE: u32 = 100;
pub const MAX_MARKETING_DELIVERY_PROJECTION_PAGE_SIZE: u32 = 100;
pub const MIN_MARKETING_DELIVERY_LEASE_SECONDS: u64 = 1;
pub const MAX_MARKETING_DELIVERY_LEASE_SECONDS: u64 = 3_600;

const MARKETING_DELIVERY_PROJECTION_NAME: &str = "marketing_delivery";
/// Internal channel marker emitted by `NotifyOwner`. The generic delivery
/// aggregate still owns claim/lease/retry/outbox/read-model behavior, while
/// this marker tells the write-side claim path that profile marketing consent
/// is not the authorization for an internal owner handoff.
pub const MARKETING_JOURNEY_OWNER_NOTIFICATION_CHANNEL_EVENT: &str =
    "marketing.journey.owner_notification";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingDeliveryWriteReceipt {
    pub delivery_id: Uuid,
    pub version: u64,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingProviderCallbackReceipt {
    pub inbox_key: String,
    pub order: MarketingProviderCallbackOrder,
    pub duplicate: bool,
    pub delivery: Option<MarketingDeliveryWriteReceipt>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MarketingDeliveryProjectionReconciliation {
    pub source_event_count: u64,
    pub source_delivery_count: u64,
    pub read_row_count: u64,
    pub missing_delivery_count: u64,
    pub stale_delivery_count: u64,
    pub ahead_delivery_count: u64,
    pub payload_mismatch_count: u64,
    pub orphan_read_row_count: u64,
    pub checkpoint_mismatch_count: u64,
    pub healthy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingDeliveryClaim {
    pub dispatch: DeliveryDispatch,
    pub authorization: AttemptAuthorization,
    pub claim_id: Uuid,
    pub worker_id: Uuid,
    pub claim_until: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct SqlxMarketingDeliveryStore {
    write_pool: PgPool,
    read_pool: PgPool,
    holding_ref: String,
    allowed_channels: Option<Vec<String>>,
}

impl SqlxMarketingDeliveryStore {
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
            allowed_channels: None,
        })
    }

    /// Constructs a tenant delivery source scoped to one provider account's
    /// enabled channels. The scope is applied in the authoritative claim SQL,
    /// before a worker lease is created, so an account cannot consume work for
    /// another account's channel.
    pub fn new_for_channels(
        write_pool: PgPool,
        read_pool: PgPool,
        holding_ref: impl Into<String>,
        channels: impl IntoIterator<Item = String>,
    ) -> Result<Self, PersistenceError> {
        let holding_ref = holding_ref.into();
        validate_holding_ref(&holding_ref)?;
        let allowed_channels = normalize_channel_filter(channels)?;
        Ok(Self {
            write_pool,
            read_pool,
            holding_ref,
            allowed_channels: Some(allowed_channels),
        })
    }

    pub fn from_pools(
        write_pool: &PgPool,
        read_pool: &PgPool,
        holding_ref: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        Self::new(write_pool.clone(), read_pool.clone(), holding_ref)
    }

    pub async fn enqueue(
        &self,
        dispatch: &DeliveryDispatch,
    ) -> Result<MarketingDeliveryWriteReceipt, PersistenceError> {
        dispatch.idempotency_key.validate().map_err(domain_error)?;
        let mut tx = self.begin_write().await?;
        let receipt = enqueue_in_transaction(&mut tx, &self.holding_ref, dispatch).await?;
        tx.commit().await?;
        Ok(receipt)
    }

    /// Load one authoritative delivery payload for authenticated callback
    /// routing. The caller must derive callback scope from this row; request
    /// headers are never used to select a company or branch.
    pub async fn load_dispatch(
        &self,
        delivery_id: Uuid,
    ) -> Result<DeliveryDispatch, PersistenceError> {
        validate_uuid(delivery_id, "Marketing delivery ID")?;
        let mut tx = self.begin_write().await?;
        let row = sqlx::query(
            "SELECT payload, version
             FROM erpx_tenant.marketing_deliveries
             WHERE delivery_id=$1 AND holding_ref=$2",
        )
        .bind(delivery_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "Marketing delivery",
            id: delivery_id,
        })?;
        let dispatch: DeliveryDispatch = serde_json::from_value(row.try_get("payload")?)?;
        let version = as_u64(row.try_get("version")?, "Marketing delivery version")?;
        if dispatch.id != delivery_id || dispatch.version != version {
            return Err(PersistenceError::InvalidData(
                "Marketing delivery payload identity or version mismatch".into(),
            ));
        }
        tx.commit().await?;
        Ok(dispatch)
    }

    pub async fn claim_due(
        &self,
        worker_id: Uuid,
        now: DateTime<Utc>,
        limit: u32,
        lease: Duration,
    ) -> Result<Vec<MarketingDeliveryClaim>, PersistenceError> {
        validate_uuid(worker_id, "Marketing delivery worker ID")?;
        validate_claim_limit(limit)?;
        if lease < Duration::seconds(MIN_MARKETING_DELIVERY_LEASE_SECONDS as i64)
            || lease > Duration::seconds(MAX_MARKETING_DELIVERY_LEASE_SECONDS as i64)
        {
            return Err(PersistenceError::InvalidData(
                "Marketing delivery lease is outside its bound".into(),
            ));
        }
        let mut tx = self.begin_write().await?;
        let rows = if let Some(allowed_channels) = &self.allowed_channels {
            sqlx::query(
                "SELECT delivery_id, payload, version, claim_id, claimed_by, claim_until
                 FROM erpx_tenant.marketing_deliveries
                 WHERE holding_ref=$1
                   AND payload->>'channel' = ANY($2::text[])
                   AND state IN ('queued', 'retry_scheduled')
                   AND next_attempt_at <= $3
                   AND (claim_until IS NULL OR claim_until <= $3)
                 ORDER BY next_attempt_at, delivery_id
                 LIMIT $4
                 FOR UPDATE SKIP LOCKED",
            )
            .bind(&self.holding_ref)
            .bind(allowed_channels)
            .bind(now)
            .bind(i64::from(limit))
            .fetch_all(&mut *tx)
            .await?
        } else {
            sqlx::query(
                "SELECT delivery_id, payload, version, claim_id, claimed_by, claim_until
                 FROM erpx_tenant.marketing_deliveries
                 WHERE holding_ref=$1
                   AND state IN ('queued', 'retry_scheduled')
                   AND next_attempt_at <= $2
                   AND (claim_until IS NULL OR claim_until <= $2)
                 ORDER BY next_attempt_at, delivery_id
                 LIMIT $3
                 FOR UPDATE SKIP LOCKED",
            )
            .bind(&self.holding_ref)
            .bind(now)
            .bind(i64::from(limit))
            .fetch_all(&mut *tx)
            .await?
        };
        let mut claims = Vec::with_capacity(rows.len());
        for row in rows {
            let delivery_id: Uuid = row.try_get("delivery_id")?;
            let version = as_u64(row.try_get("version")?, "Marketing delivery version")?;
            let mut dispatch: DeliveryDispatch = serde_json::from_value(row.try_get("payload")?)?;
            if dispatch.id != delivery_id || dispatch.version != version {
                return Err(PersistenceError::InvalidData(
                    "Marketing delivery payload identity or version mismatch".into(),
                ));
            }
            let decision = if is_internal_owner_notification(&dispatch) {
                internal_owner_notification_recheck(now)
            } else {
                let consents =
                    load_consents(&mut tx, &self.holding_ref, dispatch.profile_id).await?;
                let suppressions =
                    load_suppressions(&mut tx, &self.holding_ref, dispatch.profile_id).await?;
                dispatch.recheck(&consents, &suppressions, now)
            };
            let version_before_attempt = dispatch.version;
            let authorization = match dispatch.begin_attempt(&decision, now) {
                Ok(authorization) => authorization,
                Err(_error) => {
                    // A final consent/suppression/expiry/max-attempts check can
                    // mutate a queued aggregate to a terminal state even
                    // though no provider attempt is claimed. Keep that state
                    // change on the authoritative event stream; otherwise the
                    // read model and every rebuild would retain stale queued
                    // work.
                    if dispatch.state.is_terminal() && dispatch.version != version_before_attempt {
                        update_dispatch(&mut tx, &self.holding_ref, &dispatch, None, None, None)
                            .await?;
                        write_source_event(
                            &mut tx,
                            &self.holding_ref,
                            dispatch.id,
                            dispatch.version,
                            &serde_json::to_value(&dispatch)?,
                            terminal_transition_event_type(&dispatch),
                        )
                        .await?;
                    }
                    continue;
                }
            };
            let claim_id = Uuid::new_v4();
            let claim_until = now + lease;
            update_dispatch(
                &mut tx,
                &self.holding_ref,
                &dispatch,
                Some(claim_id),
                Some(worker_id),
                Some(claim_until),
            )
            .await?;
            claims.push(MarketingDeliveryClaim {
                dispatch,
                authorization,
                claim_id,
                worker_id,
                claim_until,
            });
        }
        tx.commit().await?;
        Ok(claims)
    }

    pub async fn record_provider_result(
        &self,
        worker_id: Uuid,
        delivery_id: Uuid,
        claim_id: Uuid,
        authorization: &AttemptAuthorization,
        result: ProviderResult,
    ) -> Result<MarketingDeliveryWriteReceipt, PersistenceError> {
        validate_uuid(worker_id, "Marketing delivery worker ID")?;
        validate_uuid(delivery_id, "Marketing delivery ID")?;
        validate_uuid(claim_id, "Marketing delivery claim ID")?;
        let mut tx = self.begin_write().await?;
        let authoritative_at = sqlx::query_scalar::<_, DateTime<Utc>>("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        let row = sqlx::query(
            "SELECT payload, version, claim_id, claimed_by, claim_until
             FROM erpx_tenant.marketing_deliveries
             WHERE delivery_id=$1 AND holding_ref=$2 FOR UPDATE",
        )
        .bind(delivery_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "Marketing delivery",
            id: delivery_id,
        })?;
        let mut dispatch: DeliveryDispatch = serde_json::from_value(row.try_get("payload")?)?;
        let current_claim_id: Option<Uuid> = row.try_get("claim_id")?;
        let claimed_by: Option<Uuid> = row.try_get("claimed_by")?;
        let claim_until: Option<DateTime<Utc>> = row.try_get("claim_until")?;
        if current_claim_id != Some(claim_id)
            || claimed_by != Some(worker_id)
            || claim_until.is_none_or(|until| authoritative_at >= until)
        {
            return Err(PersistenceError::ClaimLost {
                worker_id,
                event_id: delivery_id,
            });
        }
        if authorization.dispatch_id != delivery_id
            || authorization.attempt != dispatch.attempt_count
            || authorization.idempotency_key != dispatch.idempotency_key
        {
            return Err(PersistenceError::InvalidData(
                "Marketing delivery attempt authorization does not match".into(),
            ));
        }
        dispatch
            .record_provider_result(authorization.attempt, authoritative_at, result)
            .map_err(domain_error)?;
        let event_type = provider_result_event_type(dispatch.state);
        let receipt =
            update_dispatch(&mut tx, &self.holding_ref, &dispatch, None, None, None).await?;
        write_source_event(
            &mut tx,
            &self.holding_ref,
            dispatch.id,
            dispatch.version,
            &serde_json::to_value(&dispatch)?,
            event_type,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    /// Renew a live delivery claim using the database clock. Worker-provided
    /// timestamps are never accepted as proof that a lease is still live.
    pub async fn renew_claim(
        &self,
        worker_id: Uuid,
        delivery_id: Uuid,
        claim_id: Uuid,
        lease: Duration,
    ) -> Result<DateTime<Utc>, PersistenceError> {
        validate_uuid(worker_id, "Marketing delivery worker ID")?;
        validate_uuid(delivery_id, "Marketing delivery ID")?;
        validate_uuid(claim_id, "Marketing delivery claim ID")?;
        if lease < Duration::seconds(MIN_MARKETING_DELIVERY_LEASE_SECONDS as i64)
            || lease > Duration::seconds(MAX_MARKETING_DELIVERY_LEASE_SECONDS as i64)
        {
            return Err(PersistenceError::InvalidData(
                "Marketing delivery lease is outside its bound".into(),
            ));
        }
        let mut tx = self.begin_write().await?;
        let authoritative_now = sqlx::query_scalar::<_, DateTime<Utc>>("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        let claim_until = authoritative_now + lease;
        let renewed = sqlx::query(
            "UPDATE erpx_tenant.marketing_deliveries
             SET claim_until=$5, updated_at=$6
             WHERE delivery_id=$1 AND holding_ref=$2 AND claim_id=$3
               AND claimed_by=$4 AND claim_until IS NOT NULL
               AND claim_until > $6
             RETURNING claim_until",
        )
        .bind(delivery_id)
        .bind(&self.holding_ref)
        .bind(claim_id)
        .bind(worker_id)
        .bind(claim_until)
        .bind(authoritative_now)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::ClaimLost {
            worker_id,
            event_id: delivery_id,
        })?;
        tx.commit().await?;
        renewed
            .try_get("claim_until")
            .map_err(PersistenceError::from)
    }

    /// Atomically persist an already-authenticated provider callback and apply
    /// its ordered outcome to the authoritative delivery aggregate. The raw
    /// body is represented only by its verified digest; duplicate and stale
    /// callbacks are retained in the tenant inbox without regressing state.
    pub async fn accept_provider_callback(
        &self,
        callback: &MarketingProviderCallback,
        received_at: DateTime<Utc>,
        correlation_id: Uuid,
    ) -> Result<MarketingProviderCallbackReceipt, PersistenceError> {
        validate_provider_callback_integrity(callback, received_at)?;
        callback
            .binding
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        if correlation_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "Marketing callback correlation is required".into(),
            ));
        }
        let mut tx = self.begin_write().await?;
        let row = sqlx::query(
            "SELECT payload FROM erpx_tenant.marketing_deliveries
             WHERE delivery_id=$1 AND holding_ref=$2 FOR UPDATE",
        )
        .bind(callback.delivery_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "Marketing delivery",
            id: callback.delivery_id,
        })?;
        let mut dispatch: DeliveryDispatch = serde_json::from_value(row.try_get("payload")?)?;
        callback
            .bind_to_delivery(&dispatch)
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;

        let previous = sqlx::query(
            "SELECT provider_event_id, provider_event_at, event_fingerprint
             FROM erpx_tenant.marketing_provider_callback_inbox
             WHERE holding_ref=$1 AND provider_account_ref=$2 AND delivery_id=$3
             ORDER BY provider_event_at DESC, provider_event_id DESC
             LIMIT 1",
        )
        .bind(&self.holding_ref)
        .bind(&callback.binding.provider_account_ref)
        .bind(callback.delivery_id)
        .fetch_optional(&mut *tx)
        .await?
        .map(callback_watermark_from_row)
        .transpose()?;
        let order = callback.classify_against(previous.as_ref());
        let inbox_key = callback.idempotency.inbox_key.as_str().to_owned();
        let fingerprint = hex::encode(callback.idempotency.event_fingerprint);
        let raw_body_sha256 = hex::encode(callback.authentication.raw_body_sha256);
        let payload = serde_json::to_value(callback)?;
        let inserted = sqlx::query(
            "INSERT INTO erpx_tenant.marketing_provider_callback_inbox
                (inbox_key, holding_ref, provider_account_ref, provider_event_id,
                 delivery_id, provider_message_ref, provider_event_at, received_at,
                 correlation_id, kind, order_class, event_fingerprint, raw_body_sha256,
                 authentication_scheme, verifier_key_ref, payload)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)
             ON CONFLICT (holding_ref, provider_account_ref, provider_event_id)
             DO NOTHING RETURNING inbox_key",
        )
        .bind(&inbox_key)
        .bind(&self.holding_ref)
        .bind(&callback.binding.provider_account_ref)
        .bind(&callback.provider_event_id)
        .bind(callback.delivery_id)
        .bind(&callback.provider_message_ref)
        .bind(callback.provider_event_at)
        .bind(received_at)
        .bind(correlation_id)
        .bind(callback_kind_text(callback.kind))
        .bind(callback_order_text(order))
        .bind(&fingerprint)
        .bind(&raw_body_sha256)
        .bind(&callback.authentication.scheme)
        .bind(&callback.authentication.verifier_key_ref)
        .bind(payload)
        .fetch_optional(&mut *tx)
        .await?;
        if inserted.is_none() {
            let existing = sqlx::query_scalar::<_, String>(
                "SELECT event_fingerprint
                 FROM erpx_tenant.marketing_provider_callback_inbox
                 WHERE holding_ref=$1 AND provider_account_ref=$2 AND provider_event_id=$3",
            )
            .bind(&self.holding_ref)
            .bind(&callback.binding.provider_account_ref)
            .bind(&callback.provider_event_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(PersistenceError::InvalidData(
                "Marketing callback inbox conflict disappeared".into(),
            ))?;
            if existing != fingerprint {
                return Err(PersistenceError::InvalidData(
                    "Marketing callback event identity conflict".into(),
                ));
            }
            tx.commit().await?;
            return Ok(MarketingProviderCallbackReceipt {
                inbox_key,
                order: MarketingProviderCallbackOrder::Duplicate,
                duplicate: true,
                delivery: None,
            });
        }
        if matches!(
            order,
            MarketingProviderCallbackOrder::OutOfOrder
                | MarketingProviderCallbackOrder::ConflictingDuplicate
        ) {
            tx.commit().await?;
            return Ok(MarketingProviderCallbackReceipt {
                inbox_key,
                order,
                duplicate: false,
                delivery: None,
            });
        }

        match callback.kind {
            erpx_domain::marketing_callback::MarketingProviderCallbackKind::Delivered => {
                dispatch
                    .mark_delivered_for_provider(
                        &callback.provider_message_ref,
                        callback.provider_event_at,
                    )
                    .map_err(domain_error)?;
            }
            erpx_domain::marketing_callback::MarketingProviderCallbackKind::Failed => {
                dispatch
                    .fail_for_provider(
                        &callback.provider_message_ref,
                        "provider_callback_failed",
                        callback.provider_event_at,
                    )
                    .map_err(domain_error)?;
            }
            erpx_domain::marketing_callback::MarketingProviderCallbackKind::Bounced => {
                dispatch
                    .fail_for_provider(
                        &callback.provider_message_ref,
                        "provider_callback_bounced",
                        callback.provider_event_at,
                    )
                    .map_err(domain_error)?;
            }
        }
        let receipt =
            update_dispatch(&mut tx, &self.holding_ref, &dispatch, None, None, None).await?;
        write_source_event(
            &mut tx,
            &self.holding_ref,
            dispatch.id,
            dispatch.version,
            &serde_json::to_value(&dispatch)?,
            "marketing.delivery_callback_applied.v1",
        )
        .await?;
        tx.commit().await?;
        Ok(MarketingProviderCallbackReceipt {
            inbox_key,
            order,
            duplicate: false,
            delivery: Some(receipt),
        })
    }

    /// Apply one authoritative delivery state to the disposable tenant read
    /// model. This method is intentionally separate from enqueue and command
    /// transitions so CQRS projection writes cannot make a read row appear
    /// before the worker consumes the source event. The exact source outbox
    /// envelope is checked first; a caller cannot project a fabricated state
    /// merely by possessing a validly shaped `DeliveryDispatch`.
    pub async fn project_read_model(
        &self,
        dispatch: &DeliveryDispatch,
    ) -> Result<MarketingDeliveryWriteReceipt, PersistenceError> {
        dispatch.idempotency_key.validate().map_err(domain_error)?;
        let payload = serde_json::to_value(dispatch)?;
        self.verify_authoritative_source_event(dispatch, &payload)
            .await?;
        self.validate_projection_version_fence(dispatch.id, dispatch.version)
            .await?;
        let mut tx = self.begin_read().await?;
        let row = sqlx::query(
            "INSERT INTO erpx_tenant.marketing_delivery_read_rows
                (delivery_id, holding_ref, profile_id, campaign_id, idempotency_key,
                 state, attempt_count, next_attempt_at, expires_at, version, payload)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
             ON CONFLICT (delivery_id) DO UPDATE SET
                 holding_ref=$2, profile_id=$3, campaign_id=$4, idempotency_key=$5,
                 state=$6, attempt_count=$7, next_attempt_at=$8, expires_at=$9,
                 version=$10, payload=$11, projected_at=now()
             WHERE erpx_tenant.marketing_delivery_read_rows.version <= EXCLUDED.version
             RETURNING version",
        )
        .bind(dispatch.id)
        .bind(&self.holding_ref)
        .bind(dispatch.profile_id)
        .bind(dispatch.campaign_id)
        .bind(dispatch.idempotency_key.as_str())
        .bind(delivery_state_text(dispatch.state))
        .bind(i32::try_from(dispatch.attempt_count).map_err(|_| {
            PersistenceError::InvalidData("Marketing delivery attempt count exceeds INTEGER".into())
        })?)
        .bind(dispatch.next_attempt_at)
        .bind(dispatch.expires_at)
        .bind(as_i64(dispatch.version, "Marketing delivery version")?)
        .bind(payload)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        let replayed = row.is_none();
        let version = row
            .map(|row| row.try_get("version"))
            .transpose()?
            .map(|version| as_u64(version, "Marketing delivery read version"))
            .transpose()?
            .unwrap_or(dispatch.version);
        Ok(MarketingDeliveryWriteReceipt {
            delivery_id: dispatch.id,
            version,
            replayed,
        })
    }

    /// Consume one bounded, ordered page of authoritative delivery outbox
    /// events and advance the disposable read cursor only after the exact
    /// source event has been projected. A worker crash between those two
    /// commits replays the event, which is safe because read rows are fenced
    /// by aggregate version and source-event authority is rechecked.
    pub async fn project_pending_read_model(
        &self,
        page_size: u32,
    ) -> Result<u64, PersistenceError> {
        if page_size == 0 || page_size > MAX_MARKETING_DELIVERY_PROJECTION_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(format!(
                "Marketing delivery projection page size must be 1..={MAX_MARKETING_DELIVERY_PROJECTION_PAGE_SIZE}"
            )));
        }
        let checkpoint = self.read_delivery_projection_checkpoint().await?;
        let mut source_tx = self.begin_write().await?;
        let limit = i64::from(page_size) + 1;
        let rows = if let Some((last_occurred_at, last_event_id)) = checkpoint {
            sqlx::query(
                "SELECT event_id, aggregate_id, aggregate_version, event_type,
                        payload, occurred_at
                 FROM erpx_tenant.outbox_events
                 WHERE holding_ref=$1 AND aggregate_type='marketing_delivery'
                   AND (occurred_at > $2 OR (occurred_at = $2 AND event_id > $3))
                 ORDER BY occurred_at, event_id
                 LIMIT $4",
            )
            .bind(&self.holding_ref)
            .bind(last_occurred_at)
            .bind(last_event_id)
            .bind(limit)
            .fetch_all(&mut *source_tx)
            .await?
        } else {
            sqlx::query(
                "SELECT event_id, aggregate_id, aggregate_version, event_type,
                        payload, occurred_at
                 FROM erpx_tenant.outbox_events
                 WHERE holding_ref=$1 AND aggregate_type='marketing_delivery'
                 ORDER BY occurred_at, event_id
                 LIMIT $2",
            )
            .bind(&self.holding_ref)
            .bind(limit)
            .fetch_all(&mut *source_tx)
            .await?
        };
        source_tx.commit().await?;

        let page_len = usize::try_from(page_size).map_err(|_| {
            PersistenceError::InvalidData(
                "Marketing delivery projection page size is not representable".into(),
            )
        })?;
        let mut projected = 0_u64;
        for row in rows.into_iter().take(page_len) {
            let event_id: Uuid = row.try_get("event_id")?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            let aggregate_version = as_u64(
                row.try_get::<i64, _>("aggregate_version")?,
                "Marketing delivery source version",
            )?;
            let event_type: String = row.try_get("event_type")?;
            let occurred_at: DateTime<Utc> = row.try_get("occurred_at")?;
            let envelope: Value = row.try_get("payload")?;
            let payload = envelope.get("aggregate").cloned().ok_or_else(|| {
                PersistenceError::InvalidData(
                    "Marketing delivery source event aggregate is missing".into(),
                )
            })?;
            let dispatch: DeliveryDispatch = serde_json::from_value(payload)?;
            if dispatch.id != aggregate_id
                || dispatch.version != aggregate_version
                || envelope.get("aggregate_type").and_then(Value::as_str)
                    != Some("marketing_delivery")
                || envelope.get("version").and_then(Value::as_u64) != Some(aggregate_version)
                || envelope.get("event_type").and_then(Value::as_str) != Some(event_type.as_str())
            {
                return Err(PersistenceError::InvalidData(
                    "Marketing delivery source event identity is invalid".into(),
                ));
            }
            self.validate_projection_version_fence(aggregate_id, aggregate_version)
                .await?;
            self.project_read_model(&dispatch).await?;
            self.advance_delivery_projection_checkpoint(event_id, occurred_at)
                .await?;
            projected = projected.saturating_add(1);
        }
        Ok(projected)
    }

    /// Clear only the disposable delivery read model and its cursor. The
    /// authoritative write aggregate and outbox are never touched; the next
    /// projection pass deterministically rebuilds rows from version-one
    /// delivery events.
    pub async fn clear_delivery_read_model(&self) -> Result<u64, PersistenceError> {
        let mut tx = self.begin_read().await?;
        let deleted = sqlx::query(
            "DELETE FROM erpx_tenant.marketing_delivery_read_rows
             WHERE holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        sqlx::query(
            "DELETE FROM erpx_tenant.marketing_delivery_projection_checkpoints
             WHERE projection_name=$1 AND holding_ref=$2",
        )
        .bind(MARKETING_DELIVERY_PROJECTION_NAME)
        .bind(&self.holding_ref)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(deleted)
    }

    async fn read_delivery_projection_checkpoint(
        &self,
    ) -> Result<Option<(DateTime<Utc>, Uuid)>, PersistenceError> {
        let mut tx = self.begin_read().await?;
        let row = sqlx::query(
            "SELECT last_occurred_at, last_event_id
             FROM erpx_tenant.marketing_delivery_projection_checkpoints
             WHERE projection_name=$1 AND holding_ref=$2",
        )
        .bind(MARKETING_DELIVERY_PROJECTION_NAME)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let occurred_at: Option<DateTime<Utc>> = row.try_get("last_occurred_at")?;
        let occurred_at = occurred_at.ok_or_else(|| {
            PersistenceError::InvalidData(
                "Marketing delivery projection checkpoint timestamp is missing".into(),
            )
        })?;
        let event_id: Option<Uuid> = row.try_get("last_event_id")?;
        let event_id = event_id.ok_or_else(|| {
            PersistenceError::InvalidData(
                "Marketing delivery projection checkpoint event ID is missing".into(),
            )
        })?;
        Ok(Some((occurred_at, event_id)))
    }

    async fn advance_delivery_projection_checkpoint(
        &self,
        event_id: Uuid,
        occurred_at: DateTime<Utc>,
    ) -> Result<(), PersistenceError> {
        let mut tx = self.begin_read().await?;
        // Projector retries are expected after a crash or concurrent delivery
        // worker. Advance only for a strictly newer source cursor so a replay
        // cannot double-count the checkpoint or move it backwards.
        let affected = sqlx::query(
            "INSERT INTO erpx_tenant.marketing_delivery_projection_checkpoints
                (projection_name, holding_ref, last_event_id, last_occurred_at,
                 source_event_count)
             VALUES ($1,$2,$3,$4,1)
             ON CONFLICT (projection_name, holding_ref) DO UPDATE SET
                holding_ref=$2, last_event_id=$3, last_occurred_at=$4,
                source_event_count=
                    erpx_tenant.marketing_delivery_projection_checkpoints.source_event_count + 1,
                projected_at=now()
             WHERE erpx_tenant.marketing_delivery_projection_checkpoints.holding_ref=$2
               AND (
                    erpx_tenant.marketing_delivery_projection_checkpoints.last_occurred_at IS NULL
                    OR $4 > erpx_tenant.marketing_delivery_projection_checkpoints.last_occurred_at
                    OR (
                        $4 = erpx_tenant.marketing_delivery_projection_checkpoints.last_occurred_at
                        AND $3 > erpx_tenant.marketing_delivery_projection_checkpoints.last_event_id
                    )
               )",
        )
        .bind(MARKETING_DELIVERY_PROJECTION_NAME)
        .bind(&self.holding_ref)
        .bind(event_id)
        .bind(occurred_at)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if affected == 0 {
            let scoped_checkpoint = sqlx::query_scalar::<_, i32>(
                "SELECT 1
                 FROM erpx_tenant.marketing_delivery_projection_checkpoints
                 WHERE projection_name=$1 AND holding_ref=$2",
            )
            .bind(MARKETING_DELIVERY_PROJECTION_NAME)
            .bind(&self.holding_ref)
            .fetch_optional(&mut *tx)
            .await?;
            if scoped_checkpoint.is_none() {
                return Err(PersistenceError::InvalidData(
                    "Marketing delivery projection checkpoint scope conflict".into(),
                ));
            }
        }
        tx.commit().await?;
        Ok(())
    }

    async fn verify_authoritative_source_event(
        &self,
        dispatch: &DeliveryDispatch,
        dispatch_payload: &Value,
    ) -> Result<(), PersistenceError> {
        let mut tx = self.begin_write().await?;
        let rows = sqlx::query(
            "SELECT event_type, payload
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1
               AND aggregate_type='marketing_delivery'
               AND aggregate_id=$2
               AND aggregate_version=$3
             ORDER BY event_id",
        )
        .bind(&self.holding_ref)
        .bind(dispatch.id)
        .bind(as_i64(dispatch.version, "Marketing delivery version")?)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;

        let authoritative = rows.first().map(|row| {
            let event_type = row.try_get::<String, _>("event_type")?;
            let payload: Value = row.try_get("payload")?;
            Ok::<_, PersistenceError>((event_type, payload))
        });
        let Some((event_type, source_payload)) = authoritative.transpose()? else {
            return Err(PersistenceError::InvalidData(
                "Marketing delivery projection event is not authoritative".into(),
            ));
        };
        let expected_payload = serde_json::json!({
            "aggregate_type": "marketing_delivery",
            "aggregate": dispatch_payload,
            "version": dispatch.version,
            "event_type": event_type,
        });
        if rows.len() != 1
            || source_payload != expected_payload
            || source_payload.get("event_type").and_then(Value::as_str) != Some(event_type.as_str())
        {
            return Err(PersistenceError::InvalidData(
                "Marketing delivery projection event is not authoritative".into(),
            ));
        }
        Ok(())
    }

    pub async fn get(
        &self,
        delivery_id: Uuid,
    ) -> Result<Option<DeliveryDispatch>, PersistenceError> {
        validate_uuid(delivery_id, "Marketing delivery ID")?;
        let mut tx = self.begin_read().await?;
        let row = sqlx::query(
            "SELECT payload, version FROM erpx_tenant.marketing_delivery_read_rows
             WHERE delivery_id=$1 AND holding_ref=$2",
        )
        .bind(delivery_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let dispatch: DeliveryDispatch = serde_json::from_value(row.try_get("payload")?)?;
        let version = as_u64(row.try_get("version")?, "Marketing delivery read version")?;
        if dispatch.id != delivery_id || dispatch.version != version {
            return Err(PersistenceError::InvalidData(
                "Marketing delivery read payload identity or version mismatch".into(),
            ));
        }
        Ok(Some(dispatch))
    }

    /// Compare the authoritative delivery event streams with the disposable
    /// read rows. This is read-only and never repairs source state.
    pub async fn reconcile_projection(
        &self,
    ) -> Result<MarketingDeliveryProjectionReconciliation, PersistenceError> {
        let mut source_tx = self.begin_write().await?;
        let source_rows = sqlx::query(
            "SELECT event_id, aggregate_id, aggregate_version, event_type,
                    payload, occurred_at
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1 AND aggregate_type='marketing_delivery'
             ORDER BY occurred_at, event_id",
        )
        .bind(&self.holding_ref)
        .fetch_all(&mut *source_tx)
        .await?;
        source_tx.commit().await?;
        let mut source = std::collections::BTreeMap::new();
        let mut source_versions = std::collections::BTreeMap::<Uuid, u64>::new();
        let mut source_event_count = 0_u64;
        let mut source_cursor = None;
        for row in source_rows {
            let event_id: Uuid = row.try_get("event_id")?;
            let delivery_id: Uuid = row.try_get("aggregate_id")?;
            let version = as_u64(
                row.try_get("aggregate_version")?,
                "Marketing delivery source version",
            )?;
            let event_type: String = row.try_get("event_type")?;
            let envelope: Value = row.try_get("payload")?;
            let aggregate = envelope.get("aggregate").cloned().ok_or_else(|| {
                PersistenceError::InvalidData(
                    "Marketing delivery source aggregate is missing".into(),
                )
            })?;
            if envelope.get("aggregate_type").and_then(Value::as_str) != Some("marketing_delivery")
                || envelope.get("version").and_then(Value::as_u64) != Some(version)
                || envelope.get("event_type").and_then(Value::as_str) != Some(event_type.as_str())
            {
                return Err(PersistenceError::InvalidData(
                    "Marketing delivery source envelope is invalid".into(),
                ));
            }
            let dispatch: DeliveryDispatch = serde_json::from_value(aggregate.clone())?;
            if dispatch.id != delivery_id || dispatch.version != version {
                return Err(PersistenceError::InvalidData(
                    "Marketing delivery source aggregate identity is invalid".into(),
                ));
            }
            source_event_count = source_event_count.checked_add(1).ok_or_else(|| {
                PersistenceError::InvalidData("Marketing delivery event count overflow".into())
            })?;
            source_cursor = Some((row.try_get("occurred_at")?, event_id));
            if let Some(previous_version) = source_versions.get(&delivery_id).copied() {
                if version != previous_version.saturating_add(1) {
                    return Err(PersistenceError::InvalidData(
                        "Marketing delivery source event versions are not contiguous".into(),
                    ));
                }
            } else if version != 1 {
                return Err(PersistenceError::InvalidData(
                    "Marketing delivery source event stream must start at version one".into(),
                ));
            }
            source_versions.insert(delivery_id, version);
            if let Some((existing_version, existing_payload)) = source.get(&delivery_id) {
                if *existing_version == version && existing_payload != &aggregate {
                    return Err(PersistenceError::InvalidData(
                        "Marketing delivery source version has conflicting payloads".into(),
                    ));
                }
                if *existing_version > version {
                    continue;
                }
            }
            source.insert(delivery_id, (version, aggregate));
        }

        let mut read_tx = self.begin_read().await?;
        let read_rows = sqlx::query(
            "SELECT delivery_id, version, payload
             FROM erpx_tenant.marketing_delivery_read_rows
             WHERE holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .fetch_all(&mut *read_tx)
        .await?;
        let checkpoint = sqlx::query(
            "SELECT last_event_id, last_occurred_at, source_event_count
             FROM erpx_tenant.marketing_delivery_projection_checkpoints
             WHERE projection_name=$1 AND holding_ref=$2",
        )
        .bind(MARKETING_DELIVERY_PROJECTION_NAME)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *read_tx)
        .await?;
        read_tx.commit().await?;
        let mut read = std::collections::BTreeMap::new();
        for row in read_rows {
            let delivery_id: Uuid = row.try_get("delivery_id")?;
            let version = as_u64(row.try_get("version")?, "Marketing delivery read version")?;
            let payload: Value = row.try_get("payload")?;
            let dispatch: DeliveryDispatch = serde_json::from_value(payload.clone())?;
            if dispatch.id != delivery_id || dispatch.version != version {
                return Err(PersistenceError::InvalidData(
                    "Marketing delivery read payload identity or version mismatch".into(),
                ));
            }
            read.insert(delivery_id, (version, payload));
        }
        let missing_delivery_count = source.keys().filter(|id| !read.contains_key(id)).count();
        let stale_delivery_count = source
            .iter()
            .filter(|(id, (version, _))| {
                read.get(id)
                    .is_some_and(|(read_version, _)| read_version < version)
            })
            .count();
        let ahead_delivery_count = read
            .iter()
            .filter(|(id, (read_version, _))| {
                source
                    .get(id)
                    .is_some_and(|(source_version, _)| read_version > source_version)
            })
            .count();
        let payload_mismatch_count = source
            .iter()
            .filter(|(id, (source_version, source_payload))| {
                read.get(id).is_some_and(|(read_version, read_payload)| {
                    read_version == source_version && read_payload != source_payload
                })
            })
            .count();
        let orphan_read_row_count = read.keys().filter(|id| !source.contains_key(id)).count();
        let checkpoint_mismatch_count = match (source_cursor, checkpoint) {
            (None, None) => 0,
            (Some((expected_at, expected_event_id)), Some(row)) => {
                let actual_event_id: Option<Uuid> = row.try_get("last_event_id")?;
                let actual_at: Option<DateTime<Utc>> = row.try_get("last_occurred_at")?;
                let actual_count = as_u64(
                    row.try_get("source_event_count")?,
                    "Marketing delivery checkpoint event count",
                )?;
                if actual_event_id != Some(expected_event_id)
                    || actual_at != Some(expected_at)
                    || actual_count != source_event_count
                {
                    1
                } else {
                    0
                }
            }
            (None, Some(_)) | (Some(_), None) => 1,
        };
        Ok(MarketingDeliveryProjectionReconciliation {
            source_event_count,
            source_delivery_count: source.len() as u64,
            read_row_count: read.len() as u64,
            missing_delivery_count: missing_delivery_count as u64,
            stale_delivery_count: stale_delivery_count as u64,
            ahead_delivery_count: ahead_delivery_count as u64,
            payload_mismatch_count: payload_mismatch_count as u64,
            orphan_read_row_count: orphan_read_row_count as u64,
            checkpoint_mismatch_count,
            healthy: missing_delivery_count == 0
                && stale_delivery_count == 0
                && ahead_delivery_count == 0
                && payload_mismatch_count == 0
                && orphan_read_row_count == 0
                && checkpoint_mismatch_count == 0,
        })
    }

    async fn begin_write(&self) -> Result<Transaction<'_, Postgres>, PersistenceError> {
        begin_scoped(&self.write_pool, &self.holding_ref).await
    }

    async fn begin_read(&self) -> Result<Transaction<'_, Postgres>, PersistenceError> {
        begin_scoped(&self.read_pool, &self.holding_ref).await
    }

    async fn validate_projection_version_fence(
        &self,
        delivery_id: Uuid,
        incoming_version: u64,
    ) -> Result<(), PersistenceError> {
        let mut tx = self.begin_read().await?;
        let current = sqlx::query_scalar::<_, i64>(
            "SELECT version
             FROM erpx_tenant.marketing_delivery_read_rows
             WHERE holding_ref=$1 AND delivery_id=$2",
        )
        .bind(&self.holding_ref)
        .bind(delivery_id)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        let Some(current) = current else {
            if incoming_version == 1 {
                return Ok(());
            }
            return Err(PersistenceError::InvalidData(
                "Marketing delivery projection must start at version one".into(),
            ));
        };
        let current = as_u64(current, "Marketing delivery projected version")?;
        if incoming_version != current && incoming_version != current.saturating_add(1) {
            return Err(PersistenceError::InvalidData(
                "Marketing delivery projection version is not contiguous".into(),
            ));
        }
        Ok(())
    }
}

/// Enqueue a dispatch in a caller-owned tenant-scoped transaction. This is
/// used by journey execution so the cursor advance, delivery idempotency
/// reservation, delivery snapshot, and delivery outbox event commit or roll
/// back together.
pub(crate) async fn enqueue_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    dispatch: &DeliveryDispatch,
) -> Result<MarketingDeliveryWriteReceipt, PersistenceError> {
    dispatch.idempotency_key.validate().map_err(domain_error)?;
    let payload = serde_json::to_value(dispatch)?;
    let payload_hash = hash_payload(&payload)?;
    let existing = sqlx::query(
        "SELECT delivery_id, payload_hash
         FROM erpx_tenant.marketing_delivery_idempotency
         WHERE idempotency_key=$1 AND holding_ref=$2 FOR UPDATE",
    )
    .bind(dispatch.idempotency_key.as_str())
    .bind(holding_ref)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(row) = existing {
        let delivery_id: Uuid = row.try_get("delivery_id")?;
        let existing_hash: String = row.try_get("payload_hash")?;
        if delivery_id != dispatch.id || existing_hash != payload_hash {
            return Err(PersistenceError::IdempotencyConflict {
                key: dispatch.idempotency_key.as_str().to_owned(),
            });
        }
        let version = sqlx::query_scalar::<_, i64>(
            "SELECT version FROM erpx_tenant.marketing_deliveries
             WHERE delivery_id=$1 AND holding_ref=$2",
        )
        .bind(dispatch.id)
        .bind(holding_ref)
        .fetch_one(&mut **tx)
        .await?;
        return Ok(MarketingDeliveryWriteReceipt {
            delivery_id,
            version: as_u64(version, "Marketing delivery version")?,
            replayed: true,
        });
    }
    sqlx::query(
        "INSERT INTO erpx_tenant.marketing_delivery_idempotency
            (idempotency_key, holding_ref, delivery_id, payload_hash)
         VALUES ($1,$2,$3,$4)",
    )
    .bind(dispatch.idempotency_key.as_str())
    .bind(holding_ref)
    .bind(dispatch.id)
    .bind(&payload_hash)
    .execute(&mut **tx)
    .await?;
    insert_dispatch(tx, holding_ref, dispatch, &payload).await?;
    write_source_event(
        tx,
        holding_ref,
        dispatch.id,
        dispatch.version,
        &payload,
        "marketing.delivery_queued.v1",
    )
    .await?;
    Ok(MarketingDeliveryWriteReceipt {
        delivery_id: dispatch.id,
        version: dispatch.version,
        replayed: false,
    })
}

async fn insert_dispatch(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    dispatch: &DeliveryDispatch,
    payload: &Value,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_tenant.marketing_deliveries
            (delivery_id, holding_ref, profile_id, campaign_id, idempotency_key,
             state, attempt_count, next_attempt_at, expires_at, version, payload)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
    )
    .bind(dispatch.id)
    .bind(holding_ref)
    .bind(dispatch.profile_id)
    .bind(dispatch.campaign_id)
    .bind(dispatch.idempotency_key.as_str())
    .bind(delivery_state_text(dispatch.state))
    .bind(i32::try_from(dispatch.attempt_count).map_err(|_| {
        PersistenceError::InvalidData("Marketing delivery attempt count exceeds INTEGER".into())
    })?)
    .bind(dispatch.next_attempt_at)
    .bind(dispatch.expires_at)
    .bind(as_i64(dispatch.version, "Marketing delivery version")?)
    .bind(payload)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn update_dispatch(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    dispatch: &DeliveryDispatch,
    claim_id: Option<Uuid>,
    claimed_by: Option<Uuid>,
    claim_until: Option<DateTime<Utc>>,
) -> Result<MarketingDeliveryWriteReceipt, PersistenceError> {
    let payload = serde_json::to_value(dispatch)?;
    let affected = sqlx::query(
        "UPDATE erpx_tenant.marketing_deliveries
         SET state=$3, attempt_count=$4, next_attempt_at=$5, expires_at=$6,
             version=$7, payload=$8, claim_id=$9, claimed_by=$10,
             claim_until=$11, last_failure=$12, terminal_reason=$13,
             updated_at=now()
         WHERE delivery_id=$1 AND holding_ref=$2 AND version=$14",
    )
    .bind(dispatch.id)
    .bind(holding_ref)
    .bind(delivery_state_text(dispatch.state))
    .bind(i32::try_from(dispatch.attempt_count).map_err(|_| {
        PersistenceError::InvalidData("Marketing delivery attempt count exceeds INTEGER".into())
    })?)
    .bind(dispatch.next_attempt_at)
    .bind(dispatch.expires_at)
    .bind(as_i64(dispatch.version, "Marketing delivery version")?)
    .bind(payload)
    .bind(claim_id)
    .bind(claimed_by)
    .bind(claim_until)
    .bind(&dispatch.last_failure)
    .bind(dispatch.terminal_reason.map(terminal_reason_text))
    .bind(as_i64(
        dispatch.version.saturating_sub(1),
        "Marketing previous version",
    )?)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    if affected != 1 {
        return Err(PersistenceError::OptimisticConcurrency {
            aggregate_type: "marketing_delivery".into(),
            aggregate_id: dispatch.id,
            expected: dispatch.version.checked_sub(1),
            actual: None,
        });
    }
    Ok(MarketingDeliveryWriteReceipt {
        delivery_id: dispatch.id,
        version: dispatch.version,
        replayed: false,
    })
}

async fn write_source_event(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    delivery_id: Uuid,
    version: u64,
    payload: &Value,
    event_type: &str,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_tenant.outbox_events
            (event_id, aggregate_type, aggregate_id, holding_ref,
             aggregate_version, event_type, payload, occurred_at)
         VALUES ($1,'marketing_delivery',$2,$3,$4,$5,$6,now())",
    )
    .bind(Uuid::new_v4())
    .bind(delivery_id)
    .bind(holding_ref)
    .bind(as_i64(version, "Marketing delivery version")?)
    .bind(event_type)
    .bind(serde_json::json!({
        "aggregate_type": "marketing_delivery",
        "aggregate": payload,
        "version": version,
        "event_type": event_type,
    }))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn callback_kind_text(
    kind: erpx_domain::marketing_callback::MarketingProviderCallbackKind,
) -> &'static str {
    match kind {
        erpx_domain::marketing_callback::MarketingProviderCallbackKind::Delivered => "delivered",
        erpx_domain::marketing_callback::MarketingProviderCallbackKind::Failed => "failed",
        erpx_domain::marketing_callback::MarketingProviderCallbackKind::Bounced => "bounced",
    }
}

fn callback_order_text(order: MarketingProviderCallbackOrder) -> &'static str {
    match order {
        MarketingProviderCallbackOrder::New => "new",
        MarketingProviderCallbackOrder::InOrder => "in_order",
        MarketingProviderCallbackOrder::OutOfOrder => "out_of_order",
        MarketingProviderCallbackOrder::Duplicate => "duplicate",
        MarketingProviderCallbackOrder::ConflictingDuplicate => "conflicting_duplicate",
    }
}

fn provider_result_event_type(state: DeliveryState) -> &'static str {
    match state {
        DeliveryState::ProviderAccepted => "marketing.delivery_provider_accepted.v1",
        DeliveryState::RetryScheduled => "marketing.delivery_retry_scheduled.v1",
        DeliveryState::FailedTerminal => "marketing.delivery_failed.v1",
        _ => "marketing.delivery_provider_result.v1",
    }
}

fn terminal_transition_event_type(dispatch: &DeliveryDispatch) -> &'static str {
    match dispatch.state {
        DeliveryState::Suppressed => "marketing.delivery_suppressed.v1",
        DeliveryState::FailedTerminal => "marketing.delivery_failed.v1",
        _ => "marketing.delivery_terminal.v1",
    }
}

fn callback_watermark_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<MarketingProviderCallbackWatermark, PersistenceError> {
    let fingerprint = hex::decode(row.try_get::<String, _>("event_fingerprint")?)
        .map_err(|_| PersistenceError::InvalidData("callback fingerprint is invalid".into()))?;
    let event_fingerprint: [u8; 32] = fingerprint.try_into().map_err(|_| {
        PersistenceError::InvalidData("callback fingerprint length is invalid".into())
    })?;
    Ok(MarketingProviderCallbackWatermark {
        provider_event_id: row.try_get("provider_event_id")?,
        provider_event_at: row.try_get("provider_event_at")?,
        event_fingerprint,
    })
}

/// The domain verifier derives callback idempotency from the authenticated
/// event fields. Keep the persistence boundary fail-closed as well: callers
/// cannot replace that derived envelope with a forged key or fingerprint after
/// verification and before the callback reaches the inbox transaction.
fn validate_provider_callback_integrity(
    callback: &MarketingProviderCallback,
    received_at: DateTime<Utc>,
) -> Result<(), PersistenceError> {
    let request = MarketingProviderCallbackRequest::new(
        callback.binding.clone(),
        callback.delivery_id,
        callback.provider_message_ref.clone(),
        callback.provider_event_id.clone(),
        callback.provider_event_at,
        callback.kind,
        ProviderAuthenticationDecision::Verified(callback.authentication.clone()),
    )
    .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
    let derived = request
        .verify(&callback.binding, received_at)
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
    if derived != *callback {
        return Err(PersistenceError::InvalidData(
            "Marketing callback idempotency envelope is not derived from its authenticated event"
                .into(),
        ));
    }
    Ok(())
}

async fn load_consents(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    profile_id: Uuid,
) -> Result<Vec<ConsentRecord>, PersistenceError> {
    let rows = sqlx::query(
        "SELECT payload FROM erpx_tenant.marketing_consents
         WHERE holding_ref=$1 AND profile_id=$2 ORDER BY recorded_at, consent_id",
    )
    .bind(holding_ref)
    .bind(profile_id)
    .fetch_all(&mut **tx)
    .await?;
    rows.into_iter()
        .map(|row| Ok(serde_json::from_value(row.try_get("payload")?)?))
        .collect()
}

async fn load_suppressions(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    profile_id: Uuid,
) -> Result<Vec<SuppressionRecord>, PersistenceError> {
    let rows = sqlx::query(
        "SELECT payload FROM erpx_tenant.marketing_suppressions
         WHERE holding_ref=$1 AND profile_id=$2 ORDER BY created_at, suppression_id",
    )
    .bind(holding_ref)
    .bind(profile_id)
    .fetch_all(&mut **tx)
    .await?;
    rows.into_iter()
        .map(|row| Ok(serde_json::from_value(row.try_get("payload")?)?))
        .collect()
}

fn delivery_state_text(state: DeliveryState) -> &'static str {
    match state {
        DeliveryState::Queued => "queued",
        DeliveryState::Sending => "sending",
        DeliveryState::RetryScheduled => "retry_scheduled",
        DeliveryState::ProviderAccepted => "provider_accepted",
        DeliveryState::Delivered => "delivered",
        DeliveryState::Suppressed => "suppressed",
        DeliveryState::FailedTerminal => "failed_terminal",
    }
}

fn is_internal_owner_notification(dispatch: &DeliveryDispatch) -> bool {
    let erpx_domain::marketing_delivery::ChannelDispatchRecord::Webhook {
        destination_ref,
        event_type,
        payload_ref,
    } = &dispatch.record
    else {
        return false;
    };
    dispatch.channel == erpx_domain::marketing::MarketingChannel::Webhook
        && event_type == MARKETING_JOURNEY_OWNER_NOTIFICATION_CHANNEL_EVENT
        && !destination_ref.trim().is_empty()
        && Uuid::parse_str(payload_ref).is_ok_and(|notification_ref| {
            dispatch.id
                == crate::marketing_journey_execution_persistence::journey_owner_notification_delivery_id(
                    notification_ref,
                )
        })
        && dispatch.logical_message_ref == format!("journey-owner-notification:{payload_ref}")
}

fn internal_owner_notification_recheck(
    at: DateTime<Utc>,
) -> erpx_domain::marketing_delivery::DeliveryRecheckDecision {
    DeliveryRecheckDecision {
        checked_at: at,
        consent: ConsentRecheckDecision::Granted,
        suppression: SuppressionRecheckDecision::Clear,
        result: DeliveryRecheckResult::Allowed,
    }
}

fn terminal_reason_text(reason: DeliveryTerminalReason) -> &'static str {
    match reason {
        DeliveryTerminalReason::Suppression => "suppression",
        DeliveryTerminalReason::Expired => "expired",
        DeliveryTerminalReason::MaxAttempts => "max_attempts",
        DeliveryTerminalReason::PermanentProviderFailure => "permanent_provider_failure",
    }
}

fn domain_error(
    error: erpx_domain::marketing_delivery::MarketingDeliveryError,
) -> PersistenceError {
    PersistenceError::InvalidData(error.to_string())
}

fn hash_payload(payload: &Value) -> Result<String, PersistenceError> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(payload)?)))
}

fn validate_claim_limit(limit: u32) -> Result<(), PersistenceError> {
    if limit == 0 || limit > MAX_MARKETING_DELIVERY_CLAIM_BATCH_SIZE {
        Err(PersistenceError::InvalidData(format!(
            "Marketing delivery claim limit must be 1..={MAX_MARKETING_DELIVERY_CLAIM_BATCH_SIZE}"
        )))
    } else {
        Ok(())
    }
}

fn validate_uuid(value: Uuid, field: &'static str) -> Result<(), PersistenceError> {
    if value.is_nil() {
        Err(PersistenceError::InvalidData(format!(
            "{field} cannot be nil"
        )))
    } else {
        Ok(())
    }
}

fn validate_holding_ref(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
        Err(PersistenceError::Configuration(
            "holding reference must be bounded and non-empty",
        ))
    } else {
        Ok(())
    }
}

fn normalize_channel_filter(
    channels: impl IntoIterator<Item = String>,
) -> Result<Vec<String>, PersistenceError> {
    let mut channels: Vec<String> = channels.into_iter().collect();
    channels.sort_unstable();
    channels.dedup();
    if channels.is_empty()
        || channels.len() > 5
        || channels.iter().any(|channel| {
            !matches!(
                channel.as_str(),
                "email" | "sms" | "social" | "push" | "webhook"
            )
        })
    {
        return Err(PersistenceError::InvalidData(
            "Marketing delivery channel filter is invalid".into(),
        ));
    }
    Ok(channels)
}

fn as_i64(value: u64, field: &'static str) -> Result<i64, PersistenceError> {
    i64::try_from(value)
        .map_err(|_| PersistenceError::InvalidData(format!("{field} exceeds BIGINT")))
}

fn as_u64(value: i64, field: &'static str) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| PersistenceError::InvalidData(format!("{field} is negative")))
}

async fn begin_scoped<'a>(
    pool: &'a PgPool,
    holding_ref: &str,
) -> Result<Transaction<'a, Postgres>, PersistenceError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(holding_ref)
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_bounds_and_lease_limits_are_closed() {
        assert!(validate_claim_limit(0).is_err());
        assert!(validate_claim_limit(MAX_MARKETING_DELIVERY_CLAIM_BATCH_SIZE + 1).is_err());
        assert!(validate_holding_ref(" ").is_err());
        assert!(validate_holding_ref("holding-a").is_ok());
        assert!(
            Duration::seconds(MIN_MARKETING_DELIVERY_LEASE_SECONDS as i64)
                < Duration::seconds(MAX_MARKETING_DELIVERY_LEASE_SECONDS as i64)
        );
    }

    #[test]
    fn channel_filters_are_bounded_and_canonical() {
        assert_eq!(
            normalize_channel_filter(vec!["sms".into(), "email".into(), "sms".into()])
                .expect("channel filter"),
            vec!["email", "sms"]
        );
        assert!(normalize_channel_filter(vec!["fax".into()]).is_err());
        assert!(normalize_channel_filter(Vec::new()).is_err());
    }

    #[test]
    fn state_and_terminal_reason_mappings_are_closed() {
        assert_eq!(delivery_state_text(DeliveryState::Queued), "queued");
        assert_eq!(delivery_state_text(DeliveryState::Delivered), "delivered");
        assert_eq!(
            terminal_reason_text(DeliveryTerminalReason::Expired),
            "expired"
        );
    }

    #[test]
    fn owner_notification_dispatch_is_internal_but_marker_is_strict() {
        use erpx_domain::contracts::TenantScope;
        use erpx_domain::marketing::{MarketingChannel, MarketingPurpose};
        use erpx_domain::marketing_delivery::{
            ChannelDispatchRecord, DeliveryDispatch, RetryPolicy,
        };

        let at = Utc::now();
        let notification_ref = Uuid::from_u128(44);
        let dispatch = DeliveryDispatch::new(
            crate::marketing_journey_execution_persistence::journey_owner_notification_delivery_id(
                notification_ref,
            ),
            TenantScope::Holding {
                holding_id: Uuid::from_u128(46),
            },
            Uuid::from_u128(47),
            Uuid::from_u128(48),
            MarketingPurpose::Transactional,
            MarketingChannel::Webhook,
            ChannelDispatchRecord::Webhook {
                destination_ref: "operations-owner".into(),
                event_type: MARKETING_JOURNEY_OWNER_NOTIFICATION_CHANNEL_EVENT.into(),
                payload_ref: notification_ref.to_string(),
            },
            format!("journey-owner-notification:{notification_ref}"),
            1,
            at,
            at + Duration::days(1),
            RetryPolicy::default(),
        )
        .expect("owner notification dispatch");
        assert!(is_internal_owner_notification(&dispatch));

        let mut forged = dispatch.clone();
        forged.logical_message_ref = "ordinary-webhook".into();
        assert!(!is_internal_owner_notification(&forged));
        assert!(internal_owner_notification_recheck(at).allows_send());
    }

    #[test]
    fn provider_callback_idempotency_envelope_is_recomputed_at_persistence_boundary() {
        use erpx_domain::{
            contracts::TenantScope,
            marketing_callback::{
                MarketingProviderCallbackBinding, MarketingProviderCallbackKind,
                MarketingProviderCallbackRequest, ProviderAuthenticationDecision,
                VerifiedProviderAuthentication,
            },
        };

        let holding_id = Uuid::from_u128(1);
        let binding = MarketingProviderCallbackBinding::new(
            "provider-account",
            "tenant",
            holding_id,
            TenantScope::Holding { holding_id },
        )
        .expect("binding");
        let received_at = Utc::now();
        let callback = MarketingProviderCallbackRequest::new(
            binding.clone(),
            Uuid::from_u128(2),
            "provider-message",
            "provider-event",
            received_at,
            MarketingProviderCallbackKind::Delivered,
            ProviderAuthenticationDecision::Verified(
                VerifiedProviderAuthentication::new("hmac-sha256", "key-ref", [7; 32])
                    .expect("authentication"),
            ),
        )
        .expect("request")
        .verify(&binding, received_at)
        .expect("verified callback");

        assert!(validate_provider_callback_integrity(&callback, received_at).is_ok());

        let mut forged = callback.clone();
        forged.idempotency.event_fingerprint = [8; 32];
        let error = validate_provider_callback_integrity(&forged, received_at)
            .expect_err("forged idempotency must be rejected");
        assert!(
            error
                .to_string()
                .contains("idempotency envelope is not derived")
        );
    }
}
