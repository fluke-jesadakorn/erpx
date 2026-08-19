use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use erpx_infrastructure::{
    ControlPlaneDatabaseConfig, ControlPlanePools, HoldingHoldingBinding,
    HoldingProjectionSchedulerError, HoldingProjectionSchedulerRepository,
    HoldingStoreRegistration, ProjectionJobKind, ProjectionJobState, ProjectionPolicy,
    ProjectionScheduleRequest, ProjectionSchedulerAuthorization, SqlxControlPlane,
    SqlxHoldingStoreRegistry,
};
use erpx_migration::{migrate_control_plane, migrate_control_read};
use serde_json::json;
use uuid::Uuid;

fn test_config() -> ControlPlaneDatabaseConfig {
    ControlPlaneDatabaseConfig {
        write_url: std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required for this ignored test"),
        read_url: std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required for this ignored test"),
    }
}

fn schedule_request(binding: HoldingHoldingBinding) -> ProjectionScheduleRequest {
    ProjectionScheduleRequest {
        binding,
        projection_name: "scheduler_authorization_test".into(),
        job_key: "job-1".into(),
        kind: ProjectionJobKind::Incremental,
        available_at: Utc::now(),
        policy: ProjectionPolicy::new(10, 1, 3, 1, 10).expect("policy"),
        payload: json!({"test": true}),
    }
}

