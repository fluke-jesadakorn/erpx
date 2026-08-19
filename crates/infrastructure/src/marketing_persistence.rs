//! Holding-routed SQLx persistence for the Marketing write boundary.
//!
//! Domain values are validated before serialization. Mutations run in a
//! transaction with the routed holding reference installed through a local
//! PostgreSQL setting, so the table policies remain the final holding fence.
//! Audience reads deliberately use the read pool and never become write
//! authority.

use chrono::{DateTime, Utc};
use erpx_domain::contracts::AuthorityScope;
use erpx_domain::marketing::{
    AudienceMembership, ConsentRecord, ConsentState, MarketingChannel, MarketingProfile,
    MarketingProfileErasure, MarketingProjectionReconciliation, MarketingPurpose,
    SuppressionRecord, dedupe_key,
};
use erpx_domain::marketing_crm::{
    AcceptedLeadHandoff, AttributionModel, AttributionResult, CrmOpportunity,
};
use erpx_domain::marketing_journey::{JourneyConversion, JourneyOwnerNotification, SegmentChange};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, PgPool, Postgres, Row, Transaction};
use std::collections::{BTreeSet, HashMap};
use uuid::Uuid;

use crate::PersistenceError;

pub const MAX_MARKETING_PAGE_SIZE: u32 = 500;
pub const MAX_MARKETING_SOURCE_EVENT_PAGE_SIZE: u32 = 500;
pub const MAX_CRM_READ_PAGE_SIZE: u32 = 100;
/// Maximum source-event history one projection invocation may drain.
/// Exactly one million events is supported; a larger history remains an
/// explicit bounded retry/rebuild condition instead of an unbounded worker.
pub const MAX_MARKETING_PROJECTION_EVENTS: u64 = 1_000_000;

const CRM_HANDOFF_AGGREGATE: &str = "marketing_crm_accepted_lead_handoff";
const CRM_OPPORTUNITY_AGGREGATE: &str = "marketing_crm_opportunity";
const CRM_ATTRIBUTION_AGGREGATE: &str = "marketing_crm_attribution";
pub const MARKETING_JOURNEY_CONVERSION_AGGREGATE: &str = "marketing_journey_conversion";
pub const MARKETING_JOURNEY_CONVERSION_EVENT: &str = "marketing.journey_conversion_recorded.v1";
pub const MARKETING_JOURNEY_OWNER_NOTIFICATION_AGGREGATE: &str =
    "marketing_journey_owner_notification";
