use std::{io, sync::Arc};

use chrono::{Duration, Utc};
use erpx_infrastructure::{
    ControlPlaneDatabaseConfig, ControlPlanePools, SqlxControlPlane, SqlxTenantStoreRegistry,
    TenantBackupJobRequest, TenantDatabaseProvisioner, TenantDatabaseProvisioningConfig,
    TenantDatabaseRouter, TenantProvisioner, TenantStoreRegistration,
};
use erpx_migration::{migrate_control_plane, migrate_control_read};
use erpx_worker::{NamedWorkerHandler, TenantBackupJobRuntimeHandler};
use sqlx::{Row, postgres::PgPoolOptions};
use uuid::Uuid;

type TestError = Box<dyn std::error::Error + Send + Sync>;
type TestResult<T = ()> = Result<T, TestError>;

fn test_error(message: &'static str) -> TestError {
    io::Error::other(message).into()
}

fn database_name(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4().simple())
}

fn database_url(base_url: &str, database: &str) -> TestResult<String> {
    let scheme_end = base_url
        .find("://")
        .ok_or_else(|| test_error("invalid control PostgreSQL URL"))?;
    let authority_start = scheme_end + 3;
    let path_start = base_url[authority_start..]
        .find('/')
        .map(|offset| authority_start + offset)
        .unwrap_or(base_url.len());
    let query_start = base_url[path_start..]
        .find('?')
        .map(|offset| path_start + offset)
        .unwrap_or(base_url.len());
    let mut url = String::with_capacity(base_url.len() + database.len() + 1);
    url.push_str(&base_url[..path_start]);
    url.push('/');
    url.push_str(database);
    url.push_str(&base_url[query_start..]);
    Ok(url)
}

#[derive(Debug)]
struct DisposableControlDatabases {
    admin_write_url: String,
    admin_read_url: String,
    write_name: String,
    read_name: String,
}

impl DisposableControlDatabases {
    async fn create(write_base_url: &str, read_base_url: &str) -> TestResult<Self> {
        let databases = Self {
            admin_write_url: database_url(write_base_url, "postgres")?,
            admin_read_url: database_url(read_base_url, "postgres")?,
            write_name: database_name("erpx_backup_external_blocker_write"),
            read_name: database_name("erpx_backup_external_blocker_read"),
        };

        let result = async {
            let write_admin = PgPoolOptions::new()
                .max_connections(2)
                .connect(&databases.admin_write_url)
                .await
                .map_err(|_| test_error("connect control PostgreSQL admin database"))?;
            sqlx::query(&format!("CREATE DATABASE \"{}\"", databases.write_name))
                .execute(&write_admin)
                .await
                .map_err(|_| test_error("create disposable control write database"))?;
            write_admin.close().await;

            let read_admin = PgPoolOptions::new()
                .max_connections(2)
                .connect(&databases.admin_read_url)
                .await
                .map_err(|_| test_error("connect control PostgreSQL read admin database"))?;
            sqlx::query(&format!("CREATE DATABASE \"{}\"", databases.read_name))
                .execute(&read_admin)
                .await
                .map_err(|_| test_error("create disposable control read database"))?;
            read_admin.close().await;

            Ok::<(), TestError>(())
        }
        .await;

        if let Err(error) = result {
            let _ = databases.drop_all().await;
            return Err(error);
        }
        Ok(databases)
    }

    fn config(&self) -> TestResult<ControlPlaneDatabaseConfig> {
        Ok(ControlPlaneDatabaseConfig {
            write_url: database_url(&self.admin_write_url, &self.write_name)?,
            read_url: database_url(&self.admin_read_url, &self.read_name)?,
        })
    }

    async fn drop_all(&self) -> TestResult {
        let mut first_error = None;
        for (admin_url, name) in [
            (&self.admin_read_url, &self.read_name),
            (&self.admin_write_url, &self.write_name),
        ] {
            let admin = match PgPoolOptions::new()
                .max_connections(2)
                .connect(admin_url)
                .await
            {
                Ok(admin) => admin,
                Err(_) => {
                    if first_error.is_none() {
                        first_error =
                            Some(test_error("connect control PostgreSQL cleanup database"));
                    }
                    continue;
                }
            };
            if sqlx::query(
                "SELECT pg_terminate_backend(pid)
                 FROM pg_stat_activity
                 WHERE datname = $1 AND pid <> pg_backend_pid()",
            )
            .bind(name)
            .execute(&admin)
            .await
            .is_err()
                && first_error.is_none()
            {
                first_error = Some(test_error(
                    "terminate disposable control database connections",
                ));
            }
            if sqlx::query(&format!("DROP DATABASE IF EXISTS \"{}\"", name))
                .execute(&admin)
                .await
                .is_err()
                && first_error.is_none()
            {
                first_error = Some(test_error("drop disposable control database"));
            }
            admin.close().await;
        }
        first_error.map_or(Ok(()), Err)
    }
}

