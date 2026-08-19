//! Durable rebuild-job/rebuild-record binding and rollback acceptance.

use std::sync::Arc;

use chrono::{Duration, Utc};
use erpx_application::RoleAssignmentStore;
use erpx_domain::{
    access::{RiskTier, RoleAssignment},
    contracts::AuthorityScope as Scope,
};
use erpx_infrastructure::{
    ControlPlaneDatabaseConfig, ControlPlanePools, HoldingHoldingBinding,
    HoldingProjectionSchedulerError, HoldingProjectionSchedulerRepository,
    HoldingStoreRegistration, ProjectionJobKind, ProjectionPolicy, ProjectionScheduleRequest,
    ProjectionSchedulerAuthorization, RebuildProjectionRequest, SqlxControlPlane,
    SqlxHoldingStoreRegistry,
};
use erpx_migration::{migrate_control_plane, migrate_control_read};
use serde_json::json;
use uuid::Uuid;

fn config() -> ControlPlaneDatabaseConfig {
    ControlPlaneDatabaseConfig {
        write_url: std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required"),
        read_url: std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required"),
    }
}

#[tokio::test]
#[ignore = "requires two disposable PostgreSQL control databases"]
async fn rebuild_key_conflict_rejects_cross_job_link_and_rolls_back_scheduler_job() {
    let pools = ControlPlanePools::connect(&config())
        .await
        .expect("control pools");
    migrate_control_plane(&pools.write)
        .await
        .expect("control write migrations");
    migrate_control_read(&pools.read)
        .await
        .expect("control read migrations");

    let control = Arc::new(SqlxControlPlane::new(pools));
    let registry = Arc::new(SqlxHoldingStoreRegistry::new(Arc::clone(&control)));
    let holding_id = Uuid::new_v4();
    let binding = HoldingHoldingBinding::new(
        format!("scheduler-atomicity-{}", Uuid::new_v4()),
        holding_id.to_string(),
    )
    .expect("binding");
    registry
        .register(HoldingStoreRegistration {
            holding_id: binding.holding_id.clone(),
            data_profile_key: binding.data_profile_key.clone(),
            write_database_key: "scheduler_atomicity_write".into(),
            read_database_key: "scheduler_atomicity_read".into(),
            storage_namespace: "scheduler_atomicity_storage".into(),
            credential_key_ref: "scheduler_atomicity_credential".into(),
            backup_chain_ref: "scheduler_atomicity_backup".into(),
            state: "active".into(),
            registry_version: 1,
        })
        .await
        .expect("active registration");
    let now = Utc::now();
    RoleAssignmentStore::create(
        control.as_ref(),
        RoleAssignment {
            id: Uuid::new_v4(),
            principal_ref: "scheduler-atomicity-test".into(),
            role_key: "holding_projection_scheduler".into(),
            permissions: vec!["holding.projection.execute".into()],
            functional_domain: "platform_operations".into(),
            authority_level: 10,
            approval_limit_minor: 0,
            scope: Scope::Holding { holding_id },
            valid_from: now - Duration::minutes(1),
            valid_until: now + Duration::hours(1),
            timezone: "UTC".into(),
            risk_tier: RiskTier::PlatformRoot,
            grant_reason: "scheduler atomicity acceptance".into(),
            ticket_reference: Some("scheduler-atomicity-acceptance".into()),
            grantor: "platform-root".into(),
            approver: Some("platform-root".into()),
            renewal_reviewer: Some("platform-root".into()),
            parent_assignment_id: None,
            assignment_version: 1,
        },
    )
    .await
    .expect("scheduler authorization role");
    let repository = HoldingProjectionSchedulerRepository::new(
        control.pools().write.clone(),
        Arc::clone(&registry),
        ProjectionSchedulerAuthorization::new(
            "scheduler-atomicity-test",
            "scheduler rebuild atomicity test",
            Uuid::new_v4(),
        )
        .expect("authorization"),
    );
    let policy = ProjectionPolicy::new(10, 1, 3, 1, 10).expect("policy");
    let projection_name = "scheduler_atomicity_projection";
    let rebuild_key = "rebuild-conflict";
    let conflicting_job = repository
        .schedule(
            ProjectionScheduleRequest {
                binding: binding.clone(),
                projection_name: projection_name.into(),
                job_key: "different-job".into(),
                kind: ProjectionJobKind::Rebuild,
                available_at: now,
                policy,
                payload: json!({"conflict": true}),
            },
            now,
        )
        .await
        .expect("conflicting scheduler job");
    let conflicting_rebuild_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO erpx_control.holding_projection_rebuilds
            (id, holding_id, data_profile_key, projection_name, rebuild_key, job_id,
             state, payload, requested_at, updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,'pending',$7,$8,$8)",
    )
    .bind(conflicting_rebuild_id)
    .bind(&binding.holding_id)
    .bind(&binding.data_profile_key)
    .bind(projection_name)
    .bind(rebuild_key)
    .bind(conflicting_job.id)
    .bind(json!({"conflict": true}))
    .bind(now)
    .execute(&control.pools().write)
    .await
    .expect("seed cross-job rebuild record");

    let error = repository
        .enqueue_rebuild(
            RebuildProjectionRequest {
                binding: binding.clone(),
                projection_name: projection_name.into(),
                rebuild_key: rebuild_key.into(),
                available_at: now,
                policy,
                payload: json!({"candidate_read_database_key": "candidate_read"}),
            },
            now,
        )
        .await
        .expect_err("cross-job rebuild ownership must fail closed");
    assert_eq!(
        error,
        HoldingProjectionSchedulerError::ScheduleIdempotencyConflict
    );

    let target_job_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM erpx_control.holding_projection_jobs
         WHERE holding_id=$1 AND data_profile_key=$2
           AND projection_name=$3 AND job_key=$4",
    )
    .bind(&binding.holding_id)
    .bind(&binding.data_profile_key)
    .bind(projection_name)
    .bind(rebuild_key)
    .fetch_one(&control.pools().write)
    .await
    .expect("rolled-back scheduler job count");
    assert_eq!(target_job_count, 0);

    let linked_job_id: Uuid = sqlx::query_scalar(
        "SELECT job_id FROM erpx_control.holding_projection_rebuilds
         WHERE id=$1",
    )
    .bind(conflicting_rebuild_id)
    .fetch_one(&control.pools().write)
    .await
    .expect("existing rebuild record");
    assert_eq!(linked_job_id, conflicting_job.id);
}