pub const MARKETING_JOURNEY_OWNER_NOTIFICATION_EVENT: &str =
    "marketing.journey_owner_notification_requested.v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingProjectionApplication {
    pub event_id: Uuid,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingProjectionRebuildPreparation {
    pub data_profile_key: String,
    pub rows_cleared: u64,
    pub checkpoints_cleared: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingProjectionFencedRebuild {
    pub preparation: MarketingProjectionRebuildPreparation,
    pub source_version: u64,
    pub pages_read: u64,
    pub events_replayed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingSourceEventCursor {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarketingSourceEvent {
    pub event_id: Uuid,
    pub event_type: String,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub payload: Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarketingSourceEventPage {
    pub events: Vec<MarketingSourceEvent>,
    pub next_cursor: Option<MarketingSourceEventCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileUpsertReceipt {
    pub profile_id: Uuid,
    pub version: u64,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingMutationReceipt {
    pub resource_id: Uuid,
    pub version: Option<u64>,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudienceMembershipPage {
    pub audience_id: Uuid,
    pub profile_ids: Vec<Uuid>,
    pub materialized_at: DateTime<Utc>,
    pub source_version: u64,
    pub next_cursor: Option<Uuid>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CrmReadRow {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub source_version: u64,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CrmReadPage {
    pub rows: Vec<CrmReadRow>,
    pub has_more: bool,
    pub next_after_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarketingJourneyConversionReadRow {
    pub conversion_ref: Uuid,
    pub journey_id: Uuid,
    pub run_id: Uuid,
    pub profile_id: Uuid,
    pub scope: AuthorityScope,
    pub revenue_minor: Option<i64>,
    pub currency: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub source_version: u64,
    pub source_event_id: Uuid,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarketingJourneyConversionReadPage {
    pub rows: Vec<MarketingJourneyConversionReadRow>,
    pub next_after_ref: Option<Uuid>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarketingJourneyOwnerNotificationReadRow {
    pub notification_ref: Uuid,
    pub journey_id: Uuid,
    pub run_id: Uuid,
    pub profile_id: Uuid,
    pub scope: AuthorityScope,
    pub owner_ref: String,
    pub occurred_at: DateTime<Utc>,
    pub source_version: u64,
    pub source_event_id: Uuid,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarketingJourneyOwnerNotificationReadPage {
    pub rows: Vec<MarketingJourneyOwnerNotificationReadRow>,
    pub next_after_ref: Option<Uuid>,
    pub has_more: bool,
}

/// Holding-scoped read seam joining an immutable `NotifyOwner` request to the
/// generic durable delivery projection. The notification source/read row is
/// still authoritative for the handoff facts; delivery fields are disposable
/// and may be absent until the delivery projector consumes its outbox event.
#[derive(Debug, Clone, PartialEq)]
pub struct MarketingJourneyOwnerNotificationDeliveryReadRow {
    pub notification_ref: Uuid,
    pub journey_id: Uuid,
    pub run_id: Uuid,
    pub profile_id: Uuid,
    pub scope: AuthorityScope,
    pub owner_ref: String,
    pub occurred_at: DateTime<Utc>,
    pub source_version: u64,
    pub source_event_id: Uuid,
    pub payload: Value,
    pub delivery_id: Option<Uuid>,
    pub delivery_state: Option<String>,
    pub delivery_attempt_count: Option<u32>,
    pub delivery_next_attempt_at: Option<DateTime<Utc>>,
    pub delivery_version: Option<u64>,
    pub delivery_payload: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarketingJourneyOwnerNotificationDeliveryReadPage {
    pub rows: Vec<MarketingJourneyOwnerNotificationDeliveryReadRow>,
    pub next_after_ref: Option<Uuid>,
    pub has_more: bool,
}

#[derive(Debug, Clone)]
pub struct SqlxMarketingStore {
    write_pool: PgPool,
    read_pool: PgPool,
    data_profile_key: String,
}

impl SqlxMarketingStore {
    pub fn new(
        write_pool: PgPool,
        read_pool: PgPool,
        data_profile_key: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let data_profile_key = data_profile_key.into();
        validate_data_profile_key(&data_profile_key)?;
        Ok(Self {
            write_pool,
            read_pool,
            data_profile_key,
        })
    }

    pub fn from_pools(
        write_pool: &PgPool,
        read_pool: &PgPool,
        data_profile_key: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        Self::new(write_pool.clone(), read_pool.clone(), data_profile_key)
    }

    pub fn data_profile_key(&self) -> &str {
        &self.data_profile_key
    }

    /// Insert or update a profile under an optimistic aggregate version.
    /// The domain-computed dedupe key is persisted and remains unique within
    /// the routed holding.
    pub async fn upsert_profile(
        &self,
        profile: &MarketingProfile,
        expected_version: Option<u64>,
    ) -> Result<ProfileUpsertReceipt, PersistenceError> {
        validate_profile(profile)?;
        let payload = serde_json::to_value(profile)?;
        let attributes = serde_json::to_value(&profile.attributes)?;
        let payload_hash = hash_value(&payload)?;
        let mut tx = self.begin_write().await?;
        let current_row = sqlx::query(
            "SELECT scope, version, erased_at FROM erpx_holding.marketing_profiles
             WHERE profile_id = $1 AND data_profile_key = $2 FOR UPDATE",
        )
        .bind(profile.id)
        .bind(&self.data_profile_key)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(row) = current_row.as_ref() {
            let stored_scope: AuthorityScope = serde_json::from_value(row.try_get("scope")?)?;
            validate_immutable_scope(&stored_scope, &profile.scope, "marketing profile")?;
            let erased_at: Option<DateTime<Utc>> = row.try_get("erased_at")?;
            if erased_at.is_some() {
                return Err(PersistenceError::InvalidData(
                    "marketing profile has been erased and cannot be updated".into(),
                ));
            }
        }
        let current = current_row
            .map(|row| as_u64(row.try_get("version")?, "marketing profile version"))
            .transpose()?;
        if current != expected_version {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: "marketing_profile".into(),
                aggregate_id: profile.id,
                expected: expected_version,
                actual: current,
            });
        }
        if sqlx::query_scalar::<_, Uuid>(
            "SELECT profile_id FROM erpx_holding.marketing_profiles
             WHERE data_profile_key = $1 AND dedupe_key = $2 AND profile_id <> $3
             FOR UPDATE",
        )
        .bind(&self.data_profile_key)
        .bind(&profile.dedupe_key)
        .bind(profile.id)
        .fetch_optional(&mut *tx)
        .await?
        .is_some()
        {
            return Err(PersistenceError::InvalidData(
                "marketing profile dedupe key is already bound".into(),
            ));
        }
        let version = current.unwrap_or(0).checked_add(1).ok_or_else(|| {
            PersistenceError::InvalidData("marketing profile version overflow".into())
        })?;
        if current.is_some() {
            sqlx::query(
                "UPDATE erpx_holding.marketing_profiles
                 SET scope = $2, external_ref = $3, dedupe_key = $4, email = $5,
                     phone = $6, locale = $7, attributes = $8, payload = $9,
                     payload_hash = $10, version = $11, updated_at = $12,
                     erased_at = NULL, erasure_reason = NULL
                 WHERE profile_id = $1 AND data_profile_key = $13",
            )
            .bind(profile.id)
            .bind(serde_json::to_value(&profile.scope)?)
            .bind(&profile.external_ref)
            .bind(&profile.dedupe_key)
            .bind(&profile.email)
            .bind(&profile.phone)
            .bind(&profile.locale)
            .bind(attributes)
            .bind(&payload)
            .bind(payload_hash)
            .bind(as_i64(version, "marketing profile version")?)
            .bind(profile.updated_at)
            .bind(&self.data_profile_key)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "INSERT INTO erpx_holding.marketing_profiles
                    (profile_id, data_profile_key, scope, external_ref, dedupe_key, email,
                     phone, locale, attributes, payload, payload_hash, version,
                     created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
            )
            .bind(profile.id)
            .bind(&self.data_profile_key)
            .bind(serde_json::to_value(&profile.scope)?)
            .bind(&profile.external_ref)
            .bind(&profile.dedupe_key)
            .bind(&profile.email)
            .bind(&profile.phone)
            .bind(&profile.locale)
            .bind(attributes)
            .bind(&payload)
            .bind(payload_hash)
            .bind(as_i64(version, "marketing profile version")?)
            .bind(profile.created_at)
            .bind(profile.updated_at)
            .execute(&mut *tx)
            .await?;
        }
        write_marketing_source_event(
            &mut tx,
            &self.data_profile_key,
            "marketing_profile",
            profile.id,
            version,
            &payload,
            "marketing.profile_saved.v1",
        )
        .await?;
        tx.commit().await?;
        Ok(ProfileUpsertReceipt {
            profile_id: profile.id,
            version,
            replayed: false,
        })
    }

    /// Erase contact and profile attributes while retaining the profile ID,
    /// scope, audit facts, and a permanent suppression fact. The erasure
    /// event contains only the typed tombstone and cannot rehydrate the old
    /// profile snapshot during a read-model rebuild.
    pub async fn erase_profile(
        &self,
        erasure: &MarketingProfileErasure,
        idempotency_key: &str,
    ) -> Result<MarketingMutationReceipt, PersistenceError> {
        erasure
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        validate_idempotency_key(idempotency_key)?;
        let payload = serde_json::to_value(erasure)?;
        let request_hash = hash_operation("erase_profile", &payload)?;
        let mut tx = self.begin_write().await?;
        if let Some(receipt) = reserve_idempotency(
            &mut tx,
            &self.data_profile_key,
            "erase_profile",
            idempotency_key,
            request_hash,
            erasure.profile_id,
        )
        .await?
        {
            tx.commit().await?;
            return Ok(receipt);
        }
        let row = sqlx::query(
            "SELECT scope, version, erased_at
               FROM erpx_holding.marketing_profiles
              WHERE profile_id=$1 AND data_profile_key=$2 FOR UPDATE",
        )
        .bind(erasure.profile_id)
        .bind(&self.data_profile_key)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "marketing profile",
            id: erasure.profile_id,
        })?;
        let stored_scope: AuthorityScope = serde_json::from_value(row.try_get("scope")?)?;
        if stored_scope != erasure.scope {
            return Err(PersistenceError::InvalidData(
                "marketing profile erasure scope does not match source profile".into(),
            ));
        }
        if row
            .try_get::<Option<DateTime<Utc>>, _>("erased_at")?
            .is_some()
        {
            return Err(PersistenceError::InvalidData(
                "marketing profile is already erased".into(),
            ));
        }
        let current_version = as_u64(row.try_get("version")?, "marketing profile version")?;
        let version = current_version.checked_add(1).ok_or_else(|| {
            PersistenceError::InvalidData("marketing profile version overflow".into())
        })?;
        let redacted_external_ref = format!("erased:{}", erasure.profile_id);
        let redacted_dedupe_key = hex::encode(Sha256::digest(redacted_external_ref.as_bytes()));
        let redacted_payload = serde_json::to_value(erasure.redacted_projection())?;
        let redacted_payload_hash = hash_value(&redacted_payload)?;
        sqlx::query(
            "UPDATE erpx_holding.marketing_profiles
                SET external_ref=$3, dedupe_key=$4, email=NULL, phone=NULL,
                    locale='und', attributes='{}'::jsonb, payload=$5,
                    payload_hash=$6, version=$7, updated_at=$8,
                    erased_at=$8, erasure_reason=$9
              WHERE profile_id=$1 AND data_profile_key=$2",
        )
        .bind(erasure.profile_id)
        .bind(&self.data_profile_key)
        .bind(&redacted_external_ref)
        .bind(&redacted_dedupe_key)
        .bind(&redacted_payload)
        .bind(&redacted_payload_hash)
        .bind(as_i64(version, "marketing profile erasure version")?)
        .bind(erasure.erased_at)
        .bind(&erasure.reason)
        .execute(&mut *tx)
        .await?;
        write_marketing_source_event(
            &mut tx,
            &self.data_profile_key,
            "marketing_profile",
            erasure.profile_id,
            version,
            &payload,
            "marketing.profile_erased.v1",
        )
        .await?;

        let suppression = SuppressionRecord {
            id: Uuid::new_v4(),
            profile_id: erasure.profile_id,
            scope: erasure.scope.clone(),
            reason: "profile_erasure".into(),
            source: "marketing_privacy".into(),
            created_at: erasure.erased_at,
            expires_at: None,
        };
        let suppression_payload = serde_json::to_value(&suppression)?;
        sqlx::query(
            "INSERT INTO erpx_holding.marketing_suppressions
                (suppression_id, profile_id, data_profile_key, scope, reason, source,
                 created_at, expires_at, payload, payload_hash, version)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,1)",
        )
        .bind(suppression.id)
        .bind(suppression.profile_id)
        .bind(&self.data_profile_key)
        .bind(serde_json::to_value(&suppression.scope)?)
        .bind(&suppression.reason)
        .bind(&suppression.source)
        .bind(suppression.created_at)
        .bind(suppression.expires_at)
        .bind(&suppression_payload)
        .bind(hash_value(&suppression_payload)?)
        .execute(&mut *tx)
        .await?;
        write_marketing_source_event(
            &mut tx,
            &self.data_profile_key,
            "marketing_suppression",
            suppression.id,
            1,
            &suppression_payload,
            "marketing.suppression_saved.v1",
        )
        .await?;
        // Existing profile outbox payloads are privacy-scrubbed in the same
        // transaction. The event identity and audit metadata remain, while
        // every replayable payload becomes the contact-free tombstone.
        sqlx::query(
            "UPDATE erpx_holding.outbox_events
                SET payload=jsonb_build_object(
                    'aggregate_type', aggregate_type,
                    'aggregate', $3::jsonb,
                    'version', aggregate_version,
                    'event_type', event_type,
                    'privacy_redacted', true)
              WHERE data_profile_key=$1 AND aggregate_type='marketing_profile'
                AND aggregate_id=$2",
        )
        .bind(&self.data_profile_key)
        .bind(erasure.profile_id)
        .bind(&payload)
        .execute(&mut *tx)
        .await?;
        let receipt = MarketingMutationReceipt {
            resource_id: erasure.profile_id,
            version: Some(version),
            replayed: false,
        };
        complete_idempotency(&mut tx, &self.data_profile_key, idempotency_key, &receipt).await?;
        tx.commit().await?;
        Ok(receipt)
    }

    pub async fn append_consent(
        &self,
        consent: &ConsentRecord,
        idempotency_key: &str,
    ) -> Result<MarketingMutationReceipt, PersistenceError> {
        self.persist_consent("append_consent", consent, idempotency_key)
            .await
    }

    pub async fn revoke_consent(
        &self,
        consent: &ConsentRecord,
        idempotency_key: &str,
    ) -> Result<MarketingMutationReceipt, PersistenceError> {
        if consent.state != ConsentState::Revoked {
            return Err(PersistenceError::InvalidData(
                "consent revocation must use revoked state".into(),
            ));
        }
        self.persist_consent("revoke_consent", consent, idempotency_key)
            .await
    }

    async fn persist_consent(
        &self,
        operation: &'static str,
        consent: &ConsentRecord,
        idempotency_key: &str,
    ) -> Result<MarketingMutationReceipt, PersistenceError> {
        validate_consent(consent)?;
        validate_idempotency_key(idempotency_key)?;
        let payload = serde_json::to_value(consent)?;
        let request_hash = hash_operation(operation, &payload)?;
        let mut tx = self.begin_write().await?;
        ensure_profile_scope(
            &mut tx,
            &self.data_profile_key,
            consent.profile_id,
            &consent.scope,
        )
        .await?;
        if let Some(receipt) = reserve_idempotency(
            &mut tx,
            &self.data_profile_key,
            operation,
            idempotency_key,
            request_hash,
            consent.id,
        )
        .await?
        {
            tx.commit().await?;
            return Ok(receipt);
        }
        if sqlx::query_scalar::<_, Uuid>(
            "SELECT consent_id FROM erpx_holding.marketing_consents
             WHERE consent_id = $1 AND data_profile_key = $2",
        )
        .bind(consent.id)
        .bind(&self.data_profile_key)
        .fetch_optional(&mut *tx)
        .await?
        .is_some()
        {
            return Err(PersistenceError::InvalidData(
                "marketing consent ID already exists".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO erpx_holding.marketing_consents
                (consent_id, profile_id, data_profile_key, scope, purpose, channel, state,
                 source, jurisdiction, policy_version, proof_ref, recorded_at,
                 expires_at, payload)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        )
        .bind(consent.id)
        .bind(consent.profile_id)
        .bind(&self.data_profile_key)
        .bind(serde_json::to_value(&consent.scope)?)
        .bind(purpose_text(consent.purpose))
        .bind(channel_text(consent.channel))
        .bind(consent_state_text(consent.state))
        .bind(&consent.source)
        .bind(&consent.jurisdiction)
        .bind(&consent.policy_version)
        .bind(&consent.proof_ref)
        .bind(consent.recorded_at)
        .bind(consent.expires_at)
        .bind(&payload)
        .execute(&mut *tx)
        .await?;
        let receipt = MarketingMutationReceipt {
            resource_id: consent.id,
            version: None,
            replayed: false,
        };
        complete_idempotency(&mut tx, &self.data_profile_key, idempotency_key, &receipt).await?;
        write_marketing_source_event(
            &mut tx,
            &self.data_profile_key,
            "marketing_consent",
            consent.id,
            1,
            &payload,
            "marketing.consent_saved.v1",
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    pub async fn upsert_suppression(
        &self,
        suppression: &SuppressionRecord,
        idempotency_key: &str,
    ) -> Result<MarketingMutationReceipt, PersistenceError> {
        validate_suppression(suppression)?;
        validate_idempotency_key(idempotency_key)?;
        let payload = serde_json::to_value(suppression)?;
        let payload_hash = hash_value(&payload)?;
        let request_hash = hash_operation("upsert_suppression", &payload)?;
        let mut tx = self.begin_write().await?;
        ensure_profile_scope(
            &mut tx,
            &self.data_profile_key,
            suppression.profile_id,
            &suppression.scope,
        )
        .await?;
        if let Some(receipt) = reserve_idempotency(
            &mut tx,
            &self.data_profile_key,
            "upsert_suppression",
            idempotency_key,
            request_hash,
            suppression.id,
        )
        .await?
        {
            tx.commit().await?;
            return Ok(receipt);
        }
        let current = sqlx::query(
            "SELECT profile_id, data_profile_key, scope, version
             FROM erpx_holding.marketing_suppressions
             WHERE suppression_id = $1 FOR UPDATE",
        )
        .bind(suppression.id)
        .fetch_optional(&mut *tx)
        .await?;
        let current_version = if let Some(row) = current {
            let profile_id: Uuid = row.try_get("profile_id")?;
            let stored_data_profile_key: String = row.try_get("data_profile_key")?;
            if profile_id != suppression.profile_id
                || stored_data_profile_key != self.data_profile_key
            {
                return Err(PersistenceError::InvalidData(
                    "marketing suppression identity cannot be rebound".into(),
                ));
            }
            let stored_scope: AuthorityScope = serde_json::from_value(row.try_get("scope")?)?;
            validate_immutable_scope(&stored_scope, &suppression.scope, "marketing suppression")?;
            as_u64(row.try_get("version")?, "marketing suppression version")?
        } else {
            0
        };
        let version = current_version.checked_add(1).ok_or_else(|| {
            PersistenceError::InvalidData("marketing suppression version overflow".into())
        })?;
        if current_version == 0 {
            sqlx::query(
                "INSERT INTO erpx_holding.marketing_suppressions
                    (suppression_id, profile_id, data_profile_key, scope, reason, source,
                     created_at, expires_at, payload, payload_hash, version)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
            )
            .bind(suppression.id)
            .bind(suppression.profile_id)
            .bind(&self.data_profile_key)
            .bind(serde_json::to_value(&suppression.scope)?)
            .bind(&suppression.reason)
            .bind(&suppression.source)
            .bind(suppression.created_at)
            .bind(suppression.expires_at)
            .bind(&payload)
            .bind(payload_hash)
            .bind(as_i64(version, "marketing suppression version")?)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "UPDATE erpx_holding.marketing_suppressions
                 SET scope = $2, reason = $3, source = $4, created_at = $5,
                     expires_at = $6, payload = $7, payload_hash = $8,
                     version = $9, updated_at = now()
                 WHERE suppression_id = $1 AND data_profile_key = $10",
            )
            .bind(suppression.id)
            .bind(serde_json::to_value(&suppression.scope)?)
            .bind(&suppression.reason)
            .bind(&suppression.source)
            .bind(suppression.created_at)
            .bind(suppression.expires_at)
            .bind(&payload)
            .bind(payload_hash)
            .bind(as_i64(version, "marketing suppression version")?)
            .bind(&self.data_profile_key)
            .execute(&mut *tx)
            .await?;
        }
        let receipt = MarketingMutationReceipt {
            resource_id: suppression.id,
            version: Some(version),
            replayed: false,
        };
        complete_idempotency(&mut tx, &self.data_profile_key, idempotency_key, &receipt).await?;
        write_marketing_source_event(
            &mut tx,
            &self.data_profile_key,
            "marketing_suppression",
            suppression.id,
            version,
            &payload,
            "marketing.suppression_saved.v1",
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    pub async fn upsert_audience_membership(
        &self,
        membership: &AudienceMembership,
        idempotency_key: &str,
    ) -> Result<MarketingMutationReceipt, PersistenceError> {
        validate_audience_membership(membership)?;
        validate_idempotency_key(idempotency_key)?;
        let payload = serde_json::to_value(membership)?;
        let membership_hash = hash_value(&payload)?;
        let request_hash = hash_operation("upsert_audience_membership", &payload)?;
        let mut tx = self.begin_write().await?;
        if let Some(receipt) = reserve_idempotency(
            &mut tx,
            &self.data_profile_key,
            "upsert_audience_membership",
            idempotency_key,
            request_hash,
            membership.audience_id,
        )
        .await?
        {
            tx.commit().await?;
            return Ok(receipt);
        }
        let profile_rows = sqlx::query(
            "SELECT profile_id, scope
             FROM erpx_holding.marketing_profiles
             WHERE data_profile_key = $1 AND profile_id = ANY($2) AND erased_at IS NULL
             FOR SHARE",
        )
        .bind(&self.data_profile_key)
        .bind(&membership.profile_ids)
        .fetch_all(&mut *tx)
        .await?;
        if profile_rows.len() != membership.profile_ids.len() {
            return Err(PersistenceError::InvalidData(
                "audience membership references an unknown or erased profile".into(),
            ));
        }
        for row in profile_rows {
            let profile_scope: AuthorityScope = serde_json::from_value(row.try_get("scope")?)?;
            if !membership.scope.contains(&profile_scope) {
                return Err(PersistenceError::InvalidData(
                    "audience membership profile is outside the audience scope".into(),
                ));
            }
        }
        let current = sqlx::query(
            "SELECT scope, version FROM erpx_holding.marketing_audiences
             WHERE audience_id = $1 AND data_profile_key = $2 FOR UPDATE",
        )
        .bind(membership.audience_id)
        .bind(&self.data_profile_key)
        .fetch_optional(&mut *tx)
        .await?;
        let current_version = if let Some(row) = current {
            let stored_scope: AuthorityScope = serde_json::from_value(row.try_get("scope")?)?;
            validate_immutable_scope(&stored_scope, &membership.scope, "marketing audience")?;
            as_u64(row.try_get("version")?, "marketing audience version")?
        } else {
            0
        };
        let version = current_version.checked_add(1).ok_or_else(|| {
            PersistenceError::InvalidData("marketing audience version overflow".into())
        })?;
        sqlx::query(
            "DELETE FROM erpx_holding.marketing_audience_memberships
             WHERE audience_id = $1 AND data_profile_key = $2",
        )
        .bind(membership.audience_id)
        .bind(&self.data_profile_key)
        .execute(&mut *tx)
        .await?;
        if current_version == 0 {
            sqlx::query(
                "INSERT INTO erpx_holding.marketing_audiences
                    (audience_id, data_profile_key, scope, materialized_at, profile_count,
                     membership_hash, version)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(membership.audience_id)
            .bind(&self.data_profile_key)
            .bind(serde_json::to_value(&membership.scope)?)
            .bind(membership.materialized_at)
            .bind(i64::try_from(membership.profile_ids.len()).map_err(|_| {
                PersistenceError::InvalidData("audience membership count exceeds BIGINT".into())
            })?)
            .bind(&membership_hash)
            .bind(as_i64(version, "marketing audience version")?)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "UPDATE erpx_holding.marketing_audiences
                 SET scope = $2, materialized_at = $3, profile_count = $4,
                     membership_hash = $5, version = $6, updated_at = now()
                 WHERE audience_id = $1 AND data_profile_key = $7",
            )
            .bind(membership.audience_id)
            .bind(serde_json::to_value(&membership.scope)?)
            .bind(membership.materialized_at)
            .bind(i64::try_from(membership.profile_ids.len()).map_err(|_| {
                PersistenceError::InvalidData("audience membership count exceeds BIGINT".into())
            })?)
            .bind(&membership_hash)
            .bind(as_i64(version, "marketing audience version")?)
            .bind(&self.data_profile_key)
            .execute(&mut *tx)
            .await?;
        }
        for profile_id in &membership.profile_ids {
            sqlx::query(
                "INSERT INTO erpx_holding.marketing_audience_memberships
                    (audience_id, profile_id, data_profile_key, membership_version, materialized_at)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(membership.audience_id)
            .bind(profile_id)
            .bind(&self.data_profile_key)
            .bind(as_i64(version, "marketing audience version")?)
            .bind(membership.materialized_at)
            .execute(&mut *tx)
            .await?;
        }
        let receipt = MarketingMutationReceipt {
            resource_id: membership.audience_id,
            version: Some(version),
            replayed: false,
        };
        complete_idempotency(&mut tx, &self.data_profile_key, idempotency_key, &receipt).await?;
        write_marketing_source_event(
            &mut tx,
            &self.data_profile_key,
            "marketing_audience",
            membership.audience_id,
            version,
            &payload,
            "marketing.audience_saved.v1",
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    /// Read only from the holding read database. The cursor is a UUID keyset,
    /// so callers cannot request an unbounded audience page.
    pub async fn list_audience_membership(
        &self,
        audience_id: Uuid,
        after_profile_id: Option<Uuid>,
        limit: u32,
    ) -> Result<AudienceMembershipPage, PersistenceError> {
        validate_uuid(audience_id, "audience_id")?;
        if limit == 0 || limit > MAX_MARKETING_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(format!(
                "marketing page size must be between 1 and {MAX_MARKETING_PAGE_SIZE}"
            )));
        }
        let mut tx = self.begin_read().await?;
        let audience = sqlx::query(
            "SELECT materialized_at, source_version
             FROM erpx_holding.marketing_audiences
             WHERE audience_id = $1 AND data_profile_key = $2",
        )
        .bind(audience_id)
        .bind(&self.data_profile_key)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "marketing audience",
            id: audience_id,
        })?;
        let materialized_at: DateTime<Utc> = audience.try_get("materialized_at")?;
        let source_version = as_u64(
            audience.try_get("source_version")?,
            "marketing audience source version",
        )?;
        let fetch_limit = i64::from(limit) + 1;
        let rows = sqlx::query(
            "SELECT profile_id FROM erpx_holding.marketing_audience_memberships
             WHERE audience_id = $1 AND data_profile_key = $2
               AND ($3::uuid IS NULL OR profile_id > $3)
             ORDER BY profile_id LIMIT $4",
        )
        .bind(audience_id)
        .bind(&self.data_profile_key)
        .bind(after_profile_id)
        .bind(fetch_limit)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        let mut profile_ids = rows
            .into_iter()
            .map(|row| row.try_get::<Uuid, _>("profile_id"))
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        let has_more = profile_ids.len() > usize::try_from(limit).unwrap_or(usize::MAX);
        if has_more {
            profile_ids.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
        }
        let next_cursor = if has_more {
            profile_ids.last().copied()
        } else {
            None
        };
        Ok(AudienceMembershipPage {
            audience_id,
            profile_ids,
            materialized_at,
            source_version,
            next_cursor,
            has_more,
        })
    }

    pub async fn read_source_event_page(
        &self,
        cursor: Option<&MarketingSourceEventCursor>,
        page_size: u32,
    ) -> Result<MarketingSourceEventPage, PersistenceError> {
        if page_size == 0 || page_size > MAX_MARKETING_SOURCE_EVENT_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(format!(
                "Marketing source event page size must be 1..={MAX_MARKETING_SOURCE_EVENT_PAGE_SIZE}"
            )));
        }
        let mut tx = self.begin_write().await?;
        let page = read_source_event_page_in_transaction(
            &mut tx,
            &self.data_profile_key,
            cursor,
            page_size,
        )
        .await?;
        tx.commit().await?;
        Ok(page)
    }

    pub async fn source_event_high_watermark(&self) -> Result<u64, PersistenceError> {
        let mut tx = self.begin_write().await?;
        let count =
            source_event_high_watermark_in_transaction(&mut tx, &self.data_profile_key).await?;
        tx.commit().await?;
        Ok(count)
    }

    pub async fn rebuild_projection_with_source_fence(
        &self,
        expected_source_version: u64,
        page_size: u32,
    ) -> Result<MarketingProjectionFencedRebuild, PersistenceError> {
        if expected_source_version == 0 {
            return Err(PersistenceError::InvalidData(
                "Marketing source fence must be positive".into(),
            ));
        }
        if page_size == 0 || page_size > MAX_MARKETING_SOURCE_EVENT_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(format!(
                "Marketing source event page size must be 1..={MAX_MARKETING_SOURCE_EVENT_PAGE_SIZE}"
            )));
        }

        let mut source_tx = self.begin_write().await?;
        sqlx::query("LOCK TABLE erpx_holding.outbox_events IN SHARE MODE")
            .execute(&mut *source_tx)
            .await?;
        let actual_source_version =
            source_event_high_watermark_in_transaction(&mut source_tx, &self.data_profile_key)
                .await?;
        if actual_source_version != expected_source_version {
            return Err(PersistenceError::InvalidData(
                "Marketing source fence changed before replacement rebuild".into(),
            ));
        }
        if actual_source_version > MAX_MARKETING_PROJECTION_EVENTS {
            return Err(PersistenceError::InvalidData(
                "Marketing projection source event bound exceeded".into(),
            ));
        }

        let mut read_tx = self.begin_read().await?;
        let preparation =
            clear_projection_in_transaction(&mut read_tx, &self.data_profile_key).await?;
        let mut cursor = None;
        let mut pages_read = 0_u64;
        let mut events_replayed = 0_u64;
        loop {
            let page = read_source_event_page_in_transaction(
                &mut source_tx,
                &self.data_profile_key,
                cursor.as_ref(),
                page_size,
            )
            .await?;
            pages_read = pages_read.saturating_add(1);
            for event in &page.events {
                let application = self
                    .project_event_in_transaction(
                        &mut read_tx,
                        event.event_id,
                        &event.event_type,
                        &event.aggregate_type,
                        event.aggregate_id,
                        event.aggregate_version,
                        &event.payload,
                        false,
                    )
                    .await?;
                if application.applied {
                    events_replayed = events_replayed.saturating_add(1);
                }
            }
            if !page.has_more {
                break;
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Err(PersistenceError::InvalidData(
                    "Marketing source event cursor is missing".into(),
                ));
            }
        }
        let final_source_version =
            source_event_high_watermark_in_transaction(&mut source_tx, &self.data_profile_key)
                .await?;
        if final_source_version != expected_source_version {
            return Err(PersistenceError::InvalidData(
                "Marketing source fence changed during replacement rebuild".into(),
            ));
        }
        read_tx.commit().await?;
        source_tx.commit().await?;
        Ok(MarketingProjectionFencedRebuild {
            preparation,
            source_version: final_source_version,
            pages_read,
            events_replayed,
        })
    }

    /// Compare the authoritative Marketing/CRM outbox with every disposable
    /// read aggregate. This is a read-only holding-scoped check; no command
    /// path may use it as authority or repair source data from its result.
    pub async fn reconcile_projection(
        &self,
    ) -> Result<MarketingProjectionReconciliation, PersistenceError> {
        let mut source_tx = self.begin_write().await?;
        let source_rows = sqlx::query(
            "SELECT aggregate_type, aggregate_id, aggregate_version, event_type, payload
             FROM erpx_holding.outbox_events
             WHERE data_profile_key=$1
               AND aggregate_type IN
                   ('marketing_profile', 'marketing_consent',
                    'marketing_suppression', 'marketing_audience',
                    'marketing_crm_accepted_lead_handoff',
                    'marketing_crm_opportunity', 'marketing_crm_attribution',
                    'marketing_journey_conversion',
                    'marketing_journey_owner_notification')
             ORDER BY aggregate_type, aggregate_id, aggregate_version, event_id",
        )
        .bind(&self.data_profile_key)
        .fetch_all(&mut *source_tx)
        .await?;
        source_tx.commit().await?;

        let mut source: HashMap<(String, Uuid), u64> = HashMap::with_capacity(source_rows.len());
        let mut source_payloads: HashMap<(String, Uuid), (u64, Value)> =
            HashMap::with_capacity(source_rows.len());
        let mut source_event_count = 0_u64;
        for row in source_rows {
            let aggregate_type: String = row.try_get("aggregate_type")?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            let aggregate_version = as_u64(
                row.try_get("aggregate_version")?,
                "Marketing source version",
            )?;
            let event_type: String = row.try_get("event_type")?;
            let payload: Value = row.try_get("payload")?;
            validate_marketing_source_event(
                &aggregate_type,
                aggregate_id,
                aggregate_version,
                &event_type,
                &payload,
            )?;
            source_event_count = source_event_count.checked_add(1).ok_or_else(|| {
                PersistenceError::InvalidData("Marketing event count overflow".into())
            })?;
            let key = (aggregate_type.clone(), aggregate_id);
            if let Some(previous_version) = source.get(&key).copied() {
                if aggregate_version != previous_version.saturating_add(1) {
                    return Err(PersistenceError::InvalidData(
                        "Marketing source event versions are not contiguous".into(),
                    ));
                }
            } else if aggregate_version != 1 {
                return Err(PersistenceError::InvalidData(
                    "Marketing source event stream must start at version one".into(),
                ));
            }
            if matches!(
                aggregate_type.as_str(),
                CRM_HANDOFF_AGGREGATE
                    | CRM_OPPORTUNITY_AGGREGATE
                    | CRM_ATTRIBUTION_AGGREGATE
                    | MARKETING_JOURNEY_CONVERSION_AGGREGATE
                    | MARKETING_JOURNEY_OWNER_NOTIFICATION_AGGREGATE
            ) {
                let aggregate = payload.get("aggregate").cloned().ok_or_else(|| {
                    PersistenceError::InvalidData(
                        "Marketing CRM source aggregate is missing".into(),
                    )
                })?;
                source_payloads.insert(key.clone(), (aggregate_version, aggregate));
            }
            source.insert(key, aggregate_version);
        }

        let mut read_tx = self.begin_read().await?;
        let read_rows = sqlx::query(
            "SELECT aggregate_type, aggregate_id, source_version, NULL::jsonb AS payload
             FROM erpx_holding.marketing_projection_checkpoints
             WHERE data_profile_key=$1
             UNION ALL
             SELECT 'marketing_crm_accepted_lead_handoff', handoff_id, source_version, payload
             FROM erpx_holding.crm_accepted_lead_handoff_read_rows
             WHERE data_profile_key=$1
             UNION ALL
             SELECT 'marketing_crm_opportunity', opportunity_id, source_version, payload
             FROM erpx_holding.crm_opportunity_read_rows
             WHERE data_profile_key=$1
             UNION ALL
             SELECT 'marketing_crm_attribution', record_id, source_version, payload
             FROM erpx_holding.crm_attribution_read_rows
             WHERE data_profile_key=$1
             UNION ALL
             SELECT 'marketing_journey_conversion', conversion_ref, source_version, payload
             FROM erpx_holding.marketing_journey_conversion_read_rows
             WHERE data_profile_key=$1
             UNION ALL
             SELECT 'marketing_journey_owner_notification', notification_ref, source_version, payload
             FROM erpx_holding.marketing_journey_owner_notification_read_rows
             WHERE data_profile_key=$1",
        )
        .bind(&self.data_profile_key)
        .fetch_all(&mut *read_tx)
        .await?;
        read_tx.commit().await?;
        let mut projected = HashMap::with_capacity(read_rows.len());
        let mut projected_payloads = HashMap::with_capacity(read_rows.len());
        for row in read_rows {
            let aggregate_type: String = row.try_get("aggregate_type")?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            let key = (aggregate_type.clone(), aggregate_id);
            let version = as_u64(
                row.try_get("source_version")?,
                "Marketing projected version",
            )?;
            if matches!(
                aggregate_type.as_str(),
                CRM_HANDOFF_AGGREGATE
                    | CRM_OPPORTUNITY_AGGREGATE
                    | CRM_ATTRIBUTION_AGGREGATE
                    | MARKETING_JOURNEY_CONVERSION_AGGREGATE
                    | MARKETING_JOURNEY_OWNER_NOTIFICATION_AGGREGATE
            ) {
                let payload: Option<Value> = row.try_get("payload")?;
                projected_payloads.insert(key.clone(), (version, payload));
            }
            projected.insert(key, version);
        }

        let missing_aggregate_count = source
            .keys()
            .filter(|key| !projected.contains_key(*key))
            .count() as u64;
        let stale_aggregate_count = source
            .iter()
            .filter(|(key, version)| {
                projected
                    .get(*key)
                    .is_some_and(|current| current != *version)
            })
            .count() as u64;
        let orphan_aggregate_count = projected
            .keys()
            .filter(|key| !source.contains_key(*key))
            .count() as u64;
        let payload_mismatch_count = source_payloads
            .iter()
            .filter(|(key, (source_version, source_payload))| {
                projected_payloads.get(*key).is_some_and(
                    |(projected_version, projected_payload)| {
                        projected_version == source_version
                            && projected_payload.as_ref() != Some(source_payload)
                    },
                )
            })
            .count() as u64;
        Ok(MarketingProjectionReconciliation {
            source_event_count,
            source_aggregate_count: source.len() as u64,
            projected_aggregate_count: projected.len() as u64,
            missing_aggregate_count,
            stale_aggregate_count,
            orphan_aggregate_count,
            payload_mismatch_count,
        })
    }

    /// Read a bounded CRM projection page under the caller's explicit
    /// holding/company/branch scope. Attribution rows join their projected
    /// opportunity so attribution never becomes an unscoped read surface.
    pub async fn read_crm_page(
        &self,
        aggregate_type: &str,
        scope: &AuthorityScope,
        after_id: Option<Uuid>,
        page_size: u32,
    ) -> Result<CrmReadPage, PersistenceError> {
        if !matches!(
            aggregate_type,
            CRM_HANDOFF_AGGREGATE | CRM_OPPORTUNITY_AGGREGATE | CRM_ATTRIBUTION_AGGREGATE
        ) {
            return Err(PersistenceError::InvalidData(
                "unsupported CRM read aggregate type".into(),
            ));
        }
        if page_size == 0 || page_size > MAX_CRM_READ_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(
                "CRM read page size is outside its bound".into(),
            ));
        }
        let (company_id, branch_id) = match scope {
            AuthorityScope::Holding { holding_id } => {
                validate_uuid(*holding_id, "CRM holding scope")?;
                (None, None)
            }
            AuthorityScope::Company {
                holding_id,
                company_id,
            } => {
                validate_uuid(*holding_id, "CRM company holding scope")?;
                validate_uuid(*company_id, "CRM company scope")?;
                (Some(*company_id), None)
            }
            AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => {
                validate_uuid(*holding_id, "CRM branch holding scope")?;
                validate_uuid(*company_id, "CRM branch company scope")?;
                validate_uuid(*branch_id, "CRM branch scope")?;
                (Some(*company_id), Some(*branch_id))
            }
            AuthorityScope::Platform | AuthorityScope::Subtree { .. } => {
                return Err(PersistenceError::InvalidData(
                    "CRM reads require an explicit holding, company, or branch scope".into(),
                ));
            }
        };
        let limit = i64::from(page_size) + 1;
        let query = match aggregate_type {
            CRM_HANDOFF_AGGREGATE => {
                "SELECT handoff_id AS aggregate_id, holding_id, company_id, branch_id,
                        source_version, payload
                 FROM erpx_holding.crm_accepted_lead_handoff_read_rows
                 WHERE data_profile_key=$1
                   AND ($2::uuid IS NULL OR company_id=$2)
                   AND ($3::uuid IS NULL OR branch_id=$3)
                   AND ($4::uuid IS NULL OR handoff_id>$4)
                 ORDER BY handoff_id
                 LIMIT $5"
            }
            CRM_OPPORTUNITY_AGGREGATE => {
                "SELECT opportunity_id AS aggregate_id, holding_id, company_id, branch_id,
                        source_version, payload
                 FROM erpx_holding.crm_opportunity_read_rows
                 WHERE data_profile_key=$1
                   AND ($2::uuid IS NULL OR company_id=$2)
                   AND ($3::uuid IS NULL OR branch_id=$3)
                   AND ($4::uuid IS NULL OR opportunity_id>$4)
                 ORDER BY opportunity_id
                 LIMIT $5"
            }
            CRM_ATTRIBUTION_AGGREGATE => {
                "SELECT a.record_id AS aggregate_id, o.holding_id, o.company_id, o.branch_id,
                        a.source_version, a.payload
                 FROM erpx_holding.crm_attribution_read_rows a
                 INNER JOIN erpx_holding.crm_opportunity_read_rows o
                   ON o.data_profile_key=a.data_profile_key AND o.opportunity_id=a.opportunity_id
                 WHERE a.data_profile_key=$1
                   AND ($2::uuid IS NULL OR o.company_id=$2)
                   AND ($3::uuid IS NULL OR o.branch_id=$3)
                   AND ($4::uuid IS NULL OR a.record_id>$4)
                 ORDER BY a.record_id
                 LIMIT $5"
            }
            _ => unreachable!("CRM read aggregate type was validated"),
        };
        let mut tx = self.begin_read().await?;
        let rows = sqlx::query(query)
            .bind(&self.data_profile_key)
            .bind(company_id)
            .bind(branch_id)
            .bind(after_id)
            .bind(limit)
            .fetch_all(&mut *tx)
            .await?;
        tx.commit().await?;
        let page_len = usize::try_from(page_size).map_err(|_| {
            PersistenceError::InvalidData("CRM read page size is not representable".into())
        })?;
        let has_more = rows.len() > page_len;
        let rows = rows
            .into_iter()
            .take(page_len)
            .map(|row| {
                Ok(CrmReadRow {
                    aggregate_type: aggregate_type.into(),
                    aggregate_id: row.try_get("aggregate_id")?,
                    holding_id: row.try_get("holding_id")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    source_version: as_u64(
                        row.try_get("source_version")?,
                        "CRM read source version",
                    )?,
                    payload: row.try_get("payload")?,
                })
            })
            .collect::<Result<Vec<_>, PersistenceError>>()?;
        let next_after_id = has_more
            .then(|| rows.last())
            .flatten()
            .map(|row| row.aggregate_id);
        Ok(CrmReadPage {
            rows,
            has_more,
            next_after_id,
        })
    }

    /// Read immutable journey conversions from the disposable projection.
    /// The caller's holding scope is checked before the read transaction is
    /// opened; narrower scope filtering is applied to the typed payloads.
    pub async fn read_journey_conversion_page(
        &self,
        scope: &AuthorityScope,
        after_ref: Option<Uuid>,
        page_size: u32,
    ) -> Result<MarketingJourneyConversionReadPage, PersistenceError> {
        validate_holding_scope(scope)?;
        let holding_id = match scope {
            AuthorityScope::Holding { holding_id }
            | AuthorityScope::Company { holding_id, .. }
            | AuthorityScope::Branch { holding_id, .. }
            | AuthorityScope::Subtree { holding_id, .. } => *holding_id,
            AuthorityScope::Platform => {
                return Err(PersistenceError::InvalidData(
                    "Marketing conversion reads cannot use platform scope".into(),
                ));
            }
        };
        if page_size == 0 || page_size > MAX_MARKETING_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(
                "Marketing conversion page size is outside its bound".into(),
            ));
        }
        let mut tx = self.begin_read().await?;
        let rows = sqlx::query(
            "SELECT conversion_ref, journey_id, run_id, profile_id, scope,
                    revenue_minor, currency, occurred_at, source_version,
                    source_event_id, payload
               FROM erpx_holding.marketing_journey_conversion_read_rows
              WHERE data_profile_key=$1 AND ($2::uuid IS NULL OR conversion_ref>$2)
              ORDER BY conversion_ref
              LIMIT $3",
        )
        .bind(&self.data_profile_key)
        .bind(after_ref)
        .bind(i64::from(page_size) + 1)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        let mut visible = Vec::with_capacity(rows.len());
        for row in rows {
            let row_scope: AuthorityScope = serde_json::from_value(row.try_get("scope")?)?;
            if row_scope.contains(scope) || scope.contains(&row_scope) {
                let row_holding = match &row_scope {
                    AuthorityScope::Holding { holding_id }
                    | AuthorityScope::Company { holding_id, .. }
                    | AuthorityScope::Branch { holding_id, .. }
                    | AuthorityScope::Subtree { holding_id, .. } => *holding_id,
                    AuthorityScope::Platform => Uuid::nil(),
                };
                if row_holding != holding_id {
                    continue;
                }
                visible.push(MarketingJourneyConversionReadRow {
                    conversion_ref: row.try_get("conversion_ref")?,
                    journey_id: row.try_get("journey_id")?,
                    run_id: row.try_get("run_id")?,
                    profile_id: row.try_get("profile_id")?,
                    scope: row_scope,
                    revenue_minor: row.try_get("revenue_minor")?,
                    currency: row.try_get("currency")?,
                    occurred_at: row.try_get("occurred_at")?,
                    source_version: as_u64(
                        row.try_get("source_version")?,
                        "Marketing conversion source version",
                    )?,
                    source_event_id: row.try_get("source_event_id")?,
                    payload: row.try_get("payload")?,
                });
            }
        }
        let page_len = usize::try_from(page_size).map_err(|_| {
            PersistenceError::InvalidData("Marketing conversion page size is invalid".into())
        })?;
        let has_more = visible.len() > page_len;
        visible.truncate(page_len);
        Ok(MarketingJourneyConversionReadPage {
            next_after_ref: has_more
                .then(|| visible.last())
                .flatten()
                .map(|row| row.conversion_ref),
            rows: visible,
            has_more,
        })
    }

    /// Read immutable owner-notification facts with the current projected
    /// delivery state. The join is deliberately by the deterministic logical
    /// message reference emitted by the journey execution transaction; a
    /// caller cannot supply a delivery id to cross a holding boundary.
    pub async fn read_journey_owner_notification_delivery_page(
        &self,
        scope: &AuthorityScope,
        after_ref: Option<Uuid>,
        page_size: u32,
    ) -> Result<MarketingJourneyOwnerNotificationDeliveryReadPage, PersistenceError> {
        validate_holding_scope(scope)?;
        let holding_id = match scope {
            AuthorityScope::Holding { holding_id }
            | AuthorityScope::Company { holding_id, .. }
            | AuthorityScope::Branch { holding_id, .. }
            | AuthorityScope::Subtree { holding_id, .. } => *holding_id,
            AuthorityScope::Platform => {
                return Err(PersistenceError::InvalidData(
                    "Marketing owner notification reads cannot use platform scope".into(),
                ));
            }
        };
        if page_size == 0 || page_size > MAX_MARKETING_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(
                "Marketing owner notification page size is outside its bound".into(),
            ));
        }
        let mut tx = self.begin_read().await?;
        let rows = sqlx::query(
            "SELECT n.notification_ref, n.journey_id, n.run_id, n.profile_id,
                    n.scope, n.owner_ref, n.occurred_at, n.source_version,
                    n.source_event_id, n.payload,
                    d.delivery_id, d.state AS delivery_state,
                    d.attempt_count AS delivery_attempt_count,
                    d.next_attempt_at AS delivery_next_attempt_at,
                    d.version AS delivery_version,
                    d.payload AS delivery_payload
               FROM erpx_holding.marketing_journey_owner_notification_read_rows AS n
               LEFT JOIN erpx_holding.marketing_delivery_read_rows AS d
                 ON d.data_profile_key=n.data_profile_key
                AND d.payload->>'logical_message_ref' =
                    concat('journey-owner-notification:', n.notification_ref::text)
              WHERE n.data_profile_key=$1 AND ($2::uuid IS NULL OR n.notification_ref>$2)
              ORDER BY n.notification_ref
              LIMIT $3",
        )
        .bind(&self.data_profile_key)
        .bind(after_ref)
        .bind(i64::from(page_size) + 1)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;

        let mut visible = Vec::with_capacity(rows.len());
        for row in rows {
            let row_scope: AuthorityScope = serde_json::from_value(row.try_get("scope")?)?;
            if !(row_scope.contains(scope) || scope.contains(&row_scope)) {
                continue;
            }
            let row_holding = match &row_scope {
                AuthorityScope::Holding { holding_id }
                | AuthorityScope::Company { holding_id, .. }
                | AuthorityScope::Branch { holding_id, .. }
                | AuthorityScope::Subtree { holding_id, .. } => *holding_id,
                AuthorityScope::Platform => Uuid::nil(),
            };
            if row_holding != holding_id {
                continue;
            }
            let delivery_attempt_count = row
                .try_get::<Option<i32>, _>("delivery_attempt_count")?
                .map(|value| {
                    u32::try_from(value).map_err(|_| {
                        PersistenceError::InvalidData(
                            "Marketing owner notification delivery attempt count is invalid".into(),
                        )
                    })
                })
                .transpose()?;
            let delivery_version = row
                .try_get::<Option<i64>, _>("delivery_version")?
                .map(|value| as_u64(value, "Marketing owner notification delivery version"))
                .transpose()?;
            visible.push(MarketingJourneyOwnerNotificationDeliveryReadRow {
                notification_ref: row.try_get("notification_ref")?,
                journey_id: row.try_get("journey_id")?,
                run_id: row.try_get("run_id")?,
                profile_id: row.try_get("profile_id")?,
                scope: row_scope,
                owner_ref: row.try_get("owner_ref")?,
                occurred_at: row.try_get("occurred_at")?,
                source_version: as_u64(
                    row.try_get("source_version")?,
                    "Marketing owner notification source version",
                )?,
                source_event_id: row.try_get("source_event_id")?,
                payload: row.try_get("payload")?,
                delivery_id: row.try_get("delivery_id")?,
                delivery_state: row.try_get("delivery_state")?,
                delivery_attempt_count,
                delivery_next_attempt_at: row.try_get("delivery_next_attempt_at")?,
                delivery_version,
                delivery_payload: row.try_get("delivery_payload")?,
            });
        }
        let page_len = usize::try_from(page_size).map_err(|_| {
            PersistenceError::InvalidData(
                "Marketing owner notification page size is invalid".into(),
            )
        })?;
        let has_more = visible.len() > page_len;
        visible.truncate(page_len);
        Ok(MarketingJourneyOwnerNotificationDeliveryReadPage {
            next_after_ref: has_more
                .then(|| visible.last())
                .flatten()
                .map(|row| row.notification_ref),
            rows: visible,
            has_more,
        })
    }

    pub async fn project_event(
        &self,
        event_id: Uuid,
        event_type: &str,
        aggregate_type: &str,
        aggregate_id: Uuid,
        aggregate_version: u64,
        payload: &Value,
    ) -> Result<MarketingProjectionApplication, PersistenceError> {
        let mut tx = self.begin_read().await?;
        let result = self
            .project_event_in_transaction(
                &mut tx,
                event_id,
                event_type,
                aggregate_type,
                aggregate_id,
                aggregate_version,
                payload,
                true,
            )
            .await;
        match result {
            Ok(result) => {
                tx.commit().await?;
                Ok(result)
            }
            Err(error) => {
                let _ = tx.rollback().await;
                Err(error)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn project_event_in_transaction(
        &self,
        tx: &mut PgConnection,
        event_id: Uuid,
        event_type: &str,
        aggregate_type: &str,
        aggregate_id: Uuid,
        aggregate_version: u64,
        payload: &Value,
        verify_source: bool,
    ) -> Result<MarketingProjectionApplication, PersistenceError> {
        validate_uuid(event_id, "Marketing event ID")?;
        validate_uuid(aggregate_id, "Marketing aggregate ID")?;
        validate_marketing_aggregate_type(aggregate_type)?;
        if aggregate_version == 0 {
            return Err(PersistenceError::InvalidData(
                "Marketing aggregate version must be positive".into(),
            ));
        }
        validate_marketing_source_event(
            aggregate_type,
            aggregate_id,
            aggregate_version,
            event_type,
            payload,
        )?;
        if verify_source {
            self.verify_authoritative_source_event(
                event_id,
                event_type,
                aggregate_type,
                aggregate_id,
                aggregate_version,
                payload,
            )
            .await?;
        }
        if is_crm_aggregate_type(aggregate_type) {
            return self
                .project_crm_event(
                    tx,
                    event_id,
                    event_type,
                    aggregate_type,
                    aggregate_id,
                    aggregate_version,
                    payload,
                )
                .await;
        }
        let aggregate_payload = payload.get("aggregate").cloned().ok_or_else(|| {
            PersistenceError::InvalidData("Marketing event payload is invalid".into())
        })?;
        let current = sqlx::query(
            "SELECT source_version, last_event_id
             FROM erpx_holding.marketing_projection_checkpoints
             WHERE data_profile_key=$1 AND aggregate_type=$2 AND aggregate_id=$3 FOR UPDATE",
        )
        .bind(&self.data_profile_key)
        .bind(aggregate_type)
        .bind(aggregate_id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(current) = current {
            let current_version = as_u64(
                current.try_get("source_version")?,
                "Marketing projection version",
            )?;
            let current_event_id: Uuid = current.try_get("last_event_id")?;
            if aggregate_version <= current_version {
                if aggregate_version == current_version && event_id == current_event_id {
                    return Ok(MarketingProjectionApplication {
                        event_id,
                        aggregate_type: aggregate_type.into(),
                        aggregate_id,
                        aggregate_version,
                        applied: false,
                    });
                }
                if aggregate_version == current_version {
                    return Err(PersistenceError::InvalidData(
                        "Marketing projection version is bound to another event".into(),
                    ));
                }
                return Err(PersistenceError::InvalidData(
                    "Marketing projection is missing the source event receipt".into(),
                ));
            }
            if aggregate_version != current_version.saturating_add(1) {
                return Err(PersistenceError::InvalidData(
                    "Marketing projection version is not contiguous".into(),
                ));
            }
        } else if aggregate_version != 1 {
            return Err(PersistenceError::InvalidData(
                "Marketing projection must start at version one".into(),
            ));
        }
        match aggregate_type {
            "marketing_profile" => {
                let privacy_redacted = payload
                    .get("privacy_redacted")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if event_type == "marketing.profile_erased.v1" || privacy_redacted {
                    let erasure: MarketingProfileErasure =
                        serde_json::from_value(aggregate_payload)?;
                    if erasure.profile_id != aggregate_id {
                        return Err(PersistenceError::InvalidData(
                            "Marketing profile erasure identity does not match its event".into(),
                        ));
                    }
                    erasure.validate().map_err(marketing_domain_error)?;
                    let source_scope: Option<Value> = sqlx::query_scalar(
                        "SELECT scope FROM erpx_holding.marketing_profiles
                         WHERE profile_id=$1 AND data_profile_key=$2",
                    )
                    .bind(aggregate_id)
                    .bind(&self.data_profile_key)
                    .fetch_optional(&mut *tx)
                    .await?;
                    if let Some(source_scope) = source_scope {
                        let source_scope: AuthorityScope = serde_json::from_value(source_scope)?;
                        if erasure.scope != source_scope {
                            return Err(PersistenceError::InvalidData(
                                "Marketing profile erasure scope does not match its read source"
                                    .into(),
                            ));
                        }
                    }
                    let marker = format!("erased:{aggregate_id}");
                    let marker_hash = hex::encode(Sha256::digest(marker.as_bytes()));
                    sqlx::query(
                        "INSERT INTO erpx_holding.marketing_profiles
                            (profile_id, data_profile_key, scope, external_ref, dedupe_key,
                             email, phone, locale, attributes, source_version, updated_at,
                             erased_at, erasure_reason)
                         VALUES ($1,$2,$3,$4,$5,NULL,NULL,'und','{}'::jsonb,$6,$7,$7,$8)
                         ON CONFLICT (profile_id) DO UPDATE SET
                            scope=$3, external_ref=$4, dedupe_key=$5, email=NULL,
                            phone=NULL, locale='und', attributes='{}'::jsonb,
                            source_version=$6, updated_at=$7, erased_at=$7,
                            erasure_reason=$8, projected_at=now()",
                    )
                    .bind(aggregate_id)
                    .bind(&self.data_profile_key)
                    .bind(serde_json::to_value(&erasure.scope)?)
                    .bind(&marker)
                    .bind(&marker_hash)
                    .bind(as_i64(aggregate_version, "Marketing source version")?)
                    .bind(erasure.erased_at)
                    .bind(&erasure.reason)
                    .execute(&mut *tx)
                    .await?;
                } else {
                    let profile: MarketingProfile = serde_json::from_value(aggregate_payload)?;
                    if profile.id != aggregate_id {
                        return Err(PersistenceError::InvalidData(
                            "Marketing profile identity does not match its event".into(),
                        ));
                    }
                    profile.validate().map_err(marketing_domain_error)?;
                    validate_projected_scope(
                        tx,
                        "marketing_profiles",
                        "profile_id",
                        aggregate_id,
                        &self.data_profile_key,
                        &profile.scope,
                        "marketing profile",
                    )
                    .await?;
                    sqlx::query(
                        "INSERT INTO erpx_holding.marketing_profiles
                            (profile_id, data_profile_key, scope, external_ref, dedupe_key,
                             email, phone, locale, attributes, source_version, updated_at,
                             erased_at, erasure_reason)
                         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,NULL,NULL)
                         ON CONFLICT (profile_id) DO UPDATE SET
                            scope=$3, external_ref=$4, dedupe_key=$5, email=$6,
                            phone=$7, locale=$8, attributes=$9, source_version=$10,
                            updated_at=$11, erased_at=NULL, erasure_reason=NULL,
                            projected_at=now()",
                    )
                    .bind(profile.id)
                    .bind(&self.data_profile_key)
                    .bind(serde_json::to_value(&profile.scope)?)
                    .bind(&profile.external_ref)
                    .bind(&profile.dedupe_key)
                    .bind(&profile.email)
                    .bind(&profile.phone)
                    .bind(&profile.locale)
                    .bind(serde_json::to_value(&profile.attributes)?)
                    .bind(as_i64(aggregate_version, "Marketing source version")?)
                    .bind(profile.updated_at)
                    .execute(&mut *tx)
                    .await?;
                }
            }
            "marketing_consent" => {
                let consent: ConsentRecord = serde_json::from_value(aggregate_payload)?;
                if consent.id != aggregate_id {
                    return Err(PersistenceError::InvalidData(
                        "Marketing consent identity does not match its event".into(),
                    ));
                }
                consent.validate().map_err(marketing_domain_error)?;
                validate_projected_scope(
                    tx,
                    "marketing_consents",
                    "consent_id",
                    aggregate_id,
                    &self.data_profile_key,
                    &consent.scope,
                    "marketing consent",
                )
                .await?;
                sqlx::query(
                    "INSERT INTO erpx_holding.marketing_consents
                        (consent_id, profile_id, data_profile_key, scope, purpose, channel,
                         state, source, jurisdiction, policy_version, proof_ref,
                         recorded_at, expires_at, source_version)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
                     ON CONFLICT (consent_id) DO UPDATE SET
                        state=$7, proof_ref=$11, recorded_at=$12, expires_at=$13,
                        source_version=$14, projected_at=now()",
                )
                .bind(consent.id)
                .bind(consent.profile_id)
                .bind(&self.data_profile_key)
                .bind(serde_json::to_value(&consent.scope)?)
                .bind(purpose_text(consent.purpose))
                .bind(channel_text(consent.channel))
                .bind(consent_state_text(consent.state))
                .bind(&consent.source)
                .bind(&consent.jurisdiction)
                .bind(&consent.policy_version)
                .bind(&consent.proof_ref)
                .bind(consent.recorded_at)
                .bind(consent.expires_at)
                .bind(as_i64(aggregate_version, "Marketing source version")?)
                .execute(&mut *tx)
                .await?;
            }
            "marketing_suppression" => {
                let suppression: SuppressionRecord = serde_json::from_value(aggregate_payload)?;
                if suppression.id != aggregate_id {
                    return Err(PersistenceError::InvalidData(
                        "Marketing suppression identity does not match its event".into(),
                    ));
                }
                suppression.validate().map_err(marketing_domain_error)?;
                validate_projected_scope(
                    tx,
                    "marketing_suppressions",
                    "suppression_id",
                    aggregate_id,
                    &self.data_profile_key,
                    &suppression.scope,
                    "marketing suppression",
                )
                .await?;
                sqlx::query(
                    "INSERT INTO erpx_holding.marketing_suppressions
                        (suppression_id, profile_id, data_profile_key, scope, reason,
                         source, created_at, expires_at, source_version)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
                     ON CONFLICT (suppression_id) DO UPDATE SET
                        scope=$4, reason=$5, source=$6, created_at=$7,
                        expires_at=$8, source_version=$9, projected_at=now()",
                )
                .bind(suppression.id)
                .bind(suppression.profile_id)
                .bind(&self.data_profile_key)
                .bind(serde_json::to_value(&suppression.scope)?)
                .bind(&suppression.reason)
                .bind(&suppression.source)
                .bind(suppression.created_at)
                .bind(suppression.expires_at)
                .bind(as_i64(aggregate_version, "Marketing source version")?)
                .execute(&mut *tx)
                .await?;
            }
            "marketing_audience" => {
                let membership: AudienceMembership = serde_json::from_value(aggregate_payload)?;
                if membership.audience_id != aggregate_id {
                    return Err(PersistenceError::InvalidData(
                        "Marketing audience identity does not match its event".into(),
                    ));
                }
                membership.validate().map_err(marketing_domain_error)?;
                validate_projected_scope(
                    tx,
                    "marketing_audiences",
                    "audience_id",
                    aggregate_id,
                    &self.data_profile_key,
                    &membership.scope,
                    "marketing audience",
                )
                .await?;
                sqlx::query(
                    "INSERT INTO erpx_holding.marketing_audiences
                        (audience_id, data_profile_key, scope, materialized_at,
                         profile_count, source_version)
                     VALUES ($1,$2,$3,$4,$5,$6)
                     ON CONFLICT (audience_id) DO UPDATE SET
                        scope=$3, materialized_at=$4, profile_count=$5,
                        source_version=$6, projected_at=now()",
                )
                .bind(membership.audience_id)
                .bind(&self.data_profile_key)
                .bind(serde_json::to_value(&membership.scope)?)
                .bind(membership.materialized_at)
                .bind(i64::try_from(membership.profile_ids.len()).map_err(|_| {
                    PersistenceError::InvalidData("Marketing audience count exceeds BIGINT".into())
                })?)
                .bind(as_i64(aggregate_version, "Marketing source version")?)
                .execute(&mut *tx)
                .await?;
                sqlx::query(
                    "DELETE FROM erpx_holding.marketing_audience_memberships
                     WHERE audience_id=$1 AND data_profile_key=$2",
                )
                .bind(membership.audience_id)
                .bind(&self.data_profile_key)
                .execute(&mut *tx)
                .await?;
                for profile_id in membership.profile_ids {
                    sqlx::query(
                        "INSERT INTO erpx_holding.marketing_audience_memberships
                            (audience_id, profile_id, data_profile_key, source_version,
                             materialized_at)
                         VALUES ($1,$2,$3,$4,$5)",
                    )
                    .bind(membership.audience_id)
                    .bind(profile_id)
                    .bind(&self.data_profile_key)
                    .bind(as_i64(aggregate_version, "Marketing source version")?)
                    .bind(membership.materialized_at)
                    .execute(&mut *tx)
                    .await?;
                }
            }
            MARKETING_JOURNEY_CONVERSION_AGGREGATE => {
                let conversion: JourneyConversion = serde_json::from_value(aggregate_payload)?;
                if conversion.conversion_ref != aggregate_id || aggregate_version != 1 {
                    return Err(PersistenceError::InvalidData(
                        "Marketing journey conversion identity or version is invalid".into(),
                    ));
                }
                conversion
                    .validate()
                    .map_err(marketing_journey_domain_error)?;
                sqlx::query(
                    "INSERT INTO erpx_holding.marketing_journey_conversion_read_rows
                        (data_profile_key, conversion_ref, journey_id, run_id, profile_id, scope,
                         revenue_minor, currency, occurred_at, source_version,
                         source_event_id, payload)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
                     ON CONFLICT (data_profile_key, conversion_ref) DO UPDATE SET
                        journey_id=$3, run_id=$4, profile_id=$5, scope=$6,
                        revenue_minor=$7, currency=$8, occurred_at=$9,
                        source_version=$10, source_event_id=$11, payload=$12,
                        projected_at=now()
                     WHERE erpx_holding.marketing_journey_conversion_read_rows.source_version
                           < EXCLUDED.source_version",
                )
                .bind(&self.data_profile_key)
                .bind(conversion.conversion_ref)
                .bind(conversion.journey_id)
                .bind(conversion.run_id)
                .bind(conversion.profile_id)
                .bind(serde_json::to_value(&conversion.scope)?)
                .bind(conversion.revenue_minor)
                .bind(&conversion.currency)
                .bind(conversion.occurred_at)
                .bind(as_i64(aggregate_version, "Marketing source version")?)
                .bind(event_id)
                .bind(serde_json::to_value(&conversion)?)
                .execute(&mut *tx)
                .await?;
            }
            MARKETING_JOURNEY_OWNER_NOTIFICATION_AGGREGATE => {
                let notification: JourneyOwnerNotification =
                    serde_json::from_value(aggregate_payload)?;
                if notification.notification_ref != aggregate_id || aggregate_version != 1 {
                    return Err(PersistenceError::InvalidData(
                        "Marketing journey owner notification identity or version is invalid"
                            .into(),
                    ));
                }
                notification
                    .validate()
                    .map_err(marketing_journey_domain_error)?;
                sqlx::query(
                    "INSERT INTO erpx_holding.marketing_journey_owner_notification_read_rows
                        (data_profile_key, notification_ref, journey_id, run_id, profile_id, scope,
                         owner_ref, occurred_at, source_version, source_event_id, payload)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
                     ON CONFLICT (data_profile_key, notification_ref) DO UPDATE SET
                        journey_id=$3, run_id=$4, profile_id=$5, scope=$6, owner_ref=$7,
                        occurred_at=$8, source_version=$9, source_event_id=$10,
                        payload=$11, projected_at=now()
                     WHERE erpx_holding.marketing_journey_owner_notification_read_rows.source_version
                           < EXCLUDED.source_version",
                )
                .bind(&self.data_profile_key)
                .bind(notification.notification_ref)
                .bind(notification.journey_id)
                .bind(notification.run_id)
                .bind(notification.profile_id)
                .bind(serde_json::to_value(&notification.scope)?)
                .bind(&notification.owner_ref)
                .bind(notification.occurred_at)
                .bind(as_i64(aggregate_version, "Marketing source version")?)
                .bind(event_id)
                .bind(serde_json::to_value(&notification)?)
                .execute(&mut *tx)
                .await?;
            }
            _ => unreachable!("Marketing aggregate type was validated"),
        }
        sqlx::query(
            "INSERT INTO erpx_holding.marketing_projection_checkpoints
                (aggregate_type, aggregate_id, data_profile_key, last_event_id, source_version)
             VALUES ($1,$2,$3,$4,$5)
             ON CONFLICT (aggregate_type, aggregate_id) DO UPDATE SET
                data_profile_key=$3, last_event_id=$4, source_version=$5, projected_at=now()",
        )
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(&self.data_profile_key)
        .bind(event_id)
        .bind(as_i64(aggregate_version, "Marketing source version")?)
        .execute(&mut *tx)
        .await?;
        Ok(MarketingProjectionApplication {
            event_id,
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            aggregate_version,
            applied: true,
        })
    }

    pub async fn clear_projection(
        &self,
    ) -> Result<MarketingProjectionRebuildPreparation, PersistenceError> {
        let mut tx = self.begin_read().await?;
        let preparation = clear_projection_in_transaction(&mut tx, &self.data_profile_key).await?;
        tx.commit().await?;
        Ok(preparation)
    }
}

/// Apply a journey-owned audience membership change inside the caller's
/// holding-scoped transaction. The journey execution idempotency receipt is
/// the command fence; this helper only mutates the audience aggregate and its
/// existing normalized Marketing authority/outbox contract.
pub(crate) async fn apply_journey_segment_change_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    execution_scope: &AuthorityScope,
    audience_id: Uuid,
    profile_id: Uuid,
    change: SegmentChange,
    at: DateTime<Utc>,
) -> Result<(), PersistenceError> {
    let profile_scope: AuthorityScope = sqlx::query_scalar(
        "SELECT scope
           FROM erpx_holding.marketing_profiles
          WHERE data_profile_key=$1 AND profile_id=$2 AND erased_at IS NULL
          FOR SHARE",
    )
    .bind(data_profile_key)
    .bind(profile_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(PersistenceError::NotFound {
        entity: "Marketing journey profile",
        id: profile_id,
    })
    .and_then(|value: Value| serde_json::from_value(value).map_err(PersistenceError::from))?;

    let audience = sqlx::query(
        "SELECT scope, version
           FROM erpx_holding.marketing_audiences
          WHERE data_profile_key=$1 AND audience_id=$2
          FOR UPDATE",
    )
    .bind(data_profile_key)
    .bind(audience_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(PersistenceError::NotFound {
        entity: "Marketing audience",
        id: audience_id,
    })?;
    let audience_scope: AuthorityScope = serde_json::from_value(audience.try_get("scope")?)?;
    if !execution_scope.contains(&profile_scope)
        || !execution_scope.contains(&audience_scope)
        || !audience_scope.contains(&profile_scope)
    {
        return Err(PersistenceError::InvalidData(
            "journey segment change scope is outside its execution scope".into(),
        ));
    }
    let current_version = as_u64(audience.try_get("version")?, "marketing audience version")?;
    let profile_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT profile_id
           FROM erpx_holding.marketing_audience_memberships
          WHERE data_profile_key=$1 AND audience_id=$2
          ORDER BY profile_id
          FOR UPDATE",
    )
    .bind(data_profile_key)
    .bind(audience_id)
    .fetch_all(&mut **tx)
    .await?;
    let mut members = profile_ids.into_iter().collect::<BTreeSet<_>>();
    let changed = match change {
        SegmentChange::Add => members.insert(profile_id),
        SegmentChange::Remove => members.remove(&profile_id),
    };
    if !changed {
        return Ok(());
    }
    let membership = AudienceMembership {
        audience_id,
        scope: audience_scope,
        profile_ids: members.into_iter().collect(),
        materialized_at: at,
    };
    validate_audience_membership(&membership)?;
    let payload = serde_json::to_value(&membership)?;
    let membership_hash = hash_value(&payload)?;
    let version = current_version.checked_add(1).ok_or_else(|| {
        PersistenceError::InvalidData("marketing audience version overflow".into())
    })?;
    sqlx::query(
        "UPDATE erpx_holding.marketing_audiences
            SET materialized_at=$3, profile_count=$4, membership_hash=$5,
                version=$6, updated_at=now()
          WHERE data_profile_key=$1 AND audience_id=$2",
    )
    .bind(data_profile_key)
    .bind(audience_id)
    .bind(membership.materialized_at)
    .bind(i64::try_from(membership.profile_ids.len()).map_err(|_| {
        PersistenceError::InvalidData("marketing audience count exceeds BIGINT".into())
    })?)
    .bind(&membership_hash)
    .bind(as_i64(version, "marketing audience version")?)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "DELETE FROM erpx_holding.marketing_audience_memberships
          WHERE data_profile_key=$1 AND audience_id=$2",
    )
    .bind(data_profile_key)
    .bind(audience_id)
    .execute(&mut **tx)
    .await?;
    for member_id in &membership.profile_ids {
        sqlx::query(
            "INSERT INTO erpx_holding.marketing_audience_memberships
                (audience_id, profile_id, data_profile_key, membership_version, materialized_at)
             VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(audience_id)
        .bind(member_id)
        .bind(data_profile_key)
        .bind(as_i64(version, "marketing audience version")?)
        .bind(membership.materialized_at)
        .execute(&mut **tx)
        .await?;
    }
    write_marketing_source_event(
        tx,
        data_profile_key,
        "marketing_audience",
        audience_id,
        version,
        &payload,
        "marketing.audience_saved.v1",
    )
    .await
}

/// Apply a profile tag action inside the caller's holding-scoped transaction.
/// Tags are represented as bounded profile attributes so the existing
/// authoritative profile aggregate, outbox event, and read projector remain
/// the single source of truth.
pub(crate) async fn apply_journey_profile_tag_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    execution_scope: &AuthorityScope,
    profile_id: Uuid,
    tag_ref: &str,
    add: bool,
    at: DateTime<Utc>,
) -> Result<(), PersistenceError> {
    let row = sqlx::query(
        "SELECT payload, scope, version
           FROM erpx_holding.marketing_profiles
          WHERE data_profile_key=$1 AND profile_id=$2 AND erased_at IS NULL
          FOR UPDATE",
    )
    .bind(data_profile_key)
    .bind(profile_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(PersistenceError::NotFound {
        entity: "Marketing journey profile",
        id: profile_id,
    })?;
    let stored_scope: AuthorityScope = serde_json::from_value(row.try_get("scope")?)?;
    if !execution_scope.contains(&stored_scope) {
        return Err(PersistenceError::InvalidData(
            "journey profile tag scope is outside its execution scope".into(),
        ));
    }
    let mut profile: MarketingProfile = serde_json::from_value(row.try_get("payload")?)?;
    if profile.id != profile_id || profile.scope != stored_scope {
        return Err(PersistenceError::InvalidData(
            "journey profile payload identity or scope diverges from its source".into(),
        ));
    }
    let attribute = format!("tag:{tag_ref}");
    let changed = if add {
        profile
            .attributes
            .insert(attribute, "true".into())
            .as_deref()
            != Some("true")
    } else {
        profile.attributes.remove(&attribute).is_some()
    };
    if !changed {
        return Ok(());
    }
    profile.updated_at = at;
    profile.validate().map_err(marketing_domain_error)?;
    let payload = serde_json::to_value(&profile)?;
    let payload_hash = hash_value(&payload)?;
    let current_version = as_u64(row.try_get("version")?, "marketing profile version")?;
    let version = current_version.checked_add(1).ok_or_else(|| {
        PersistenceError::InvalidData("marketing profile version overflow".into())
    })?;
    sqlx::query(
        "UPDATE erpx_holding.marketing_profiles
            SET attributes=$3, payload=$4, payload_hash=$5, version=$6, updated_at=$7
          WHERE data_profile_key=$1 AND profile_id=$2 AND version=$8",
    )
    .bind(data_profile_key)
    .bind(profile_id)
    .bind(serde_json::to_value(&profile.attributes)?)
    .bind(&payload)
    .bind(&payload_hash)
    .bind(as_i64(version, "marketing profile version")?)
    .bind(profile.updated_at)
    .bind(as_i64(
        current_version,
        "marketing previous profile version",
    )?)
    .execute(&mut **tx)
    .await?
    .rows_affected()
    .eq(&1)
    .then_some(())
    .ok_or(PersistenceError::OptimisticConcurrency {
        aggregate_type: "marketing_profile".into(),
        aggregate_id: profile_id,
        expected: Some(current_version),
        actual: None,
    })?;
    write_marketing_source_event(
        tx,
        data_profile_key,
        "marketing_profile",
        profile_id,
        version,
        &payload,
        "marketing.profile_saved.v1",
    )
    .await
}

/// Persist an immutable conversion fact inside the journey cursor transaction.
/// A conversion reference is a stable business key, so an exact retry is a
/// no-op while a divergent retry is rejected before the cursor can advance.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn apply_journey_conversion_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    execution_scope: &AuthorityScope,
    conversion_ref: Uuid,
    journey_id: Uuid,
    run_id: Uuid,
    profile_id: Uuid,
    revenue_minor: Option<i64>,
    currency: Option<String>,
    at: DateTime<Utc>,
) -> Result<(), PersistenceError> {
    let profile_scope: AuthorityScope = serde_json::from_value(
        sqlx::query_scalar::<_, Value>(
            "SELECT scope FROM erpx_holding.marketing_profiles
          WHERE data_profile_key=$1 AND profile_id=$2 AND erased_at IS NULL
          FOR SHARE",
        )
        .bind(data_profile_key)
        .bind(profile_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "Marketing journey profile",
            id: profile_id,
        })?,
    )?;
    if !execution_scope.contains(&profile_scope) {
        return Err(PersistenceError::InvalidData(
            "journey conversion profile scope is outside its execution scope".into(),
        ));
    }
    let conversion = JourneyConversion::new(
        conversion_ref,
        journey_id,
        run_id,
        profile_id,
        profile_scope,
        revenue_minor,
        currency,
        at,
    )
    .map_err(marketing_journey_domain_error)?;
    let payload = serde_json::to_value(&conversion)?;
    let payload_hash = hash_value(&payload)?;
    let existing = sqlx::query(
        "SELECT payload_hash, payload
           FROM erpx_holding.marketing_journey_conversions
          WHERE data_profile_key=$1 AND conversion_ref=$2
          FOR UPDATE",
    )
    .bind(data_profile_key)
    .bind(conversion_ref)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(existing) = existing {
        let existing_hash: String = existing.try_get("payload_hash")?;
        let existing_payload: Value = existing.try_get("payload")?;
        if existing_hash == payload_hash && existing_payload == payload {
            return Ok(());
        }
        return Err(PersistenceError::InvalidData(
            "journey conversion reference is already bound to different facts".into(),
        ));
    }
    sqlx::query(
        "INSERT INTO erpx_holding.marketing_journey_conversions
            (data_profile_key, conversion_ref, journey_id, run_id, profile_id, scope,
             revenue_minor, currency, occurred_at, version, payload, payload_hash)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,1,$10,$11)",
    )
    .bind(data_profile_key)
    .bind(conversion.conversion_ref)
    .bind(conversion.journey_id)
    .bind(conversion.run_id)
    .bind(conversion.profile_id)
    .bind(serde_json::to_value(&conversion.scope)?)
    .bind(conversion.revenue_minor)
    .bind(&conversion.currency)
    .bind(conversion.occurred_at)
    .bind(&payload)
    .bind(payload_hash)
    .execute(&mut **tx)
    .await?;
    write_marketing_source_event(
        tx,
        data_profile_key,
        MARKETING_JOURNEY_CONVERSION_AGGREGATE,
        conversion.conversion_ref,
        1,
        &payload,
        MARKETING_JOURNEY_CONVERSION_EVENT,
    )
    .await
}

/// Queue an immutable internal owner-notification handoff inside the journey
/// cursor transaction. Provider delivery is downstream and never makes the
/// journey cursor claim success by itself.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn apply_journey_owner_notification_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    execution_scope: &AuthorityScope,
    notification_ref: Uuid,
    journey_id: Uuid,
    run_id: Uuid,
    profile_id: Uuid,
    owner_ref: String,
    at: DateTime<Utc>,
) -> Result<DateTime<Utc>, PersistenceError> {
    let profile_scope: AuthorityScope = serde_json::from_value(
        sqlx::query_scalar::<_, Value>(
            "SELECT scope FROM erpx_holding.marketing_profiles
          WHERE data_profile_key=$1 AND profile_id=$2 AND erased_at IS NULL
          FOR SHARE",
        )
        .bind(data_profile_key)
        .bind(profile_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "Marketing journey profile",
            id: profile_id,
        })?,
    )?;
    if !execution_scope.contains(&profile_scope) {
        return Err(PersistenceError::InvalidData(
            "journey owner notification profile scope is outside its execution scope".into(),
        ));
    }
    // A replay may arrive with a later worker clock value than the original
    // cursor advance. The notification reference is immutable, so preserve
    // the authoritative occurrence time already stored for that reference;
    // otherwise the replay would appear to be a different payload and fail
    // idempotency instead of repairing the downstream delivery reservation.
    let existing = sqlx::query(
        "SELECT payload_hash, payload
           FROM erpx_holding.marketing_journey_owner_notifications
          WHERE data_profile_key=$1 AND notification_ref=$2
          FOR UPDATE",
    )
    .bind(data_profile_key)
    .bind(notification_ref)
    .fetch_optional(&mut **tx)
    .await?;
    let effective_at = existing
        .as_ref()
        .map(|row| {
            let payload: Value = row.try_get("payload")?;
            let existing_notification: JourneyOwnerNotification = serde_json::from_value(payload)?;
            Ok::<DateTime<Utc>, PersistenceError>(existing_notification.occurred_at)
        })
        .transpose()?
        .unwrap_or(at);
    let notification = JourneyOwnerNotification::new(
        notification_ref,
        journey_id,
        run_id,
        profile_id,
        profile_scope,
        owner_ref,
        effective_at,
    )
    .map_err(marketing_journey_domain_error)?;
    let payload = serde_json::to_value(&notification)?;
    let payload_hash = hash_value(&payload)?;
    if let Some(existing) = existing {
        let existing_hash: String = existing.try_get("payload_hash")?;
        let existing_payload: Value = existing.try_get("payload")?;
        if existing_hash == payload_hash && existing_payload == payload {
            return Ok(effective_at);
        }
        return Err(PersistenceError::InvalidData(
            "journey owner notification reference is already bound to different facts".into(),
        ));
    }
    sqlx::query(
        "INSERT INTO erpx_holding.marketing_journey_owner_notifications
            (data_profile_key, notification_ref, journey_id, run_id, profile_id, scope,
             owner_ref, occurred_at, version, payload, payload_hash)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,1,$9,$10)",
    )
    .bind(data_profile_key)
    .bind(notification.notification_ref)
    .bind(notification.journey_id)
    .bind(notification.run_id)
    .bind(notification.profile_id)
    .bind(serde_json::to_value(&notification.scope)?)
    .bind(&notification.owner_ref)
    .bind(notification.occurred_at)
    .bind(&payload)
    .bind(payload_hash)
    .execute(&mut **tx)
    .await?;
    write_marketing_source_event(
        tx,
        data_profile_key,
        MARKETING_JOURNEY_OWNER_NOTIFICATION_AGGREGATE,
        notification.notification_ref,
        1,
        &payload,
        MARKETING_JOURNEY_OWNER_NOTIFICATION_EVENT,
    )
    .await?;
    Ok(effective_at)
}

