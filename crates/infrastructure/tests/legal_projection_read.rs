//! Focused and disposable verification for the typed Legal read projection.

use chrono::{Duration, Utc};
use erpx_domain::contracts::{ActorAttribution, ActorKind, AuthorityScope, CommandContext};
use erpx_domain::legal::{LegalAccessHistoryEntry, LegalContract, LegalDocument, LegalScope};
use erpx_infrastructure::{
    LegalReadScope, SqlxLegalPersistence, SqlxLegalProjectionStore, SqlxLegalReadStore,
};
use sqlx::PgPool;
use uuid::Uuid;

fn context(actor: &str, scope: AuthorityScope, key: &str) -> CommandContext {
    CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: actor.into(),
            kind: ActorKind::HoldingUser,
            session_id: Some(Uuid::new_v4()),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope,
        "legal.projection.test",
    )
    .expect("context")
}

#[test]
fn migration_has_exact_digest_constraints_envelope_metadata_and_forced_rls() {
    let sql = include_str!("../../migration/migrations_holding_read/0001_baseline.sql");
    let legal_sql = sql
        .split_inclusive(';')
        .filter(|statement| statement.contains("erpx_holding.legal_"))
        .collect::<String>();
    for table in [
        "legal_contract_read_rows",
        "legal_contract_party_read_rows",
        "legal_contract_approval_read_rows",
        "legal_contract_retention_read_rows",
        "legal_document_read_rows",
        "legal_document_version_read_rows",
        "legal_access_history_read_rows",
        "legal_projection_checkpoints",
        "legal_projection_event_receipts",
    ] {
        assert!(sql.contains(&format!("CREATE TABLE IF NOT EXISTS erpx_holding.{table}")));
        assert!(sql.contains(&format!(
            "ALTER TABLE erpx_holding.{table} FORCE ROW LEVEL SECURITY"
        )));
    }
    assert_eq!(
        legal_sql.matches("^sha256:[0-9A-Fa-f]{64}$").count(),
        2,
        "contract and document-version digests must be exact SHA-256 references"
    );
    for column in [
        "source_contract_version",
        "source_event_version",
        "source_actor_ref",
        "source_actor_kind",
        "source_correlation_id",
        "source_causation_id",
        "source_idempotency_key",
    ] {
        assert!(
            legal_sql.contains(column),
            "missing read envelope column {column}"
        );
    }
    let lower = legal_sql.to_ascii_lowercase();
    assert!(!lower.contains("bytea"));
    assert!(!lower.contains("document_bytes"));
    assert!(!lower.contains("payload json"));
}

