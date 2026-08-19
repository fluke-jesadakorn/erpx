//! Self-contained G13 disposable PostgreSQL acceptance.
//!
//! This test creates six fresh local databases, applies the checked-in
//! control/holding migrators, and exercises the real source snapshot, target
//! write ledger, separate read model, rollback store, retention, outbox, and
//! Legacy-write fence. It never changes command ownership or writes to Legacy.
//!
//! Run explicitly with:
//! `cargo test -p erpx-infrastructure --test g13_disposable_acceptance --offline -- --ignored --nocapture`

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    str::FromStr,
    sync::Arc,
};

use chrono::{Duration, Utc};
use erpx_infrastructure::{
    ControlPlanePools, G13_EXECUTE_PERMISSION, G13AuthorizationProof, G13TargetImportError,
    G13TargetImporter, SqlxControlPlane, SqlxG13MigrationStore,
};
use erpx_migration::{
    LegacySourceSnapshot, g13::G13StartRunRequest, migrate_control_plane, migrate_control_read,
    migrate_holding_read, migrate_holding_write,
};
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;

type TestResult = Result<(), Box<dyn Error + Send + Sync>>;

const DATA_PROFILE_KEY: &str = "holding-g13-disposable";
const SOURCE_FENCE: &str = "post-cutover-legacy-write-fence";

struct DisposableDatabases {
    admin: PgPool,
    base_options: PgConnectOptions,
    names: Vec<String>,
}

impl DisposableDatabases {
    async fn create() -> Result<Self, Box<dyn Error + Send + Sync>> {
        let admin_url = std::env::var("ERPX_G13_DISPOSABLE_ADMIN_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://localhost/postgres?sslmode=verify-full".to_owned());
        let base_options = PgConnectOptions::from_str(&admin_url)?;
        let admin = PgPoolOptions::new()
            .max_connections(2)
            .connect_with(base_options.clone())
            .await?;
        let suffix = Uuid::new_v4().simple().to_string();
        let names = [
            "source",
            "control_write",
            "control_read",
            "target_write",
            "target_read",
            "rollback",
        ]
        .into_iter()
        .map(|kind| format!("erpx_g13_{kind}_{suffix}"))
        .collect::<Vec<_>>();
        for name in &names {
            sqlx::query(&format!("CREATE DATABASE {}", quote_identifier(name)))
                .execute(&admin)
                .await?;
        }
        Ok(Self {
            admin,
            base_options,
            names,
        })
    }

    async fn pool(&self, index: usize) -> Result<PgPool, sqlx::Error> {
        PgPoolOptions::new()
            .max_connections(4)
            .connect_with(self.base_options.clone().database(&self.names[index]))
            .await
    }

    async fn destroy(self) -> Result<(), sqlx::Error> {
        self.admin.close().await;
        let admin_url = std::env::var("ERPX_G13_DISPOSABLE_ADMIN_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://localhost/postgres?sslmode=verify-full".to_owned());
        let admin_options = PgConnectOptions::from_str(&admin_url)
            .map_err(|error| sqlx::Error::Configuration(Box::new(error)))?;
        let admin = PgPoolOptions::new()
            .max_connections(1)
            .connect_with(admin_options)
            .await?;
        for name in self.names {
            sqlx::query(&format!(
                "DROP DATABASE IF EXISTS {} WITH (FORCE)",
                quote_identifier(&name)
            ))
            .execute(&admin)
            .await?;
        }
        admin.close().await;
        Ok(())
    }
}

fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn proof(now: chrono::DateTime<Utc>) -> G13AuthorizationProof {
    G13AuthorizationProof {
        actor_ref: "local:g13-disposable-acceptance".into(),
        permissions: BTreeSet::from([G13_EXECUTE_PERMISSION.into()]),
        mfa_verified: true,
        break_glass: false,
        expires_at: now + Duration::minutes(15),
        cutover_approval_ref: None,
    }
}

