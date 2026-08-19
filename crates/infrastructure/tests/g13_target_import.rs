//! Real disposable PostgreSQL acceptance for the G13 target importer.
//!
//! This test uses six fresh local databases: a source fixture, control write
//! and read stores, target write and read stores, and a separate rollback
//! store. It deliberately does not require or fabricate external Legacy
//! credentials. The source fixture is read through `LegacySourceExtractor`,
//! while every target-side assertion is made against migrated SQLx tables.
//!
//! Run explicitly with:
//! `cargo test -p erpx-infrastructure --test g13_target_import --offline -- --ignored --nocapture`

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    str::FromStr,
    sync::Arc,
};

use chrono::{DateTime, Duration, Utc};
use erpx_infrastructure::{
    ControlPlanePools, G13_EXECUTE_PERMISSION, G13AuthorizationProof, G13TargetImportError,
    G13TargetImporter, SqlxControlPlane, SqlxG13MigrationStore,
};
use erpx_migration::{
    LegacySourceSnapshot, g13::G13StartRunRequest, migrate_control_plane, migrate_control_read,
    migrate_holding_read, migrate_holding_write,
};
use serde_json::Value;
use sqlx::{
    PgPool, Postgres, Row, Transaction,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;

type TestResult = Result<(), Box<dyn Error + Send + Sync>>;

const DATA_PROFILE_KEY: &str = "holding-g13-target-disposable";

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredImportRow {
    source_contract_hash: String,
    source_fence: String,
    source_kind: String,
    source_key: String,
    target_id: Uuid,
    payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReadProjectionRow {
    source_kind: String,
    source_key_hash: String,
    target_id: Uuid,
    source_contract_hash: String,
    source_fence: String,
    payload_hash: String,
}

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
        .map(|kind| format!("erpx_g13_target_{kind}_{suffix}"))
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

    async fn destroy(self) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.admin.close().await;
        let admin_url = std::env::var("ERPX_G13_DISPOSABLE_ADMIN_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://localhost/postgres?sslmode=verify-full".to_owned());
        let admin_options = PgConnectOptions::from_str(&admin_url)?;
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

fn proof(now: DateTime<Utc>) -> G13AuthorizationProof {
    G13AuthorizationProof {
        actor_ref: "local:g13-target-disposable".into(),
        permissions: BTreeSet::from([G13_EXECUTE_PERMISSION.into()]),
        mfa_verified: true,
        break_glass: false,
        expires_at: now + Duration::minutes(15),
        cutover_approval_ref: None,
    }
}

fn source_snapshot(namespace: &str) -> Result<LegacySourceSnapshot, serde_json::Error> {
    let customer_one = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"g13-target:customer:one");
    let customer_two = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"g13-target:customer:two");
    let user = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"g13-target:user:one");
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

    sqlx::query("INSERT INTO legacy.customers (id, status) VALUES ($1, 'active'), ($2, 'active')")
        .bind(Uuid::new_v4())
        .bind(Uuid::new_v4())
        .execute(source)
        .await?;
    sqlx::query("INSERT INTO legacy.users (id) VALUES ($1)")
        .bind(Uuid::new_v4())
        .execute(source)
        .await?;
    Ok(())
}

async fn set_holding(
    transaction: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(data_profile_key)
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

async fn scoped_count(pool: &PgPool, query: &str, run_id: Uuid) -> Result<i64, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    set_holding(&mut transaction, DATA_PROFILE_KEY).await?;
    let count = sqlx::query_scalar::<_, i64>(query)
        .bind(DATA_PROFILE_KEY)
        .bind(run_id)
        .fetch_one(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(count)
}

async fn scoped_event_count(
    pool: &PgPool,
    run_id: Uuid,
    event_type: &str,
) -> Result<i64, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    set_holding(&mut transaction, DATA_PROFILE_KEY).await?;
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND event_type=$2 AND payload->>'run_id'=$3::text",
    )
    .bind(DATA_PROFILE_KEY)
    .bind(event_type)
    .bind(run_id)
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(count)
}