async fn clear_projection_in_transaction(
    tx: &mut PgConnection,
    data_profile_key: &str,
) -> Result<MarketingProjectionRebuildPreparation, PersistenceError> {
    let mut rows_cleared = 0_u64;
    for table in [
        "marketing_profiles",
        "marketing_consents",
        "marketing_suppressions",
        "marketing_audiences",
        "marketing_audience_memberships",
    ] {
        let query = format!("DELETE FROM erpx_holding.{table} WHERE data_profile_key=$1");
        rows_cleared = rows_cleared.saturating_add(
            sqlx::query(&query)
                .bind(data_profile_key)
                .execute(&mut *tx)
                .await?
                .rows_affected(),
        );
    }
    for table in [
        "crm_accepted_lead_handoff_read_rows",
        "crm_opportunity_read_rows",
        "crm_attribution_read_rows",
        "marketing_journey_conversion_read_rows",
        "marketing_journey_owner_notification_read_rows",
    ] {
        let query = format!("DELETE FROM erpx_holding.{table} WHERE data_profile_key=$1");
        rows_cleared = rows_cleared.saturating_add(
            sqlx::query(&query)
                .bind(data_profile_key)
                .execute(&mut *tx)
                .await?
                .rows_affected(),
        );
    }
    let checkpoints_cleared = sqlx::query(
        "DELETE FROM erpx_holding.marketing_projection_checkpoints WHERE data_profile_key=$1",
    )
    .bind(data_profile_key)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    let crm_receipts_cleared = sqlx::query(
        "DELETE FROM erpx_holding.crm_projection_event_receipts WHERE data_profile_key=$1",
    )
    .bind(data_profile_key)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    Ok(MarketingProjectionRebuildPreparation {
        data_profile_key: data_profile_key.to_owned(),
        rows_cleared,
        checkpoints_cleared: checkpoints_cleared.saturating_add(crm_receipts_cleared),
    })
}

