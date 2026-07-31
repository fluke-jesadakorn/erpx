//! Durable control-plane persistence for role-review snapshots and recovery.
//!
//! Role assignments remain authoritative. Review rows are immutable, derived
//! snapshots used by bounded worker/UI reads; recovery receipts are an audit
//! record for the separate role-assignment command.

use chrono::{DateTime, Duration, Utc};
use erpx_domain::access::{
    RoleAssignment, RoleExpiryNotificationKind, RoleRecoveryRequest, RoleRecoveryTarget,
    RoleReviewClassification, RoleReviewQueue, Scope, evaluate_approver_scope_intersection,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::collections::HashSet;
use thiserror::Error;
use uuid::Uuid;

const MAX_REVIEW_ITEMS: usize = 1_000;
const MAX_NOTIFICATION_CLAIM: i64 = 100;
const MAX_REVIEW_HORIZON: Duration = Duration::days(365);
const MAX_NOTIFICATION_LEASE: Duration = Duration::hours(1);
const MAX_REFERENCE_LENGTH: usize = 255;
const MAX_AUTHORITY_ASSIGNMENTS: i64 = 128;
pub const ROLE_EXPIRY_NOTIFICATION_EVENT_TYPE: &str = "role_expiry_notification.requested";
const ROLE_EXPIRY_NOTIFICATION_AGGREGATE_TYPE: &str = "role_expiry_notification";

#[derive(Debug, Error)]
pub enum RoleReviewPersistenceError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("invalid role-review value: {0}")]
    InvalidData(String),
    #[error("role-review idempotency key was reused with a different request")]
    IdempotencyConflict,
    #[error("role-review snapshot contains too many items")]
    TooManyItems,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleReviewSnapshotReceipt {
    pub snapshot_key: String,
    pub snapshot_version: u64,
    pub item_count: usize,
    pub content_hash: String,
    pub reused_idempotency: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleRecoveryReceipt {
    pub request_id: Uuid,
    pub review_version: u64,
    pub reused_idempotency: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimedRoleExpiryNotification {
    pub notification_key: String,
    pub assignment_id: Uuid,
    pub principal_ref: String,
    pub valid_until: DateTime<Utc>,
    pub kind: RoleExpiryNotificationKind,
    pub claim_key: String,
}

#[derive(Debug, Clone)]
pub struct RoleReviewPersistence {
    write: PgPool,
}

impl RoleReviewPersistence {
    pub fn new(write: PgPool) -> Self {
        Self { write }
    }

    /// Persist one deterministic review result. The snapshot and all its
    /// notifications are committed atomically and can be replayed safely.
    pub async fn persist_snapshot(
        &self,
        scope: &Scope,
        queue: &RoleReviewQueue,
        actor_ref: &str,
        correlation_id: Uuid,
        idempotency_key: &str,
        request_hash: &str,
    ) -> Result<RoleReviewSnapshotReceipt, RoleReviewPersistenceError> {
        validate_reference(actor_ref, "actor")?;
        validate_reference(idempotency_key, "idempotency key")?;
        if correlation_id.is_nil() {
            return Err(RoleReviewPersistenceError::InvalidData(
                "correlation id is required".into(),
            ));
        }
        validate_sha256(request_hash)?;
        if queue.items.len() > MAX_REVIEW_ITEMS {
            return Err(RoleReviewPersistenceError::TooManyItems);
        }
        validate_review_queue(scope, queue)?;
        let content_hash = sha256_hex(&queue.canonical_bytes()?);
        let snapshot_key = format!(
            "role-review:{}:{}:{}",
            scope.canonical_key(),
            queue.evaluated_at.to_rfc3339(),
            content_hash
        );
        let parts = ScopeParts::from_scope(scope);
        let mut tx = self.write.begin().await?;

        // The idempotency table references the snapshot table, so reserve the
        // request under a transaction advisory lock and insert the snapshot
        // before its receipt. This also serializes concurrent first attempts
        // without weakening the database foreign key.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(idempotency_key)
            .execute(&mut *tx)
            .await?;

        if let Some(row) = sqlx::query(
            "SELECT i.request_hash, i.snapshot_key, i.snapshot_version, i.item_count,
                    s.content_hash, s.scope_key, s.actor_ref
             FROM erpx_control.role_review_snapshot_idempotency i
             JOIN erpx_control.role_review_snapshots s ON s.snapshot_key = i.snapshot_key
             WHERE i.idempotency_key = $1
             FOR UPDATE OF i",
        )
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            if row.try_get::<String, _>("request_hash")? != request_hash {
                return Err(RoleReviewPersistenceError::IdempotencyConflict);
            }
            if row.try_get::<String, _>("scope_key")? != scope.canonical_key()
                || row.try_get::<String, _>("actor_ref")? != actor_ref
            {
                return Err(RoleReviewPersistenceError::InvalidData(
                    "role-review snapshot replay actor or scope is not authoritative".into(),
                ));
            }
            tx.commit().await?;
            return Ok(RoleReviewSnapshotReceipt {
                snapshot_key: row.try_get("snapshot_key")?,
                snapshot_version: positive_u64(row.try_get("snapshot_version")?)?,
                item_count: usize::try_from(row.try_get::<i32, _>("item_count")?).map_err(
                    |_| RoleReviewPersistenceError::InvalidData("negative count".into()),
                )?,
                content_hash: row.try_get("content_hash")?,
                reused_idempotency: true,
            });
        }

        validate_authoritative_identity_scope(
            &mut tx,
            actor_ref,
            scope,
            queue.evaluated_at,
            "role-review snapshot actor",
            None,
        )
        .await?;

        sqlx::query(
            "INSERT INTO erpx_control.role_review_snapshots
                (snapshot_key, scope_key, scope_kind, holding_id, company_id, branch_id,
                 scope_company_ids, scope_branch_ids, evaluated_at, expiry_horizon_until,
                 snapshot_version, content_hash, item_count, actor_ref, correlation_id)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,1,$11,$12,$13,$14)
             ON CONFLICT (snapshot_key) DO NOTHING",
        )
        .bind(&snapshot_key)
        .bind(scope.canonical_key())
        .bind(parts.kind)
        .bind(parts.holding_id)
        .bind(parts.company_id)
        .bind(parts.branch_id)
        .bind(&parts.company_ids)
        .bind(&parts.branch_ids)
        .bind(queue.evaluated_at)
        .bind(queue.expiry_horizon_until)
        .bind(&content_hash)
        .bind(i32::try_from(queue.items.len()).map_err(|_| {
            RoleReviewPersistenceError::InvalidData("review item count exceeds INTEGER".into())
        })?)
        .bind(actor_ref)
        .bind(correlation_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO erpx_control.role_review_snapshot_idempotency
                (idempotency_key, snapshot_key, request_hash, snapshot_version, item_count)
             VALUES ($1, $2, $3, 1, $4)
             ON CONFLICT (idempotency_key) DO NOTHING",
        )
        .bind(idempotency_key)
        .bind(&snapshot_key)
        .bind(request_hash)
        .bind(i32::try_from(queue.items.len()).map_err(|_| {
            RoleReviewPersistenceError::InvalidData("review item count exceeds INTEGER".into())
        })?)
        .execute(&mut *tx)
        .await?;

        for item in &queue.items {
            let assignment = &item.assignment;
            let item_parts = ScopeParts::from_scope(&assignment.scope);
            let classifications = item
                .classifications
                .iter()
                .map(|classification| classification.code().to_owned())
                .collect::<Vec<_>>();
            let (notification_kind, notification_key) = item
                .notification
                .as_ref()
                .map(|notification| {
                    let kind = match notification.kind {
                        RoleExpiryNotificationKind::ExpiringSoon => "expiring_soon",
                        RoleExpiryNotificationKind::Expired => "expired",
                    };
                    (Some(kind), Some(notification.stable_key()))
                })
                .unwrap_or((None, None));

            sqlx::query(
                "INSERT INTO erpx_control.role_review_items
                    (item_key, snapshot_key, assignment_id, scope_kind, holding_id, company_id,
                     branch_id, scope_company_ids, scope_branch_ids, principal_ref, valid_from,
                     valid_until, parent_assignment_id, assignment_version, classifications,
                     notification_kind, notification_key, role_key, permissions, functional_domain,
                     authority_level, approval_limit_minor, timezone, risk_tier)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24)
                 ON CONFLICT (item_key) DO NOTHING",
            )
            .bind(format!("{}:{}", snapshot_key, item.assignment_id))
            .bind(&snapshot_key)
            .bind(item.assignment_id)
            .bind(item_parts.kind)
            .bind(item_parts.holding_id)
            .bind(item_parts.company_id)
            .bind(item_parts.branch_id)
            .bind(&item_parts.company_ids)
            .bind(&item_parts.branch_ids)
            .bind(&item.principal_ref)
            .bind(item.valid_from)
            .bind(item.valid_until)
            .bind(item.parent_assignment_id)
            .bind(i32::try_from(assignment.assignment_version).map_err(|_| {
                RoleReviewPersistenceError::InvalidData("assignment version exceeds INTEGER".into())
            })?)
            .bind(&classifications)
            .bind(notification_kind)
            .bind(notification_key.clone())
            .bind(&assignment.role_key)
            .bind(&assignment.permissions)
            .bind(&assignment.functional_domain)
            .bind(i16::from(assignment.authority_level))
            .bind(i64::try_from(assignment.approval_limit_minor).map_err(|_| {
                RoleReviewPersistenceError::InvalidData("approval limit exceeds BIGINT".into())
            })?)
            .bind(&assignment.timezone)
            .bind(risk_tier(assignment.risk_tier))
            .execute(&mut *tx)
            .await?;

            if let Some(notification) = &item.notification {
                let kind = match notification.kind {
                    RoleExpiryNotificationKind::ExpiringSoon => "expiring_soon",
                    RoleExpiryNotificationKind::Expired => "expired",
                };
                sqlx::query(
                    "INSERT INTO erpx_control.role_expiry_notifications
                        (notification_key, snapshot_key, assignment_id, scope_kind, holding_id,
                         company_id, branch_id, scope_company_ids, scope_branch_ids, principal_ref,
                         valid_until, notification_kind, status)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,'pending')
                     ON CONFLICT (notification_key) DO NOTHING",
                )
                .bind(notification.stable_key())
                .bind(&snapshot_key)
                .bind(notification.assignment_id)
                .bind(item_parts.kind)
                .bind(item_parts.holding_id)
                .bind(item_parts.company_id)
                .bind(item_parts.branch_id)
                .bind(&item_parts.company_ids)
                .bind(&item_parts.branch_ids)
                .bind(&notification.principal_ref)
                .bind(notification.valid_until)
                .bind(kind)
                .execute(&mut *tx)
                .await?;
            }
        }
        tx.commit().await?;
        Ok(RoleReviewSnapshotReceipt {
            snapshot_key,
            snapshot_version: 1,
            item_count: queue.items.len(),
            content_hash,
            reused_idempotency: false,
        })
    }

    /// Claim only notifications in one exact scope. `SKIP LOCKED` makes the
    /// worker safe for multiple instances; the lease fields are the handoff
    /// boundary for retry/reconciliation.
    pub async fn claim_expiry_notifications(
        &self,
        scope: &Scope,
        claimant_ref: &str,
        at: DateTime<Utc>,
        horizon: Duration,
        lease: Duration,
    ) -> Result<Vec<ClaimedRoleExpiryNotification>, RoleReviewPersistenceError> {
        if claimant_ref.trim().is_empty()
            || horizon <= Duration::zero()
            || horizon > MAX_REVIEW_HORIZON
            || lease <= Duration::zero()
            || lease > MAX_NOTIFICATION_LEASE
        {
            return Err(RoleReviewPersistenceError::InvalidData(
                "claimant, horizon, and lease are required".into(),
            ));
        }
        let parts = ScopeParts::from_scope(scope);
        let claim_key = format!("role-review-claim:{}", Uuid::new_v4());
        let mut tx = self.write.begin().await?;
        let rows = sqlx::query(
            "SELECT notification_key, assignment_id, principal_ref, valid_until, notification_kind
             FROM erpx_control.role_expiry_notifications
             WHERE scope_kind = $1
               AND holding_id IS NOT DISTINCT FROM $2
               AND company_id IS NOT DISTINCT FROM $3
               AND branch_id IS NOT DISTINCT FROM $4
               AND scope_company_ids = $5
               AND scope_branch_ids = $6
               AND (status = 'pending'
                    OR (status = 'claimed' AND lease_until IS NOT NULL AND lease_until <= $7))
               AND valid_until <= $8
             ORDER BY valid_until, notification_key
             FOR UPDATE SKIP LOCKED
             LIMIT $9",
        )
        .bind(parts.kind)
        .bind(parts.holding_id)
        .bind(parts.company_id)
        .bind(parts.branch_id)
        .bind(&parts.company_ids)
        .bind(&parts.branch_ids)
        .bind(at)
        .bind(at + horizon)
        .bind(MAX_NOTIFICATION_CLAIM)
        .fetch_all(&mut *tx)
        .await?;
        let lease_until = at + lease;
        let claimed_by = Uuid::new_v4();
        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            let key: String = row.try_get("notification_key")?;
            sqlx::query(
                "UPDATE erpx_control.role_expiry_notifications
                 SET status='claimed', claim_key=$2, claimed_by=$3, claimed_at=$4,
                     lease_until=$5, attempt_count=attempt_count+1, version=version+1
                 WHERE notification_key=$1
                   AND (status='pending'
                        OR (status='claimed' AND lease_until IS NOT NULL AND lease_until <= $6))",
            )
            .bind(&key)
            .bind(&claim_key)
            .bind(claimed_by)
            .bind(at)
            .bind(lease_until)
            .bind(at)
            .execute(&mut *tx)
            .await?;
            result.push(ClaimedRoleExpiryNotification {
                notification_key: key,
                assignment_id: row.try_get("assignment_id")?,
                principal_ref: row.try_get("principal_ref")?,
                valid_until: row.try_get("valid_until")?,
                kind: parse_notification_kind(
                    row.try_get::<String, _>("notification_kind")?.as_str(),
                )?,
                claim_key: claim_key.clone(),
            });
        }
        if !result.is_empty() {
            let notification_keys = result
                .iter()
                .map(|notification| notification.notification_key.clone())
                .collect::<Vec<_>>();
            sqlx::query(
                "INSERT INTO erpx_control.role_review_notification_claims
                    (claim_key, scope_key, scope_kind, holding_id, company_id, branch_id,
                     scope_company_ids, scope_branch_ids, claimant_ref, requested_limit,
                     notification_keys, claimed_at, lease_until)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
            )
            .bind(&claim_key)
            .bind(scope.canonical_key())
            .bind(parts.kind)
            .bind(parts.holding_id)
            .bind(parts.company_id)
            .bind(parts.branch_id)
            .bind(&parts.company_ids)
            .bind(&parts.branch_ids)
            .bind(claimant_ref)
            .bind(i32::try_from(result.len()).map_err(|_| {
                RoleReviewPersistenceError::InvalidData("claim count exceeds INTEGER".into())
            })?)
            .bind(notification_keys)
            .bind(at)
            .bind(lease_until)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(result)
    }

    /// Mark successfully delivered notifications as terminal. The claim key
    /// is required so a late worker cannot reconcile another worker's lease.
    pub async fn reconcile_expiry_notifications(
        &self,
        claim_key: &str,
        notification_keys: &[String],
        at: DateTime<Utc>,
    ) -> Result<u64, RoleReviewPersistenceError> {
        validate_claim_keys(claim_key, notification_keys)?;
        let unique_keys = notification_keys.iter().collect::<HashSet<_>>();
        if unique_keys.len() != notification_keys.len() {
            return Err(RoleReviewPersistenceError::InvalidData(
                "role-review notification keys must be unique".into(),
            ));
        }
        let mut tx = self.write.begin().await?;
        let claim_keys: Vec<String> = sqlx::query_scalar(
            "SELECT notification_keys
             FROM erpx_control.role_review_notification_claims
             WHERE claim_key=$1
             FOR UPDATE",
        )
        .bind(claim_key)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| {
            RoleReviewPersistenceError::InvalidData(
                "role-review notification claim does not exist".into(),
            )
        })?;
        let claim_key_set = claim_keys.iter().collect::<HashSet<_>>();
        if notification_keys
            .iter()
            .any(|notification_key| !claim_key_set.contains(notification_key))
        {
            return Err(RoleReviewPersistenceError::InvalidData(
                "role-review notification is outside its durable claim".into(),
            ));
        }
        let result = sqlx::query(
            "UPDATE erpx_control.role_expiry_notifications
             SET status='reconciled', reconciled_at=$3, claimed_by=NULL,
                 claimed_at=NULL, lease_until=NULL, version=version+1
             WHERE claim_key=$1 AND notification_key = ANY($2)
               AND status='claimed' AND lease_until > $3",
        )
        .bind(claim_key)
        .bind(notification_keys)
        .bind(at)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE erpx_control.role_review_notification_claims
             SET reconciled_count = reconciled_count + $2
             WHERE claim_key=$1",
        )
        .bind(claim_key)
        .bind(i32::try_from(result.rows_affected()).map_err(|_| {
            RoleReviewPersistenceError::InvalidData("reconciled count exceeds INTEGER".into())
        })?)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(result.rows_affected())
    }

    /// Return undelivered notifications to the pending queue after a
    /// publisher failure. Claim identity prevents another worker from being
    /// released accidentally.
    pub async fn release_expiry_notifications(
        &self,
        claim_key: &str,
        notification_keys: &[String],
    ) -> Result<u64, RoleReviewPersistenceError> {
        validate_claim_keys(claim_key, notification_keys)?;
        let result = sqlx::query(
            "UPDATE erpx_control.role_expiry_notifications
             SET status='pending', claim_key=NULL, claimed_by=NULL,
                 claimed_at=NULL, lease_until=NULL, version=version+1
             WHERE claim_key=$1 AND notification_key = ANY($2)
               AND status='claimed'",
        )
        .bind(claim_key)
        .bind(notification_keys)
        .execute(&self.write)
        .await?;
        Ok(result.rows_affected())
    }

    /// Append one idempotent notification request to the control-plane outbox.
    ///
    /// This is the worker's durable publication boundary. It records only the
    /// validated role-review identity and expiry metadata; an external delivery
    /// adapter must consume the outbox event and report its own provider result.
    /// Replaying a claimed notification therefore cannot create a second
    /// request or turn an outbox append into a claim of external delivery.
    pub async fn append_expiry_notification_event(
        &self,
        notification: &ClaimedRoleExpiryNotification,
        actor_ref: &str,
    ) -> Result<bool, RoleReviewPersistenceError> {
        validate_reference(actor_ref, "notification actor")?;
        validate_reference(&notification.notification_key, "notification key")?;
        validate_reference(&notification.principal_ref, "notification principal")?;
        if notification.assignment_id.is_nil() || notification.claim_key.trim().is_empty() {
            return Err(RoleReviewPersistenceError::InvalidData(
                "notification identity is required".into(),
            ));
        }
        let idempotency_key = format!("role-expiry-notification:{}", notification.notification_key);
        let event_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, idempotency_key.as_bytes());
        let correlation_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("role-expiry-correlation:{idempotency_key}").as_bytes(),
        );
        let mut tx = self.write.begin().await?;
        let source = sqlx::query(
            "SELECT assignment_id, principal_ref, valid_until, notification_kind,
                    status, claim_key, scope_kind, holding_id, company_id, branch_id,
                    scope_company_ids, scope_branch_ids
             FROM erpx_control.role_expiry_notifications
             WHERE notification_key = $1
             FOR UPDATE",
        )
        .bind(&notification.notification_key)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| {
            RoleReviewPersistenceError::InvalidData("notification source is not available".into())
        })?;
        let source_assignment_id: Uuid = source.try_get("assignment_id")?;
        let source_principal_ref: String = source.try_get("principal_ref")?;
        let source_valid_until: DateTime<Utc> = source.try_get("valid_until")?;
        let source_kind_value: String = source.try_get("notification_kind")?;
        let source_kind = parse_notification_kind(&source_kind_value)?;
        let source_status: String = source.try_get("status")?;
        let source_claim_key: Option<String> = source.try_get("claim_key")?;
        validate_authoritative_notification_source(
            notification,
            &source_status,
            source_claim_key.as_deref(),
            source_assignment_id,
            &source_principal_ref,
            source_valid_until,
            source_kind,
        )?;
        let claim = sqlx::query(
            "SELECT claim_key, scope_kind, holding_id, company_id, branch_id,
                    scope_company_ids, scope_branch_ids
             FROM erpx_control.role_review_notification_claims
             WHERE claim_key = $1
               AND notification_keys @> ARRAY[$2]::TEXT[]
               AND lease_until > now()
             FOR UPDATE",
        )
        .bind(&notification.claim_key)
        .bind(&notification.notification_key)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| {
            RoleReviewPersistenceError::InvalidData("notification claim is not active".into())
        })?;
        let claim_scope = scope_from_columns(
            claim.try_get("scope_kind")?,
            claim.try_get("holding_id")?,
            claim.try_get("company_id")?,
            claim.try_get("branch_id")?,
            claim.try_get("scope_company_ids")?,
            claim.try_get("scope_branch_ids")?,
        )?;
        let source_scope = scope_from_columns(
            source.try_get("scope_kind")?,
            source.try_get("holding_id")?,
            source.try_get("company_id")?,
            source.try_get("branch_id")?,
            source.try_get("scope_company_ids")?,
            source.try_get("scope_branch_ids")?,
        )?;
        if !claim_scope.contains(&source_scope) {
            return Err(RoleReviewPersistenceError::InvalidData(
                "notification source is outside its durable claim scope".into(),
            ));
        }
        validate_authoritative_identity_scope(
            &mut tx,
            actor_ref,
            &claim_scope,
            Utc::now(),
            "role-review notification actor",
            None,
        )
        .await?;
        let kind = match source_kind {
            RoleExpiryNotificationKind::ExpiringSoon => "expiring_soon",
            RoleExpiryNotificationKind::Expired => "expired",
        };
        let payload = serde_json::json!({
            "notification_key": notification.notification_key,
            "assignment_id": notification.assignment_id,
            "principal_ref": source_principal_ref,
            "valid_until": source_valid_until,
            "kind": kind,
        });
        let result = sqlx::query(
            "INSERT INTO erpx_control.outbox_events
                (id, event_type, event_version, aggregate_type, aggregate_id,
                 aggregate_version, tenant_scope, actor, correlation_id,
                 idempotency_key, payload, occurred_at, available_at)
             VALUES ($1, $2, 1, $3, $4, 1, NULL, $5, $6, $7, $8, now(), now())
             ON CONFLICT (idempotency_key) DO NOTHING",
        )
        .bind(event_id)
        .bind(ROLE_EXPIRY_NOTIFICATION_EVENT_TYPE)
        .bind(ROLE_EXPIRY_NOTIFICATION_AGGREGATE_TYPE)
        .bind(notification.assignment_id)
        .bind(serde_json::json!({ "actor_ref": actor_ref }))
        .bind(correlation_id)
        .bind(&idempotency_key)
        .bind(payload)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() == 0 {
            let stored_actor: Value = sqlx::query_scalar(
                "SELECT actor
                 FROM erpx_control.outbox_events
                 WHERE idempotency_key=$1
                 FOR SHARE",
            )
            .bind(&idempotency_key)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| {
                RoleReviewPersistenceError::InvalidData(
                    "role-review notification replay lost its outbox event".into(),
                )
            })?;
            if stored_actor.get("actor_ref") != Some(&Value::String(actor_ref.into())) {
                return Err(RoleReviewPersistenceError::InvalidData(
                    "role-review notification replay actor is not authoritative".into(),
                ));
            }
        }
        tx.commit().await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn record_recovery_receipt(
        &self,
        request: &RoleRecoveryRequest,
        assignment_version: u64,
        decision: &str,
        idempotency_key: &str,
        actor_ref: &str,
        correlation_id: Uuid,
    ) -> Result<RoleRecoveryReceipt, RoleReviewPersistenceError> {
        if decision != "approved" && decision != "rejected" {
            return Err(RoleReviewPersistenceError::InvalidData(
                "recovery decision must be approved or rejected".into(),
            ));
        }
        validate_reference(actor_ref, "recovery actor")?;
        validate_reference(idempotency_key, "recovery idempotency key")?;
        if correlation_id.is_nil() {
            return Err(RoleReviewPersistenceError::InvalidData(
                "recovery correlation id is required".into(),
            ));
        }
        if assignment_version == 0 {
            return Err(RoleReviewPersistenceError::InvalidData(
                "recovery assignment version must be positive".into(),
            ));
        }
        let request = canonical_recovery_request(request)?;
        validate_recovery_identity_binding(&request, actor_ref)?;
        let recovery_assignment = request
            .materialize(&[], request.valid_from)
            .map_err(|error| RoleReviewPersistenceError::InvalidData(error.to_string()))?;
        if decision == "approved" {
            if assignment_version != 1 {
                return Err(RoleReviewPersistenceError::InvalidData(
                    "approved recovery assignment version must be one".into(),
                ));
            }
        } else {
            validate_recovery_shape(&request)?;
        }
        let target = match &request.target {
            erpx_domain::access::RoleRecoveryTarget::PlatformRoot => ("platform_root", None),
            erpx_domain::access::RoleRecoveryTarget::HoldingOwner { holding_id } => {
                ("holding_owner", Some(*holding_id))
            }
        };
        let reason_hash = sha256_hex(request.grant_reason.as_bytes());
        let ticket_reference_hash = request
            .ticket_reference
            .as_deref()
            .map(|value| sha256_hex(value.as_bytes()));
        let request_hash = sha256_hex(&serde_json::to_vec(&(
            &request,
            assignment_version,
            decision,
        ))?);
        let scope = match &request.target {
            erpx_domain::access::RoleRecoveryTarget::PlatformRoot => Scope::Platform,
            erpx_domain::access::RoleRecoveryTarget::HoldingOwner { holding_id } => {
                Scope::Holding {
                    holding_id: *holding_id,
                }
            }
        };
        let parts = ScopeParts::from_scope(&scope);
        let mut tx = self.write.begin().await?;
        // Serialize review-version allocation for one recovery request. The
        // lock is scoped to this transaction and contains no customer data.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(request.request_id.to_string())
            .execute(&mut *tx)
            .await?;

        if let Some(row) = sqlx::query(
            "SELECT request_id, review_version, request_hash
             FROM erpx_control.role_recovery_review_receipts
             WHERE idempotency_key = $1
             FOR UPDATE",
        )
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let stored_hash: String = row.try_get("request_hash")?;
            if stored_hash != request_hash {
                return Err(RoleReviewPersistenceError::IdempotencyConflict);
            }
            let review_version = positive_u64(row.try_get("review_version")?)?;
            tx.commit().await?;
            return Ok(RoleRecoveryReceipt {
                request_id: row.try_get("request_id")?,
                review_version,
                reused_idempotency: true,
            });
        }

        let approver_roles: &[&str] = match request.target {
            RoleRecoveryTarget::PlatformRoot => &["platform_root"],
            RoleRecoveryTarget::HoldingOwner { .. } => &["platform_root", "holding_owner"],
        };
        validate_authoritative_identity_scope(
            &mut tx,
            &request.approver_ref,
            &scope,
            request.valid_from,
            "recovery approver",
            Some(approver_roles),
        )
        .await?;
        validate_recovery_approver_intersection(
            &mut tx,
            &recovery_assignment,
            request.valid_from,
            approver_roles,
        )
        .await?;

        let review_version = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(review_version), 0) + 1
             FROM erpx_control.role_recovery_review_receipts
             WHERE request_id = $1",
        )
        .bind(request.request_id)
        .fetch_one(&mut *tx)
        .await?;
        let review_version = positive_u64(review_version)?;

        sqlx::query(
            "INSERT INTO erpx_control.role_recovery_review_receipts
                (request_id, review_version, scope_key, scope_kind, holding_id, company_id,
                 branch_id, scope_company_ids, scope_branch_ids, target_kind, target_holding_id,
                 principal_ref, permissions, functional_domain, authority_level, valid_from,
                 valid_until, timezone, reason_hash, ticket_reference_hash, requester_ref,
                 approver_ref, reviewer_ref, decision, request_hash, idempotency_key, actor_ref,
                 correlation_id)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,
                     $21,$22,$23,$24,$25,$26,$27,$28)",
        )
        .bind(request.request_id)
        .bind(i64::try_from(review_version).map_err(|_| {
            RoleReviewPersistenceError::InvalidData("recovery review version exceeds BIGINT".into())
        })?)
        .bind(scope.canonical_key())
        .bind(parts.kind)
        .bind(parts.holding_id)
        .bind(parts.company_id)
        .bind(parts.branch_id)
        .bind(&parts.company_ids)
        .bind(&parts.branch_ids)
        .bind(target.0)
        .bind(target.1)
        .bind(&request.principal_ref)
        .bind(&request.permissions)
        .bind(&request.functional_domain)
        .bind(i16::from(request.authority_level))
        .bind(request.valid_from)
        .bind(request.valid_until)
        .bind(&request.timezone)
        .bind(reason_hash)
        .bind(ticket_reference_hash)
        .bind(&request.requester_ref)
        .bind(&request.approver_ref)
        .bind(&request.reviewer_ref)
        .bind(decision)
        .bind(request_hash)
        .bind(idempotency_key)
        .bind(actor_ref)
        .bind(correlation_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(RoleRecoveryReceipt {
            request_id: request.request_id,
            review_version,
            reused_idempotency: false,
        })
    }
}

