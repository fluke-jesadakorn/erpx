use erpx_infrastructure::{
    ControlPlanePools, HoldingProjectionStatus, ProjectionJobKind, ProjectionJobState,
    SqlxControlPlane, SqlxHoldingProjectionStatusProjectionStore,
};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

fn store() -> SqlxHoldingProjectionStatusProjectionStore {
    let write = PgPool::connect_lazy("postgres://invalid/write").expect("lazy write");
    let read = PgPool::connect_lazy("postgres://invalid/read").expect("lazy read");
    let control = Arc::new(SqlxControlPlane::new(ControlPlanePools {
        write: write.clone(),
        read: read.clone(),
    }));
    SqlxHoldingProjectionStatusProjectionStore::from_pools(
        control,
        ControlPlanePools { write, read },
    )
}

#[tokio::test]
async fn invalid_status_is_rejected_before_database_access() {
    let store = store();
    let status = HoldingProjectionStatus {
        holding_id: "holding".into(),
        data_profile_key: "holding".into(),
        projection_name: "operations".into(),
        job_key: "job".into(),
        job_id: Uuid::new_v4(),
        kind: ProjectionJobKind::Incremental,
        state: ProjectionJobState::Pending,
        available_at: chrono::Utc::now(),
        attempts: 2,
        max_attempts: 1,
        lease_until: None,
        last_error: None,
        dead_lettered_at: None,
        rebuild_id: None,
        version: 1,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        published_at: chrono::Utc::now(),
    };
    assert!(store.upsert_status(&status).await.is_err());
}

#[test]
fn typed_status_shape_contains_no_payload_or_claim_identity() {
    let status = HoldingProjectionStatus {
        holding_id: "holding".into(),
        data_profile_key: "holding".into(),
        projection_name: "operations".into(),
        job_key: "job".into(),
        job_id: Uuid::new_v4(),
        kind: ProjectionJobKind::Incremental,
        state: ProjectionJobState::Pending,
        available_at: chrono::Utc::now(),
        attempts: 0,
        max_attempts: 5,
        lease_until: None,
        last_error: None,
        dead_lettered_at: None,
        rebuild_id: None,
        version: 1,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        published_at: chrono::Utc::now(),
    };
    let debug = format!("{status:?}");
    assert!(!debug.contains("payload"));
    assert!(!debug.contains("worker_id"));
    assert!(!debug.contains("claim_id"));
}