impl SqlxMarketingStore {
    /// Marketing has multiple source aggregate families, including CRM. A
    /// projection caller therefore supplies the event type explicitly; all
    /// supplied identity and payload fields must still match the immutable
    /// holding-scoped outbox row before the read model is touched.
    async fn verify_authoritative_source_event(
        &self,
        event_id: Uuid,
        event_type: &str,
        aggregate_type: &str,
        aggregate_id: Uuid,
        aggregate_version: u64,
        payload: &Value,
    ) -> Result<(), PersistenceError> {
        let mut tx = self.begin_write().await?;
        let row = sqlx::query(
            "SELECT event_type, aggregate_type, aggregate_id, aggregate_version, payload
             FROM erpx_holding.outbox_events
             WHERE event_id=$1 AND data_profile_key=$2",
        )
        .bind(event_id)
        .bind(&self.data_profile_key)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;

        let Some(row) = row else {
            return Err(PersistenceError::InvalidData(
                "Marketing projection event is not authoritative".into(),
            ));
        };
        let source_event_type: String = row.try_get("event_type")?;
        let source_aggregate_type: String = row.try_get("aggregate_type")?;
        let source_aggregate_id: Uuid = row.try_get("aggregate_id")?;
        let source_version = as_u64(
            row.try_get("aggregate_version")?,
            "Marketing source event version",
        )?;
        let source_payload: Value = row.try_get("payload")?;
        if source_event_type != event_type
            || source_aggregate_type != aggregate_type
            || source_aggregate_id != aggregate_id
            || source_version != aggregate_version
            || source_payload != *payload
        {
            return Err(PersistenceError::InvalidData(
                "Marketing projection event does not match the authoritative outbox".into(),
            ));
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn project_crm_event(
        &self,
        tx: &mut PgConnection,
        event_id: Uuid,
        event_type: &str,
        aggregate_type: &str,
        aggregate_id: Uuid,
        aggregate_version: u64,
        payload: &Value,
    ) -> Result<MarketingProjectionApplication, PersistenceError> {
        let envelope = payload
            .get("aggregate")
            .cloned()
            .ok_or_else(|| PersistenceError::InvalidData("CRM event payload is invalid".into()))?;
        if payload.get("aggregate_type").and_then(Value::as_str) != Some(aggregate_type)
            || payload.get("version").and_then(Value::as_u64) != Some(aggregate_version)
        {
            return Err(PersistenceError::InvalidData(
                "CRM event envelope does not match its outbox row".into(),
            ));
        }
        if event_type != crm_event_type(aggregate_type) {
            return Err(PersistenceError::InvalidData(
                "CRM event type does not match its aggregate type".into(),
            ));
        }
        let payload_hash = hash_value(payload)?;
        if let Some(row) = sqlx::query(
            "SELECT aggregate_type, aggregate_id, aggregate_version, payload_hash
             FROM erpx_holding.crm_projection_event_receipts
             WHERE data_profile_key=$1 AND event_id=$2 FOR SHARE",
        )
        .bind(&self.data_profile_key)
        .bind(event_id)
        .fetch_optional(&mut *tx)
        .await?
        {
            let same = row.get::<String, _>("aggregate_type") == aggregate_type
                && row.get::<Uuid, _>("aggregate_id") == aggregate_id
                && row.get::<i64, _>("aggregate_version")
                    == as_i64(aggregate_version, "CRM source version")?
                && row.get::<String, _>("payload_hash") == payload_hash;
            if !same {
                return Err(PersistenceError::InvalidData(
                    "CRM projection event ID was reused with different data".into(),
                ));
            }
            return Ok(MarketingProjectionApplication {
                event_id,
                aggregate_type: aggregate_type.into(),
                aggregate_id,
                aggregate_version,
                applied: false,
            });
        }
        let current =
            crm_projection_version(tx, &self.data_profile_key, aggregate_type, aggregate_id)
                .await?;
        let applied = match current {
            Some(version) if version == aggregate_version => {
                return Err(PersistenceError::InvalidData(
                    "CRM aggregate version was published by a different event".into(),
                ));
            }
            Some(version) if version > aggregate_version => false,
            Some(version) => {
                if aggregate_version != version.saturating_add(1) {
                    return Err(PersistenceError::InvalidData(
                        "CRM projection version is not contiguous".into(),
                    ));
                }
                true
            }
            None => {
                if aggregate_version != 1 {
                    return Err(PersistenceError::InvalidData(
                        "CRM projection must start at version one".into(),
                    ));
                }
                true
            }
        };
        if applied {
            match aggregate_type {
                CRM_HANDOFF_AGGREGATE => {
                    let value: AcceptedLeadHandoff = serde_json::from_value(envelope.clone())?;
                    if value.id != aggregate_id || aggregate_version != 1 {
                        return Err(PersistenceError::InvalidData(
                            "CRM handoff identity or version is invalid".into(),
                        ));
                    }
                    value.validate().map_err(marketing_crm_domain_error)?;
                    let (holding_id, company_id, branch_id) = crm_scope_columns(&value.scope)?;
                    sqlx::query(
                        "INSERT INTO erpx_holding.crm_accepted_lead_handoff_read_rows
                            (data_profile_key, handoff_id, holding_id, company_id, branch_id,
                             scope, reference, account_id, contact_id, accepted_at,
                             source_version, source_event_id, payload)
                         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
                         ON CONFLICT (data_profile_key, handoff_id) DO UPDATE SET
                            holding_id=$3, company_id=$4, branch_id=$5, scope=$6,
                            reference=$7, account_id=$8, contact_id=$9,
                            accepted_at=$10, source_version=$11, source_event_id=$12,
                            payload=$13, projected_at=now()
                         WHERE erpx_holding.crm_accepted_lead_handoff_read_rows.source_version
                               < EXCLUDED.source_version",
                    )
                    .bind(&self.data_profile_key)
                    .bind(value.id)
                    .bind(holding_id)
                    .bind(company_id)
                    .bind(branch_id)
                    .bind(serde_json::to_value(&value.scope)?)
                    .bind(&value.reference)
                    .bind(value.account.id.as_str())
                    .bind(value.contact.id.as_str())
                    .bind(value.accepted_at)
                    .bind(as_i64(aggregate_version, "CRM source version")?)
                    .bind(event_id)
                    .bind(envelope)
                    .execute(&mut *tx)
                    .await?;
                }
                CRM_OPPORTUNITY_AGGREGATE => {
                    let value: CrmOpportunity = serde_json::from_value(envelope.clone())?;
                    if value.id != aggregate_id {
                        return Err(PersistenceError::InvalidData(
                            "CRM opportunity identity is invalid".into(),
                        ));
                    }
                    value.validate().map_err(marketing_crm_domain_error)?;
                    let (holding_id, company_id, branch_id) = crm_scope_columns(&value.scope)?;
                    sqlx::query(
                        "INSERT INTO erpx_holding.crm_opportunity_read_rows
                            (data_profile_key, opportunity_id, holding_id, company_id, branch_id,
                             scope, reference, accepted_lead_reference, account_id,
                             contact_id, name, currency, expected_revenue_minor, status,
                             created_at, updated_at, source_version, source_event_id, payload)
                         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)
                         ON CONFLICT (data_profile_key, opportunity_id) DO UPDATE SET
                            holding_id=$3, company_id=$4, branch_id=$5, scope=$6,
                            reference=$7, accepted_lead_reference=$8, account_id=$9,
                            contact_id=$10, name=$11, currency=$12,
                            expected_revenue_minor=$13, status=$14, created_at=$15,
                            updated_at=$16, source_version=$17, source_event_id=$18,
                            payload=$19, projected_at=now()
                         WHERE erpx_holding.crm_opportunity_read_rows.source_version
                               < EXCLUDED.source_version",
                    )
                    .bind(&self.data_profile_key)
                    .bind(value.id)
                    .bind(holding_id)
                    .bind(company_id)
                    .bind(branch_id)
                    .bind(serde_json::to_value(&value.scope)?)
                    .bind(&value.reference)
                    .bind(value.accepted_lead.id.as_str())
                    .bind(value.account.id.as_str())
                    .bind(value.contact.id.as_str())
                    .bind(&value.name)
                    .bind(&value.currency)
                    .bind(value.expected_revenue_minor)
                    .bind(crm_opportunity_status(value.status))
                    .bind(value.created_at)
                    .bind(value.updated_at)
                    .bind(as_i64(aggregate_version, "CRM source version")?)
                    .bind(event_id)
                    .bind(envelope)
                    .execute(&mut *tx)
                    .await?;
                }
                CRM_ATTRIBUTION_AGGREGATE => {
                    let value: CrmAttributionEvent = serde_json::from_value(envelope.clone())?;
                    if value.record_id != aggregate_id
                        || value.source_opportunity_version == 0
                        || value.result.total_revenue_minor < 0
                        || value.result.total_cost_minor < 0
                        || value
                            .result
                            .entries
                            .iter()
                            .any(|entry| entry.revenue_minor < 0 || entry.cost_minor < 0)
                    {
                        return Err(PersistenceError::InvalidData(
                            "CRM attribution payload is invalid".into(),
                        ));
                    }
                    if value
                        .result
                        .entries
                        .iter()
                        .try_fold(0_i64, |sum, entry| sum.checked_add(entry.revenue_minor))
                        != Some(value.result.total_revenue_minor)
                        || value
                            .result
                            .entries
                            .iter()
                            .try_fold(0_i64, |sum, entry| sum.checked_add(entry.cost_minor))
                            != Some(value.result.total_cost_minor)
                    {
                        return Err(PersistenceError::InvalidData(
                            "CRM attribution totals are invalid".into(),
                        ));
                    }
                    sqlx::query(
                        "INSERT INTO erpx_holding.crm_attribution_read_rows
                            (data_profile_key, record_id, opportunity_id, model,
                             source_opportunity_version, total_revenue_minor,
                             total_cost_minor, source_version, source_event_id, payload)
                         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
                         ON CONFLICT (data_profile_key, record_id) DO UPDATE SET
                            opportunity_id=$3, model=$4, source_opportunity_version=$5,
                            total_revenue_minor=$6, total_cost_minor=$7,
                            source_version=$8, source_event_id=$9, payload=$10,
                            projected_at=now()
                         WHERE erpx_holding.crm_attribution_read_rows.source_version
                               < EXCLUDED.source_version",
                    )
                    .bind(&self.data_profile_key)
                    .bind(value.record_id)
                    .bind(value.opportunity_id)
                    .bind(crm_attribution_model(value.model))
                    .bind(as_i64(
                        value.source_opportunity_version,
                        "CRM source opportunity version",
                    )?)
                    .bind(value.result.total_revenue_minor)
                    .bind(value.result.total_cost_minor)
                    .bind(as_i64(aggregate_version, "CRM source version")?)
                    .bind(event_id)
                    .bind(envelope)
                    .execute(&mut *tx)
                    .await?;
                }
                _ => unreachable!("CRM aggregate type was validated"),
            }
        }
        sqlx::query(
            "INSERT INTO erpx_holding.crm_projection_event_receipts
                (data_profile_key, event_id, aggregate_type, aggregate_id,
                 aggregate_version, event_type, payload_hash, occurred_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,now())",
        )
        .bind(&self.data_profile_key)
        .bind(event_id)
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(as_i64(aggregate_version, "CRM source version")?)
        .bind(event_type)
        .bind(payload_hash)
        .execute(&mut *tx)
        .await?;
        Ok(MarketingProjectionApplication {
            event_id,
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            aggregate_version,
            applied,
        })
    }

    async fn begin_write(&self) -> Result<Transaction<'_, Postgres>, PersistenceError> {
        begin_scoped(&self.write_pool, &self.data_profile_key).await
    }

