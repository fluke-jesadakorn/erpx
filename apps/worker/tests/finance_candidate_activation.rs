//! Disposable PostgreSQL proof for the Finance replacement-read candidate path.
//!
//! The proof deliberately invokes only public worker and infrastructure APIs.
//! It creates unique databases, drops them after the test, and maps database
//! failures to generic messages so credentials and URLs never reach output.

use std::{io, sync::Arc};

use chrono::{Duration, NaiveDate, Utc};
use erpx_domain::{
    contracts::{ActorAttribution, ActorKind, CommandContext, TenantScope},
    finance_subledgers::{Invoice, InvoiceKind, InvoiceLine, PartyKind, PartyReference},
};
use erpx_infrastructure::{
    ControlPlaneDatabaseConfig, ControlPlanePools, FINANCE_SUBLEDGER_READ_PROJECTION,
    FinanceSubledgerAggregate, FinanceSubledgerSourceEvent, ProjectionClaimRecord,
    ProjectionJobKind, ProjectionJobRecord, ProjectionLease, ProjectionPolicy,
    ProjectionScheduleRequest, RoutedDatabaseSide, SqlxControlPlane, SqlxFinanceSubledgerStore,
    SqlxTenantStoreRegistry, TenantDatabaseProvisioner, TenantDatabaseProvisioningConfig,
    TenantDatabaseRouter, TenantHoldingBinding, TenantProvisioner, TenantStoreRegistration,
};
use erpx_migration::{
    migrate_control_plane, migrate_control_read, migrate_tenant_read, migrate_tenant_write,
};
use erpx_worker::{
    FinanceSubledgerTenantProjectionExecutor, RoutedTenantProjectionExecutor,
    TenantProjectionExecutor,
};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

type TestError = Box<dyn std::error::Error + Send + Sync>;
type TestResult<T = ()> = Result<T, TestError>;

fn test_error(message: &'static str) -> Box<dyn std::error::Error + Send + Sync> {
    io::Error::other(message).into()
}

fn database_name(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4().simple())
}

fn database_url(
    base_url: &str,
    database: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let scheme_end = base_url
        .find("://")
        .ok_or_else(|| test_error("invalid disposable PostgreSQL admin URL"))?;
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

fn context(scope: TenantScope, key: &str) -> CommandContext {
    CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: "finance-candidate-activation-test".into(),
            kind: ActorKind::TenantUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope,
        "finance.candidate.activation.test",
    )
    .expect("valid test command context")
}

fn invoice(
    holding_id: Uuid,
    company_id: Uuid,
    branch_id: Uuid,
    period_id: Uuid,
    id: Uuid,
) -> Invoice {
    let scope = TenantScope::Branch {
        holding_id,
        company_id,
        branch_id,
    };
    let party = PartyReference::new(
        PartyKind::Vendor,
        Uuid::new_v4(),
        TenantScope::Company {
            holding_id,
            company_id,
        },
        holding_id,
        company_id,
    )
    .expect("valid test vendor");
    Invoice::new(
        InvoiceKind::Payable,
        id,
        scope.clone(),
        holding_id,
        company_id,
        period_id,
        NaiveDate::from_ymd_opt(2026, 7, 26).expect("valid test date"),
        NaiveDate::from_ymd_opt(2026, 7, 26).expect("valid test date"),
        NaiveDate::from_ymd_opt(2026, 8, 25).expect("valid test date"),
        party,
        "THB",
        vec![InvoiceLine {
            line_id: Uuid::new_v4(),
            description: "candidate activation fixture".into(),
            quantity: 1,
            unit_price_minor: 100,
            tax_minor: 0,
        }],
        &context(scope, "finance-candidate-activation-invoice"),
    )
    .expect("valid test invoice")
}

