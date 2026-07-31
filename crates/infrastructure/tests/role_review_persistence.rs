use chrono::{Duration, Utc};
use erpx_domain::access::{
    RiskTier, RoleAssignment, RoleExpiryNotification, RoleExpiryNotificationKind,
    RoleRecoveryRequest, RoleRecoveryTarget, RoleReviewClassification, RoleReviewItem,
    RoleReviewQueue, Scope,
};
use erpx_infrastructure::{RoleRecoveryReceipt, RoleReviewPersistence, RoleReviewPersistenceError};
use erpx_migration::migrate_control_plane;
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

fn database_url() -> String {
    std::env::var("ERPX_G2_ROLE_REVIEW_DATABASE_URL")
        .or_else(|_| std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL"))
        .expect("ERPX_G2_ROLE_REVIEW_DATABASE_URL or ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
}

fn review_fixture(now: chrono::DateTime<Utc>) -> (Scope, RoleReviewQueue) {
    let assignment_id = Uuid::new_v4();
    let valid_until = now + Duration::hours(1);
    let assignment = RoleAssignment {
        id: assignment_id,
        principal_ref: "review-principal".into(),
        role_key: "platform_reviewer".into(),
        permissions: vec!["role.review".into(), "role.assignment.approve".into()],
        functional_domain: "platform".into(),
        authority_level: 1,
        approval_limit_minor: 0,
        scope: Scope::Platform,
        valid_from: now - Duration::minutes(5),
        valid_until,
        timezone: "UTC".into(),
        risk_tier: RiskTier::Standard,
        grant_reason: "bounded role review fixture".into(),
        ticket_reference: Some("G2-REVIEW".into()),
        grantor: "fixture-grantor".into(),
        approver: None,
        renewal_reviewer: None,
        parent_assignment_id: None,
        assignment_version: 1,
    };
    let item = RoleReviewItem {
        assignment_id,
        principal_ref: assignment.principal_ref.clone(),
        valid_from: assignment.valid_from,
        valid_until: assignment.valid_until,
        parent_assignment_id: None,
        classifications: vec![RoleReviewClassification::ExpiringSoon],
        notification: Some(RoleExpiryNotification {
            assignment_id,
            principal_ref: assignment.principal_ref.clone(),
            valid_until,
            kind: RoleExpiryNotificationKind::ExpiringSoon,
        }),
        assignment,
    };
    (
        Scope::Platform,
        RoleReviewQueue {
            evaluated_at: now,
            expiry_horizon_until: now + Duration::days(7),
            items: vec![item],
        },
    )
}

fn recovery_fixture(now: chrono::DateTime<Utc>, request_id: Uuid) -> RoleRecoveryRequest {
    RoleRecoveryRequest {
        request_id,
        target: RoleRecoveryTarget::PlatformRoot,
        principal_ref: "recovered-root".into(),
        permissions: vec!["platform.write".into(), "platform.read".into()],
        functional_domain: "platform".into(),
        authority_level: 255,
        valid_from: now,
        valid_until: now + Duration::hours(1),
        timezone: "UTC".into(),
        grant_reason: "recover the final platform root".into(),
        ticket_reference: Some("RECOVERY-1".into()),
        requester_ref: "requester".into(),
        approver_ref: "approver".into(),
        reviewer_ref: Some("reviewer".into()),
    }
}

fn authority_assignment(
    principal_ref: &str,
    role_key: &str,
    scope: Scope,
    now: chrono::DateTime<Utc>,
) -> RoleAssignment {
    RoleAssignment {
        id: Uuid::new_v4(),
        principal_ref: principal_ref.into(),
        role_key: role_key.into(),
        permissions: vec!["role.review".into(), "role.assignment.approve".into()],
        functional_domain: "platform".into(),
        authority_level: 255,
        approval_limit_minor: 0,
        scope,
        valid_from: now - Duration::minutes(5),
        valid_until: now + Duration::hours(2),
        timezone: "UTC".into(),
        risk_tier: if role_key == "platform_root" {
            RiskTier::PlatformRoot
        } else {
            RiskTier::Standard
        },
        grant_reason: "G2 role-review authority fixture".into(),
        ticket_reference: Some("G2-AUTHORITY".into()),
        grantor: "fixture-grantor".into(),
        approver: Some("fixture-approver".into()),
        renewal_reviewer: None,
        parent_assignment_id: None,
        assignment_version: 1,
    }
}

async fn seed_authority_assignment(pool: &PgPool, assignment: &RoleAssignment) {
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
    .bind(serde_json::to_value(assignment).expect("authority assignment payload"))
    .execute(pool)
    .await
    .expect("seed authority assignment");
}

#[tokio::test]
async fn forged_expiry_notification_kind_is_rejected_before_persistence() {
    let pool = PgPool::connect_lazy("postgres://unused").expect("lazy role-review pool");
    let persistence = RoleReviewPersistence::new(pool);
    let now = Utc::now();
    let (scope, mut queue) = review_fixture(now);
    queue.items[0]
        .notification
        .as_mut()
        .expect("review fixture notification")
        .kind = RoleExpiryNotificationKind::Expired;

    let error = persistence
        .persist_snapshot(
            &scope,
            &queue,
            "review-worker",
            Uuid::new_v4(),
            "g2-review-semantic-binding",
            &"a".repeat(64),
        )
        .await
        .expect_err("inconsistent notification must fail closed");
    assert!(matches!(
        error,
        RoleReviewPersistenceError::InvalidData(message)
            if message.contains("not bound to its authoritative review state")
    ));
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database URL"]
async fn role_review_snapshot_and_notification_receipts_are_durable_and_replay_safe() {
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url())
        .await
        .expect("connect role-review database");
    migrate_control_plane(&pool)
        .await
        .expect("apply control-plane migrations");
    let persistence = RoleReviewPersistence::new(pool.clone());
    let now = Utc::now();
    let (scope, queue) = review_fixture(now);
    seed_authority_assignment(
        &pool,
        &authority_assignment("review-worker", "platform_reviewer", Scope::Platform, now),
    )
    .await;
    seed_authority_assignment(
        &pool,
        &authority_assignment(
            "role-review-outbox-worker",
            "platform_reviewer",
            Scope::Platform,
            now,
        ),
    )
    .await;
    seed_authority_assignment(
        &pool,
        &authority_assignment("approver", "platform_root", Scope::Platform, now),
    )
    .await;
    let other_holding = Uuid::new_v4();
    seed_authority_assignment(
        &pool,
        &authority_assignment(
            "holding-reviewer",
            "holding_owner",
            Scope::Holding {
                holding_id: other_holding,
            },
            now,
        ),
    )
    .await;
    let correlation_id = Uuid::new_v4();
    let request_hash = "a".repeat(64);

    let first = persistence
        .persist_snapshot(
            &scope,
            &queue,
            "review-worker",
            correlation_id,
            "g2-review-snapshot-1",
            &request_hash,
        )
        .await
        .expect("persist the first snapshot");
    assert!(!first.reused_idempotency);
    assert_eq!(first.item_count, 1);

    let replay = persistence
        .persist_snapshot(
            &scope,
            &queue,
            "review-worker",
            correlation_id,
            "g2-review-snapshot-1",
            &request_hash,
        )
        .await
        .expect("replay the snapshot");
    assert!(replay.reused_idempotency);
    assert_eq!(replay.snapshot_key, first.snapshot_key);
    assert_eq!(replay.content_hash, first.content_hash);

    let out_of_scope_actor = persistence
        .persist_snapshot(
            &scope,
            &queue,
            "holding-reviewer",
            Uuid::new_v4(),
            "g2-review-out-of-scope-actor",
            &request_hash,
        )
        .await
        .expect_err("snapshot actor outside the requested scope must be rejected");
    assert!(matches!(
        out_of_scope_actor,
        RoleReviewPersistenceError::InvalidData(message)
            if message.contains("snapshot actor is not active in the requested scope")
    ));

    let conflict = persistence
        .persist_snapshot(
            &scope,
            &queue,
            "review-worker",
            correlation_id,
            "g2-review-snapshot-1",
            &"b".repeat(64),
        )
        .await
        .expect_err("different request must conflict");
    assert!(matches!(
        conflict,
        RoleReviewPersistenceError::IdempotencyConflict
    ));

    let claimed = persistence
        .claim_expiry_notifications(
            &scope,
            "role-review-worker:one",
            now,
            Duration::days(7),
            Duration::minutes(5),
        )
        .await
        .expect("claim notification");
    assert_eq!(claimed.len(), 1);
    let claim_key = claimed[0].claim_key.clone();
    let notification_key = claimed[0].notification_key.clone();
    assert!(
        persistence
            .append_expiry_notification_event(&claimed[0], "role-review-outbox-worker")
            .await
            .expect("append notification outbox event")
    );
    assert!(
        !persistence
            .append_expiry_notification_event(&claimed[0], "role-review-outbox-worker")
            .await
            .expect("replay notification outbox event")
    );
    let outbox_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_control.outbox_events
         WHERE event_type = 'role_expiry_notification.requested'
           AND idempotency_key = $1",
    )
    .bind(format!("role-expiry-notification:{notification_key}"))
    .fetch_one(&pool)
    .await
    .expect("read notification outbox event");
    assert_eq!(outbox_count, 1);
    assert_eq!(
        persistence
            .release_expiry_notifications(&claim_key, std::slice::from_ref(&notification_key))
            .await
            .expect("release notification for retry"),
        1
    );

    let retried = persistence
        .claim_expiry_notifications(
            &scope,
            "role-review-worker:two",
            now + Duration::seconds(1),
            Duration::days(7),
            Duration::minutes(5),
        )
        .await
        .expect("reclaim notification");
    assert_eq!(retried.len(), 1);
    assert_eq!(retried[0].notification_key, notification_key);
    let out_of_scope_notification_actor = persistence
        .append_expiry_notification_event(&retried[0], "holding-reviewer")
        .await
        .expect_err("notification actor outside the claim scope must be rejected");
    assert!(matches!(
        out_of_scope_notification_actor,
        RoleReviewPersistenceError::InvalidData(message)
            if message.contains("notification actor is not active in the requested scope")
    ));
    assert_eq!(
        persistence
            .reconcile_expiry_notifications(
                &retried[0].claim_key,
                std::slice::from_ref(&notification_key),
                now + Duration::minutes(1),
            )
            .await
            .expect("reconcile delivered notification"),
        1
    );
    assert!(
        persistence
            .claim_expiry_notifications(
                &scope,
                "role-review-worker:three",
                now + Duration::minutes(2),
                Duration::days(7),
                Duration::minutes(5),
            )
            .await
            .expect("verify terminal notification")
            .is_empty()
    );

    let status: String = sqlx::query_scalar(
        "SELECT status FROM erpx_control.role_expiry_notifications
         WHERE notification_key = $1",
    )
    .bind(notification_key)
    .fetch_one(&pool)
    .await
    .expect("read notification state");
    assert_eq!(status, "reconciled");
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database URL"]
async fn recovery_receipts_bind_requester_approver_and_replay_version() {
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url())
        .await
        .expect("connect role-review database");
    migrate_control_plane(&pool)
        .await
        .expect("apply control-plane migrations");
    let persistence = RoleReviewPersistence::new(pool.clone());
    let now = Utc::now();
    let request_id = Uuid::new_v4();
    let request = recovery_fixture(now, request_id);
    seed_authority_assignment(
        &pool,
        &authority_assignment("approver", "platform_root", Scope::Platform, now),
    )
    .await;

    let first = persistence
        .record_recovery_receipt(
            &request,
            1,
            "approved",
            "g2-recovery-1",
            "requester",
            Uuid::new_v4(),
        )
        .await
        .expect("record recovery receipt");
    assert_eq!(
        first,
        RoleRecoveryReceipt {
            request_id,
            review_version: 1,
            reused_idempotency: false,
        }
    );

    let mut replay_request = request.clone();
    replay_request.permissions.reverse();
    let replay = persistence
        .record_recovery_receipt(
            &replay_request,
            1,
            "approved",
            "g2-recovery-1",
            "requester",
            Uuid::new_v4(),
        )
        .await
        .expect("replay recovery receipt");
    assert_eq!(replay.request_id, request_id);
    assert_eq!(replay.review_version, 1);
    assert!(replay.reused_idempotency);

    let wrong_actor = persistence
        .record_recovery_receipt(
            &request,
            1,
            "approved",
            "g2-recovery-wrong-actor",
            "approver",
            Uuid::new_v4(),
        )
        .await
        .expect_err("receipt actor must remain requester-bound");
    assert!(matches!(
        wrong_actor,
        RoleReviewPersistenceError::InvalidData(message)
            if message.contains("actor must match requester")
    ));

    let conflict = persistence
        .record_recovery_receipt(
            &request,
            1,
            "rejected",
            "g2-recovery-1",
            "requester",
            Uuid::new_v4(),
        )
        .await
        .expect_err("decision changes must conflict on replay");
    assert!(matches!(
        conflict,
        RoleReviewPersistenceError::IdempotencyConflict
    ));

    let requested_holding = Uuid::new_v4();
    seed_authority_assignment(
        &pool,
        &authority_assignment(
            "holding-approver",
            "holding_owner",
            Scope::Holding {
                holding_id: Uuid::new_v4(),
            },
            now,
        ),
    )
    .await;
    let mut out_of_scope_request = recovery_fixture(now, Uuid::new_v4());
    out_of_scope_request.target = RoleRecoveryTarget::HoldingOwner {
        holding_id: requested_holding,
    };
    out_of_scope_request.approver_ref = "holding-approver".into();
    let out_of_scope_approver = persistence
        .record_recovery_receipt(
            &out_of_scope_request,
            1,
            "rejected",
            "g2-recovery-out-of-scope-approver",
            "requester",
            Uuid::new_v4(),
        )
        .await
        .expect_err("approver outside the requested holding must be rejected");
    assert!(matches!(
        out_of_scope_approver,
        RoleReviewPersistenceError::InvalidData(message)
            if message.contains("recovery approver is not active in the requested scope")
    ));

    let stored_permissions: Vec<String> = sqlx::query_scalar(
        "SELECT permissions FROM erpx_control.role_recovery_review_receipts
         WHERE request_id = $1 AND review_version = 1",
    )
    .bind(request_id)
    .fetch_one(&pool)
    .await
    .expect("read persisted recovery receipt");
    assert_eq!(stored_permissions, vec!["platform.read", "platform.write"]);
    pool.close().await;
}