    async fn begin_read(&self) -> Result<Transaction<'_, Postgres>, PersistenceError> {
        begin_scoped(&self.read_pool, &self.data_profile_key).await
    }
}

async fn read_source_event_page_in_transaction(
    tx: &mut PgConnection,
    data_profile_key: &str,
    cursor: Option<&MarketingSourceEventCursor>,
    page_size: u32,
) -> Result<MarketingSourceEventPage, PersistenceError> {
    if page_size == 0 || page_size > MAX_MARKETING_SOURCE_EVENT_PAGE_SIZE {
        return Err(PersistenceError::InvalidData(format!(
            "Marketing source event page size must be 1..={MAX_MARKETING_SOURCE_EVENT_PAGE_SIZE}"
        )));
    }
    let limit = i64::from(page_size) + 1;
    let rows = if let Some(cursor) = cursor {
        validate_marketing_aggregate_type(&cursor.aggregate_type)?;
        validate_uuid(cursor.aggregate_id, "Marketing source cursor aggregate ID")?;
        sqlx::query(
            "SELECT event_id, event_type, aggregate_type, aggregate_id,
                    aggregate_version, payload, occurred_at
             FROM erpx_holding.outbox_events
             WHERE data_profile_key=$1
               AND aggregate_type IN
                   ('marketing_profile', 'marketing_consent',
                    'marketing_suppression', 'marketing_audience',
                    'marketing_crm_accepted_lead_handoff',
                    'marketing_crm_opportunity', 'marketing_crm_attribution',
                    'marketing_journey_conversion',
                    'marketing_journey_owner_notification')
               AND (aggregate_type > $2
                    OR (aggregate_type = $2 AND aggregate_id > $3)
                    OR (aggregate_type = $2 AND aggregate_id = $3
                        AND aggregate_version > $4))
             ORDER BY aggregate_type, aggregate_id, aggregate_version
             LIMIT $5",
        )
        .bind(data_profile_key)
        .bind(&cursor.aggregate_type)
        .bind(cursor.aggregate_id)
        .bind(as_i64(
            cursor.aggregate_version,
            "Marketing source cursor version",
        )?)
        .bind(limit)
        .fetch_all(tx)
        .await?
    } else {
        sqlx::query(
            "SELECT event_id, event_type, aggregate_type, aggregate_id,
                    aggregate_version, payload, occurred_at
             FROM erpx_holding.outbox_events
             WHERE data_profile_key=$1
               AND aggregate_type IN
                   ('marketing_profile', 'marketing_consent',
                    'marketing_suppression', 'marketing_audience',
                    'marketing_crm_accepted_lead_handoff',
                    'marketing_crm_opportunity', 'marketing_crm_attribution',
                    'marketing_journey_conversion',
                    'marketing_journey_owner_notification')
             ORDER BY aggregate_type, aggregate_id, aggregate_version
             LIMIT $2",
        )
        .bind(data_profile_key)
        .bind(limit)
        .fetch_all(tx)
        .await?
    };
    let page_len = usize::try_from(page_size).map_err(|_| {
        PersistenceError::InvalidData("Marketing source page size is not representable".into())
    })?;
    let has_more = rows.len() > page_len;
    let events = rows
        .into_iter()
        .take(page_len)
        .map(|row| {
            let event_id: Uuid = row.try_get("event_id")?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            let aggregate_type: String = row.try_get("aggregate_type")?;
            validate_uuid(event_id, "Marketing source event ID")?;
            validate_uuid(aggregate_id, "Marketing source aggregate ID")?;
            validate_marketing_aggregate_type(&aggregate_type)?;
            Ok(MarketingSourceEvent {
                event_id,
                event_type: row.try_get("event_type")?,
                aggregate_type,
                aggregate_id,
                aggregate_version: as_u64(
                    row.try_get("aggregate_version")?,
                    "Marketing source event version",
                )?,
                payload: row.try_get("payload")?,
                occurred_at: row.try_get("occurred_at")?,
            })
        })
        .collect::<Result<Vec<_>, PersistenceError>>()?;
    let next_cursor = if has_more {
        events.last().map(|event| MarketingSourceEventCursor {
            aggregate_type: event.aggregate_type.clone(),
            aggregate_id: event.aggregate_id,
            aggregate_version: event.aggregate_version,
        })
    } else {
        None
    };
    Ok(MarketingSourceEventPage {
        events,
        next_cursor,
        has_more,
    })
}