#[derive(Debug, Clone)]
struct ScopeParts {
    kind: &'static str,
    holding_id: Option<Uuid>,
    company_id: Option<Uuid>,
    branch_id: Option<Uuid>,
    company_ids: Vec<Uuid>,
    branch_ids: Vec<Uuid>,
}

impl ScopeParts {
    fn from_scope(scope: &Scope) -> Self {
        match scope {
            Scope::Platform => Self {
                kind: "platform",
                holding_id: None,
                company_id: None,
                branch_id: None,
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
            },
            Scope::Holding { holding_id } => Self {
                kind: "holding",
                holding_id: Some(*holding_id),
                company_id: None,
                branch_id: None,
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
            },
            Scope::Company {
                holding_id,
                company_id,
            } => Self {
                kind: "company",
                holding_id: Some(*holding_id),
                company_id: Some(*company_id),
                branch_id: None,
                company_ids: vec![*company_id],
                branch_ids: Vec::new(),
            },
            Scope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Self {
                kind: "branch",
                holding_id: Some(*holding_id),
                company_id: Some(*company_id),
                branch_id: Some(*branch_id),
                company_ids: vec![*company_id],
                branch_ids: vec![*branch_id],
            },
            Scope::Subtree {
                holding_id,
                company_ids,
                branch_ids,
            } => Self {
                kind: "subtree",
                holding_id: Some(*holding_id),
                company_id: None,
                branch_id: None,
                company_ids: sorted_ids(company_ids),
                branch_ids: sorted_ids(branch_ids),
            },
        }
    }
}