fn candidate_claim(
    tenant_id: &str,
    holding_ref: &str,
    candidate_database_key: &str,
    source_version: u64,
    generation: u64,
) -> Result<ProjectionClaimRecord, Box<dyn std::error::Error + Send + Sync>> {
    let now = Utc::now();
    let binding = TenantHoldingBinding::new(tenant_id, holding_ref)
        .map_err(|_| test_error("construct candidate scheduler binding"))?;
    let policy = ProjectionPolicy::new(1, 1, 3, 1, 1)
        .map_err(|_| test_error("construct candidate scheduler policy"))?;
    let request = ProjectionScheduleRequest {
        binding,
        projection_name: FINANCE_SUBLEDGER_READ_PROJECTION.into(),
        job_key: format!("finance-candidate-{}", Uuid::new_v4().simple()),
        kind: ProjectionJobKind::Rebuild,
        available_at: now,
        policy,
        payload: serde_json::json!({
            "candidate_read_database_key": candidate_database_key,
            "source_version": source_version,
            "generation": generation,
            "page_size": 1,
        }),
    };
    let mut job = ProjectionJobRecord::initial(Uuid::new_v4(), &request, now)
        .map_err(|_| test_error("construct candidate scheduler job"))?;
    let lease = ProjectionLease::new(Uuid::new_v4(), Uuid::new_v4(), now, Duration::seconds(30))
        .map_err(|_| test_error("construct candidate scheduler lease"))?;
    job.claim(lease.clone(), now)
        .map_err(|_| test_error("claim candidate scheduler job"))?;
    Ok(ProjectionClaimRecord {
        claim_key: format!("finance-candidate-claim-{}", Uuid::new_v4().simple()),
        job,
        lease,
        replayed: false,
    })
}

