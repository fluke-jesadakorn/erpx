use std::{io, time::Duration};

use erpx_infrastructure::{
    ControlPlaneDatabaseConfig, ControlPlanePoolSettings, ControlPlanePools, HoldingBackupRun,
    HoldingDatabaseProvisioner, HoldingDatabaseProvisioningConfig, HoldingDatabaseRouter,
    HoldingProvisioner, HoldingSourceProjection, HoldingStoreRegistration, PersistenceError,
    RoutedDatabaseSide, SqlxControlPlane,
};
use erpx_migration::{migrate_control_plane, migrate_control_read, migrate_holding_read};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

fn test_error(message: &'static str) -> Box<dyn std::error::Error + Send + Sync> {
    io::Error::other(message).into()
}

async fn connect_database_with_retry(
    url: &str,
    failure: &'static str,
) -> Result<PgPool, Box<dyn std::error::Error + Send + Sync>> {
    const ATTEMPTS: u32 = 5;

    for attempt in 0..ATTEMPTS {
        match PgPoolOptions::new().max_connections(2).connect(url).await {
            Ok(pool) => return Ok(pool),
            Err(_) if attempt + 1 < ATTEMPTS => {
                tokio::time::sleep(Duration::from_millis(100 * (1 << attempt))).await;
            }
            Err(_) => return Err(test_error(failure)),
        }
    }

    unreachable!("the bounded connection loop always returns")
}

fn database_url(
    base_url: &str,
    database: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let scheme_end = base_url
        .find("://")
        .ok_or_else(|| test_error("database URL is missing a scheme"))?;
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

fn template_url(
    template: &str,
    database: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    if template.matches("{database}").count() != 1 {
        return Err(test_error("holding database URL template is invalid"));
    }
    Ok(template.replace("{database}", database))
}

fn database_name(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4().simple())
}

#[derive(Debug)]
struct DisposableDatabases {
    admin_url: String,
    names: Vec<String>,
}

impl DisposableDatabases {
    async fn create(
        admin_url: String,
        names: Vec<String>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let admin = PgPoolOptions::new()
            .max_connections(2)
            .connect(&admin_url)
            .await
            .map_err(|_| test_error("connect provisioning admin database"))?;
        let result = async {
            for name in &names {
                let statement = format!("CREATE DATABASE \"{name}\"");
                sqlx::query(&statement)
                    .execute(&admin)
                    .await
                    .map_err(|_| test_error("create disposable provisioning database"))?;
            }
            Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
        }
        .await;
        admin.close().await;
        if let Err(error) = result {
            let cleanup = Self { admin_url, names };
            let _ = cleanup.drop_all().await;
            return Err(error);
        }
        Ok(Self { admin_url, names })
    }

    async fn drop_all(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let admin = PgPoolOptions::new()
            .max_connections(2)
            .connect(&self.admin_url)
            .await
            .map_err(|_| test_error("connect provisioning admin database for cleanup"))?;
        let mut first_error = None;
        for name in self.names.iter().rev() {
            let terminate = sqlx::query(
                "SELECT pg_terminate_backend(pid)
                 FROM pg_stat_activity
                 WHERE datname = $1 AND pid <> pg_backend_pid()",
            )
            .bind(name)
            .execute(&admin)
            .await;
            if terminate.is_err() && first_error.is_none() {
                first_error = Some(test_error("terminate disposable database connections"));
            }
            let statement = format!("DROP DATABASE IF EXISTS \"{name}\"");
            if sqlx::query(&statement).execute(&admin).await.is_err() && first_error.is_none() {
                first_error = Some(test_error("drop disposable provisioning database"));
            }
        }
        admin.close().await;
        first_error.map_or(Ok(()), Err)
    }
}

async fn migrate_candidate_read(url: &str) -> TestResult {
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(url)
        .await
        .map_err(|_| test_error("connect replacement read database"))?;
    migrate_holding_read(&pool)
        .await
        .map_err(|_| test_error("migrate replacement read database"))?;
    pool.close().await;
    Ok(())
}

async fn assert_forced_rls(pool: &PgPool) -> TestResult {
    let unprotected: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM pg_class AS c
         JOIN pg_namespace AS n ON n.oid = c.relnamespace
         WHERE n.nspname = 'erpx_holding'
           AND c.relkind IN ('r', 'p')
           AND (NOT c.relrowsecurity OR NOT c.relforcerowsecurity)",
    )
    .fetch_one(pool)
    .await
    .map_err(|_| test_error("inspect holding RLS metadata"))?;
    if unprotected != 0 {
        return Err(test_error(
            "holding relation is not protected by forced RLS",
        ));
    }
    Ok(())
}

