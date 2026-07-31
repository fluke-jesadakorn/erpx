//! Live SQLx acceptance for the role-review worker's authoritative ack fence.
//!
//! This test uses the real control-plane migration and RoleReviewPersistence.
//! The source adapter deliberately causes the real repository to acknowledge
//! only one of two claimed rows, proving that the worker does not report a
//! successful batch when the authoritative acknowledgement is incomplete.

use async_trait::async_trait;
use chrono::{Duration, Utc};
use erpx_domain::access::{
    RiskTier, RoleAssignment, RoleExpiryNotification, RoleExpiryNotificationKind,
    RoleReviewClassification, RoleReviewItem, RoleReviewQueue, Scope,
};
use erpx_infrastructure::{
    ClaimedRoleExpiryNotification, RoleReviewPersistence, RoleReviewPersistenceError,
};
use erpx_migration::migrate_control_plane;
use erpx_worker::{
    NamedWorkerHandler, RoleReviewNotificationHandler, RoleReviewNotificationPublishError,
    RoleReviewNotificationPublisher, RoleReviewNotificationSource, WorkerRuntimeError,
};
use sqlx::{Row, postgres::PgPoolOptions};
use uuid::Uuid;

fn database_url() -> String {
    std::env::var("ERPX_G2_ROLE_REVIEW_DATABASE_URL")
        .or_else(|_| std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL"))
        .expect("ERPX_G2_ROLE_REVIEW_DATABASE_URL or ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
}

fn review_fixture(now: chrono::DateTime<Utc>) -> (Scope, RoleReviewQueue, Vec<String>) {
    let mut items = Vec::new();
    let mut notification_keys = Vec::new();
    for index in 1..=2_u128 {
        let assignment_id = Uuid::from_u128(index);
        let principal_ref = format!("role-review-principal-{index}");
        let valid_until = now + Duration::hours(1) + Duration::minutes(index as i64);
        let assignment = RoleAssignment {
            id: assignment_id,
            principal_ref: principal_ref.clone(),
            role_key: "platform_reviewer".into(),
            permissions: vec!["role.review".into()],
            functional_domain: "platform".into(),
            authority_level: 1,
            approval_limit_minor: 0,
            scope: Scope::Platform,
            valid_from: now - Duration::minutes(5),
            valid_until,
            timezone: "UTC".into(),
            risk_tier: RiskTier::Standard,
            grant_reason: "bounded role-review worker acceptance".into(),
            ticket_reference: Some(format!("G2-WORKER-{index}")),
            grantor: "fixture-grantor".into(),
            approver: None,
            renewal_reviewer: None,
            parent_assignment_id: None,
            assignment_version: 1,
        };
        let notification = RoleExpiryNotification {
            assignment_id,
            principal_ref: principal_ref.clone(),
            valid_until,
            kind: RoleExpiryNotificationKind::ExpiringSoon,
        };
        notification_keys.push(notification.stable_key());
        items.push(RoleReviewItem {
            assignment_id,
            principal_ref,
            valid_from: assignment.valid_from,
            valid_until,
            parent_assignment_id: None,
            classifications: vec![RoleReviewClassification::ExpiringSoon],
            notification: Some(notification),
            assignment,
        });
    }
    (
        Scope::Platform,
        RoleReviewQueue {
            evaluated_at: now,
            expiry_horizon_until: now + Duration::days(7),
            items,
        },
        notification_keys,
    )
}

async fn seed_review_worker_authority(pool: &sqlx::PgPool, now: chrono::DateTime<Utc>) {
    let assignment = RoleAssignment {
        id: Uuid::new_v4(),
        principal_ref: "role-review-worker-test".into(),
        role_key: "platform_reviewer".into(),
        permissions: vec!["role.review".into()],
        functional_domain: "platform".into(),
        authority_level: 255,
        approval_limit_minor: 0,
        scope: Scope::Platform,
        valid_from: now - Duration::minutes(5),
        valid_until: now + Duration::hours(2),
        timezone: "UTC".into(),
        risk_tier: RiskTier::Standard,
        grant_reason: "bounded role-review worker acceptance authority".into(),
        ticket_reference: Some("G2-WORKER-AUTHORITY".into()),
        grantor: "fixture-grantor".into(),
        approver: Some("fixture-approver".into()),
        renewal_reviewer: None,
        parent_assignment_id: None,
        assignment_version: 1,
    };
    sqlx::query(
        "INSERT INTO erpx_control.role_assignments
            (id, principal_ref, role_key, parent_assignment_id, valid_from,
             valid_until, assignment_version, payload)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(assignment.id)
    .bind(&assignment.principal_ref)
    .bind(&assignment.role_key)
    .bind(assignment.parent_assignment_id)
    .bind(assignment.valid_from)
    .bind(assignment.valid_until)
    .bind(i32::try_from(assignment.assignment_version).expect("assignment version"))
    .bind(serde_json::to_value(&assignment).expect("authority assignment payload"))
    .execute(pool)
    .await
    .expect("seed role-review worker authority");
}

#[derive(Clone)]
struct PartialReconcileSource {
    persistence: RoleReviewPersistence,
}

#[async_trait]
impl RoleReviewNotificationSource for PartialReconcileSource {
    async fn claim(
        &self,
        scope: &Scope,
        claimant_ref: &str,
        at: chrono::DateTime<Utc>,
        horizon: Duration,
        lease: Duration,
    ) -> Result<Vec<ClaimedRoleExpiryNotification>, RoleReviewPersistenceError> {
        self.persistence
            .claim_expiry_notifications(scope, claimant_ref, at, horizon, lease)
            .await
    }

    async fn reconcile(
        &self,
        claim_key: &str,
        notification_keys: &[String],
        at: chrono::DateTime<Utc>,
    ) -> Result<u64, RoleReviewPersistenceError> {
        let first = notification_keys
            .first()
            .ok_or_else(|| RoleReviewPersistenceError::InvalidData("empty ack".into()))?;
        self.persistence
            .reconcile_expiry_notifications(claim_key, std::slice::from_ref(first), at)
            .await
    }

    async fn release(
        &self,
        claim_key: &str,
        notification_keys: &[String],
    ) -> Result<(), RoleReviewPersistenceError> {
        self.persistence
            .release_expiry_notifications(claim_key, notification_keys)
            .await
            .map(|_| ())
    }
}

#[derive(Clone, Copy)]
struct SuccessfulPublisher;

#[async_trait]
impl RoleReviewNotificationPublisher for SuccessfulPublisher {
    async fn publish(
        &self,
        _notification: &ClaimedRoleExpiryNotification,
    ) -> Result<(), RoleReviewNotificationPublishError> {
        Ok(())
    }
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database URL"]
async fn partial_sqlx_ack_is_not_reported_as_success() {
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url())
        .await
        .expect("connect role-review worker database");
    migrate_control_plane(&pool)
        .await
        .expect("apply control-plane migrations");

    let persistence = RoleReviewPersistence::new(pool.clone());
    let now = Utc::now();
    let (scope, queue, notification_keys) = review_fixture(now);
    seed_review_worker_authority(&pool, now).await;
    persistence
        .persist_snapshot(
            &scope,
            &queue,
            "role-review-worker-test",
            Uuid::new_v4(),
            &format!("g2-worker-ack-{}", Uuid::new_v4()),
            &"a".repeat(64),
        )
        .await
        .expect("persist role-review notifications");

    let handler = RoleReviewNotificationHandler::new(
        PartialReconcileSource {
            persistence: persistence.clone(),
        },
        SuccessfulPublisher,
        Scope::Platform,
        "role-review-worker",
        Duration::days(7),
        Duration::minutes(5),
    )
    .expect("construct role-review handler");

    let error = handler
        .run_once(Uuid::new_v4())
        .await
        .expect_err("partial authoritative ack must fail closed");
    assert!(matches!(
        error,
        WorkerRuntimeError::Configuration(message)
            if message == "role-review notification acknowledgement mismatch"
    ));

    let statuses = sqlx::query(
        "SELECT notification_key, status
         FROM erpx_control.role_expiry_notifications
         WHERE notification_key = ANY($1)
         ORDER BY notification_key",
    )
    .bind(&notification_keys)
    .fetch_all(&pool)
    .await
    .expect("read authoritative notification status");
    assert_eq!(statuses.len(), 2);
    assert_eq!(
        statuses[0].try_get::<String, _>("status").unwrap(),
        "reconciled"
    );
    assert_eq!(
        statuses[1].try_get::<String, _>("status").unwrap(),
        "pending"
    );
    pool.close().await;
}