#[tokio::test]
#[ignore = "requires two disposable PostgreSQL control databases"]
async fn scheduler_rejects_unregistered_and_inactive_bindings_before_job_mutation() {
    let pools = ControlPlanePools::connect(&test_config())
        .await
        .expect("control pools");
    migrate_control_plane(&pools.write)
        .await
        .expect("write migration");
    migrate_control_read(&pools.read)
        .await
        .expect("read migration");

    let binding_constraints: Vec<String> = sqlx::query_scalar(
        "SELECT conname::text
         FROM pg_constraint
         WHERE conname IN (
             'holding_projection_jobs_holding_holding_fk',
             'holding_projection_claim_batches_holding_holding_fk',
             'holding_projection_claims_holding_holding_fk',
             'holding_projection_claims_job_binding_fk',
             'holding_projection_claims_batch_binding_fk',
             'holding_projection_rebuilds_holding_holding_fk',
             'holding_projection_rebuilds_job_binding_fk'
         )
         ORDER BY conname",
    )
    .fetch_all(&pools.write)
    .await
    .expect("scheduler binding constraints");
    assert_eq!(binding_constraints.len(), 7);

    let store = Arc::new(SqlxControlPlane::new(pools));
    let registry = Arc::new(SqlxHoldingStoreRegistry::new(Arc::clone(&store)));
    let authorization = ProjectionSchedulerAuthorization::new(
        "scheduler-test-worker",
        "scheduler authorization integration test",
        Uuid::new_v4(),
    )
    .expect("authorization");
    let repository = HoldingProjectionSchedulerRepository::new(
        store.pools().write.clone(),
        Arc::clone(&registry),
        authorization,
    );
    let now: DateTime<Utc> = Utc::now();
    let missing = repository
        .schedule(
            schedule_request(
                HoldingHoldingBinding::new("missing-scheduler-holding", Uuid::new_v4().to_string())
                    .expect("binding"),
            ),
            now,
        )
        .await;
    assert!(matches!(
        missing,
        Err(HoldingProjectionSchedulerError::InvalidRequest(message))
            if message.contains("not registered")
    ));

    let data_profile_key = Uuid::new_v4().to_string();
    registry
        .register(HoldingStoreRegistration {
            holding_id: "inactive-scheduler-holding".into(),
            data_profile_key: data_profile_key.clone(),
            write_database_key: "inactive_scheduler_write".into(),
            read_database_key: "inactive_scheduler_read".into(),
            storage_namespace: "inactive_scheduler_storage".into(),
            credential_key_ref: "inactive_scheduler_credential".into(),
            backup_chain_ref: "inactive_scheduler_backup".into(),
            state: "suspended".into(),
            registry_version: 1,
        })
        .await
        .expect("inactive registration");
    let inactive = repository
        .schedule(
            schedule_request(
                HoldingHoldingBinding::new("inactive-scheduler-holding", data_profile_key)
                    .expect("binding"),
            ),
            now + Duration::seconds(1),
        )
        .await;
    assert!(matches!(
        inactive,
        Err(HoldingProjectionSchedulerError::InvalidRequest(message))
            if message.contains("not active")
    ));

    let active_holding = Uuid::new_v4().to_string();
    let active_binding =
        HoldingHoldingBinding::new("active-scheduler-holding", active_holding.clone())
            .expect("active binding");
    registry
        .register(HoldingStoreRegistration {
            holding_id: active_binding.holding_id.clone(),
            data_profile_key: active_holding.clone(),
            write_database_key: "active_scheduler_write".into(),
            read_database_key: "active_scheduler_read".into(),
            storage_namespace: "active_scheduler_storage".into(),
            credential_key_ref: "active_scheduler_credential".into(),
            backup_chain_ref: "active_scheduler_backup".into(),
            state: "active".into(),
            registry_version: 1,
        })
        .await
        .expect("active registration");
    let (generation, rebuild) = repository
        .begin_rebuild_after_platform_authorization(
            active_binding.clone(),
            "operations_read_model",
            "active_scheduler_candidate",
            4,
            ProjectionPolicy::new(500, 1, 5, 5, 300).expect("policy"),
            now,
        )
        .await
        .expect("atomic platform-authorized durable rebuild");
    assert_eq!(generation, 1);
    assert_eq!(rebuild.job.kind, ProjectionJobKind::Rebuild);
    assert_eq!(rebuild.job.state, ProjectionJobState::RebuildPending);
    assert_eq!(rebuild.job.payload["generation"], 1);
    let claim_key = "stale-rebuild-claim";
    let claim_id = Uuid::new_v4();
    let worker_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO erpx_control.holding_projection_claim_batches
            (claim_key, holding_id, data_profile_key, worker_id, sealed, result_count, created_at)
         VALUES ($1,$2,$3,$4,false,0,$5)",
    )
    .bind(claim_key)
    .bind(&active_binding.holding_id)
    .bind(&active_binding.data_profile_key)
    .bind(worker_id)
    .bind(now + Duration::seconds(1))
    .execute(&store.pools().write)
    .await
    .expect("seed stale rebuild claim batch");
    sqlx::query(
        "INSERT INTO erpx_control.holding_projection_claims
            (claim_id, claim_key, job_id, holding_id, data_profile_key, worker_id,
             claimed_at, lease_until, state)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'active')",
    )
    .bind(claim_id)
    .bind(claim_key)
    .bind(rebuild.job.id)
    .bind(&active_binding.holding_id)
    .bind(&active_binding.data_profile_key)
    .bind(worker_id)
    .bind(now + Duration::seconds(1))
    .bind(now + Duration::minutes(10))
    .execute(&store.pools().write)
    .await
    .expect("seed stale rebuild claim");
    sqlx::query(
        "UPDATE erpx_control.holding_projection_jobs
         SET state='claimed', current_claim_id=$2, worker_id=$3,
             lease_until=$4, version=version+1, updated_at=$5
         WHERE id=$1",
    )
    .bind(rebuild.job.id)
    .bind(claim_id)
    .bind(worker_id)
    .bind(now + Duration::minutes(10))
    .bind(now + Duration::seconds(1))
    .execute(&store.pools().write)
    .await
    .expect("seed claimed stale rebuild job");
    assert_eq!(
        repository
            .terminalize_stale_rebuild(
                &active_binding,
                "operations_read_model",
                "active_scheduler_candidate",
                4,
                generation,
                now + Duration::seconds(2),
            )
            .await
            .expect("terminalize stale rebuild"),
        1
    );
    let state: String = sqlx::query_scalar(
        "SELECT state FROM erpx_control.holding_projection_jobs
         WHERE id=$1",
    )
    .bind(rebuild.job.id)
    .fetch_one(&store.pools().write)
    .await
    .expect("stale rebuild state");
    assert_eq!(state, "dead_letter");
    let claim_state: (String, Option<String>) = sqlx::query_as(
        "SELECT state, outcome FROM erpx_control.holding_projection_claims
         WHERE job_id=$1",
    )
    .bind(rebuild.job.id)
    .fetch_one(&store.pools().write)
    .await
    .expect("stale rebuild claim state");
    assert_eq!(
        claim_state,
        (
            "dead_letter".into(),
            Some("projection_candidate_source_moved".into())
        )
    );
    let rebuild_state: String = sqlx::query_scalar(
        "SELECT state FROM erpx_control.holding_projection_rebuilds
         WHERE job_id=$1",
    )
    .bind(rebuild.job.id)
    .fetch_one(&store.pools().write)
    .await
    .expect("stale rebuild record state");
    assert_eq!(rebuild_state, "dead_letter");
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_control.audit_events
         WHERE action = 'platform.projection_rebuild.start'",
    )
    .fetch_one(&store.pools().write)
    .await
    .expect("atomic rebuild audit");
    assert_eq!(audit_count, 1);
}