async fn source_event_high_watermark_in_transaction(
    tx: &mut PgConnection,
    data_profile_key: &str,
) -> Result<u64, PersistenceError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1
           AND aggregate_type IN
               ('marketing_profile', 'marketing_consent',
                'marketing_suppression', 'marketing_audience',
                'marketing_crm_accepted_lead_handoff',
                'marketing_crm_opportunity', 'marketing_crm_attribution',
                'marketing_journey_conversion',
                'marketing_journey_owner_notification')",
    )
    .bind(data_profile_key)
    .fetch_one(tx)
    .await?;
    u64::try_from(count).map_err(|_| {
        PersistenceError::InvalidData("Marketing source event high-watermark is invalid".into())
    })
}

async fn write_marketing_source_event(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
    version: u64,
    payload: &Value,
    event_type: &str,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_holding.outbox_events
            (event_id, aggregate_type, aggregate_id, data_profile_key,
             aggregate_version, event_type, payload, occurred_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,now())",
    )
    .bind(Uuid::new_v4())
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(data_profile_key)
    .bind(as_i64(version, "marketing aggregate version")?)
    .bind(event_type)
    .bind(serde_json::json!({
        "aggregate_type": aggregate_type,
        "aggregate": payload,
        "version": version,
        "event_type": event_type,
    }))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn ensure_profile_scope(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    profile_id: Uuid,
    requested_scope: &AuthorityScope,
) -> Result<(), PersistenceError> {
    let row = sqlx::query(
        "SELECT profile_id, scope FROM erpx_holding.marketing_profiles
         WHERE profile_id = $1 AND data_profile_key = $2 AND erased_at IS NULL FOR SHARE",
    )
    .bind(profile_id)
    .bind(data_profile_key)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(PersistenceError::NotFound {
        entity: "marketing profile",
        id: profile_id,
    })?;
    let stored_scope: AuthorityScope = serde_json::from_value(row.try_get("scope")?)?;
    validate_immutable_scope(&stored_scope, requested_scope, "marketing profile")?;
    Ok(())
}

