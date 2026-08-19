//! Disposable end-to-end evidence for legal source persistence.

use chrono::{Duration, Utc};
use erpx_domain::contracts::{ActorAttribution, ActorKind, AuthorityScope, CommandContext};
use erpx_domain::legal::{ContractParty, LegalContract, LegalDocument, LegalScope};
use erpx_infrastructure::SqlxLegalPersistence;
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
        "legal.persistence.test",
    )
    .expect("context")
}

#[tokio::test]
#[ignore = "requires one disposable holding PostgreSQL write database"]
async fn live_legal_write_is_normalized_scoped_and_idempotent() {
    let url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("write pool");
    erpx_migration::migrate_holding_write(&pool)
        .await
        .expect("holding write migration");

    let data_profile_key = format!("holding-legal-{}", Uuid::new_v4());
    let scope = LegalScope::new(Uuid::new_v4(), Uuid::new_v4(), None).expect("legal scope");
    let create_context = context("creator", scope.active_scope(), "contract-command-1");
    let mut contract = LegalContract::new(
        Uuid::new_v4(),
        "contract-001",
        "Anonymized supplier agreement",
        scope.clone(),
        "opaque/contracts/001-v1",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        Utc::now() + Duration::days(30),
        &create_context,
    )
    .expect("contract");
    let document_context = context("creator", scope.active_scope(), "document-command-1");
    let mut document = LegalDocument::new(
        Uuid::new_v4(),
        scope,
        "Anonymized document",
        &document_context,
    )
    .expect("document");
    document
        .add_version(
            "opaque/documents/001-v1",
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            &document_context,
        )
        .expect("document version");

    let store = SqlxLegalPersistence::new(pool.clone(), &data_profile_key).expect("legal store");
    let mut unauthenticated_context = context(
        "unauthenticated-creator",
        contract.scope.active_scope(),
        "contract-command-without-session",
    );
    unauthenticated_context.actor.session_id = None;
    let unauthenticated_contract = LegalContract::new(
        Uuid::new_v4(),
        "contract-no-session",
        "Rejected without authenticated session",
        contract.scope.clone(),
        "opaque/contracts/no-session-v1",
        "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        Utc::now() + Duration::days(30),
        &unauthenticated_context,
    )
    .expect("unauthenticated contract fixture");
    let unauthorized = store
        .save_contract(&unauthenticated_contract, &unauthenticated_context, None)
        .await
        .expect_err("human Legal writes without session provenance must fail closed");
    assert!(
        unauthorized
            .to_string()
            .contains("authenticated session provenance")
    );

    let first = store
        .save_contract(&contract, &create_context, None)
        .await
        .expect("contract write");
    assert_eq!(first.version, 1);
    assert!(!first.replayed);
    let replay = store
        .save_contract(&contract, &create_context, None)
        .await
        .expect("contract replay");
    assert!(replay.replayed);
    assert_eq!(replay.event_id, first.event_id);
    let initial_contract = contract.clone();
    let party_context = context(
        "creator",
        contract.scope.active_scope(),
        "contract-command-2",
    );
    contract
        .add_party(
            ContractParty::new(Uuid::new_v4(), "supplier").expect("contract party"),
            &party_context,
        )
        .expect("draft party mutation");
    let second = store
        .save_contract(&contract, &party_context, Some(1))
        .await
        .expect("contract party write");
    assert_eq!(second.version, 2);
    let mut stale_context = context(
        "creator",
        initial_contract.scope.active_scope(),
        "contract-stale-command",
    );
    stale_context.command_id = initial_contract.last_command_id;
    let stale = store
        .save_contract(&initial_contract, &stale_context, Some(1))
        .await;
    assert!(stale.is_err(), "stale contract version must be rejected");

    let document_receipt = store
        .save_document(&document, &document_context, None)
        .await
        .expect("document write");
    assert_eq!(document_receipt.version, 1);
    let document_v2_context = context(
        "creator",
        document.scope.active_scope(),
        "document-command-2",
    );
    document
        .add_version(
            "opaque/documents/001-v2",
            "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            &document_v2_context,
        )
        .expect("second document version");
    let document_v2_receipt = store
        .save_document(&document, &document_v2_context, Some(1))
        .await
        .expect("second document write");
    assert_eq!(document_v2_receipt.version, 2);
    let access_context = context("creator", document.scope.active_scope(), "access-command-1");
    let access = erpx_domain::legal::LegalAccessHistoryEntry::new(
        document.id,
        1,
        "download",
        "contract review",
        &access_context,
        Utc::now(),
        document.scope.clone(),
    )
    .expect("access history");
    let access_receipt = store
        .record_access_history(&access, &access_context)
        .await
        .expect("access history write");
    assert_eq!(access_receipt.version, 1);
    let access_replay = store
        .record_access_history(&access, &access_context)
        .await
        .expect("access history replay");
    assert!(access_replay.replayed);
    assert_eq!(access_replay.event_id, access_receipt.event_id);
    let access_v2_context = context("creator", document.scope.active_scope(), "access-command-2");
    let access_v2 = erpx_domain::legal::LegalAccessHistoryEntry::new(
        document.id,
        2,
        "download",
        "contract review",
        &access_v2_context,
        Utc::now(),
        document.scope.clone(),
    )
    .expect("second access history");
    let access_v2_receipt = store
        .record_access_history(&access_v2, &access_v2_context)
        .await
        .expect("second access history write");
    assert_eq!(access_v2_receipt.version, 1);

    let mut tx = pool.begin().await.expect("inspection transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *tx)
        .await
        .expect("holding scope");
    let counts: (i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM erpx_holding.legal_contracts WHERE data_profile_key=$1),
            (SELECT count(*) FROM erpx_holding.legal_contract_parties WHERE data_profile_key=$1),
            (SELECT count(*) FROM erpx_holding.legal_documents WHERE data_profile_key=$1),
            (SELECT count(*) FROM erpx_holding.legal_document_versions WHERE data_profile_key=$1),
            (SELECT count(*) FROM erpx_holding.legal_access_history WHERE data_profile_key=$1),
            (SELECT count(*) FROM erpx_holding.outbox_events WHERE data_profile_key=$1 AND aggregate_type IN ('legal_contract','legal_document','legal_access_history'))",
    )
    .bind(&data_profile_key)
    .fetch_one(&mut *tx)
    .await
    .expect("normalized legal counts");
    assert_eq!(counts, (1, 1, 1, 2, 2, 6));
    let access_v2_aggregate_version: i64 = sqlx::query_scalar(
        "SELECT aggregate_version FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND event_id=$2",
    )
    .bind(&data_profile_key)
    .bind(access_v2_receipt.event_id)
    .fetch_one(&mut *tx)
    .await
    .expect("access history aggregate version");
    assert_eq!(access_v2_aggregate_version, 1);
    let contract_idempotency: String = sqlx::query_scalar(
        "SELECT idempotency_key FROM erpx_holding.legal_contracts
         WHERE data_profile_key=$1 AND contract_id=$2",
    )
    .bind(&data_profile_key)
    .bind(contract.id)
    .fetch_one(&mut *tx)
    .await
    .expect("contract idempotency");
    assert_eq!(contract_idempotency, party_context.idempotency_key);
    let envelope: (
        i32,
        i32,
        serde_json::Value,
        serde_json::Value,
        Uuid,
        Option<Uuid>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT contract_version, event_version, active_scope, actor,
                    correlation_id, causation_id, idempotency_key
             FROM erpx_holding.outbox_events
             WHERE data_profile_key=$1 AND event_id=$2",
    )
    .bind(&data_profile_key)
    .bind(first.event_id)
    .fetch_one(&mut *tx)
    .await
    .expect("shared event envelope");
    assert_eq!(envelope.0, 1);
    assert_eq!(envelope.1, 1);
    assert_eq!(
        envelope.2,
        serde_json::to_value(&create_context.active_scope).unwrap()
    );
    assert_eq!(
        envelope.3,
        serde_json::to_value(&create_context.actor).unwrap()
    );
    assert_eq!(envelope.4, create_context.correlation_id);
    assert_eq!(envelope.5, create_context.causation_id);
    assert_eq!(
        envelope.6.as_deref(),
        Some(create_context.idempotency_key.as_str())
    );
    tx.commit().await.expect("inspection commit");

    let rls_role = format!("erpx_legal_rls_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE ROLE \"{rls_role}\" NOLOGIN"))
        .execute(&pool)
        .await
        .expect("RLS test role");
    sqlx::query(&format!(
        "GRANT USAGE ON SCHEMA erpx_holding TO \"{rls_role}\""
    ))
    .execute(&pool)
    .await
    .expect("RLS schema grant");
    sqlx::query(&format!(
        "GRANT SELECT ON erpx_holding.legal_contracts TO \"{rls_role}\""
    ))
    .execute(&pool)
    .await
    .expect("RLS table grant");
    let mut wrong_scope_tx = pool.begin().await.expect("wrong scope transaction");
    sqlx::query(&format!("SET LOCAL ROLE \"{rls_role}\""))
        .execute(&mut *wrong_scope_tx)
        .await
        .expect("RLS role");
    sqlx::query("SELECT set_config('erpx.data_profile_key', 'other-holding', true)")
        .execute(&mut *wrong_scope_tx)
        .await
        .expect("wrong holding scope");
    let visible: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_holding.legal_contracts WHERE contract_id=$1",
    )
    .bind(contract.id)
    .fetch_one(&mut *wrong_scope_tx)
    .await
    .expect("RLS read");
    assert_eq!(visible, 0);
    wrong_scope_tx.commit().await.expect("wrong scope commit");
    sqlx::query(&format!("DROP OWNED BY \"{rls_role}\""))
        .execute(&pool)
        .await
        .expect("drop RLS test privileges");
    sqlx::query(&format!("DROP ROLE \"{rls_role}\""))
        .execute(&pool)
        .await
        .expect("drop RLS test role");

    let mut version_trigger_tx = pool.begin().await.expect("version trigger transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *version_trigger_tx)
        .await
        .expect("holding scope for trigger");
    let version_update = sqlx::query(
        "UPDATE erpx_holding.legal_document_versions
         SET object_ref='opaque/documents/001-mutated'
         WHERE data_profile_key=$1 AND document_id=$2 AND version=1",
    )
    .bind(&data_profile_key)
    .bind(document.id)
    .execute(&mut *version_trigger_tx)
    .await;
    assert!(version_update.is_err(), "document versions are immutable");
    version_trigger_tx
        .rollback()
        .await
        .expect("trigger rollback");
}

