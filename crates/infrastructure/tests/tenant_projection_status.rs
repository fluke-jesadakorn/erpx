use chrono::{TimeZone, Utc};
use erpx_infrastructure::{
    ControlPlanePools, MAX_PROJECTION_STATUS_FRESHNESS_SECONDS, MAX_PROJECTION_STATUS_PAGE_SIZE,
    ProjectionJobKind, ProjectionJobState, ProjectionSchedulerAuthorization,
    ProjectionStatusChangeCursor, ProjectionStatusCursor, SqlxControlPlane,
    SqlxTenantStoreRegistry, TenantProjectionSchedulerRepository, TenantProjectionStatus,
    TenantProjectionStatusPage, TenantProjectionStatusReadRepository,
};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

fn repository() -> TenantProjectionSchedulerRepository {
    let pools = ControlPlanePools {
        write: PgPool::connect_lazy("postgres://unused").expect("lazy write"),
        read: PgPool::connect_lazy("postgres://unused-read").expect("lazy read"),
    };
    let registry = Arc::new(SqlxTenantStoreRegistry::new(Arc::new(
        SqlxControlPlane::new(pools.clone()),
    )));
    TenantProjectionSchedulerRepository::new(
        pools.write,
        registry,
        ProjectionSchedulerAuthorization::new("platform/root", "projection status", Uuid::new_v4())
            .expect("authorization"),
    )
}

fn read_repository() -> TenantProjectionStatusReadRepository {
    TenantProjectionStatusReadRepository::new(
        PgPool::connect_lazy("postgres://unused-read").expect("lazy read"),
    )
}

#[test]
fn status_cursor_is_bounded_and_opaque_fields_are_only_ordering_metadata() {
    let cursor = ProjectionStatusCursor::new(
        "tenant-opaque",
        "holding-opaque",
        "operations",
        "job-20260725",
        Uuid::new_v4(),
    )
    .expect("valid cursor");
    let rendered = serde_json::to_string(&cursor).expect("cursor serialization");
    assert!(rendered.contains("tenant-opaque"));
    assert!(!rendered.contains("payload"));
    assert!(!rendered.contains("worker_id"));
    assert!(
        ProjectionStatusCursor::new(
            "tenant\nwith-control",
            "holding-opaque",
            "operations",
            "job",
            Uuid::new_v4(),
        )
        .is_err()
    );
}

#[test]
fn status_dto_contains_no_job_payload_or_claim_identity() {
    let status = TenantProjectionStatus {
        tenant_id: "tenant-opaque".into(),
        holding_ref: "holding-opaque".into(),
        projection_name: "operations".into(),
        job_key: "job-1".into(),
        job_id: Uuid::new_v4(),
        kind: ProjectionJobKind::Incremental,
        state: ProjectionJobState::Succeeded,
        available_at: Utc
            .timestamp_opt(1_750_000_000, 0)
            .single()
            .expect("timestamp"),
        attempts: 1,
        max_attempts: 5,
        lease_until: None,
        last_error: None,
        dead_lettered_at: None,
        rebuild_id: None,
        version: 2,
        created_at: Utc
            .timestamp_opt(1_750_000_000, 0)
            .single()
            .expect("timestamp"),
        updated_at: Utc
            .timestamp_opt(1_750_000_000, 0)
            .single()
            .expect("timestamp"),
        published_at: Utc
            .timestamp_opt(1_750_000_000, 0)
            .single()
            .expect("timestamp"),
    };
    let rendered = format!("{status:?}");
    assert!(!rendered.contains("payload"));
    assert!(!rendered.contains("worker_id"));
    assert!(!rendered.contains("claim_id"));
}

#[test]
fn status_change_cursor_and_publication_freshness_are_bounded() {
    let now = Utc
        .timestamp_opt(1_750_000_000, 0)
        .single()
        .expect("timestamp");
    assert!(ProjectionStatusChangeCursor::new(now, Uuid::nil()).is_err());
    let cursor = ProjectionStatusChangeCursor::new(now, Uuid::new_v4()).expect("cursor");
    assert_eq!(cursor.updated_at, now);

    let mut status = TenantProjectionStatus {
        tenant_id: "tenant-opaque".into(),
        holding_ref: "holding-opaque".into(),
        projection_name: "operations".into(),
        job_key: "job-1".into(),
        job_id: Uuid::new_v4(),
        kind: ProjectionJobKind::Incremental,
        state: ProjectionJobState::Succeeded,
        available_at: now,
        attempts: 1,
        max_attempts: 5,
        lease_until: None,
        last_error: None,
        dead_lettered_at: None,
        rebuild_id: None,
        version: 2,
        created_at: now,
        updated_at: now,
        published_at: now - chrono::Duration::seconds(MAX_PROJECTION_STATUS_FRESHNESS_SECONDS),
    };
    assert!(status.is_fresh_at(now));
    status.published_at =
        now - chrono::Duration::seconds(MAX_PROJECTION_STATUS_FRESHNESS_SECONDS + 1);
    assert!(!status.is_fresh_at(now));
    status.published_at = now + chrono::Duration::seconds(1);
    assert!(!status.is_fresh_at(now));
}

#[tokio::test]
async fn status_page_rejects_unbounded_or_ambiguous_filters_before_database_access() {
    let repository = repository();
    assert!(
        repository
            .list_status_page(None, None, None, None, MAX_PROJECTION_STATUS_PAGE_SIZE + 1)
            .await
            .is_err()
    );
    assert!(
        repository
            .list_status_page(None, Some("holding-opaque"), None, None, 10)
            .await
            .is_err()
    );
    let cursor = ProjectionStatusCursor::new(
        "tenant-a",
        "holding-a",
        "operations",
        "job-a",
        Uuid::new_v4(),
    )
    .expect("cursor");
    assert!(
        repository
            .list_status_page(Some("tenant-b"), None, None, Some(&cursor), 10)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn read_status_adapter_rejects_invalid_requests_before_database_access() {
    let repository = read_repository();
    assert!(
        repository
            .list_status_page(None, None, None, None, MAX_PROJECTION_STATUS_PAGE_SIZE + 1)
            .await
            .is_err()
    );
    assert!(
        repository
            .list_status_page(None, Some("holding-opaque"), None, None, 10)
            .await
            .is_err()
    );
}

#[test]
fn status_page_type_is_explicitly_keyset_paginated() {
    let page = TenantProjectionStatusPage {
        statuses: Vec::new(),
        has_more: false,
        next_cursor: None,
    };
    assert!(!page.has_more);
    assert!(page.next_cursor.is_none());
}