async fn run_real_provisioning_replacement() -> TestResult {
    let admin_url = std::env::var("ERPX_TEST_ADMIN_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/postgres?sslmode=verify-full".into());
    let holding_template = std::env::var("ERPX_TEST_HOLDING_DATABASE_URL_TEMPLATE")
        .unwrap_or_else(|_| "postgres://localhost/{database}?sslmode=verify-full".into());
    let control_write_key = database_name("erpx_prv007_cw");
    let control_read_key = database_name("erpx_prv007_cr");
    let holding_write_key = database_name("erpx_prv007_tw");
    let holding_read_key = database_name("erpx_prv007_tr");
    let managed_candidate_key = database_name("erpx_prv020_rm");
    let failed_candidate_key = database_name("erpx_prv007_rf");
    let successful_candidate_key = database_name("erpx_prv007_rs");
    let database_names = vec![
        control_write_key.clone(),
        control_read_key.clone(),
        holding_write_key.clone(),
        holding_read_key.clone(),
        managed_candidate_key.clone(),
        failed_candidate_key.clone(),
        successful_candidate_key.clone(),
    ];
    let disposable = DisposableDatabases::create(admin_url.clone(), database_names).await?;

    let result = async {
        let control_config = ControlPlaneDatabaseConfig {
            write_url: database_url(&admin_url, &control_write_key)?,
            read_url: database_url(&admin_url, &control_read_key)?,
        };
        let pools = ControlPlanePools::connect_with_settings(
            &control_config,
            ControlPlanePoolSettings {
                write_max_connections: 3,
                read_max_connections: 3,
                ..ControlPlanePoolSettings::default()
            },
        )
        .await
        .map_err(|_| test_error("connect disposable control databases"))?;
        migrate_control_plane(&pools.write)
            .await
            .map_err(|_| test_error("migrate disposable control write database"))?;
        migrate_control_read(&pools.read)
            .await
            .map_err(|_| test_error("migrate disposable control read database"))?;
        let store = std::sync::Arc::new(SqlxControlPlane::new(pools));
        let provisioner = HoldingProvisioner::new(store.clone());
        let holding_id = Uuid::new_v4().to_string();
        let data_profile_key = format!("holding-prv007-{}", Uuid::new_v4().simple());
        provisioner
            .begin(HoldingStoreRegistration {
                holding_id: holding_id.clone(),
                data_profile_key: data_profile_key.clone(),
                write_database_key: holding_write_key.clone(),
                read_database_key: holding_read_key.clone(),
                storage_namespace: format!("storage-prv007-{}", Uuid::new_v4().simple()),
                credential_key_ref: "credential-ref-prv007".into(),
                backup_chain_ref: "backup-chain-prv007".into(),
                state: "provisioning".into(),
                registry_version: 1,
            })
            .await
            .map_err(|_| test_error("register disposable holding"))?;

        let routing_config = HoldingDatabaseProvisioningConfig::from_values(
            admin_url.clone(),
            holding_template.clone(),
        )
        .map_err(|_| test_error("validate holding routing configuration"))?;
        let physical = HoldingDatabaseProvisioner::new(store.clone(), routing_config.clone())
            .map_err(|_| test_error("construct holding database provisioner"))?
            .provision_databases(&holding_id)
            .await
            .map_err(|_| test_error("provision holding write and read databases"))?;
        let repeated = HoldingDatabaseProvisioner::new(store.clone(), routing_config.clone())
            .map_err(|_| test_error("construct idempotent holding provisioner"))?
            .provision_databases(&holding_id)
            .await
            .map_err(|_| test_error("repeat holding database provisioning"))?;
        if physical != repeated {
            return Err(test_error(
                "repeat provisioning changed holding database identity",
            ));
        }

        let write_url = template_url(&holding_template, &holding_write_key)?;
        let read_url = template_url(&holding_template, &holding_read_key)?;
        let write_pool =
            connect_database_with_retry(&write_url, "connect provisioned holding write database")
                .await?;
        let read_pool =
            connect_database_with_retry(&read_url, "connect provisioned holding read database")
                .await?;
        assert_forced_rls(&write_pool).await?;
        assert_forced_rls(&read_pool).await?;
        let write_database: String = sqlx::query_scalar("SELECT current_database()")
            .fetch_one(&write_pool)
            .await
            .map_err(|_| test_error("identify holding write database"))?;
        let read_database: String = sqlx::query_scalar("SELECT current_database()")
            .fetch_one(&read_pool)
            .await
            .map_err(|_| test_error("identify holding read database"))?;
        if write_database != holding_write_key || read_database != holding_read_key {
            return Err(test_error(
                "holding write/read route is not physically isolated",
            ));
        }
        write_pool.close().await;
        read_pool.close().await;

        let router = HoldingDatabaseRouter::new(store.clone(), routing_config.clone())
            .map_err(|_| test_error("construct holding database router"))?;
        if router
            .route(&holding_id, &data_profile_key, RoutedDatabaseSide::Write)
            .await
            .is_ok()
        {
            return Err(test_error("inactive holding was routable"));
        }
        if provisioner.activate(&holding_id).await.is_ok() {
            return Err(test_error("activation bypassed storage health"));
        }
        let failed_storage = HoldingDatabaseProvisioner::new(store.clone(), routing_config.clone())
            .map_err(|_| test_error("construct failed storage provisioner"))?
            .provision_storage_namespace(&holding_id, |_namespace| async {
                Err(erpx_infrastructure::ObjectStorageError::HttpStatus { status: 503 })
            })
            .await;
        if failed_storage.is_ok() || provisioner.activate(&holding_id).await.is_ok() {
            return Err(test_error(
                "failed storage verification did not block activation",
            ));
        }
        let successful_storage =
            HoldingDatabaseProvisioner::new(store.clone(), routing_config.clone())
                .map_err(|_| test_error("construct successful storage provisioner"))?
                .provision_storage_namespace(&holding_id, move |_namespace| async { Ok(()) })
                .await
                .map_err(|_| test_error("verify holding storage namespace"))?;
        if !successful_storage.reachable {
            return Err(test_error(
                "successful storage verification was not reachable",
            ));
        }
        provisioner
            .activate(&holding_id)
            .await
            .map_err(|_| test_error("activate physically provisioned holding"))?;

        let owned_candidate_key = format!("erpx_prv020_owned_{}", Uuid::new_v4().simple());
        let other_holding_id = Uuid::new_v4().to_string();
        provisioner
            .begin(HoldingStoreRegistration {
                holding_id: other_holding_id,
                data_profile_key: format!("holding-prv020-owner-{}", Uuid::new_v4().simple()),
                write_database_key: format!("erpx_prv020_other_write_{}", Uuid::new_v4().simple()),
                read_database_key: owned_candidate_key.clone(),
                storage_namespace: format!("storage-prv020-owner-{}", Uuid::new_v4().simple()),
                credential_key_ref: "credential-ref-prv020-owner".into(),
                backup_chain_ref: "backup-chain-prv020-owner".into(),
                state: "provisioning".into(),
                registry_version: 1,
            })
            .await
            .map_err(|_| test_error("register candidate owner holding"))?;
        if provisioner
            .begin_read_rebuild(&holding_id, &owned_candidate_key, 42)
            .await
            .is_ok()
        {
            return Err(test_error("cross-holding candidate ownership was accepted"));
        }

        let routed_write = router
            .route(&holding_id, &data_profile_key, RoutedDatabaseSide::Write)
            .await
            .map_err(|_| test_error("route active holding write database"))?;
        routed_write
            .probe()
            .await
            .map_err(|_| test_error("probe active holding write database"))?;
        if router
            .route(&holding_id, "wrong-holding", RoutedDatabaseSide::Write)
            .await
            .is_ok()
        {
            return Err(test_error("holding route crossed holding scope"));
        }
        let mut scoped = routed_write
            .begin_scoped()
            .await
            .map_err(|_| test_error("begin holding-scoped holding write"))?;
        let scoped_holding: String =
            sqlx::query_scalar("SELECT current_setting('erpx.data_profile_key')")
                .fetch_one(&mut *scoped)
                .await
                .map_err(|_| test_error("read holding-scoped holding setting"))?;
        if scoped_holding != data_profile_key {
            return Err(test_error("holding write transaction lost holding scope"));
        }
        sqlx::query(
            "INSERT INTO erpx_cqrs_write.outbox_events
                (event_id, context_name, partition_key, command_id, event_type,
                 event_version, aggregate_type, aggregate_id, aggregate_version,
                 active_scope, actor, correlation_id, causation_id, payload,
                 occurred_at, available_at)
             VALUES ($1, 'operations', $2, $3, 'operations.restore-test-fixture',
                     1, 'fixture', $4, 1, $5, $6, $7, NULL, '{}'::jsonb, $8, $8)",
        )
        .bind(Uuid::new_v4())
        .bind(&data_profile_key)
        .bind(Uuid::new_v4())
        .bind(Uuid::new_v4())
        .bind(serde_json::json!({"data_profile_key": data_profile_key}))
        .bind(serde_json::json!({"actor_ref": "restore-test-fixture"}))
        .bind(Uuid::new_v4())
        .bind(chrono::Utc::now())
        .execute(&mut *scoped)
        .await
        .map_err(|_| test_error("insert source watermark fixture"))?;
        scoped
            .commit()
            .await
            .map_err(|_| test_error("commit holding-scoped holding write"))?;
        routed_write.close().await;

        migrate_candidate_read(&template_url(&holding_template, &failed_candidate_key)?).await?;
        migrate_candidate_read(&template_url(&holding_template, &successful_candidate_key)?)
            .await?;
        let previous_read_key = holding_read_key.clone();
        let failed_run_id = Uuid::new_v4();
        let failed_run = HoldingBackupRun {
            id: failed_run_id,
            holding_id: holding_id.clone(),
            kind: "restore_test".into(),
            status: "started".into(),
            backup_ref: "backup-prv007".into(),
            restore_target_ref: Some("restore-prv007-failed".into()),
            pitr_recovery_point: None,
            started_at: chrono::Utc::now(),
            completed_at: None,
            verified: false,
            error: None,
        };
        if provisioner
            .execute_restore_test_with_read_replacement(
                failed_run,
                &failed_candidate_key,
                42,
                |_registration, _run, _candidate, _source_version| async {
                    Ok::<(bool, bool), PersistenceError>((false, false))
                },
            )
            .await
            .is_ok()
        {
            return Err(test_error("unverified restore test was accepted"));
        }
        let failed_state: (String, bool, Option<String>) = sqlx::query_as(
            "SELECT status, verified, error
             FROM erpx_control.holding_backup_runs WHERE id = $1",
        )
        .bind(failed_run_id)
        .fetch_one(&store.pools().write)
        .await
        .map_err(|_| test_error("read failed restore test state"))?;
        if failed_state.0 != "failed"
            || failed_state.1
            || failed_state.2.as_deref() != Some("restore test was not verified")
        {
            return Err(test_error("failed restore test state was not durable"));
        }
        let after_failed_read: String = sqlx::query_scalar(
            "SELECT read_database_key FROM erpx_control.holding_registry WHERE holding_id = $1",
        )
        .bind(&holding_id)
        .fetch_one(&store.pools().write)
        .await
        .map_err(|_| test_error("read holding registry after failed restore"))?;
        if after_failed_read != previous_read_key {
            return Err(test_error(
                "failed restore test switched the active read database",
            ));
        }
        let failed_switches: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM erpx_control.holding_projection_switches WHERE holding_id = $1",
        )
        .bind(&holding_id)
        .fetch_one(&store.pools().write)
        .await
        .map_err(|_| test_error("count failed restore switches"))?;
        if failed_switches != 0 {
            return Err(test_error(
                "failed restore test created a projection switch",
            ));
        }

        let managed_generation = provisioner
            .begin_read_rebuild_with_generation(&holding_id, &managed_candidate_key, 42)
            .await
            .map_err(|_| test_error("begin managed replacement read rebuild"))?;
        if provisioner
            .complete_read_rebuild_if_generation(
                &holding_id,
                &managed_candidate_key,
                managed_generation.saturating_add(1),
            )
            .await
            .is_ok()
        {
            return Err(test_error("stale replacement generation was accepted"));
        }
        HoldingDatabaseProvisioner::new(store.clone(), routing_config.clone())
            .map_err(|_| test_error("construct managed replacement provisioner"))?
            .provision_replacement_read_database(
                &holding_id,
                &data_profile_key,
                &managed_candidate_key,
            )
            .await
            .map_err(|_| test_error("provision managed replacement read database"))?;
        let managed_candidate = router
            .route_replacement_read_for_generation(
                &holding_id,
                &data_profile_key,
                &managed_candidate_key,
                managed_generation,
                42,
            )
            .await
            .map_err(|_| test_error("route fenced managed replacement read database"))?;
        managed_candidate
            .probe()
            .await
            .map_err(|_| test_error("probe managed replacement read database"))?;
        managed_candidate.close().await;
        provisioner
            .complete_read_rebuild_if_generation(
                &holding_id,
                &managed_candidate_key,
                managed_generation,
            )
            .await
            .map_err(|_| test_error("complete managed replacement read rebuild"))?;
        if !router
            .replacement_read_is_ready(&holding_id, &managed_candidate_key, managed_generation, 42)
            .await
            .map_err(|_| test_error("observe ready managed replacement read"))?
        {
            return Err(test_error("completed replacement was not ready"));
        }
        provisioner
            .activate_read_database_if_generation(
                &holding_id,
                &managed_candidate_key,
                managed_generation,
            )
            .await
            .map_err(|_| test_error("activate managed replacement read rebuild"))?;
        if !router
            .replacement_read_is_activated(&holding_id, &managed_candidate_key, managed_generation)
            .await
            .map_err(|_| test_error("observe activated managed replacement read"))?
        {
            return Err(test_error("activated replacement was not observed"));
        }
        let active_managed_read: String = sqlx::query_scalar(
            "SELECT read_database_key FROM erpx_control.holding_registry WHERE holding_id = $1",
        )
        .bind(&holding_id)
        .fetch_one(&store.pools().write)
        .await
        .map_err(|_| test_error("read active managed replacement"))?;
        if active_managed_read != managed_candidate_key {
            return Err(test_error("generation-bound replacement did not activate"));
        }
        let stale_run_id = Uuid::new_v4();
        let stale_run = HoldingBackupRun {
            id: stale_run_id,
            holding_id: holding_id.clone(),
            kind: "restore_test".into(),
            status: "started".into(),
            backup_ref: "backup-prv007".into(),
            restore_target_ref: Some("restore-prv007-stale".into()),
            pitr_recovery_point: None,
            started_at: chrono::Utc::now(),
            completed_at: None,
            verified: false,
            error: None,
        };
        if provisioner
            .execute_restore_test_with_routed_read_replacement(
                &router,
                stale_run,
                &data_profile_key,
                &successful_candidate_key,
                HoldingSourceProjection::Operations,
                2,
                |_registration, _run, _candidate, _source_version| async {
                    Ok::<(bool, bool), PersistenceError>((true, true))
                },
            )
            .await
            .is_ok()
        {
            return Err(test_error("stale source watermark was accepted"));
        }
        let stale_state: (String, bool, Option<String>) = sqlx::query_as(
            "SELECT status, verified, error
             FROM erpx_control.holding_backup_runs WHERE id = $1",
        )
        .bind(stale_run_id)
        .fetch_one(&store.pools().write)
        .await
        .map_err(|_| test_error("read stale restore test state"))?;
        if stale_state.0 != "failed"
            || stale_state.1
            || stale_state.2.as_deref() != Some("restore source watermark is stale")
        {
            return Err(test_error("stale restore test was not durably failed"));
        }

        let successful_run = HoldingBackupRun {
            id: Uuid::new_v4(),
            holding_id: holding_id.clone(),
            kind: "restore_test".into(),
            status: "started".into(),
            backup_ref: "backup-prv007".into(),
            restore_target_ref: Some("restore-prv007-success".into()),
            pitr_recovery_point: None,
            started_at: chrono::Utc::now(),
            completed_at: None,
            verified: false,
            error: None,
        };
        let activation = provisioner
            .execute_restore_test_with_routed_read_replacement(
                &router,
                successful_run,
                &data_profile_key,
                &successful_candidate_key,
                HoldingSourceProjection::Operations,
                1,
                |_registration, _run, _candidate, _source_version| async {
                    Ok::<(bool, bool), PersistenceError>((true, true))
                },
            )
            .await
            .map_err(|_| test_error("execute route-aware restore and read replacement"))?;
        if !activation.run.verified
            || activation.candidate_read_database_key != successful_candidate_key
            || activation.source_version != 1
            || activation.generation == 0
        {
            return Err(test_error("verified restore test was not marked verified"));
        }
        let switched_read: String = sqlx::query_scalar(
            "SELECT read_database_key FROM erpx_control.holding_registry WHERE holding_id = $1",
        )
        .bind(&holding_id)
        .fetch_one(&store.pools().write)
        .await
        .map_err(|_| test_error("read holding registry after successful restore"))?;
        if switched_read != successful_candidate_key {
            return Err(test_error(
                "verified restore did not activate replacement read",
            ));
        }
        let switched_route = router
            .route(&holding_id, &data_profile_key, RoutedDatabaseSide::Read)
            .await
            .map_err(|_| test_error("route replacement read database"))?;
        switched_route
            .probe()
            .await
            .map_err(|_| test_error("probe replacement read database"))?;
        switched_route.close().await;

        store.pools().write.close().await;
        store.pools().read.close().await;
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    }
    .await;

    let cleanup = disposable.drop_all().await;
    result?;
    cleanup
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL admin access; URLs are never printed"]
async fn real_physical_provisioning_and_replacement_read_path_is_safe() {
    run_real_provisioning_replacement()
        .await
        .expect("real provisioning/replacement test");
}
