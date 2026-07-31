use std::{io, sync::Arc};

use chrono::Utc;
use erpx_domain::{
    access::Scope,
    contracts::{ActorAttribution, ActorKind},
    operations::{AggregateMetadata, Holding, OperationsEvent, OrganizationStatus, event_for},
};
use erpx_infrastructure::{
    ControlPlaneDatabaseConfig, ControlPlanePools, OPERATIONS_READ_PROJECTION, SqlxControlPlane,
    SqlxOperationsStore, SqlxTenantStoreRegistry, TenantDatabaseProvisioner,
    TenantDatabaseProvisioningConfig, TenantDatabaseRouter, TenantProvisioner,
    TenantStoreRegistration,
};
use erpx_migration::{
    migrate_control_plane, migrate_control_read, migrate_tenant_read, migrate_tenant_write,
};
use erpx_worker::{
    CandidateProjectionRebuilder, OperationsEventStreamRequest, OperationsProjectionRebuildRequest,
    OperationsProjectionRequest, OperationsProjectionRunner,
};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

type TestError = Box<dyn std::error::Error + Send + Sync>;
type TestResult<T = ()> = Result<T, TestError>;

fn test_error(message: &'static str) -> TestError {
    io::Error::other(message).into()
}

fn database_name(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4().simple())
}

fn template_url(template: &str, database: &str) -> TestResult<String> {
    if template.matches("{database}").count() != 1 {
        return Err(test_error(
            "invalid disposable tenant database URL template",
        ));
    }
    Ok(template.replace("{database}", database))
}

#[derive(Debug)]
struct DisposableDatabases {
    admin_url: String,
    names: Vec<String>,
}

impl DisposableDatabases {
    async fn create(admin_url: String, names: Vec<String>) -> TestResult<Self> {
        let admin = PgPoolOptions::new()
            .max_connections(2)
            .connect(&admin_url)
            .await
            .map_err(|_| test_error("connect disposable PostgreSQL admin database"))?;
        let result = async {
            for name in &names {
                let statement = format!("CREATE DATABASE \"{name}\"");
                sqlx::query(&statement)
                    .execute(&admin)
                    .await
                    .map_err(|_| test_error("create disposable PostgreSQL database"))?;
            }
            Ok::<(), TestError>(())
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

    async fn drop_all(&self) -> TestResult {
        let admin = PgPoolOptions::new()
            .max_connections(2)
            .connect(&self.admin_url)
            .await
            .map_err(|_| test_error("connect disposable PostgreSQL cleanup database"))?;
        let mut first_error = None;
        for name in self.names.iter().rev() {
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
                first_error = Some(test_error("terminate disposable PostgreSQL connections"));
            }
            let statement = format!("DROP DATABASE IF EXISTS \"{name}\"");
            if sqlx::query(&statement).execute(&admin).await.is_err() && first_error.is_none() {
                first_error = Some(test_error("drop disposable PostgreSQL database"));
            }
        }
        admin.close().await;
        first_error.map_or(Ok(()), Err)
    }
}

fn control_config() -> ControlPlaneDatabaseConfig {
    ControlPlaneDatabaseConfig {
        write_url: std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required"),
        read_url: std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required"),
    }
}

fn holding(at: chrono::DateTime<Utc>, holding_ref: &str) -> Holding {
    Holding {
        meta: AggregateMetadata {
            id: Uuid::from_u128(0x8701),
            holding_ref: holding_ref.into(),
            scope: Scope::Holding {
                holding_id: Uuid::from_u128(0x8701),
            },
            version: 1,
            created_at: at,
            updated_at: at,
        },
        code: "OPS-RUNNER".into(),
        name: "Operations runner fixture".into(),
        status: OrganizationStatus::Active,
    }
}

fn event_for_holding(aggregate: &Holding, at: chrono::DateTime<Utc>) -> OperationsEvent {
    event_for(
        aggregate,
        "organization.holding.created",
        1,
        ActorAttribution {
            principal_ref: "operations-runner-test".into(),
            kind: ActorKind::SystemWorker,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        Some("operations-runner-fixture-v1".into()),
        Uuid::from_u128(0x8702),
        None,
        at,
    )
    .expect("valid Operations event")
}

#[tokio::test]
#[ignore = "requires two disposable control and two disposable tenant PostgreSQL databases"]
async fn operations_projector_executes_through_active_tenant_route() {
    let config = control_config();
    assert_ne!(config.write_url, config.read_url);
    let pools = ControlPlanePools::connect(&config)
        .await
        .expect("control pools");
    migrate_control_plane(&pools.write)
        .await
        .expect("control write migration");
    migrate_control_read(&pools.read)
        .await
        .expect("control read migration");

    let store = Arc::new(SqlxControlPlane::new(pools));
    let admin_url = std::env::var("ERPX_TEST_ADMIN_DATABASE_URL")
        .expect("ERPX_TEST_ADMIN_DATABASE_URL is required");
    let template = std::env::var("ERPX_TEST_TENANT_DATABASE_URL_TEMPLATE")
        .expect("ERPX_TEST_TENANT_DATABASE_URL_TEMPLATE is required");
    let tenant_id = format!("tenant-operations-runner-{}", Uuid::new_v4().simple());
    let holding_ref = format!("holding-operations-runner-{}", Uuid::new_v4().simple());
    let write_database_key = database_name("erpx_ops_projection_write");
    let read_database_key = database_name("erpx_ops_projection_read");
    let candidate_database_key = database_name("erpx_ops_projection_candidate");
    let disposable = DisposableDatabases::create(
        admin_url.clone(),
        vec![
            write_database_key.clone(),
            read_database_key.clone(),
            candidate_database_key.clone(),
        ],
    )
    .await
    .expect("create disposable tenant databases");

    let provisioner = TenantProvisioner::new(Arc::clone(&store));
    provisioner
        .begin(TenantStoreRegistration {
            tenant_id: tenant_id.clone(),
            holding_ref: holding_ref.clone(),
            write_database_key: write_database_key.clone(),
            read_database_key: read_database_key.clone(),
            storage_namespace: format!("operations-runner-storage-{}", Uuid::new_v4().simple()),
            credential_key_ref: "operations-runner-key".into(),
            backup_chain_ref: "operations-runner-backup".into(),
            state: "provisioning".into(),
            registry_version: 1,
        })
        .await
        .expect("begin tenant provisioning");
    let tenant_database_config =
        TenantDatabaseProvisioningConfig::from_values(admin_url.clone(), template.clone())
            .expect("tenant provisioning config");
    TenantDatabaseProvisioner::new(Arc::clone(&store), tenant_database_config.clone())
        .expect("tenant database provisioner")
        .provision_databases(&tenant_id)
        .await
        .expect("provision tenant databases");
    TenantDatabaseProvisioner::new(Arc::clone(&store), tenant_database_config.clone())
        .expect("tenant database provisioner")
        .provision_storage_namespace(&tenant_id, |_namespace| async { Ok(()) })
        .await
        .expect("provision tenant storage namespace");
    provisioner
        .activate(&tenant_id)
        .await
        .expect("activate tenant");

    let write_url = template_url(&template, &write_database_key).expect("write database URL");
    let read_url = template_url(&template, &read_database_key).expect("read database URL");
    let write_pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&write_url)
        .await
        .expect("tenant write pool");
    let read_pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&read_url)
        .await
        .expect("tenant read pool");
    migrate_tenant_write(&write_pool)
        .await
        .expect("tenant write migration");
    migrate_tenant_read(&read_pool)
        .await
        .expect("tenant read migration");