/// Returns `Some(receipt)` only for a completed replay. A newly inserted
/// reservation is held by the caller's transaction until completion.
async fn reserve_idempotency(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    operation: &str,
    idempotency_key: &str,
    request_hash: String,
    resource_id: Uuid,
) -> Result<Option<MarketingMutationReceipt>, PersistenceError> {
    let inserted = sqlx::query(
        "INSERT INTO erpx_holding.marketing_command_idempotency
            (idempotency_key, data_profile_key, operation, request_hash, resource_id, status)
         VALUES ($1, $2, $3, $4, $5, 'reserved')
         ON CONFLICT (idempotency_key) DO NOTHING",
    )
    .bind(idempotency_key)
    .bind(data_profile_key)
    .bind(operation)
    .bind(&request_hash)
    .bind(resource_id)
    .execute(&mut **tx)
    .await?;
    if inserted.rows_affected() == 1 {
        return Ok(None);
    }
    let row = sqlx::query(
        "SELECT data_profile_key, operation, request_hash, resource_id, status, response
         FROM erpx_holding.marketing_command_idempotency
         WHERE idempotency_key = $1 FOR UPDATE",
    )
    .bind(idempotency_key)
    .fetch_one(&mut **tx)
    .await?;
    if row.try_get::<String, _>("data_profile_key")? != data_profile_key
        || row.try_get::<String, _>("operation")? != operation
        || row.try_get::<String, _>("request_hash")? != request_hash
        || row.try_get::<Uuid, _>("resource_id")? != resource_id
    {
        return Err(PersistenceError::IdempotencyConflict {
            key: idempotency_key.into(),
        });
    }
    match row.try_get::<String, _>("status")?.as_str() {
        "completed" => {
            let response: Value =
                row.try_get::<Option<Value>, _>("response")?
                    .ok_or_else(|| {
                        PersistenceError::InvalidData(
                            "completed marketing idempotency record has no response".into(),
                        )
                    })?;
            let mut receipt: MarketingMutationReceipt = serde_json::from_value(response)?;
            receipt.replayed = true;
            Ok(Some(receipt))
        }
        "reserved" => Err(PersistenceError::InvalidData(
            "marketing idempotency reservation is incomplete".into(),
        )),
        status => Err(PersistenceError::InvalidData(format!(
            "unknown marketing idempotency status {status}"
        ))),
    }
}

async fn complete_idempotency(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    idempotency_key: &str,
    receipt: &MarketingMutationReceipt,
) -> Result<(), PersistenceError> {
    let response = serde_json::to_value(receipt)?;
    let affected = sqlx::query(
        "UPDATE erpx_holding.marketing_command_idempotency
         SET status = 'completed', response = $3, completed_at = now()
         WHERE idempotency_key = $1 AND data_profile_key = $2 AND status = 'reserved'",
    )
    .bind(idempotency_key)
    .bind(data_profile_key)
    .bind(response)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    if affected != 1 {
        return Err(PersistenceError::InvalidData(
            "marketing idempotency reservation was lost".into(),
        ));
    }
    Ok(())
}

fn validate_profile(profile: &MarketingProfile) -> Result<(), PersistenceError> {
    profile
        .validate()
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
    validate_holding_scope(&profile.scope)?;
    let expected = dedupe_key(
        profile.email.as_deref(),
        profile.phone.as_deref(),
        &profile.external_ref,
    );
    if profile.dedupe_key != expected {
        return Err(PersistenceError::InvalidData(
            "marketing profile dedupe key does not match canonical payload".into(),
        ));
    }
    Ok(())
}

fn validate_immutable_scope(
    stored_scope: &AuthorityScope,
    requested_scope: &AuthorityScope,
    entity: &'static str,
) -> Result<(), PersistenceError> {
    if stored_scope != requested_scope {
        return Err(PersistenceError::InvalidData(format!(
            "{entity} scope cannot be rebound"
        )));
    }
    Ok(())
}

async fn validate_projected_scope(
    tx: &mut PgConnection,
    table: &str,
    id_column: &str,
    aggregate_id: Uuid,
    data_profile_key: &str,
    requested_scope: &AuthorityScope,
    entity: &'static str,
) -> Result<(), PersistenceError> {
    let query = format!(
        "SELECT scope FROM erpx_holding.{table}
         WHERE {id_column}=$1 AND data_profile_key=$2 FOR SHARE"
    );
    let stored_scope = sqlx::query_scalar::<_, Value>(&query)
        .bind(aggregate_id)
        .bind(data_profile_key)
        .fetch_optional(&mut *tx)
        .await?;
    if let Some(stored_scope) = stored_scope {
        let stored_scope: AuthorityScope = serde_json::from_value(stored_scope)?;
        validate_immutable_scope(&stored_scope, requested_scope, entity)?;
    }
    Ok(())
}

fn validate_consent(consent: &ConsentRecord) -> Result<(), PersistenceError> {
    consent
        .validate()
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
    validate_holding_scope(&consent.scope)
}

fn validate_suppression(suppression: &SuppressionRecord) -> Result<(), PersistenceError> {
    suppression
        .validate()
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
    validate_holding_scope(&suppression.scope)
}

fn validate_audience_membership(membership: &AudienceMembership) -> Result<(), PersistenceError> {
    membership
        .validate()
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
    validate_holding_scope(&membership.scope)
}

fn validate_marketing_aggregate_type(value: &str) -> Result<(), PersistenceError> {
    if matches!(
        value,
        "marketing_profile"
            | "marketing_consent"
            | "marketing_suppression"
            | "marketing_audience"
            | CRM_HANDOFF_AGGREGATE
            | CRM_OPPORTUNITY_AGGREGATE
            | CRM_ATTRIBUTION_AGGREGATE
            | MARKETING_JOURNEY_CONVERSION_AGGREGATE
            | MARKETING_JOURNEY_OWNER_NOTIFICATION_AGGREGATE
    ) {
        Ok(())
    } else {
        Err(PersistenceError::InvalidData(
            "unsupported Marketing aggregate type".into(),
        ))
    }
}

#[derive(Debug, Deserialize)]
struct CrmAttributionEvent {
    record_id: Uuid,
    opportunity_id: Uuid,
    source_opportunity_version: u64,
    model: AttributionModel,
    result: AttributionResult,
}

fn is_crm_aggregate_type(value: &str) -> bool {
    matches!(
        value,
        CRM_HANDOFF_AGGREGATE | CRM_OPPORTUNITY_AGGREGATE | CRM_ATTRIBUTION_AGGREGATE
    )
}