#[tokio::test]
#[ignore = "requires one disposable holding PostgreSQL write database"]
async fn live_legal_retention_sweep_expires_due_contract_once() {
    let url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("write pool");
    erpx_migration::migrate_holding_write(&pool)
        .await
        .expect("holding write migration");

    let data_profile_key = format!("holding-legal-retention-{}", Uuid::new_v4());
    let scope = LegalScope::new(Uuid::new_v4(), Uuid::new_v4(), None).expect("legal scope");
    let create_context = context("creator", scope.active_scope(), "retention-create");
    let mut contract = LegalContract::new(
        Uuid::new_v4(),
        "contract-retention-001",
        "Due retention contract",
        scope.clone(),
        "opaque/contracts/retention-001",
        "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        Utc::now() - Duration::hours(1),
        &create_context,
    )
    .expect("contract");
    let store = SqlxLegalPersistence::new(pool.clone(), &data_profile_key).expect("legal store");
    store
        .save_contract(&contract, &create_context, None)
        .await
        .expect("draft contract");

    let review_context = context("creator", scope.active_scope(), "retention-review");
    contract
        .submit_for_review(&review_context)
        .expect("submit for review");
    store
        .save_contract(&contract, &review_context, Some(1))
        .await
        .expect("review contract");

    let approve_context = context("approver", scope.active_scope(), "retention-approve");
    contract
        .approve(&approve_context)
        .expect("approve contract");
    store
        .save_contract(&contract, &approve_context, Some(2))
        .await
        .expect("approved contract");

    let activate_context = context("approver", scope.active_scope(), "retention-activate");
    contract
        .activate(&activate_context)
        .expect("activate contract");
    store
        .save_contract(&contract, &activate_context, Some(3))
        .await
        .expect("active contract");

    let sweep_correlation_id = Uuid::new_v4();
    let first = store
        .expire_due_contracts_with_correlation(Utc::now(), 10, sweep_correlation_id)
        .await
        .expect("retention sweep");
    assert_eq!(
        first,
        erpx_infrastructure::LegalRetentionSweepReport {
            scanned: 1,
            expired: 1,
            conflicts: 0,
        }
    );
    let second = store
        .expire_due_contracts(Utc::now(), 10)
        .await
        .expect("idempotent retention sweep");
    assert_eq!(
        second,
        erpx_infrastructure::LegalRetentionSweepReport {
            scanned: 0,
            expired: 0,
            conflicts: 0,
        }
    );

    let mut verify_tx = pool
        .begin()
        .await
        .expect("retention verification transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *verify_tx)
        .await
        .expect("retention verification holding scope");
    let (status, version, event_count): (String, i64, i64) = sqlx::query_as(
        "SELECT status, version,
                (SELECT count(*) FROM erpx_holding.outbox_events
                 WHERE data_profile_key=$1 AND aggregate_type='legal_contract'
                   AND aggregate_id=$2)
         FROM erpx_holding.legal_contracts
         WHERE data_profile_key=$1 AND contract_id=$2",
    )
    .bind(&data_profile_key)
    .bind(contract.id)
    .fetch_one(&mut *verify_tx)
    .await
    .expect("expired contract");
    assert_eq!(status, "expired");
    assert_eq!(version, 5);
    assert_eq!(event_count, 5);

    let expired_event: (
        i64,
        String,
        serde_json::Value,
        serde_json::Value,
        Option<String>,
        Uuid,
    ) = sqlx::query_as(
        "SELECT aggregate_version, event_type, active_scope, actor, idempotency_key,
                correlation_id
         FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND aggregate_type='legal_contract' AND aggregate_id=$2
         ORDER BY aggregate_version DESC
         LIMIT 1",
    )
    .bind(&data_profile_key)
    .bind(contract.id)
    .fetch_one(&mut *verify_tx)
    .await
    .expect("expired source evidence");
    assert_eq!(expired_event.0, 5);
    assert_eq!(expired_event.1, "legal.contract_saved.v1");
    assert_eq!(
        expired_event.2,
        serde_json::to_value(&activate_context.active_scope).unwrap()
    );
    assert_eq!(
        expired_event.3,
        serde_json::to_value(ActorAttribution {
            principal_ref: "system-worker:legal-retention".into(),
            kind: ActorKind::SystemWorker,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        })
        .unwrap()
    );
    assert!(
        expired_event
            .4
            .as_deref()
            .is_some_and(|key| key.starts_with("legal-retention:"))
    );
    assert_eq!(expired_event.5, sweep_correlation_id);
    verify_tx
        .commit()
        .await
        .expect("retention verification commit");
}

#[tokio::test]
#[ignore = "requires one disposable holding PostgreSQL write database"]
async fn live_legal_approval_fact_binding_failure_rolls_back_contract_and_outbox() {
    let url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("write pool");
    erpx_migration::migrate_holding_write(&pool)
        .await
        .expect("holding write migration");

    let data_profile_key = format!("holding-legal-approval-integrity-{}", Uuid::new_v4());
    let scope = LegalScope::new(Uuid::new_v4(), Uuid::new_v4(), None).expect("legal scope");
    let create_context = context("creator", scope.active_scope(), "approval-integrity-create");
    let mut contract = LegalContract::new(
        Uuid::new_v4(),
        "contract-approval-integrity-001",
        "Approval integrity contract",
        scope.clone(),
        "opaque/contracts/approval-integrity-001",
        "sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
        Utc::now() + Duration::days(30),
        &create_context,
    )
    .expect("contract");
    let store = SqlxLegalPersistence::new(pool.clone(), &data_profile_key).expect("legal store");
    store
        .save_contract(&contract, &create_context, None)
        .await
        .expect("draft contract");

    let review_context = context("creator", scope.active_scope(), "approval-integrity-review");
    contract
        .submit_for_review(&review_context)
        .expect("review transition");
    store
        .save_contract(&contract, &review_context, Some(1))
        .await
        .expect("review contract");

    let approve_context = context(
        "approver",
        scope.active_scope(),
        "approval-integrity-approve",
    );
    contract
        .approve(&approve_context)
        .expect("approval transition");
    store
        .save_contract(&contract, &approve_context, Some(2))
        .await
        .expect("approved contract");

    let (status_before, version_before, event_count_before, idempotency_count_before): (
        String,
        i64,
        i64,
        i64,
    ) = sqlx::query_as(
        "SELECT contracts.status, contracts.version,
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
    .fetch_one(&pool)
    .await
    .expect("approved aggregate snapshot");
    assert_eq!(status_before, "approved");
    assert_eq!(version_before, 3);
    assert_eq!(event_count_before, 3);
    assert_eq!(idempotency_count_before, 3);

    let wrong_holding_id = Uuid::new_v4();
    let mut corrupt_tx = pool.begin().await.expect("corruption transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *corrupt_tx)
        .await
        .expect("holding scope");
    sqlx::query(
        "ALTER TABLE erpx_holding.legal_contract_approvals
         DISABLE TRIGGER legal_contract_approvals_immutable",
    )
    .execute(&mut *corrupt_tx)
    .await
    .expect("disable approval trigger for corruption fixture");
    sqlx::query(
        "UPDATE erpx_holding.legal_contract_approvals
         SET holding_id=$1
         WHERE data_profile_key=$2 AND contract_id=$3",
    )
    .bind(wrong_holding_id)
    .bind(&data_profile_key)
    .bind(contract.id)
    .execute(&mut *corrupt_tx)
    .await
    .expect("corrupt approval binding");
    sqlx::query(
        "ALTER TABLE erpx_holding.legal_contract_approvals
         ENABLE TRIGGER legal_contract_approvals_immutable",
    )
    .execute(&mut *corrupt_tx)
    .await
    .expect("restore approval trigger");
    corrupt_tx
        .commit()
        .await
        .expect("corruption fixture commit");

    let activate_context = context(
        "approver",
        scope.active_scope(),
        "approval-integrity-activate",
    );
    contract
        .activate(&activate_context)
        .expect("activation transition");
    let error = store
        .save_contract(&contract, &activate_context, Some(3))
        .await
        .expect_err("inconsistent approval binding must fail closed");
    assert!(
        error
            .to_string()
            .contains("approval fact holding binding is inconsistent")
    );

    let (status_after, version_after, event_count_after, idempotency_count_after): (
        String,
        i64,
        i64,
        i64,
    ) = sqlx::query_as(
        "SELECT contracts.status, contracts.version,
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
    .fetch_one(&pool)
    .await
    .expect("rolled back aggregate snapshot");
    assert_eq!(status_after, status_before);
    assert_eq!(version_after, version_before);
    assert_eq!(event_count_after, event_count_before);
    assert_eq!(idempotency_count_after, idempotency_count_before);

    let (stored_holding_id, stored_contract_version): (Uuid, i64) = sqlx::query_as(
        "SELECT holding_id, contract_version
         FROM erpx_holding.legal_contract_approvals
         WHERE data_profile_key=$1 AND contract_id=$2",
    )
    .bind(&data_profile_key)
    .bind(contract.id)
    .fetch_one(&pool)
    .await
    .expect("approval fact");
    assert_eq!(stored_holding_id, wrong_holding_id);
    assert_eq!(stored_contract_version, 3);
}