#[tokio::test]
#[ignore = "requires disposable holding write and read PostgreSQL databases"]
async fn live_legal_projection_is_typed_scoped_idempotent_and_rebuildable() {
    let write_url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let read_url = std::env::var("ERPX_TEST_HOLDING_READ_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_READ_DATABASE_URL is required");
    let write_pool = PgPool::connect(&write_url).await.expect("write pool");
    let read_pool = PgPool::connect(&read_url).await.expect("read pool");
    erpx_migration::migrate_holding_write(&write_pool)
        .await
        .expect("holding write migration");
    erpx_migration::migrate_holding_read(&read_pool)
        .await
        .expect("holding read migration");

    let data_profile_key = format!("holding-legal-read-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let scope = LegalScope::new(holding_id, company_id, None).expect("legal scope");
    let contract_context = context("creator", scope.active_scope(), "legal-contract-1");
    let contract = LegalContract::new(
        Uuid::new_v4(),
        "contract-read-001",
        "Anonymized contract",
        scope.clone(),
        "opaque/contracts/read-001-v1",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        Utc::now() + Duration::days(30),
        &contract_context,
    )
    .expect("contract");
    let document_context = context("creator", scope.active_scope(), "legal-document-1");
    let mut document = LegalDocument::new(
        Uuid::new_v4(),
        scope.clone(),
        "Anonymized document",
        &document_context,
    )
    .expect("document");
    document
        .add_version(
            "opaque/documents/read-001-v1",
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            &document_context,
        )
        .expect("document version");
    let access_context = context("creator", scope.active_scope(), "legal-access-1");
    let access = LegalAccessHistoryEntry::new(
        document.id,
        1,
        "download",
        "legal projection proof",
        &access_context,
        Utc::now(),
        scope.clone(),
    )
    .expect("access history");

    let source = SqlxLegalPersistence::new(write_pool.clone(), &data_profile_key).expect("source");
    source
        .save_contract(&contract, &contract_context, None)
        .await
        .expect("contract source event");
    source
        .save_document(&document, &document_context, None)
        .await
        .expect("document source event");
    source
        .record_access_history(&access, &access_context)
        .await
        .expect("access source event");

    // A retention fact is authoritative source evidence, not a repair cache.
    // Corrupt it and prove that the next source mutation fails closed without
    // advancing the contract, outbox, or idempotency state.
    let wrong_holding_id = Uuid::new_v4();
    let mut corrupt_retention_tx = write_pool
        .begin()
        .await
        .expect("retention corruption transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *corrupt_retention_tx)
        .await
        .expect("retention corruption holding scope");
    sqlx::query(
        "UPDATE erpx_holding.legal_contract_retention
         SET holding_id=$1
         WHERE data_profile_key=$2 AND contract_id=$3",
    )
    .bind(wrong_holding_id)
    .bind(&data_profile_key)
    .bind(contract.id)
    .execute(&mut *corrupt_retention_tx)
    .await
    .expect("corrupt retention fact binding");
    corrupt_retention_tx
        .commit()
        .await
        .expect("retention corruption commit");

    let mut changed_contract = contract.clone();
    changed_contract.title = "retention integrity mutation".into();
    let changed_context = context(
        "creator",
        scope.active_scope(),
        "legal-contract-retention-integrity",
    );
    changed_contract.version = 2;
    changed_contract.last_command_id = changed_context.command_id;
    let retention_error = source
        .save_contract(&changed_contract, &changed_context, Some(1))
        .await
        .expect_err("inconsistent retention fact must fail closed");
    assert!(
        retention_error
            .to_string()
            .contains("retention fact holding binding is inconsistent"),
        "unexpected retention integrity error: {retention_error}"
    );
    let (source_version, source_event_count, idempotency_count): (i64, i64, i64) = sqlx::query_as(
        "SELECT contracts.version,
                    (SELECT count(*) FROM erpx_holding.outbox_events
                     WHERE data_profile_key=$1 AND aggregate_type='legal_contract'
                       AND aggregate_id=$2),
                    (SELECT count(*) FROM erpx_holding.legal_command_idempotency
                     WHERE data_profile_key=$1 AND aggregate_type='legal_contract'
                       AND aggregate_id=$2)
             FROM erpx_holding.legal_contracts AS contracts
             WHERE contracts.data_profile_key=$1 AND contracts.contract_id=$2",
    )
    .bind(&data_profile_key)
    .bind(contract.id)
    .fetch_one(&write_pool)
    .await
    .expect("retention rollback snapshot");
    assert_eq!(source_version, 1);
    assert_eq!(source_event_count, 1);
    assert_eq!(idempotency_count, 1);

    let mut repair_retention_tx = write_pool
        .begin()
        .await
        .expect("retention repair transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *repair_retention_tx)
        .await
        .expect("retention repair holding scope");
    sqlx::query(
        "UPDATE erpx_holding.legal_contract_retention
         SET holding_id=$1
         WHERE data_profile_key=$2 AND contract_id=$3",
    )
    .bind(holding_id)
    .bind(&data_profile_key)
    .bind(contract.id)
    .execute(&mut *repair_retention_tx)
    .await
    .expect("repair retention fact binding");
    repair_retention_tx
        .commit()
        .await
        .expect("retention repair commit");

    let projection =
        SqlxLegalProjectionStore::from_pools(&write_pool, &read_pool, &data_profile_key)
            .expect("projection store");
    let page = projection
        .read_source_event_page(None, 100)
        .await
        .expect("source page");
    assert_eq!(page.events.len(), 3);
    assert!(page.events.iter().all(|event| {
        event.contract_version == 1
            && event.event_version == 1
            && !event.correlation_id.is_nil()
            && !event.idempotency_key.is_empty()
    }));
    let contract_index = page
        .events
        .iter()
        .position(|event| {
            matches!(
                event.payload,
                erpx_infrastructure::LegalOutboxPayload::ContractSaved(_)
            )
        })
        .expect("expected a contract source event");
    let mut forged = page.events[contract_index].clone();
    if let erpx_infrastructure::LegalOutboxPayload::ContractSaved(contract) = &mut forged.payload {
        contract.title = "forged before projection".into();
    } else {
        panic!("expected the first legal source event to be a contract event");
    }
    assert!(
        projection.project_event(&forged).await.is_err(),
        "a forged typed Legal event must be rejected before projection"
    );
    for event in &page.events {
        assert!(
            projection
                .project_event(event)
                .await
                .expect("project")
                .applied
        );
    }
    for event in &page.events {
        assert!(
            !projection
                .project_event(event)
                .await
                .expect("replay")
                .applied
        );
    }

    let mut contract_v2 = contract.clone();
    let contract_v2_context = context(
        "creator",
        contract.scope.active_scope(),
        "legal-contract-source-gap-v2",
    );
    contract_v2.title = "Anonymized contract revision".into();
    contract_v2.version = 2;
    contract_v2.last_command_id = contract_v2_context.command_id;
    source
        .save_contract(&contract_v2, &contract_v2_context, Some(1))
        .await
        .expect("contract revision source event");
    let page_with_revision = projection
        .read_source_event_page(None, 100)
        .await
        .expect("revised source page");
    assert_eq!(page_with_revision.events.len(), 4);
    for event in &page_with_revision.events {
        projection
            .project_event(event)
            .await
            .expect("project revised source page");
    }

    let consistent = projection
        .reconcile_projection()
        .await
        .expect("green legal projection reconciliation");
    assert!(consistent.is_consistent());
    assert_eq!(consistent.retention_mismatch_count, 0);

    let contract_source_event_id = page_with_revision
        .events
        .iter()
        .find(|event| {
            event.aggregate_id == contract.id
                && event.aggregate_version == 2
                && matches!(
                    event.payload,
                    erpx_infrastructure::LegalOutboxPayload::ContractSaved(_)
                )
        })
        .expect("revised contract source event")
        .event_id;
    let forged_checkpoint_event_id = Uuid::new_v4();
    let mut checkpoint_corruption_tx = read_pool
        .begin()
        .await
        .expect("checkpoint corruption transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *checkpoint_corruption_tx)
        .await
        .expect("checkpoint holding scope");
    sqlx::query(
        "UPDATE erpx_holding.legal_projection_checkpoints
         SET last_event_id=$1
         WHERE data_profile_key=$2 AND projection_name='legal_documents'
           AND aggregate_type='legal_contract' AND aggregate_id=$3",
    )
    .bind(forged_checkpoint_event_id)
    .bind(&data_profile_key)
    .bind(contract.id)
    .execute(&mut *checkpoint_corruption_tx)
    .await
    .expect("corrupt checkpoint event identity");
    checkpoint_corruption_tx
        .commit()
        .await
        .expect("checkpoint corruption commit");
    let checkpoint_mismatch = projection
        .reconcile_projection()
        .await
        .expect("checkpoint identity reconciliation");
    assert!(!checkpoint_mismatch.is_consistent());
    assert_eq!(checkpoint_mismatch.stale_aggregate_count, 1);

    let mut checkpoint_repair_tx = read_pool
        .begin()
        .await
        .expect("checkpoint repair transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *checkpoint_repair_tx)
        .await
        .expect("checkpoint repair holding scope");
    sqlx::query(
        "UPDATE erpx_holding.legal_projection_checkpoints
         SET last_event_id=$1
         WHERE data_profile_key=$2 AND projection_name='legal_documents'
           AND aggregate_type='legal_contract' AND aggregate_id=$3",
    )
    .bind(contract_source_event_id)
    .bind(&data_profile_key)
    .bind(contract.id)
    .execute(&mut *checkpoint_repair_tx)
    .await
    .expect("repair checkpoint event identity");
    checkpoint_repair_tx
        .commit()
        .await
        .expect("checkpoint repair commit");
    assert!(
        projection
            .reconcile_projection()
            .await
            .expect("repaired checkpoint reconciliation")
            .is_consistent()
    );

    let wrong_source_holding_id = Uuid::new_v4();
    let mut corrupt_source_retention_tx = write_pool
        .begin()
        .await
        .expect("source retention corruption transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *corrupt_source_retention_tx)
        .await
        .expect("source retention holding scope");
    sqlx::query(
        "UPDATE erpx_holding.legal_contract_retention
         SET holding_id=$1
         WHERE data_profile_key=$2 AND contract_id=$3",
    )
    .bind(wrong_source_holding_id)
    .bind(&data_profile_key)
    .bind(contract.id)
    .execute(&mut *corrupt_source_retention_tx)
    .await
    .expect("corrupt source retention holding");
    corrupt_source_retention_tx
        .commit()
        .await
        .expect("source retention corruption commit");
    let source_retention_mismatch = projection
        .reconcile_projection()
        .await
        .expect("source retention binding reconciliation");
    assert!(!source_retention_mismatch.is_consistent());
    assert_eq!(source_retention_mismatch.retention_mismatch_count, 1);

    let mut repair_source_retention_tx = write_pool
        .begin()
        .await
        .expect("source retention repair transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *repair_source_retention_tx)
        .await
        .expect("source retention repair holding scope");
    sqlx::query(
        "UPDATE erpx_holding.legal_contract_retention
         SET holding_id=$1
         WHERE data_profile_key=$2 AND contract_id=$3",
    )
    .bind(holding_id)
    .bind(&data_profile_key)
    .bind(contract.id)
    .execute(&mut *repair_source_retention_tx)
    .await
    .expect("repair source retention holding");
    repair_source_retention_tx
        .commit()
        .await
        .expect("source retention repair commit");

    let duplicate_event_id = Uuid::new_v4();
    let mut duplicate_source_tx = write_pool
        .begin()
        .await
        .expect("duplicate source transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *duplicate_source_tx)
        .await
        .expect("duplicate source holding scope");
    sqlx::query(
        "INSERT INTO erpx_holding.outbox_events
            (event_id, aggregate_type, aggregate_id, data_profile_key, aggregate_version,
             event_type, payload, occurred_at, contract_version, event_version,
             active_scope, actor, correlation_id, causation_id, idempotency_key)
         SELECT $1, aggregate_type, aggregate_id, data_profile_key, aggregate_version,
                event_type, payload, occurred_at, contract_version, event_version,
                active_scope, actor, correlation_id, causation_id, idempotency_key
         FROM erpx_holding.outbox_events
         WHERE data_profile_key=$2 AND event_id=$3",
    )
    .bind(duplicate_event_id)
    .bind(&data_profile_key)
    .bind(contract_source_event_id)
    .execute(&mut *duplicate_source_tx)
    .await
    .expect("duplicate source event fixture");
    duplicate_source_tx
        .commit()
        .await
        .expect("duplicate source commit");
    let duplicate_source_mismatch = projection
        .reconcile_projection()
        .await
        .expect("duplicate source reconciliation");
    assert!(!duplicate_source_mismatch.is_consistent());
    assert!(duplicate_source_mismatch.stale_aggregate_count > 0);

    let mut remove_duplicate_source_tx = write_pool
        .begin()
        .await
        .expect("duplicate source cleanup transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *remove_duplicate_source_tx)
        .await
        .expect("duplicate source cleanup holding scope");
    sqlx::query("DELETE FROM erpx_holding.outbox_events WHERE event_id=$1")
        .bind(duplicate_event_id)
        .execute(&mut *remove_duplicate_source_tx)
        .await
        .expect("remove duplicate source fixture");
    remove_duplicate_source_tx
        .commit()
        .await
        .expect("duplicate source cleanup commit");
    assert!(
        projection
            .reconcile_projection()
            .await
            .expect("post-duplicate reconciliation")
            .is_consistent()
    );

    let mut corrupt_read_tx = read_pool
        .begin()
        .await
        .expect("retention corruption transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *corrupt_read_tx)
        .await
        .expect("retention read holding scope");
    sqlx::query(
        "UPDATE erpx_holding.legal_contract_retention_read_rows
         SET retention_state='ended'
         WHERE data_profile_key=$1 AND contract_id=$2",
    )
    .bind(&data_profile_key)
    .bind(contract.id)
    .execute(&mut *corrupt_read_tx)
    .await
    .expect("corrupt retention read fixture");
    corrupt_read_tx
        .commit()
        .await
        .expect("retention corruption commit");
    let retention_mismatch = projection
        .reconcile_projection()
        .await
        .expect("retention reconciliation");
    assert!(!retention_mismatch.is_consistent());
    assert_eq!(retention_mismatch.retention_mismatch_count, 1);

    let read = SqlxLegalReadStore::from_pools(&read_pool, &write_pool, &data_profile_key)
        .expect("read adapter");
    let read_scope = LegalReadScope {
        holding_id,
        company_id,
        branch_id: None,
    };
    let contracts = read
        .list_contracts(read_scope, 10, None)
        .await
        .expect("contract read");
    assert_eq!(contracts.rows.len(), 1);
    assert_eq!(contracts.rows[0].source.contract_version, 1);
    assert_eq!(contracts.rows[0].source_version, 2);
    assert_eq!(contracts.rows[0].parties.len(), 0);
    assert!(contracts.freshness.last_event_id.is_some());
    assert!(contracts.freshness.last_projected_at.is_some());
    assert!(!contracts.freshness.stale);
    let documents = read
        .list_documents(read_scope, 10, None)
        .await
        .expect("document read");
    assert_eq!(documents.rows.len(), 1);
    assert_eq!(documents.rows[0].versions.len(), 1);
    assert_eq!(documents.rows[0].versions[0].version, 1);
    assert!(documents.freshness.source_aggregate_version.is_some());
    let access_rows = read
        .list_access_history(read_scope, Some(document.id), 10, None)
        .await
        .expect("access read");
    assert_eq!(access_rows.rows.len(), 1);
    assert_eq!(access_rows.rows[0].document_id, document.id);
    assert!(access_rows.freshness.lag_seconds < 300);

    let other_scope = LegalReadScope {
        holding_id: Uuid::new_v4(),
        company_id: Uuid::new_v4(),
        branch_id: None,
    };
    assert!(
        read.list_contracts(other_scope, 10, None)
            .await
            .expect("other scope read")
            .rows
            .is_empty()
    );

    let preparation = projection.clear_projection().await.expect("clear");
    assert!(preparation.rows_cleared >= 3);
    let mut cursor = None;
    let mut replayed = 0_u64;
    loop {
        let next = projection
            .read_source_event_page(cursor.as_ref(), 2)
            .await
            .expect("rebuild source page");
        for event in &next.events {
            if projection
                .project_event(event)
                .await
                .expect("rebuild event")
                .applied
            {
                replayed += 1;
            }
        }
        if !next.has_more {
            break;
        }
        cursor = next.next_cursor;
    }
    assert_eq!(replayed, 4);

    let mut source_gap_tx = write_pool.begin().await.expect("source gap transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *source_gap_tx)
        .await
        .expect("source gap holding scope");
    sqlx::query(
        "DELETE FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND aggregate_type='legal_contract'
           AND aggregate_id=$2 AND aggregate_version=1",
    )
    .bind(&data_profile_key)
    .bind(contract.id)
    .execute(&mut *source_gap_tx)
    .await
    .expect("delete source version one fixture");
    source_gap_tx.commit().await.expect("source gap commit");
    let source_gap = projection
        .reconcile_projection()
        .await
        .expect("source gap reconciliation");
    assert!(!source_gap.is_consistent());
    assert_eq!(source_gap.stale_aggregate_count, 1);

    sqlx::query(
        "INSERT INTO erpx_holding.outbox_events
            (event_id, aggregate_type, aggregate_id, data_profile_key, aggregate_version,
             event_type, payload, occurred_at, contract_version, event_version,
             active_scope, actor, correlation_id, idempotency_key)
         VALUES ($1,'legal_contract',$2,$3,1,'legacy.legal', $4, now(), NULL, NULL,
                 NULL, NULL, NULL, NULL)",
    )
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .bind(&data_profile_key)
    .bind(serde_json::json!({"legacy": true}))
    .execute(&write_pool)
    .await
    .expect("legacy source fixture");
    assert!(projection.read_source_event_page(None, 100).await.is_err());
}