fn scope_from_columns(
    kind: String,
    holding_id: Option<Uuid>,
    company_id: Option<Uuid>,
    branch_id: Option<Uuid>,
    company_ids: Vec<Uuid>,
    branch_ids: Vec<Uuid>,
) -> Result<Scope, RoleReviewPersistenceError> {
    let scope = match (
        kind.as_str(),
        holding_id,
        company_id,
        branch_id,
        company_ids.as_slice(),
        branch_ids.as_slice(),
    ) {
        ("platform", None, None, None, [], []) => Scope::Platform,
        ("holding", Some(holding_id), None, None, [], []) => Scope::Holding { holding_id },
        ("company", Some(holding_id), Some(company_id), None, [stored_company_id], [])
            if *stored_company_id == company_id =>
        {
            Scope::Company {
                holding_id,
                company_id,
            }
        }
        (
            "branch",
            Some(holding_id),
            Some(company_id),
            Some(branch_id),
            [stored_company_id],
            [stored_branch_id],
        ) if *stored_company_id == company_id && *stored_branch_id == branch_id => Scope::Branch {
            holding_id,
            company_id,
            branch_id,
        },
        ("subtree", Some(holding_id), None, None, company_ids, branch_ids)
            if !company_ids.is_empty()
                && sorted_ids(company_ids) == company_ids
                && sorted_ids(branch_ids) == branch_ids =>
        {
            Scope::Subtree {
                holding_id,
                company_ids: company_ids.to_vec(),
                branch_ids: branch_ids.to_vec(),
            }
        }
        _ => {
            return Err(RoleReviewPersistenceError::InvalidData(
                "role-review persisted scope columns are inconsistent".into(),
            ));
        }
    };
    validate_scope(&scope)?;
    Ok(scope)
}