async fn run_candidate_activation(
    admin_url: &str,
    tenant_template: &str,
    control_write_key: &str,
    control_read_key: &str,
    tenant_write_key: &str,
    tenant_read_key: &str,
    candidate_read_key: &str,
) -> TestResult {
    let control_config = ControlPlaneDatabaseConfig {
        write_url: database_url(admin_url, control_write_key)?,
        read_url: database_url(admin_url, control_read_key)?,
    };
    let pools = ControlPlanePools::connect(&control_config)
        .await
        .map_err(|_| test_error("connect disposable control databases"))?;
    migrate_control_plane(&pools.write)
        .await
        .map_err(|_| test_error("migrate disposable control write database"))?;
    migrate_control_read(&pools.read)
        .await
        .map_err(|_| test_error("migrate disposable control read database"))?;
    let control = Arc::new(SqlxControlPlane::new(pools));

    let tenant_id = format!("tenant-fin045-{}", Uuid::new_v4().simple());
    let holding_ref = format!("holding-fin045-{}", Uuid::new_v4().simple());
    SqlxTenantStoreRegistry::new(Arc::clone(&control))
        .register(TenantStoreRegistration {
            tenant_id: tenant_id.clone(),
            holding_ref: holding_ref.clone(),
            write_database_key: tenant_write_key.into(),
            read_database_key: tenant_read_key.into(),
            storage_namespace: format!("storage-fin045-{}", Uuid::new_v4().simple()),
            credential_key_ref: "credential-ref-fin045".into(),
            backup_chain_ref: "backup-chain-fin045".into(),
            state: "active".into(),
            registry_version: 1,
        })
        .await
        .map_err(|_| test_error("register disposable active tenant"))?;

    let write_pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&template_url(tenant_template, tenant_write_key)?)
        .await
        .map_err(|_| test_error("connect disposable tenant write database"))?;
    let read_pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&template_url(tenant_template, tenant_read_key)?)
        .await
        .map_err(|_| test_error("connect disposable tenant read database"))?;
    migrate_tenant_write(&write_pool)
        .await
        .map_err(|_| test_error("migrate disposable tenant write database"))?;
    migrate_tenant_read(&read_pool)
        .await
        .map_err(|_| test_error("migrate disposable tenant read database"))?;

    let holding_id = Uuid::new_v4();
    let invoice = invoice(
        holding_id,
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
    );
    let invoice_id = invoice.metadata.id;
    let source = SqlxFinanceSubledgerStore::from_pools(&write_pool, &read_pool, &holding_ref)
        .map_err(|_| test_error("construct authoritative Finance source store"))?;
    source
        .save(&FinanceSubledgerAggregate::Invoice(invoice), None)
        .await
        .map_err(|_| test_error("seed authoritative Finance source aggregate"))?;
    let source_page = source
        .read_source_event_page(None, 10)
        .await
        .map_err(|_| test_error("read authoritative Finance source event"))?;
    if source_page.events.len() != 1 {
        return Err(test_error(
            "Finance source fixture did not produce one event",
        ));
    }
    let source_event: &FinanceSubledgerSourceEvent = &source_page.events[0];
    let source_version = source
        .source_event_high_watermark()
        .await
        .map_err(|_| test_error("read authoritative Finance source watermark"))?;
    if source_version != 1 || source_event.aggregate_type != "payable_invoice" {
        return Err(test_error(
            "Finance source fixture watermark or type is invalid",
        ));
    }

    let config = TenantDatabaseProvisioningConfig::from_values(
        admin_url.to_owned(),
        tenant_template.to_owned(),
    )
    .map_err(|_| test_error("construct tenant routing configuration"))?;
    let provisioner = TenantProvisioner::new(Arc::clone(&control));
    let generation = provisioner
        .begin_read_rebuild_with_generation(&tenant_id, candidate_read_key, source_version)
        .await
        .map_err(|_| test_error("begin candidate read generation"))?;
    TenantDatabaseProvisioner::new(Arc::clone(&control), config.clone())
        .map_err(|_| test_error("construct candidate database provisioner"))?
        .provision_replacement_read_database(&tenant_id, &holding_ref, candidate_read_key)
        .await
        .map_err(|_| test_error("provision and migrate candidate read database"))?;

    let router = TenantDatabaseRouter::new(Arc::clone(&control), config)
        .map_err(|_| test_error("construct tenant database router"))?;
    let finance_executor = FinanceSubledgerTenantProjectionExecutor::new(router.clone(), 1)
        .map_err(|_| test_error("construct Finance candidate projector"))?;
    let routed_executor = RoutedTenantProjectionExecutor::new(router.clone(), finance_executor);
    let claim = candidate_claim(
        &tenant_id,
        &holding_ref,
        candidate_read_key,
        source_version,
        generation,
    )?;
    routed_executor
        .execute(&claim)
        .await
        .map_err(|_| test_error("execute Finance candidate rebuild and activation"))?;

    let registry = SqlxTenantStoreRegistry::new(Arc::clone(&control));
    let active_registration = registry
        .get(&tenant_id)
        .await
        .map_err(|_| test_error("read activated tenant registry entry"))?
        .ok_or_else(|| test_error("activated tenant registry entry is missing"))?;
    if active_registration.read_database_key != candidate_read_key {
        return Err(test_error("candidate read database was not activated"));
    }
    let (switch_state, switch_version, current_read_database_key): (String, i64, String) =
        sqlx::query_as(
            "SELECT state, version, current_read_database_key
         FROM erpx_control.tenant_projection_switches
         WHERE tenant_id = $1 AND state = 'activated'",
        )
        .bind(&tenant_id)
        .fetch_one(&control.pools().write)
        .await
        .map_err(|_| test_error("read activated candidate switch state"))?;
    if switch_state != "activated"
        || current_read_database_key != candidate_read_key
        || switch_version <= i64::try_from(generation).unwrap_or(i64::MAX)
    {
        return Err(test_error("candidate switch generation is not activated"));
    }

    let active_read = router
        .route(&tenant_id, &holding_ref, RoutedDatabaseSide::Read)
        .await
        .map_err(|_| test_error("route activated candidate read database"))?;
    let active_store = SqlxFinanceSubledgerStore::from_pools(
        &write_pool,
        active_read.database_pool(),
        &holding_ref,
    )
    .map_err(|_| test_error("construct activated Finance read store"))?;
    if active_store
        .get("payable_invoice", invoice_id)
        .await
        .map_err(|_| test_error("read projected candidate Finance row"))?
        .is_none()
    {
        return Err(test_error("candidate Finance row is missing"));
    }
    let source_after = active_store
        .source_event_high_watermark()
        .await
        .map_err(|_| test_error("verify authoritative Finance source watermark"))?;
    if source_after != source_version {
        return Err(test_error("authoritative Finance source watermark changed"));
    }

    let mut scoped = active_read
        .begin_scoped()
        .await
        .map_err(|_| test_error("begin scoped candidate read verification"))?;
    let current_database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&mut *scoped)
        .await
        .map_err(|_| test_error("identify activated candidate database"))?;
    let row_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM erpx_tenant.finance_subledger_read_rows
         WHERE holding_ref = $1 AND aggregate_type = 'payable_invoice' AND aggregate_id = $2",
    )
    .bind(&holding_ref)
    .bind(invoice_id)
    .fetch_one(&mut *scoped)
    .await
    .map_err(|_| test_error("count projected candidate Finance rows"))?;
    let (last_event_id, checkpoint_version): (Uuid, i64) = sqlx::query_as(
        "SELECT last_event_id, source_version
         FROM erpx_tenant.finance_subledger_projection_checkpoints
         WHERE holding_ref = $1 AND aggregate_type = 'payable_invoice' AND aggregate_id = $2",
    )
    .bind(&holding_ref)
    .bind(invoice_id)
    .fetch_one(&mut *scoped)
    .await
    .map_err(|_| test_error("read candidate Finance projection checkpoint"))?;
    scoped
        .commit()
        .await
        .map_err(|_| test_error("commit candidate read verification"))?;
    if current_database != candidate_read_key
        || row_count != 1
        || last_event_id != source_event.event_id
        || checkpoint_version != i64::try_from(source_version).unwrap_or(i64::MAX)
    {
        return Err(test_error(
            "candidate row, checkpoint, or physical database is invalid",
        ));
    }

    active_read.close().await;
    write_pool.close().await;
    read_pool.close().await;
    control.pools().write.close().await;
    control.pools().read.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires disposable control, tenant-write, tenant-read, and candidate PostgreSQL databases"]