    let seeded = SqlxOperationsStore::from_pools(&write_pool, &read_pool, &holding_ref)
        .expect("seed Operations store");
    let at = Utc::now();
    let aggregate = holding(at, &holding_ref);
    let event = event_for_holding(&aggregate, at);
    seeded
        .apply_event(&event, &aggregate, None, &[])
        .await
        .expect("seed Operations event");

    let router = TenantDatabaseRouter::new(Arc::clone(&store), tenant_database_config.clone())
        .expect("tenant router");
    let runner = OperationsProjectionRunner::new(router.clone());
    let first = runner
        .project_page(
            OperationsProjectionRequest::new(&tenant_id, &holding_ref, None, 100)
                .expect("projection request"),
        )
        .await
        .expect("projected page");
    assert_eq!(first.events_seen, 1);
    assert_eq!(first.events_applied, 1);
    assert_eq!(first.events_noop, 0);

    let replay = runner
        .project_page(
            OperationsProjectionRequest::new(&tenant_id, &holding_ref, None, 100)
                .expect("replay request"),
        )
        .await
        .expect("replayed page");
    assert_eq!(replay.events_seen, 1);
    assert_eq!(replay.events_applied, 0);
    assert_eq!(replay.events_noop, 1);

    let rebuild = runner
        .rebuild(
            OperationsProjectionRebuildRequest::new(
                &tenant_id,
                &holding_ref,
                OPERATIONS_READ_PROJECTION,
            )
            .expect("rebuild request"),
        )
        .await
        .expect("rebuilt projection");
    assert_eq!(rebuild.events_seen, 1);
    assert_eq!(rebuild.events_replayed, 1);

    let generation = provisioner
        .begin_read_rebuild_with_generation(&tenant_id, &candidate_database_key, 1)
        .await
        .expect("begin candidate rebuild");
    TenantDatabaseProvisioner::new(Arc::clone(&store), tenant_database_config)
        .expect("candidate provisioner")
        .provision_replacement_read_database(&tenant_id, &holding_ref, &candidate_database_key)
        .await
        .expect("provision candidate read database");
    let candidate = router
        .route_replacement_read(&tenant_id, &holding_ref, &candidate_database_key)
        .await
        .expect("route candidate read database");
    let candidate_rebuild = runner
        .rebuild_candidate(OPERATIONS_READ_PROJECTION, &candidate, 1)
        .await
        .expect("rebuild Operations candidate");
    assert_eq!(candidate_rebuild.events_replayed, 1);
    candidate.close().await;
    provisioner
        .complete_read_rebuild_if_generation(&tenant_id, &candidate_database_key, generation)
        .await
        .expect("complete candidate rebuild");
    provisioner
        .activate_read_database_if_generation(&tenant_id, &candidate_database_key, generation)
        .await
        .expect("activate candidate read database");
    let active_read = SqlxTenantStoreRegistry::new(Arc::clone(&store))
        .get(&tenant_id)
        .await
        .expect("read active registration")
        .expect("active registration");
    assert_eq!(active_read.read_database_key, candidate_database_key);

    let stream = OperationsEventStreamRequest::new(&tenant_id, &holding_ref, None, 100)
        .expect("stream request");
    assert_eq!(stream.holding_ref, holding_ref);

    write_pool.close().await;
    read_pool.close().await;
    store.pools().write.close().await;
    store.pools().read.close().await;
    disposable
        .drop_all()
        .await
        .expect("drop disposable tenant databases");
}