fn source_snapshot(namespace: &str) -> Result<LegacySourceSnapshot, serde_json::Error> {
    let customer_one = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"g13:customer:one");
    let customer_two = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"g13:customer:two");
    let user = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"g13:user:one");
    LegacySourceSnapshot {
        extractor_version: format!("external-generic-fixture-v1:{namespace}"),
        generated_at: Utc::now(),
        source_contract_hash: String::new(),
        missing_tables: Vec::new(),
        counts: BTreeMap::from([("customers".into(), 2), ("users".into(), 1)]),
        statuses: BTreeMap::from([("customers".into(), BTreeMap::from([("active".into(), 2)]))]),
        totals_minor: BTreeMap::new(),
        object_bytes: BTreeMap::new(),
        id_mappings: BTreeMap::from([
            (
                "customers".into(),
                BTreeMap::from([
                    (customer_one.to_string(), customer_one),
                    (customer_two.to_string(), customer_two),
                ]),
            ),
            ("users".into(), BTreeMap::from([(user.to_string(), user)])),
        ]),
    }
    .seal()
}

async fn set_source_fixture(source: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("CREATE SCHEMA legacy").execute(source).await?;
    sqlx::query("CREATE TABLE legacy.customers (id UUID PRIMARY KEY, status TEXT NOT NULL)")
        .execute(source)
        .await?;
    sqlx::query("CREATE TABLE legacy.users (id UUID PRIMARY KEY)")
        .execute(source)
        .await?;
    let customer_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO legacy.customers (id, status) VALUES ($1, 'active'), ($2, 'active')")
        .bind(customer_id)
        .bind(Uuid::new_v4())
        .execute(source)
        .await?;
    sqlx::query("INSERT INTO legacy.users (id) VALUES ($1)")
        .bind(user_id)
        .execute(source)
        .await?;
    Ok(())
}

async fn scoped_count(pool: &PgPool, query: &str, run_id: Uuid) -> Result<i64, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(DATA_PROFILE_KEY)
        .execute(&mut *transaction)
        .await?;
    let count = sqlx::query_scalar::<_, i64>(query)
        .bind(DATA_PROFILE_KEY)
        .bind(run_id)
        .fetch_one(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(count)
}