async fn scoped_event_counts(
    pool: &PgPool,
    run_id: Uuid,
) -> Result<BTreeMap<String, i64>, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    set_holding(&mut transaction, DATA_PROFILE_KEY).await?;
    let rows = sqlx::query(
        "SELECT event_type, count(*)::bigint AS event_count
         FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND payload->>'run_id'=$2::text
         GROUP BY event_type
         ORDER BY event_type",
    )
    .bind(DATA_PROFILE_KEY)
    .bind(run_id)
    .fetch_all(&mut *transaction)
    .await?;
    let counts = rows
        .into_iter()
        .map(|row| Ok::<_, sqlx::Error>((row.try_get("event_type")?, row.try_get("event_count")?)))
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    transaction.commit().await?;
    Ok(counts)
}

async fn scoped_event_payload(
    pool: &PgPool,
    run_id: Uuid,
    event_type: &str,
) -> Result<Value, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    set_holding(&mut transaction, DATA_PROFILE_KEY).await?;
    let payload = sqlx::query_scalar::<_, Value>(
        "SELECT payload FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND event_type=$2 AND payload->>'run_id'=$3::text
         ORDER BY occurred_at, event_id LIMIT 1",
    )
    .bind(DATA_PROFILE_KEY)
    .bind(event_type)
    .bind(run_id)
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(payload)
}

async fn load_rows(
    pool: &PgPool,
    run_id: Uuid,
    rollback: bool,
) -> Result<Vec<StoredImportRow>, sqlx::Error> {
    let table = if rollback {
        "erpx_holding.g13_migration_rollback_rows"
    } else {
        "erpx_holding.g13_migration_import_rows"
    };
    let mut transaction = pool.begin().await?;
    set_holding(&mut transaction, DATA_PROFILE_KEY).await?;
    let rows = sqlx::query(&format!(
        "SELECT source_contract_hash, source_fence, source_kind,
                source_key, target_id, payload
         FROM {table}
         WHERE data_profile_key=$1 AND run_id=$2
         ORDER BY source_kind, source_key"
    ))
    .bind(DATA_PROFILE_KEY)
    .bind(run_id)
    .fetch_all(&mut *transaction)
    .await?
    .into_iter()
    .map(|row| {
        Ok::<_, sqlx::Error>(StoredImportRow {
            source_contract_hash: row.try_get("source_contract_hash")?,
            source_fence: row.try_get("source_fence")?,
            source_kind: row.try_get("source_kind")?,
            source_key: row.try_get("source_key")?,
            target_id: row.try_get("target_id")?,
            payload: row.try_get("payload")?,
        })
    })
    .collect::<Result<Vec<_>, _>>()?;
    transaction.commit().await?;
    Ok(rows)
}

async fn load_read_rows(
    pool: &PgPool,
    run_id: Uuid,
) -> Result<Vec<ReadProjectionRow>, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    set_holding(&mut transaction, DATA_PROFILE_KEY).await?;
    let rows = sqlx::query(
        "SELECT source_kind, source_key_hash, target_id,
                source_contract_hash, source_fence, payload_hash
         FROM erpx_holding.g13_migration_import_read_rows
         WHERE data_profile_key=$1 AND run_id=$2
         ORDER BY source_kind, source_key_hash",
    )
    .bind(DATA_PROFILE_KEY)
    .bind(run_id)
    .fetch_all(&mut *transaction)
    .await?
    .into_iter()
    .map(|row| {
        Ok::<_, sqlx::Error>(ReadProjectionRow {
            source_kind: row.try_get("source_kind")?,
            source_key_hash: row.try_get("source_key_hash")?,
            target_id: row.try_get("target_id")?,
            source_contract_hash: row.try_get("source_contract_hash")?,
            source_fence: row.try_get("source_fence")?,
            payload_hash: row.try_get("payload_hash")?,
        })
    })
    .collect::<Result<Vec<_>, _>>()?;
    transaction.commit().await?;
    Ok(rows)
}

async fn database_name(pool: &PgPool) -> Result<String, sqlx::Error> {
    sqlx::query_scalar("SELECT current_database()")
        .fetch_one(pool)
        .await
}

async fn read_payload_column_exists(pool: &PgPool) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM information_schema.columns
             WHERE table_schema='erpx_holding'
               AND table_name='g13_migration_import_read_rows'
               AND column_name='payload'
         )",
    )
    .fetch_one(pool)
    .await
}