#[tokio::test]
#[ignore = "requires two disposable control PostgreSQL databases"]
async fn missing_external_backup_credentials_leave_failed_run_and_retryable_blocker_evidence() {
    let write_base_url = std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required");
    let read_base_url = std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
        .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required");
    // The worker claims the shared due-job queue. Give this test its own
    // control databases so unrelated ignored tests cannot add claims and
    // change the report cardinality under test.
    let disposable = DisposableControlDatabases::create(&write_base_url, &read_base_url)
        .await
        .expect("create disposable isolated control databases");
    let pools = ControlPlanePools::connect(&disposable.config().expect("control test config"))
        .await
        .expect("control pools");
    migrate_control_plane(&pools.write)
        .await
        .expect("control write migration");
    migrate_control_read(&pools.read)
        .await
        .expect("control read migration");

    let store = Arc::new(SqlxControlPlane::new(pools));
    let suffix = Uuid::new_v4().simple().to_string();
    let tenant_id = format!("tenant-backup-blocker-{suffix}");
    let holding_ref = format!("holding-backup-blocker-{suffix}");
    SqlxTenantStoreRegistry::new(Arc::clone(&store))
        .register(TenantStoreRegistration {
            tenant_id: tenant_id.clone(),
            holding_ref: holding_ref.clone(),
            write_database_key: format!("backup-blocker-write-{suffix}"),
            read_database_key: format!("backup-blocker-read-{suffix}"),
            storage_namespace: format!("backup-blocker-storage-{suffix}"),
            credential_key_ref: format!("backup-blocker-credential-{suffix}"),
            backup_chain_ref: format!("backup-blocker-chain-{suffix}"),
            state: "active".into(),
            registry_version: 1,
        })
        .await
        .expect("active tenant registry");

    let job = TenantProvisioner::new(Arc::clone(&store))
        .schedule_backup_job(TenantBackupJobRequest {
            tenant_id: tenant_id.clone(),
            holding_ref: holding_ref.clone(),
            kind: "full".into(),
            backup_ref: format!("backup-blocker-{suffix}"),
            restore_target_ref: None,
            pitr_recovery_point: None,
            candidate_read_database_key: None,
            source_version: None,
            source_projection: "operations".into(),
            available_at: Utc::now(),
            idempotency_key: format!("backup-blocker-job-{suffix}"),
            max_attempts: 2,
        })
        .await
        .expect("schedule backup job");

    let routing = TenantDatabaseProvisioningConfig::from_values(
        "postgres://backup-blocker-admin@localhost/postgres?sslmode=verify-full",
        "postgres://backup-blocker-user@localhost/{database}?sslmode=verify-full",
    )
    .expect("routing config");
    let database_provisioner = TenantDatabaseProvisioner::new(Arc::clone(&store), routing.clone())
        .expect("database provisioner");
    let router = TenantDatabaseRouter::new(Arc::clone(&store), routing).expect("database router");
    let provisioner = TenantProvisioner::new(Arc::clone(&store));
    let handler = TenantBackupJobRuntimeHandler::from_parts_without_external_adapter(
        provisioner.clone(),
        database_provisioner,
        router,
        10,
        Duration::seconds(30),
    )
    .expect("fail-closed backup handler");

    let report = handler
        .run_once(Uuid::new_v4())
        .await
        .expect("worker records the blocker instead of claiming success");
    assert_eq!(report.claimed, 1);
    assert_eq!(report.processed, 0);
    assert_eq!(report.retried, 1);
    assert_eq!(report.dead_lettered, 0);

    let job_row = sqlx::query(
        "SELECT state, last_error
         FROM erpx_control.tenant_backup_jobs
         WHERE id = $1",
    )
    .bind(job.id)
    .fetch_one(&store.pools().write)
    .await
    .expect("read durable blocker job");
    assert_eq!(
        job_row.try_get::<String, _>("state").expect("job state"),
        "retryable"
    );
    assert_eq!(
        job_row
            .try_get::<Option<String>, _>("last_error")
            .expect("job error"),
        Some("backup_external_credentials_unavailable".into())
    );

    let run_row = sqlx::query(
        "SELECT r.status, r.error
         FROM erpx_control.tenant_backup_job_runs link
         JOIN erpx_control.tenant_backup_runs r ON r.id = link.run_id
         WHERE link.job_id = $1",
    )
    .bind(job.id)
    .fetch_one(&store.pools().write)
    .await
    .expect("read failed backup run");
    assert_eq!(
        run_row.try_get::<String, _>("status").expect("run status"),
        "failed"
    );
    assert_eq!(
        run_row
            .try_get::<Option<String>, _>("error")
            .expect("run error"),
        Some("backup external credentials unavailable".into())
    );

    provisioner
        .project_backup_jobs_to_read(100)
        .await
        .expect("project blocker job");
    let projected: (String, bool) = sqlx::query_as(
        "SELECT state, error_present
         FROM erpx_control.tenant_backup_jobs
         WHERE id = $1",
    )
    .bind(job.id)
    .fetch_one(&store.pools().read)
    .await
    .expect("read projected blocker job");
    assert_eq!(projected, ("retryable".into(), true));

    store.pools().write.close().await;
    store.pools().read.close().await;
    disposable
        .drop_all()
        .await
        .expect("drop disposable isolated control databases");
}