async fn crm_projection_version(
    tx: &mut PgConnection,
    data_profile_key: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
) -> Result<Option<u64>, PersistenceError> {
    let (table, id_column) = match aggregate_type {
        CRM_HANDOFF_AGGREGATE => ("crm_accepted_lead_handoff_read_rows", "handoff_id"),
        CRM_OPPORTUNITY_AGGREGATE => ("crm_opportunity_read_rows", "opportunity_id"),
        CRM_ATTRIBUTION_AGGREGATE => ("crm_attribution_read_rows", "record_id"),
        _ => {
            return Err(PersistenceError::InvalidData(
                "unsupported CRM aggregate".into(),
            ));
        }
    };
    let query = format!(
        "SELECT source_version FROM erpx_holding.{table}
         WHERE data_profile_key=$1 AND {id_column}=$2 FOR SHARE"
    );
    sqlx::query_scalar::<_, i64>(&query)
        .bind(data_profile_key)
        .bind(aggregate_id)
        .fetch_optional(&mut *tx)
        .await?
        .map(|value| as_u64(value, "CRM projection source version"))
        .transpose()
}

fn crm_scope_columns(
    scope: &AuthorityScope,
) -> Result<(Uuid, Uuid, Option<Uuid>), PersistenceError> {
    match scope {
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => Ok((*holding_id, *company_id, None)),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok((*holding_id, *company_id, Some(*branch_id))),
        _ => Err(PersistenceError::InvalidData(
            "CRM projection scope must be company or branch".into(),
        )),
    }
}

fn crm_opportunity_status(status: erpx_domain::marketing_crm::OpportunityStatus) -> &'static str {
    match status {
        erpx_domain::marketing_crm::OpportunityStatus::Draft => "draft",
        erpx_domain::marketing_crm::OpportunityStatus::Qualified => "qualified",
        erpx_domain::marketing_crm::OpportunityStatus::Won => "won",
        erpx_domain::marketing_crm::OpportunityStatus::Lost => "lost",
    }
}

fn crm_attribution_model(model: AttributionModel) -> &'static str {
    match model {
        AttributionModel::FirstTouch => "first_touch",
        AttributionModel::LastTouch => "last_touch",
        AttributionModel::Linear => "linear",
        AttributionModel::UShaped => "u_shaped",
        AttributionModel::Weighted => "weighted",
    }
}

fn crm_event_type(aggregate_type: &str) -> &'static str {
    match aggregate_type {
        CRM_HANDOFF_AGGREGATE => "marketing.crm.accepted_lead_handoff_saved.v1",
        CRM_OPPORTUNITY_AGGREGATE => "marketing.crm.opportunity_saved.v1",
        CRM_ATTRIBUTION_AGGREGATE => "marketing.crm.attribution_saved.v1",
        _ => "unknown",
    }
}

fn marketing_crm_domain_error(
    error: erpx_domain::marketing_crm::MarketingCrmError,
) -> PersistenceError {
    PersistenceError::InvalidData(error.to_string())
}

fn marketing_journey_domain_error(
    error: erpx_domain::marketing_journey::MarketingJourneyError,
) -> PersistenceError {
    PersistenceError::InvalidData(error.to_string())
}

fn marketing_domain_error(error: erpx_domain::marketing::MarketingError) -> PersistenceError {
    PersistenceError::InvalidData(error.to_string())
}

fn validate_holding_scope(scope: &AuthorityScope) -> Result<(), PersistenceError> {
    let valid = match scope {
        AuthorityScope::Platform => false,
        AuthorityScope::Holding { holding_id }
        | AuthorityScope::Company { holding_id, .. }
        | AuthorityScope::Branch { holding_id, .. }
        | AuthorityScope::Subtree { holding_id, .. } => {
            !holding_id.is_nil()
                && match scope {
                    AuthorityScope::Company { company_id, .. } => !company_id.is_nil(),
                    AuthorityScope::Branch {
                        company_id,
                        branch_id,
                        ..
                    } => !company_id.is_nil() && !branch_id.is_nil(),
                    AuthorityScope::Subtree {
                        organization_node_id,
                        ..
                    } => !organization_node_id.is_nil(),
                    _ => true,
                }
        }
    };
    if valid {
        Ok(())
    } else {
        Err(PersistenceError::InvalidData(
            "marketing payload must use a non-platform holding scope".into(),
        ))
    }
}

/// Validate the immutable source envelope before reconciliation compares it
/// with a disposable read model. Reconciliation must fail closed for a
/// deleted intermediate event, a duplicate version, a forged aggregate
/// identity, or an event type that the Marketing projection does not own.
fn validate_marketing_source_event(
    aggregate_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
    event_type: &str,
    payload: &Value,
) -> Result<(), PersistenceError> {
    validate_uuid(aggregate_id, "Marketing source aggregate ID")?;
    if aggregate_version == 0
        || payload.get("aggregate_type").and_then(Value::as_str) != Some(aggregate_type)
        || payload.get("version").and_then(Value::as_u64) != Some(aggregate_version)
        || payload.get("event_type").and_then(Value::as_str) != Some(event_type)
    {
        return Err(PersistenceError::InvalidData(
            "Marketing source event envelope is invalid".into(),
        ));
    }
    let aggregate = payload.get("aggregate").ok_or_else(|| {
        PersistenceError::InvalidData("Marketing source aggregate is missing".into())
    })?;
    let (expected_event, identity) = match aggregate_type {
        "marketing_profile" => {
            let privacy_redacted = payload
                .get("privacy_redacted")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if privacy_redacted {
                if !matches!(
                    event_type,
                    "marketing.profile_saved.v1" | "marketing.profile_erased.v1"
                ) {
                    return Err(PersistenceError::InvalidData(
                        "Marketing redacted profile event type is invalid".into(),
                    ));
                }
                ("", aggregate.get("profile_id").and_then(Value::as_str))
            } else if event_type == "marketing.profile_erased.v1" {
                (
                    "marketing.profile_erased.v1",
                    aggregate.get("profile_id").and_then(Value::as_str),
                )
            } else {
                (
                    "marketing.profile_saved.v1",
                    aggregate.get("id").and_then(Value::as_str),
                )
            }
        }
        "marketing_consent" => (
            "marketing.consent_saved.v1",
            aggregate.get("id").and_then(Value::as_str),
        ),
        "marketing_suppression" => (
            "marketing.suppression_saved.v1",
            aggregate.get("id").and_then(Value::as_str),
        ),
        "marketing_audience" => (
            "marketing.audience_saved.v1",
            aggregate.get("audience_id").and_then(Value::as_str),
        ),
        "marketing_crm_accepted_lead_handoff" => (
            "marketing.crm.accepted_lead_handoff_saved.v1",
            aggregate.get("id").and_then(Value::as_str),
        ),
        "marketing_crm_opportunity" => (
            "marketing.crm.opportunity_saved.v1",
            aggregate.get("id").and_then(Value::as_str),
        ),
        "marketing_crm_attribution" => (
            "marketing.crm.attribution_saved.v1",
            aggregate.get("record_id").and_then(Value::as_str),
        ),
        MARKETING_JOURNEY_CONVERSION_AGGREGATE => (
            MARKETING_JOURNEY_CONVERSION_EVENT,
            aggregate.get("conversion_ref").and_then(Value::as_str),
        ),
        MARKETING_JOURNEY_OWNER_NOTIFICATION_AGGREGATE => (
            MARKETING_JOURNEY_OWNER_NOTIFICATION_EVENT,
            aggregate.get("notification_ref").and_then(Value::as_str),
        ),
        _ => {
            return Err(PersistenceError::InvalidData(
                "Marketing source aggregate type is unsupported".into(),
            ));
        }
    };
    if !expected_event.is_empty() && event_type != expected_event {
        return Err(PersistenceError::InvalidData(
            "Marketing source event type is invalid".into(),
        ));
    }
    let identity = identity.ok_or_else(|| {
        PersistenceError::InvalidData("Marketing source aggregate identity is missing".into())
    })?;
    let identity = Uuid::parse_str(identity).map_err(|_| {
        PersistenceError::InvalidData("Marketing source aggregate identity is invalid".into())
    })?;
    if identity != aggregate_id {
        return Err(PersistenceError::InvalidData(
            "Marketing source aggregate identity does not match its event".into(),
        ));
    }
    Ok(())
}

fn validate_data_profile_key(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value.chars().any(|character| character.is_control())
    {
        return Err(PersistenceError::Configuration(
            "holding reference must be bounded and non-empty",
        ));
    }
    Ok(())
}

fn validate_idempotency_key(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
        return Err(PersistenceError::InvalidData(
            "marketing idempotency key must be bounded and non-empty".into(),
        ));
    }
    Ok(())
}

fn validate_uuid(value: Uuid, field: &'static str) -> Result<(), PersistenceError> {
    if value.is_nil() {
        return Err(PersistenceError::InvalidData(format!(
            "{field} cannot be nil"
        )));
    }
    Ok(())
}

async fn begin_scoped<'a>(
    pool: &'a PgPool,
    data_profile_key: &str,
) -> Result<Transaction<'a, Postgres>, PersistenceError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(data_profile_key)
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

fn hash_operation(operation: &str, payload: &Value) -> Result<String, PersistenceError> {
    let request = serde_json::to_vec(&(operation, payload))?;
    Ok(hex::encode(Sha256::digest(request)))
}

fn hash_value(payload: &Value) -> Result<String, PersistenceError> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(payload)?)))
}

fn as_i64(value: u64, field: &'static str) -> Result<i64, PersistenceError> {
    i64::try_from(value)
        .map_err(|_| PersistenceError::InvalidData(format!("{field} exceeds BIGINT")))
}

fn as_u64(value: i64, field: &'static str) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| PersistenceError::InvalidData(format!("{field} is negative")))
}

fn purpose_text(value: MarketingPurpose) -> &'static str {
    match value {
        MarketingPurpose::Newsletter => "newsletter",
        MarketingPurpose::ProductUpdates => "product_updates",
        MarketingPurpose::Promotional => "promotional",
        MarketingPurpose::Transactional => "transactional",
        MarketingPurpose::LeadNurture => "lead_nurture",
    }
}

fn channel_text(value: MarketingChannel) -> &'static str {
    match value {
        MarketingChannel::Email => "email",
        MarketingChannel::Sms => "sms",
        MarketingChannel::Social => "social",
        MarketingChannel::Push => "push",
        MarketingChannel::Webhook => "webhook",
    }
}

fn consent_state_text(value: ConsentState) -> &'static str {
    match value {
        ConsentState::PendingDoubleOptIn => "pending_double_opt_in",
        ConsentState::Granted => "granted",
        ConsentState::Revoked => "revoked",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use chrono::Duration;

    fn scope() -> AuthorityScope {
        AuthorityScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        }
    }

    #[test]
    fn canonical_profile_hash_and_holding_scope_are_enforced_without_a_pool() {
        let profile = MarketingProfile::new(
            Uuid::from_u128(10),
            scope(),
            "crm-10",
            Some("person@example.test".into()),
            None,
            "th-TH",
            BTreeMap::new(),
            Utc::now(),
        )
        .expect("profile");
        assert!(validate_profile(&profile).is_ok());
        let mut invalid = profile.clone();
        invalid.dedupe_key = "not-canonical".into();
        assert!(validate_profile(&invalid).is_err());
        assert!(validate_holding_scope(&AuthorityScope::Platform).is_err());
    }

    #[test]
    fn profile_scope_is_immutable_after_identity_creation() {
        let holding_id = Uuid::from_u128(1);
        let stored = AuthorityScope::Company {
            holding_id,
            company_id: Uuid::from_u128(2),
        };
        let other_company = AuthorityScope::Company {
            holding_id,
            company_id: Uuid::from_u128(3),
        };

        assert!(validate_immutable_scope(&stored, &stored, "marketing profile").is_ok());
        assert!(validate_immutable_scope(&stored, &other_company, "marketing profile").is_err());
    }

    #[test]
    fn mutation_hash_binds_operation_and_payload() {
        let payload = serde_json::json!({"consent_id": Uuid::from_u128(30)});
        let append = hash_operation("append_consent", &payload).expect("hash");
        let revoke = hash_operation("revoke_consent", &payload).expect("hash");
        assert_ne!(append, revoke);
        assert_eq!(append.len(), 64);
        assert!(validate_idempotency_key("marketing-30").is_ok());
        assert!(validate_idempotency_key("\n").is_err());
    }

    #[test]
    fn consent_expiry_and_audience_bounds_are_delegated_to_domain_validation() {
        let now = Utc::now();
        let consent = ConsentRecord {
            id: Uuid::from_u128(30),
            profile_id: Uuid::from_u128(10),
            scope: scope(),
            purpose: MarketingPurpose::Newsletter,
            channel: MarketingChannel::Email,
            state: ConsentState::Granted,
            source: "double_opt_in".into(),
            jurisdiction: "TH".into(),
            policy_version: "2026-01".into(),
            proof_ref: Some("proof-30".into()),
            recorded_at: now,
            expires_at: Some(now - Duration::seconds(1)),
        };
        assert!(validate_consent(&consent).is_err());
        assert!(validate_idempotency_key("marketing-30").is_ok());
    }

    #[test]
    fn crm_event_allow_list_and_receipt_hash_are_deterministic() {
        for aggregate in [
            CRM_HANDOFF_AGGREGATE,
            CRM_OPPORTUNITY_AGGREGATE,
            CRM_ATTRIBUTION_AGGREGATE,
        ] {
            assert!(validate_marketing_aggregate_type(aggregate).is_ok());
            assert!(is_crm_aggregate_type(aggregate));
            assert_ne!(crm_event_type(aggregate), "unknown");
        }
        assert!(!is_crm_aggregate_type("marketing_profile"));
        let payload = serde_json::json!({"version": 1, "aggregate": {"id": 10}});
        let first = hash_value(&payload).expect("hash");
        let second = hash_value(&payload).expect("same hash");
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn crm_attribution_event_payload_decodes_and_preserves_model() {
        let payload = serde_json::json!({
            "record_id": Uuid::from_u128(40),
            "opportunity_id": Uuid::from_u128(41),
            "source_opportunity_version": 2,
            "model": "linear",
            "result": {
                "model": "linear",
                "entries": [],
                "total_revenue_minor": 0,
                "total_cost_minor": 0
            }
        });
        let decoded: CrmAttributionEvent = serde_json::from_value(payload).expect("CRM event");
        assert_eq!(decoded.model, AttributionModel::Linear);
        assert_eq!(decoded.source_opportunity_version, 2);
    }

    #[test]
    fn source_reconciliation_rejects_forged_identity_and_event_shape() {
        let profile_id = Uuid::from_u128(50);
        let valid = serde_json::json!({
            "aggregate_type": "marketing_profile",
            "aggregate": { "id": profile_id },
            "version": 1,
            "event_type": "marketing.profile_saved.v1"
        });
        assert!(
            validate_marketing_source_event(
                "marketing_profile",
                profile_id,
                1,
                "marketing.profile_saved.v1",
                &valid,
            )
            .is_ok()
        );

        let mut forged = valid.clone();
        forged["aggregate"]["id"] = serde_json::json!(Uuid::from_u128(51));
        assert!(
            validate_marketing_source_event(
                "marketing_profile",
                profile_id,
                1,
                "marketing.profile_saved.v1",
                &forged,
            )
            .is_err()
        );

        assert!(
            validate_marketing_source_event(
                "marketing_profile",
                profile_id,
                1,
                "marketing.profile_erased.v1",
                &valid,
            )
            .is_err()
        );
    }

    #[test]
    fn source_reconciliation_accepts_privacy_redacted_profile_tombstones() {
        let profile_id = Uuid::from_u128(52);
        let payload = serde_json::json!({
            "aggregate_type": "marketing_profile",
            "aggregate": { "profile_id": profile_id, "scope": scope() },
            "version": 2,
            "event_type": "marketing.profile_erased.v1",
            "privacy_redacted": true
        });
        assert!(
            validate_marketing_source_event(
                "marketing_profile",
                profile_id,
                2,
                "marketing.profile_erased.v1",
                &payload,
            )
            .is_ok()
        );
        let historical = serde_json::json!({
            "aggregate_type": "marketing_profile",
            "aggregate": { "profile_id": profile_id, "scope": scope() },
            "version": 1,
            "event_type": "marketing.profile_saved.v1",
            "privacy_redacted": true
        });
        assert!(
            validate_marketing_source_event(
                "marketing_profile",
                profile_id,
                1,
                "marketing.profile_saved.v1",
                &historical,
            )
            .is_ok()
        );
    }
}