async fn validate_authoritative_identity_scope(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    principal_ref: &str,
    requested_scope: &Scope,
    at: DateTime<Utc>,
    field: &str,
    allowed_role_keys: Option<&[&str]>,
) -> Result<(), RoleReviewPersistenceError> {
    validate_reference(principal_ref, field)?;
    validate_scope(requested_scope)?;
    let rows = sqlx::query(
        "SELECT payload
         FROM erpx_control.role_assignments
         WHERE principal_ref=$1
         ORDER BY valid_until DESC, id
         LIMIT $2",
    )
    .bind(principal_ref)
    .bind(MAX_AUTHORITY_ASSIGNMENTS)
    .fetch_all(&mut **tx)
    .await?;

    let authorized = rows.into_iter().try_fold(
        false,
        |authorized, row| -> Result<bool, RoleReviewPersistenceError> {
            if authorized {
                return Ok(true);
            }
            let assignment: RoleAssignment = serde_json::from_value(row.try_get("payload")?)?;
            let role_allowed = allowed_role_keys.is_none_or(|roles| {
                roles
                    .iter()
                    .any(|role_key| assignment.role_key == *role_key)
            });
            Ok(assignment.principal_ref == principal_ref
                && assignment.is_active_at(at)
                && assignment.scope.contains(requested_scope)
                && role_allowed)
        },
    )?;
    if !authorized {
        return Err(RoleReviewPersistenceError::InvalidData(format!(
            "{field} is not active in the requested scope"
        )));
    }
    Ok(())
}