async fn finance_subledger_candidate_rebuild_activates_generation_and_projects_checkpoint()
-> TestResult {
    let admin_url = std::env::var("ERPX_TEST_ADMIN_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/postgres?sslmode=verify-full".into());
    let tenant_template = std::env::var("ERPX_TEST_TENANT_DATABASE_URL_TEMPLATE")
        .unwrap_or_else(|_| "postgres://localhost/{database}?sslmode=verify-full".into());
    let control_write_key = database_name("erpx_fin045_cw");
    let control_read_key = database_name("erpx_fin045_cr");
    let tenant_write_key = database_name("erpx_fin045_tw");
    let tenant_read_key = database_name("erpx_fin045_tr");
    let candidate_read_key = database_name("erpx_fin045_ca");
    let disposable = DisposableDatabases::create(
        admin_url.clone(),
        vec![
            control_write_key.clone(),
            control_read_key.clone(),
            tenant_write_key.clone(),
            tenant_read_key.clone(),
            candidate_read_key.clone(),
        ],
    )
    .await
    .map_err(|_| test_error("create disposable Finance candidate databases"))?;

    let result = run_candidate_activation(
        &admin_url,
        &tenant_template,
        &control_write_key,
        &control_read_key,
        &tenant_write_key,
        &tenant_read_key,
        &candidate_read_key,
    )
    .await;
    let cleanup = disposable.drop_all().await;
    result?;
    cleanup
}
