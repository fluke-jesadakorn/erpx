use chrono::{Duration, Utc};
use erpx_infrastructure::{
    ControlPlaneDatabaseConfig, ControlPlanePools, SqlxControlPlane,
    SqlxTenantProjectionStatusProjectionStore, SqlxTenantStoreRegistry, TenantStoreRegistration,
};
use erpx_migration::{migrate_control_plane, migrate_control_read};
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

fn test_config() -> ControlPlaneDatabaseConfig {
    ControlPlaneDatabaseConfig {
        write_url: std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required for this ignored test"),
        read_url: std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required for this ignored test"),
    }
}

#[tokio::test]
#[ignore = "requires two disposable PostgreSQL databases"]
async fn incremental_status_publication_advances_a_durable_checkpoint_idempotently() {
    let config = test_config();
    assert_ne!(config.write_url, config.read_url);
    let pools = ControlPlanePools::connect(&config)
        .await
        .expect("control pools");
    migrate_control_plane(&pools.write)
        .await
        .expect("control-write migrations");
    migrate_control_read(&pools.read)
        .await
        .expect("control-read migrations");

    let control = Arc::new(SqlxControlPlane::new(pools.clone()));
    SqlxTenantStoreRegistry::new(Arc::clone(&control))
        .register(TenantStoreRegistration {
            tenant_id: "tenant-incremental".into(),
            holding_ref: "holding-incremental".into(),
            write_database_key: "tenant_incremental_write".into(),
            read_database_key: "tenant_incremental_read".into(),
            storage_namespace: "tenant_incremental_storage".into(),
            credential_key_ref: "tenant_incremental_credential".into(),
            backup_chain_ref: "tenant_incremental_backup".into(),
            state: "active".into(),
            registry_version: 1,
        })
        .await
        .expect("incremental tenant registration");
    let projection = SqlxTenantProjectionStatusProjectionStore::new(control);
    projection
        .publish_read_model()
        .await
        .expect("drain pre-existing status work");
    let job_id = Uuid::new_v4();
    let checkpoint_updated_at: Option<chrono::DateTime<Utc>> = sqlx::query_scalar(
        "SELECT source_updated_at
         FROM erpx_control.tenant_projection_status_checkpoints
         WHERE projection_name = 'tenant_projection_status'",
    )
    .fetch_optional(&pools.read)
    .await
    .expect("read existing checkpoint");
    let source_updated_at = [
        Utc::now() - Duration::seconds(30),
        checkpoint_updated_at
            .map(|updated_at| updated_at + Duration::microseconds(1))
            .unwrap_or_else(Utc::now),
    ]
    .into_iter()
    .max()
    .expect("source timestamp");
    sqlx::query(
        "INSERT INTO erpx_control.tenant_projection_jobs
            (id, tenant_id, holding_ref, projection_name, job_key, kind, state,
             available_at, page_size, batch_size, max_attempts,
             backoff_base_seconds, backoff_max_seconds, payload, request_hash,
             version, created_at, updated_at)
         VALUES ($1, 'tenant-incremental', 'holding-incremental', 'status', 'job-1',
                 'incremental', 'pending', $2, 10, 1, 3, 1, 10, '{}'::jsonb,
                 repeat('a', 64), 1, $2, $2)",
    )
    .bind(job_id)
    .bind(source_updated_at)
    .execute(&pools.write)
    .await
    .expect("source scheduler row");

    assert_eq!(
        projection
            .publish_read_model()
            .await
            .expect("first publish"),
        1
    );
    assert_eq!(
        projection
            .publish_read_model()
            .await
            .expect("replay publish"),
        0
    );

    let checkpoint = sqlx::query(
        "SELECT source_updated_at, source_job_id
         FROM erpx_control.tenant_projection_status_checkpoints
         WHERE projection_name = 'tenant_projection_status'",
    )
    .fetch_one(&pools.read)
    .await
    .expect("checkpoint");
    let checkpoint_updated_at: chrono::DateTime<Utc> =
        checkpoint.try_get("source_updated_at").expect("updated at");
    let checkpoint_job_id: Uuid = checkpoint.try_get("source_job_id").expect("job id");
    assert_eq!(checkpoint_updated_at, source_updated_at);
    assert_eq!(checkpoint_job_id, job_id);

    let later_updated_at = source_updated_at + Duration::seconds(1);
    sqlx::query(
        "UPDATE erpx_control.tenant_projection_jobs
         SET state = 'succeeded', attempts = 1, version = 2, updated_at = $2
         WHERE id = $1",
    )
    .bind(job_id)
    .bind(later_updated_at)
    .execute(&pools.write)
    .await
    .expect("source scheduler update");

    assert_eq!(
        projection
            .publish_read_model()
            .await
            .expect("second publish"),
        1
    );
    assert_eq!(
        projection
            .publish_read_model()
            .await
            .expect("second replay"),
        0
    );

    let status = sqlx::query(
        "SELECT state, version, published_at
         FROM erpx_control.tenant_projection_status
         WHERE job_id = $1",
    )
    .bind(job_id)
    .fetch_one(&pools.read)
    .await
    .expect("published status");
    let state: String = status.try_get("state").expect("state");
    let version: i64 = status.try_get("version").expect("version");
    let published_at: chrono::DateTime<Utc> = status.try_get("published_at").expect("published");
    assert_eq!(state, "succeeded");
    assert_eq!(version, 2);
    assert!(published_at <= Utc::now());
}