fn alternate_fence(snapshot: &LegacySourceSnapshot) -> String {
    let candidate = "b".repeat(64);
    if candidate == snapshot.source_contract_hash {
        "c".repeat(64)
    } else {
        candidate
    }
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

    let source_database = database_name(&source).await?;
    let target_database = database_name(&target).await?;
    let read_database = database_name(&read).await?;
    let rollback_database = database_name(&rollback).await?;
    if source_database == target_database
        || target_database == read_database
        || target_database == rollback_database
    {
        return Err("G13 acceptance did not use physically separate databases".into());
    }

    let snapshot = source_snapshot("g13-target-disposable-source")?;
    if snapshot.counts.get("customers") != Some(&2)
        || snapshot.counts.get("users") != Some(&1)
        || snapshot
            .id_mappings
            .get("customers")
            .is_none_or(|mapping| mapping.len() != 2)
        || snapshot
            .id_mappings
            .get("users")
            .is_none_or(|mapping| mapping.len() != 1)
        || snapshot.source_contract_hash.len() != 64
    {
        return Err("source extractor did not capture the disposable G13 fixture".into());
    }
    let source_count_before: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM legacy.customers)
             + (SELECT count(*) FROM legacy.users)",
    )
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
                cohort: format!("g13-target-disposable-{run_id}"),
                source_contract_hash: snapshot.source_contract_hash.clone(),
                id_namespace: "g13-target-disposable-source".into(),
                actor_ref: actor.actor_ref.clone(),
                correlation_id: Uuid::new_v4(),
                idempotency_key: format!("g13-target-start:{run_id}"),
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
            &format!("g13-target-source:{run_id}"),
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
    store.project_read_model().await?;
    let status = store
        .list_read_model(10)
        .await?
        .into_iter()
        .find(|status| status.run_id == run_id)
        .ok_or("G13 status read model is missing the run")?;
    if status.phase != "source_frozen"
        || status.command_owner != "legacy"
        || status.source_snapshot_hash != snapshot.source_contract_hash
        || status.source_total_rows != 3
        || status.source_entity_kind_count != 2
    {
        return Err("G13 source-fenced control evidence is incomplete".into());
    }

    let importer = G13TargetImporter::new_with_read_pool(
        target.clone(),
        read.clone(),
        rollback.clone(),
        DATA_PROFILE_KEY,
    )?;
    let wrong_fence = alternate_fence(&snapshot);
    let rejected_wrong_fence = importer
        .import_scope(run_id, &snapshot, "customers", &wrong_fence)
        .await;
    if !matches!(
        rejected_wrong_fence,
        Err(G13TargetImportError::SourceFenceConflict)
    ) {
        return Err("target import accepted a non-snapshot source fence".into());
    }

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
        if result.rows_seen != expected
            || result.rows_applied != expected
            || result.target_count != expected
        {
            return Err(
                format!("source-fenced target import was incomplete for {source_kind}").into(),
            );
        }
    }
    let imported_rows = load_rows(&target, run_id, false).await?;
    if imported_rows.len() != 3
        || imported_rows.iter().any(|row| {
            row.source_contract_hash != snapshot.source_contract_hash
                || row.source_fence != snapshot.source_contract_hash
        })
    {
        return Err("target write ledger did not retain the source fence".into());
    }
    if scoped_event_count(&target, run_id, "migration.imported").await? != 3 {
        return Err("target import outbox evidence was not one event per source row".into());
    }

    let tampered_event_id: Uuid = {
        let mut transaction = target.begin().await?;
        set_holding(&mut transaction, DATA_PROFILE_KEY).await?;
        let event_id = sqlx::query_scalar(
            "SELECT event_id FROM erpx_holding.outbox_events
             WHERE data_profile_key=$1 AND event_type='migration.imported'
               AND payload->>'run_id'=$2::text
               AND payload->>'source_kind'='customers'
             ORDER BY event_id LIMIT 1",
        )
        .bind(DATA_PROFILE_KEY)
        .bind(run_id)
        .fetch_one(&mut *transaction)
        .await?;
        let updated = sqlx::query(
            "UPDATE erpx_holding.outbox_events
             SET event_version=2 WHERE event_id=$1",
        )
        .bind(event_id)
        .execute(&mut *transaction)
        .await?;
        if updated.rows_affected() != 1 {
            return Err("target outbox tamper fixture did not update exactly one event".into());
        }
        transaction.commit().await?;
        event_id
    };
    let tampered_replay = importer
        .import_scope(
            run_id,
            &snapshot,
            "customers",
            &snapshot.source_contract_hash,
        )
        .await;
    if !matches!(
        tampered_replay,
        Err(G13TargetImportError::SourceFenceConflict)
    ) {
        return Err(format!(
            "target import replay certified a tampered immutable outbox envelope: {tampered_replay:?}"
        )
        .into());
    }
    let mut restore_event = target.begin().await?;
    set_holding(&mut restore_event, DATA_PROFILE_KEY).await?;
    sqlx::query("UPDATE erpx_holding.outbox_events SET event_version=1 WHERE event_id=$1")
        .bind(tampered_event_id)
        .execute(&mut *restore_event)
        .await?;
    restore_event.commit().await?;

    for source_kind in ["customers", "users"] {
        let first_shadow = importer
            .shadow_replay(
                run_id,
                &snapshot,
                source_kind,
                &snapshot.source_contract_hash,
            )
            .await?;
        let replay_shadow = importer
            .shadow_replay(
                run_id,
                &snapshot,
                source_kind,
                &snapshot.source_contract_hash,
            )
            .await?;
        if !first_shadow.matched() || first_shadow != replay_shadow {
            return Err(format!("shadow replay was not stable for {source_kind}").into());
        }
    }
    if scoped_event_count(&target, run_id, "migration.shadow_replay_validated").await? != 2 {
        return Err("shadow replay rerun duplicated its durable evidence".into());
    }

    let rows_before_mapping_conflict = load_rows(&target, run_id, false).await?;
    let first_customer_key = match snapshot
        .id_mappings
        .get("customers")
        .and_then(|mapping| mapping.keys().next())
        .cloned()
    {
        Some(key) => key,
        None => return Err("customer mapping is unexpectedly empty".into()),
    };
    let mut conflicting_snapshot = snapshot.clone();
    conflicting_snapshot
        .id_mappings
        .get_mut("customers")
        .expect("customer mapping checked above")
        .insert(first_customer_key, Uuid::new_v4());
    let rejected_mapping = importer
        .import_scope(
            run_id,
            &conflicting_snapshot,
            "customers",
            &snapshot.source_contract_hash,
        )
        .await;
    if !matches!(
        rejected_mapping,
        Err(G13TargetImportError::SourceFenceConflict)
    ) {
        return Err("target import accepted an immutable mapping change".into());
    }
    if load_rows(&target, run_id, false).await? != rows_before_mapping_conflict {
        return Err("immutable mapping rejection changed the target ledger".into());
    }

    let projection = importer.project_read_model(run_id).await?;
    let projection_replay = importer.project_read_model(run_id).await?;
    if projection.rows_projected != 3
        || projection_replay.rows_projected != 3
        || projection.projection_digest != projection_replay.projection_digest
    {
        return Err("read-model projection was not complete and replay-stable".into());
    }
    if read_payload_column_exists(&read).await? {
        return Err("G13 read model exposed an authoritative payload column".into());
    }
    let projected_rows = load_read_rows(&read, run_id).await?;
    if projected_rows.len() != 3
        || projected_rows.iter().any(|row| {
            row.source_key_hash.len() != 64
                || row.payload_hash.len() != 64
                || row.source_contract_hash != snapshot.source_contract_hash
                || row.source_fence != snapshot.source_contract_hash
        })
        || projected_rows
            .iter()
            .map(|row| row.target_id)
            .collect::<BTreeSet<_>>()
            != imported_rows.iter().map(|row| row.target_id).collect()
    {
        return Err("separate G13 read-model rows did not project the target identities".into());
    }
    if scoped_event_count(&target, run_id, "migration.read_projection_rebuilt").await? != 1 {
        return Err("read-model projection replay duplicated its durable evidence".into());
    }

    let incomplete_final_delta = {
        let mut snapshot = snapshot.clone();
        snapshot.counts.insert("customers".into(), 3);
        importer
            .apply_final_delta(
                run_id,
                &snapshot,
                "customers",
                &snapshot.source_contract_hash,
                "legacy-write-fence:g13-target-test",
                true,
                Utc::now(),
            )
            .await
    };
    if !matches!(
        incomplete_final_delta,
        Err(G13TargetImportError::IncompleteSourceSnapshot)
    ) {
        return Err("final delta accepted an incomplete identity mapping".into());
    }
    let rejected_final_delta = importer
        .apply_final_delta(
            run_id,
            &snapshot,
            "customers",
            &snapshot.source_contract_hash,
            "legacy-write-fence:g13-target-test",
            false,
            Utc::now(),
        )
        .await;
    if !matches!(
        rejected_final_delta,
        Err(G13TargetImportError::WriteFenceRequired)
    ) {
        return Err("final delta accepted an unfenced Legacy write boundary".into());
    }
    let stale_final_delta = importer
        .apply_final_delta(
            run_id,
            &snapshot,
            "customers",
            &snapshot.source_contract_hash,
            "legacy-write-fence:g13-target-test",
            true,
            Utc::now() - Duration::minutes(6),
        )
        .await;
    if !matches!(
        stale_final_delta,
        Err(G13TargetImportError::WriteFenceRequired)
    ) {
        return Err("final delta accepted a stale Legacy write boundary".into());
    }
    let final_delta_validated_at = Utc::now();
    let final_delta = importer
        .apply_final_delta(
            run_id,
            &snapshot,
            "customers",
            &snapshot.source_contract_hash,
            "legacy-write-fence:g13-target-test",
            true,
            final_delta_validated_at,
        )
        .await?;
    let final_delta_replay = importer
        .apply_final_delta(
            run_id,
            &snapshot,
            "customers",
            &snapshot.source_contract_hash,
            "legacy-write-fence:g13-target-test",
            true,
            final_delta_validated_at,
        )
        .await?;
    if final_delta.rows_applied != 0
        || final_delta_replay.rows_applied != 0
        || scoped_event_count(&target, run_id, "migration.final_delta_imported").await? != 3
    {
        return Err("final delta replay was not idempotent".into());
    }

    let reconciled = importer.reconcile_run(run_id, &snapshot).await?;
    if !reconciled.report.is_reconciled()
        || reconciled.target_counts.get("customers") != Some(&2)
        || reconciled.target_counts.get("users") != Some(&1)
        || reconciled.source_identity_digest != reconciled.target_identity_digest
    {
        return Err(
            "reconciliation did not confirm the imported source identities and snapshot".into(),
        );
    }
    let mut drifted_snapshot = snapshot.clone();
    drifted_snapshot.counts.insert("customers".into(), 3);
    drifted_snapshot
        .counts
        .insert("missing-source-kind".into(), 1);
    let drift = importer.reconcile_run(run_id, &drifted_snapshot).await?;
    if drift.report.is_reconciled() || drift.report.differences.len() != 2 {
        return Err("reconciliation did not retain source/target differences".into());
    }
    for difference in &drift.report.differences {
        if difference.explanation.is_none() {
            return Err("reconciliation difference was not explained".into());
        }
        match difference.key.as_str() {
            "customers"
                if difference.source_count == Some(3) && difference.target_count == Some(2) => {}
            "missing-source-kind"
                if difference.source_count == Some(1) && difference.target_count == Some(0) => {}
            key => return Err(format!("unexpected reconciliation difference for {key}").into()),
        }
    }

    // Equal row counts must not hide an immutable source-key-to-target-ID
    // substitution. This mutation is confined to the disposable target DB;
    // restoring the original row below proves the rollback rehearsal still
    // starts from the authoritative state after the failed reconciliation.
    let customer_source_key = snapshot
        .id_mappings
        .get("customers")
        .and_then(|mapping| mapping.keys().next())
        .ok_or("customer identity mapping missing")?
        .clone();
    let original_customer_id = snapshot
        .id_mappings
        .get("customers")
        .and_then(|mapping| mapping.get(&customer_source_key))
        .copied()
        .ok_or("customer target identity missing")?;
    let substituted_customer_id = Uuid::new_v4();
    let mut target_tx = target.begin().await?;
    set_holding(&mut target_tx, DATA_PROFILE_KEY).await?;
    sqlx::query(
        "UPDATE erpx_holding.g13_migration_import_rows
         SET target_id=$1, payload=jsonb_set(payload, '{target_id}', to_jsonb($1::text), false)
         WHERE data_profile_key=$2 AND run_id=$3 AND source_kind='customers' AND source_key=$4",
    )
    .bind(substituted_customer_id)
    .bind(DATA_PROFILE_KEY)
    .bind(run_id)
    .bind(&customer_source_key)
    .execute(&mut *target_tx)
    .await?;
    target_tx.commit().await?;
    let identity_drift = importer.reconcile_run(run_id, &snapshot).await?;
    if identity_drift.report.is_reconciled()
        || identity_drift.source_identity_digest == identity_drift.target_identity_digest
        || !identity_drift
            .report
            .differences
            .iter()
            .any(|difference| difference.key == "customers")
    {
        return Err("reconciliation did not detect an equal-count identity substitution".into());
    }
    let mut target_tx = target.begin().await?;
    set_holding(&mut target_tx, DATA_PROFILE_KEY).await?;
    sqlx::query(
        "UPDATE erpx_holding.g13_migration_import_rows
         SET target_id=$1, payload=jsonb_set(payload, '{target_id}', to_jsonb($1::text), false)
         WHERE data_profile_key=$2 AND run_id=$3 AND source_kind='customers' AND source_key=$4",
    )
    .bind(original_customer_id)
    .bind(DATA_PROFILE_KEY)
    .bind(run_id)
    .bind(&customer_source_key)
    .execute(&mut *target_tx)
    .await?;
    target_tx.commit().await?;

    let target_before_rollback = load_rows(&target, run_id, false).await?;
    let rejected_rollback = importer.rehearse_rollback(run_id, &wrong_fence).await;
    if !matches!(
        rejected_rollback,
        Err(G13TargetImportError::SourceFenceConflict)
    ) {
        return Err("rollback accepted a mismatched source fence".into());
    }
    if load_rows(&target, run_id, false).await? != target_before_rollback {
        return Err("rejected rollback changed the target ledger".into());
    }
    let rollback_result = importer
        .rehearse_rollback(run_id, &snapshot.source_contract_hash)
        .await?;
    let rollback_rows = load_rows(&rollback, run_id, true).await?;
    if rollback_result.rows_captured != 3
        || rollback_result.rows_restored != 3
        || rollback_result.outbox_events_replayed != 3
        || !rollback_result.read_model_rebuilt
        || rollback_result.restored_state_hash.len() != 64
        || rollback_rows != target_before_rollback
        || load_rows(&target, run_id, false).await? != target_before_rollback
        || load_read_rows(&read, run_id).await?.len() != 3
    {
        return Err(
            "rollback capture, restore, or read-model rebuild evidence was incomplete".into(),
        );
    }
    let rollback_read_rows = load_read_rows(&read, run_id).await?;

    // Reproduce a worker crash after the immutable rollback snapshot and
    // before the disposable target/read projections finish restoring. A
    // retry must restore from the rollback database, not accept the empty
    // target as a successful no-op.
    let mut interrupted_target_tx = target.begin().await?;
    set_holding(&mut interrupted_target_tx, DATA_PROFILE_KEY).await?;
    sqlx::query(
        "DELETE FROM erpx_holding.g13_migration_import_rows
         WHERE data_profile_key=$1 AND run_id=$2",
    )
    .bind(DATA_PROFILE_KEY)
    .bind(run_id)
    .execute(&mut *interrupted_target_tx)
    .await?;
    interrupted_target_tx.commit().await?;
    let mut interrupted_read_tx = read.begin().await?;
    set_holding(&mut interrupted_read_tx, DATA_PROFILE_KEY).await?;
    sqlx::query(
        "DELETE FROM erpx_holding.g13_migration_import_read_rows
         WHERE data_profile_key=$1 AND run_id=$2",
    )
    .bind(DATA_PROFILE_KEY)
    .bind(run_id)
    .execute(&mut *interrupted_read_tx)
    .await?;
    interrupted_read_tx.commit().await?;
    if !load_rows(&target, run_id, false).await?.is_empty()
        || !load_read_rows(&read, run_id).await?.is_empty()
    {
        return Err("interrupted rollback setup did not clear disposable projections".into());
    }
    let restarted_importer = G13TargetImporter::new_with_read_pool(
        target.clone(),
        read.clone(),
        rollback.clone(),
        DATA_PROFILE_KEY,
    )?;
    // Reconstruct the runtime before retrying. The rollback fence and replay
    // evidence must come from PostgreSQL, not from importer-instance state.
    let rollback_replay = restarted_importer
        .rehearse_rollback(run_id, &snapshot.source_contract_hash)
        .await?;
    if rollback_replay.rows_captured != 3
        || rollback_replay.rows_restored != 3
        || rollback_replay.outbox_events_replayed != 3
        || rollback_replay.restored_state_hash != rollback_result.restored_state_hash
        || scoped_event_count(&target, run_id, "migration.import_rollback_replayed").await? != 3
        || load_rows(&rollback, run_id, true).await? != rollback_rows
        || load_rows(&target, run_id, false).await? != target_before_rollback
        || load_read_rows(&read, run_id).await? != rollback_read_rows
    {
        return Err("rollback replay was not idempotent or immutable".into());
    }

    let retention_at = Utc::now();
    let rejected_retention = importer
        .expire_rehearsal(run_id, retention_at, retention_at, &wrong_fence)
        .await;
    if !matches!(
        rejected_retention,
        Err(G13TargetImportError::RollbackFenceRequired)
    ) {
        return Err("retention accepted a source fence without rollback evidence".into());
    }
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
        || scoped_count(
            &target,
            "SELECT count(*) FROM erpx_holding.g13_migration_import_rows
             WHERE data_profile_key=$1 AND run_id=$2",
            run_id,
        )
        .await?
            != 0
        || scoped_count(
            &read,
            "SELECT count(*) FROM erpx_holding.g13_migration_import_read_rows
             WHERE data_profile_key=$1 AND run_id=$2",
            run_id,
        )
        .await?
            != 0
        || load_rows(&rollback, run_id, true).await?.len() != 3
    {
        return Err(
            "retention did not preserve rollback evidence while expiring projections".into(),
        );
    }
    let retention_replay = importer
        .expire_rehearsal(
            run_id,
            retention_at,
            retention_at,
            &snapshot.source_contract_hash,
        )
        .await?;
    if retention_replay.rows_expired != 0
        || retention_replay.read_rows_expired != 0
        || scoped_event_count(&target, run_id, "migration.rehearsal_retention_expired").await? != 1
    {
        return Err("retention rerun was not safe and idempotent".into());
    }
    let retention_payload =
        scoped_event_payload(&target, run_id, "migration.rehearsal_retention_expired").await?;
    if retention_payload.get("rollback_rows_preserved") != Some(&Value::Bool(true))
        || retention_payload.get("immutable_outbox_preserved") != Some(&Value::Bool(true))
    {
        return Err("retention event did not retain rollback/outbox evidence".into());
    }

    let expected_events = BTreeMap::from([
        ("migration.final_delta_imported".to_owned(), 3),
        ("migration.import_rollback_replayed".to_owned(), 3),
        ("migration.imported".to_owned(), 3),
        ("migration.read_projection_rebuilt".to_owned(), 1),
        ("migration.rehearsal_retention_expired".to_owned(), 1),
        ("migration.shadow_replay_validated".to_owned(), 2),
        ("migration.target_reconciled".to_owned(), 3),
    ]);
    if scoped_event_counts(&target, run_id).await? != expected_events {
        return Err("target outbox evidence was not complete or replay-stable".into());
    }
    if sqlx::query_scalar::<_, i64>(
        "SELECT (SELECT count(*) FROM legacy.customers)
             + (SELECT count(*) FROM legacy.users)",
    )
    .fetch_one(&source)
    .await?
        != source_count_before
    {
        return Err("G13 target acceptance mutated the source fixture".into());
    }
    let phase: String =
        sqlx::query_scalar("SELECT phase FROM erpx_control.migration_runs WHERE run_id=$1")
            .bind(run_id)
            .fetch_one(&control_write)
            .await?;
    if phase != "source_frozen" {
        return Err("target acceptance unexpectedly advanced command ownership".into());
    }
    Ok(())
}

#[tokio::test]
#[ignore = "creates six disposable local PostgreSQL databases; run explicitly with --ignored"]
async fn g13_target_import_disposable_sqlx_acceptance_is_real_and_safe_to_rerun() -> TestResult {
    let cluster = DisposableDatabases::create().await?;
    let result = execute_acceptance(&cluster).await;
    let cleanup = cluster.destroy().await;
    cleanup?;
    result
}