async fn validate_recovery_approver_intersection(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    assignment: &RoleAssignment,
    at: DateTime<Utc>,
    allowed_role_keys: &[&str],
) -> Result<(), RoleReviewPersistenceError> {
    let approver_rows = sqlx::query(
        "SELECT payload
         FROM erpx_control.role_assignments
         WHERE principal_ref=$1
         ORDER BY valid_until DESC, id
         LIMIT $2",
    )
    .bind(&assignment.approver)
    .bind(MAX_AUTHORITY_ASSIGNMENTS)
    .fetch_all(&mut **tx)
    .await?;
    let approver = approver_rows
        .into_iter()
        .map(|row| {
            serde_json::from_value::<RoleAssignment>(row.try_get("payload")?)
                .map_err(RoleReviewPersistenceError::from)
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .find(|candidate| {
            candidate.is_active_at(at)
                && candidate.scope.contains(&assignment.scope)
                && allowed_role_keys
                    .iter()
                    .any(|role_key| candidate.role_key == *role_key)
                && candidate
                    .permissions
                    .iter()
                    .any(|permission| permission == "role.assignment.approve")
        })
        .ok_or_else(|| {
            RoleReviewPersistenceError::InvalidData(
                "recovery approver has no authoritative approval assignment".into(),
            )
        })?;

    let mut parents = Vec::new();
    let mut parent_id = approver.parent_assignment_id;
    let mut seen = HashSet::new();
    while let Some(id) = parent_id {
        if !seen.insert(id) || parents.len() >= MAX_AUTHORITY_ASSIGNMENTS as usize {
            return Err(RoleReviewPersistenceError::InvalidData(
                "recovery approver parent chain is invalid".into(),
            ));
        }
        let row = sqlx::query(
            "SELECT payload
             FROM erpx_control.role_assignments
             WHERE id=$1
             FOR SHARE",
        )
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| {
            RoleReviewPersistenceError::InvalidData(
                "recovery approver parent assignment is missing".into(),
            )
        })?;
        let parent: RoleAssignment = serde_json::from_value(row.try_get("payload")?)?;
        parent_id = parent.parent_assignment_id;
        parents.push(parent);
    }

    evaluate_approver_scope_intersection(assignment, &approver, &parents, at).map_err(|error| {
        RoleReviewPersistenceError::InvalidData(format!(
            "recovery approver failed authoritative scope intersection: {error}"
        ))
    })
}

fn sorted_ids(values: &[Uuid]) -> Vec<Uuid> {
    let mut result = values.to_vec();
    result.sort_unstable();
    result.dedup();
    result
}

fn risk_tier(value: erpx_domain::access::RiskTier) -> &'static str {
    match value {
        erpx_domain::access::RiskTier::PlatformRoot => "platform_root",
        erpx_domain::access::RiskTier::Administrator => "administrator",
        erpx_domain::access::RiskTier::High => "high",
        erpx_domain::access::RiskTier::Standard => "standard",
        erpx_domain::access::RiskTier::Contractor => "contractor",
        erpx_domain::access::RiskTier::SupportDiagnostic => "support_diagnostic",
        erpx_domain::access::RiskTier::SupportMutation => "support_mutation",
        erpx_domain::access::RiskTier::Emergency => "emergency",
    }
}