async fn execute_acceptance(cluster: &DisposableDatabases) -> TestResult {
    let source = cluster.pool(0).await?;
    let control_write = cluster.pool(1).await?;
    let control_read = cluster.pool(2).await?;
    let target = cluster.pool(3).await?;
    let read = cluster.pool(4).await?;
    let rollback = cluster.pool(5).await?;

    migrate_control_plane(&control_write).await?;
    migrate_control_read(&control_read).await?;
    migrate_holding_write(&target).await?;
    migrate_holding_read(&read).await?;
    migrate_holding_write(&rollback).await?;
    set_source_fixture(&source).await?;

    let snapshot = source_snapshot("g13-disposable-source")?;
    if snapshot.counts.get("customers") != Some(&2)
        || snapshot.counts.get("users") != Some(&1)
        || snapshot
            .id_mappings
            .get("customers")
            .is_none_or(|map| map.len() != 2)
        || snapshot
            .id_mappings
            .get("users")
            .is_none_or(|map| map.len() != 1)
    {
        return Err("source extractor did not capture the disposable fixture".into());
    }
    let source_count_before = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM legacy.customers")
        .fetch_one(&source)
        .await?;

    let control = Arc::new(SqlxControlPlane::new(ControlPlanePools {
        write: control_write.clone(),
        read: control_read.clone(),
    }));
    let store = SqlxG13MigrationStore::new(Arc::clone(&control));
    let now = Utc::now();
    let actor = proof(now);
    let run_id = Uuid::new_v4();
    store
        .start_run(
            &G13StartRunRequest {
                run_id,
                cohort: format!("g13-disposable-{run_id}"),
                source_contract_hash: snapshot.source_contract_hash.clone(),
                id_namespace: "g13-disposable-source".into(),
                actor_ref: actor.actor_ref.clone(),
                correlation_id: Uuid::new_v4(),
                idempotency_key: format!("g13-disposable-start:{run_id}"),
                now,
            },
            &actor,
        )
        .await
        .map_err(|error| format!("start run: {error:?}"))?;
    store
        .record_source_snapshot(
            run_id,
            Uuid::new_v4(),
            &snapshot,
            &actor.actor_ref,
            Uuid::new_v4(),
            &format!("g13-disposable-source:{run_id}"),
            &actor,
        )
        .await
        .map_err(|error| format!("record source snapshot: {error:?}"))?;
    if store
        .load_source_snapshot(run_id, &snapshot.source_contract_hash)
        .await?
        != snapshot
    {
        return Err("durable source snapshot did not round-trip".into());
    }
    store
        .project_read_model()
        .await
        .map_err(|error| format!("project control read model: {error:?}"))?;
    let status = store
        .list_read_model(10)
        .await?
        .into_iter()
        .find(|status| status.run_id == run_id)
        .ok_or("durable G13 status read model is missing the run")?;
    if status.phase != "source_frozen"
        || status.command_owner != "legacy"
        || status.source_snapshot_hash != snapshot.source_contract_hash
        || status.source_total_rows != 3
        || status.source_entity_kind_count != 2
    {
        return Err("G13 status read model did not expose source evidence".into());
    }

    let importer = G13TargetImporter::new_with_read_pool(
        target.clone(),
        read.clone(),
        rollback.clone(),
        DATA_PROFILE_KEY,
    )?;
    for source_kind in ["customers", "users"] {
        let result = importer
            .import_scope(
                run_id,
                &snapshot,
                source_kind,
                &snapshot.source_contract_hash,
            )
            .await?;
        let expected = snapshot
            .id_mappings
            .get(source_kind)
            .map_or(0, BTreeMap::len) as u64;
        if result.rows_applied != expected || result.target_count != expected {
            return Err(format!("target import was incomplete for {source_kind}").into());
        }
        let shadow = importer
            .shadow_replay(
                run_id,
                &snapshot,
                source_kind,
                &snapshot.source_contract_hash,
            )
            .await?;
        if !shadow.matched() {
            return Err(format!("shadow replay did not match for {source_kind}").into());
        }
    }
    if scoped_count(
        &target,
        "SELECT count(*) FROM erpx_holding.outbox_events WHERE data_profile_key=$1 AND event_type IN ('migration.imported', 'migration.shadow_replay_validated') AND payload->>'run_id'=$2::text",
        run_id,
    )
    .await?
        < 4
    {
        return Err("target outbox evidence is incomplete".into());
    }
    let projected = importer.project_read_model(run_id).await?;
    if projected.rows_projected != 3
        || scoped_count(
            &read,
            "SELECT count(*) FROM erpx_holding.g13_migration_import_read_rows WHERE data_profile_key=$1 AND run_id=$2",
            run_id,
        )
        .await?
            != 3
    {
        return Err("separate G13 read projection is incomplete".into());
    }

    let rejected = importer
        .apply_final_delta(
            run_id,
            &snapshot,
            "customers",
            &snapshot.source_contract_hash,
            SOURCE_FENCE,
            false,
            Utc::now(),
        )
        .await;
    if !matches!(rejected, Err(G13TargetImportError::WriteFenceRequired)) {
        return Err("post-cutover Legacy-write fence did not reject final delta".into());
    }
    let stale_rejected = importer
        .apply_final_delta(
            run_id,
            &snapshot,
            "customers",
            &snapshot.source_contract_hash,
            SOURCE_FENCE,
            true,
            Utc::now() - Duration::minutes(6),
        )
        .await;
    if !matches!(
        stale_rejected,
        Err(G13TargetImportError::WriteFenceRequired)
    ) {
        return Err("stale Legacy-write fence was accepted".into());
    }
    let final_delta = importer
        .apply_final_delta(
            run_id,
            &snapshot,
            "customers",
            &snapshot.source_contract_hash,
            SOURCE_FENCE,
            true,
            Utc::now(),
        )
        .await?;
    if final_delta.rows_applied != 0 {
        return Err("idempotent final delta replay changed target rows".into());
    }

    let reconciliation = importer.reconcile_run(run_id, &snapshot).await?;
    if !reconciliation.report.is_reconciled() {
        return Err("G13 reconciliation reported drift".into());
    }
    let rollback_result = importer
        .rehearse_rollback(run_id, &snapshot.source_contract_hash)
        .await?;
    if rollback_result.rows_captured != 3
        || rollback_result.rows_restored != 3
        || rollback_result.outbox_events_replayed != 3
        || !rollback_result.read_model_rebuilt
    {
        return Err("rollback evidence or read-model rebuild is incomplete".into());
    }
    if scoped_count(
        &read,
        "SELECT count(*) FROM erpx_holding.g13_migration_import_read_rows WHERE data_profile_key=$1 AND run_id=$2",
        run_id,
    )
    .await?
        != 3
    {
        return Err("rollback did not rebuild the separate read model".into());
    }
    if scoped_count(
        &rollback,
        "SELECT count(*) FROM erpx_holding.g13_migration_rollback_rows WHERE data_profile_key=$1 AND run_id=$2",
        run_id,
    )
    .await?
        != 3
    {
        return Err("rollback store did not retain the captured rows".into());
    }

    let retention_at = Utc::now();
    let retention = importer
        .expire_rehearsal(
            run_id,
            retention_at,
            retention_at,
            &snapshot.source_contract_hash,
        )
        .await?;
    if retention.rows_expired != 3
        || retention.read_rows_expired != 3
        || !retention.retained_rollback_rows
    {
        return Err("G13 retention did not expire target/read rows safely".into());
    }
    let replay = importer
        .expire_rehearsal(
            run_id,
            retention_at,
            retention_at,
            &snapshot.source_contract_hash,
        )
        .await?;
    if replay.rows_expired != 0 || replay.read_rows_expired != 0 {
        return Err("G13 retention replay was not idempotent".into());
    }
    if scoped_count(
        &target,
        "SELECT count(*) FROM erpx_holding.g13_migration_import_rows WHERE data_profile_key=$1 AND run_id=$2",
        run_id,
    )
    .await?
        != 0
        || scoped_count(
            &read,
            "SELECT count(*) FROM erpx_holding.g13_migration_import_read_rows WHERE data_profile_key=$1 AND run_id=$2",
            run_id,
        )
        .await?
            != 0
        || scoped_count(
            &rollback,
            "SELECT count(*) FROM erpx_holding.g13_migration_rollback_rows WHERE data_profile_key=$1 AND run_id=$2",
            run_id,
        )
        .await?
            != 3
    {
        return Err("G13 retention violated target/read/rollback boundaries".into());
    }
    if sqlx::query_scalar::<_, i64>("SELECT count(*) FROM legacy.customers")
        .fetch_one(&source)
        .await?
        != source_count_before
    {
        return Err("source database was mutated by the G13 acceptance".into());
    }
    let phase: String =
        sqlx::query_scalar("SELECT phase FROM erpx_control.migration_runs WHERE run_id=$1")
            .bind(run_id)
            .fetch_one(&control_write)
            .await?;
    if phase != "source_frozen" {
        return Err("disposable acceptance unexpectedly advanced command ownership".into());
    }
    Ok(())
}

#[tokio::test]
#[ignore = "creates six disposable local PostgreSQL databases; run explicitly with --ignored"]
async fn g13_disposable_postgres_acceptance_is_real_and_cutover_free() {
    let cluster = DisposableDatabases::create()
        .await
        .expect("local PostgreSQL is required for disposable G13 acceptance");
    let result = execute_acceptance(&cluster).await;
    cluster
        .destroy()
        .await
        .expect("disposable G13 database cleanup");
    result.expect("G13 disposable PostgreSQL acceptance failed");
}