fn parse_notification_kind(
    value: &str,
) -> Result<RoleExpiryNotificationKind, RoleReviewPersistenceError> {
    match value {
        "expiring_soon" => Ok(RoleExpiryNotificationKind::ExpiringSoon),
        "expired" => Ok(RoleExpiryNotificationKind::Expired),
        other => Err(RoleReviewPersistenceError::InvalidData(format!(
            "unknown notification kind {other}"
        ))),
    }
}

fn validate_authoritative_notification_source(
    notification: &ClaimedRoleExpiryNotification,
    source_status: &str,
    source_claim_key: Option<&str>,
    source_assignment_id: Uuid,
    source_principal_ref: &str,
    source_valid_until: DateTime<Utc>,
    source_kind: RoleExpiryNotificationKind,
) -> Result<(), RoleReviewPersistenceError> {
    if source_status != "claimed"
        || source_claim_key != Some(notification.claim_key.as_str())
        || source_assignment_id != notification.assignment_id
        || source_principal_ref != notification.principal_ref
        || source_valid_until != notification.valid_until
        || source_kind != notification.kind
    {
        return Err(RoleReviewPersistenceError::InvalidData(
            "notification does not match its authoritative claim".into(),
        ));
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    hex::encode(digest.finalize())
}

fn validate_sha256(value: &str) -> Result<(), RoleReviewPersistenceError> {
    if value.len() != 64
        || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
        || value.bytes().any(|byte| byte.is_ascii_uppercase())
    {
        return Err(RoleReviewPersistenceError::InvalidData(
            "request hash must be a SHA-256 hex digest".into(),
        ));
    }
    Ok(())
}

fn validate_reference(value: &str, field: &str) -> Result<(), RoleReviewPersistenceError> {
    if value.trim().is_empty()
        || value.len() > MAX_REFERENCE_LENGTH
        || value.chars().any(char::is_control)
    {
        return Err(RoleReviewPersistenceError::InvalidData(format!(
            "{field} is invalid"
        )));
    }
    Ok(())
}

fn validate_scope(scope: &Scope) -> Result<(), RoleReviewPersistenceError> {
    let ids_are_valid = |ids: &[Uuid]| ids.iter().all(|id| !id.is_nil());
    match scope {
        Scope::Platform => Ok(()),
        Scope::Holding { holding_id } => {
            if holding_id.is_nil() {
                return Err(RoleReviewPersistenceError::InvalidData(
                    "holding scope id is invalid".into(),
                ));
            }
            Ok(())
        }
        Scope::Company {
            holding_id,
            company_id,
        } => {
            if holding_id.is_nil() || company_id.is_nil() {
                return Err(RoleReviewPersistenceError::InvalidData(
                    "company scope id is invalid".into(),
                ));
            }
            Ok(())
        }
        Scope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => {
            if holding_id.is_nil() || company_id.is_nil() || branch_id.is_nil() {
                return Err(RoleReviewPersistenceError::InvalidData(
                    "branch scope id is invalid".into(),
                ));
            }
            Ok(())
        }
        Scope::Subtree {
            holding_id,
            company_ids,
            branch_ids,
        } => {
            if holding_id.is_nil()
                || company_ids.is_empty()
                || !ids_are_valid(company_ids)
                || !ids_are_valid(branch_ids)
            {
                return Err(RoleReviewPersistenceError::InvalidData(
                    "subtree scope shape is invalid".into(),
                ));
            }
            Ok(())
        }
    }
}

fn validate_review_queue(
    scope: &Scope,
    queue: &RoleReviewQueue,
) -> Result<(), RoleReviewPersistenceError> {
    validate_scope(scope)?;
    if queue.expiry_horizon_until <= queue.evaluated_at
        || queue.expiry_horizon_until - queue.evaluated_at > MAX_REVIEW_HORIZON
    {
        return Err(RoleReviewPersistenceError::InvalidData(
            "review horizon is invalid".into(),
        ));
    }
    let mut assignment_ids = HashSet::with_capacity(queue.items.len());
    for item in &queue.items {
        if !assignment_ids.insert(item.assignment_id) {
            return Err(RoleReviewPersistenceError::InvalidData(
                "review contains a duplicate assignment".into(),
            ));
        }
        let assignment = &item.assignment;
        if assignment.id.is_nil()
            || assignment.id != item.assignment_id
            || assignment.assignment_version == 0
            || item.valid_from != assignment.valid_from
            || item.valid_until != assignment.valid_until
            || item.parent_assignment_id != assignment.parent_assignment_id
            || item.principal_ref != assignment.principal_ref
            || item.valid_until <= item.valid_from
            || item.classifications.is_empty()
        {
            return Err(RoleReviewPersistenceError::InvalidData(
                "review item does not match its assignment".into(),
            ));
        }
        validate_reference(&item.principal_ref, "review principal")?;
        validate_reference(&assignment.role_key, "review role")?;
        validate_reference(&assignment.functional_domain, "review functional domain")?;
        validate_reference(&assignment.timezone, "review timezone")?;
        validate_scope(&assignment.scope)?;
        if !scope.contains(&assignment.scope) {
            return Err(RoleReviewPersistenceError::InvalidData(
                "review item is outside the snapshot scope".into(),
            ));
        }
        let has_expired_classification = item
            .classifications
            .contains(&RoleReviewClassification::Expired);
        let has_expiring_soon_classification = item
            .classifications
            .contains(&RoleReviewClassification::ExpiringSoon);
        if has_expired_classification && has_expiring_soon_classification {
            return Err(RoleReviewPersistenceError::InvalidData(
                "review item has conflicting expiry classifications".into(),
            ));
        }
        match (
            &item.notification,
            has_expired_classification,
            has_expiring_soon_classification,
        ) {
            (Some(notification), _, _)
                if notification.assignment_id != item.assignment_id
                    || notification.principal_ref != item.principal_ref
                    || notification.valid_until != item.valid_until =>
            {
                return Err(RoleReviewPersistenceError::InvalidData(
                    "expiry notification does not match its review item".into(),
                ));
            }
            (Some(notification), has_expired, has_expiring_soon) => {
                let semantically_bound = match notification.kind {
                    RoleExpiryNotificationKind::Expired => {
                        has_expired && item.valid_until <= queue.evaluated_at
                    }
                    RoleExpiryNotificationKind::ExpiringSoon => {
                        has_expiring_soon
                            && item.valid_until > queue.evaluated_at
                            && item.valid_until <= queue.expiry_horizon_until
                    }
                };
                if !semantically_bound {
                    return Err(RoleReviewPersistenceError::InvalidData(
                        "expiry notification is not bound to its authoritative review state".into(),
                    ));
                }
            }
            (None, true, _) | (None, false, true) => {
                return Err(RoleReviewPersistenceError::InvalidData(
                    "expiry review classification is missing its notification".into(),
                ));
            }
            (None, false, false) => {}
        }
    }
    Ok(())
}

fn canonical_recovery_request(
    request: &RoleRecoveryRequest,
) -> Result<RoleRecoveryRequest, RoleReviewPersistenceError> {
    if request.request_id.is_nil() {
        return Err(RoleReviewPersistenceError::InvalidData(
            "recovery request id is required".into(),
        ));
    }
    let mut canonical = request.clone();
    canonical.permissions.sort_unstable();
    canonical.permissions.dedup();
    validate_recovery_shape(&canonical)?;
    Ok(canonical)
}

fn validate_recovery_shape(
    request: &RoleRecoveryRequest,
) -> Result<(), RoleReviewPersistenceError> {
    validate_reference(&request.principal_ref, "recovery principal")?;
    validate_reference(&request.functional_domain, "recovery functional domain")?;
    validate_reference(&request.timezone, "recovery timezone")?;
    validate_reference(&request.grant_reason, "recovery grant reason")?;
    validate_reference(&request.requester_ref, "recovery requester")?;
    validate_reference(&request.approver_ref, "recovery approver")?;
    if request.permissions.is_empty() {
        return Err(RoleReviewPersistenceError::InvalidData(
            "recovery permissions are required".into(),
        ));
    }
    if request.valid_until <= request.valid_from {
        return Err(RoleReviewPersistenceError::InvalidData(
            "recovery window is invalid".into(),
        ));
    }
    if let Some(ticket_reference) = request.ticket_reference.as_deref() {
        validate_reference(ticket_reference, "recovery ticket reference")?;
    }
    if let Some(reviewer_ref) = request.reviewer_ref.as_deref() {
        validate_reference(reviewer_ref, "recovery reviewer")?;
    }
    match &request.target {
        erpx_domain::access::RoleRecoveryTarget::PlatformRoot => {}
        erpx_domain::access::RoleRecoveryTarget::HoldingOwner { holding_id } => {
            if holding_id.is_nil() {
                return Err(RoleReviewPersistenceError::InvalidData(
                    "recovery holding id is invalid".into(),
                ));
            }
        }
    }
    Ok(())
}

fn validate_recovery_identity_binding(
    request: &RoleRecoveryRequest,
    actor_ref: &str,
) -> Result<(), RoleReviewPersistenceError> {
    if actor_ref != request.requester_ref {
        return Err(RoleReviewPersistenceError::InvalidData(
            "recovery receipt actor must match requester".into(),
        ));
    }
    if request.requester_ref == request.approver_ref {
        return Err(RoleReviewPersistenceError::InvalidData(
            "recovery approver must differ from requester".into(),
        ));
    }
    if let Some(reviewer_ref) = request.reviewer_ref.as_deref()
        && (reviewer_ref == request.requester_ref || reviewer_ref == request.approver_ref)
    {
        return Err(RoleReviewPersistenceError::InvalidData(
            "recovery reviewer identities must be independent".into(),
        ));
    }
    Ok(())
}

fn positive_u64(value: i64) -> Result<u64, RoleReviewPersistenceError> {
    u64::try_from(value)
        .map_err(|_| RoleReviewPersistenceError::InvalidData("negative snapshot version".into()))
}

fn validate_claim_keys(
    claim_key: &str,
    notification_keys: &[String],
) -> Result<(), RoleReviewPersistenceError> {
    if claim_key.trim().is_empty()
        || claim_key.len() > 255
        || claim_key.chars().any(char::is_control)
        || notification_keys.is_empty()
        || notification_keys.len() > MAX_NOTIFICATION_CLAIM as usize
        || notification_keys.iter().any(|key| {
            key.trim().is_empty() || key.len() > 255 || key.chars().any(char::is_control)
        })
    {
        return Err(RoleReviewPersistenceError::InvalidData(
            "role-review claim keys are invalid".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_append_requires_matching_claimed_source() {
        let assignment_id = Uuid::new_v4();
        let valid_until = Utc::now() + Duration::hours(1);
        let notification = ClaimedRoleExpiryNotification {
            notification_key: "role-review-notification".into(),
            assignment_id,
            principal_ref: "role-review-principal".into(),
            valid_until,
            kind: RoleExpiryNotificationKind::ExpiringSoon,
            claim_key: "role-review-claim".into(),
        };

        let mismatched_claim = validate_authoritative_notification_source(
            &notification,
            "claimed",
            Some("different-claim"),
            assignment_id,
            &notification.principal_ref,
            valid_until,
            notification.kind,
        )
        .expect_err("a different durable claim must be rejected");
        assert!(matches!(
            mismatched_claim,
            RoleReviewPersistenceError::InvalidData(message)
                if message.contains("authoritative claim")
        ));

        let reconciled_source = validate_authoritative_notification_source(
            &notification,
            "reconciled",
            Some(&notification.claim_key),
            assignment_id,
            &notification.principal_ref,
            valid_until,
            notification.kind,
        )
        .expect_err("a terminal source must not append another notification event");
        assert!(matches!(
            reconciled_source,
            RoleReviewPersistenceError::InvalidData(message)
                if message.contains("authoritative claim")
        ));

        validate_authoritative_notification_source(
            &notification,
            "claimed",
            Some(&notification.claim_key),
            assignment_id,
            &notification.principal_ref,
            valid_until,
            notification.kind,
        )
        .expect("the exact active source claim must remain appendable");
    }
}
