use chrono::{NaiveDate, Utc};
use erpx_domain::contracts::{ActorAttribution, ActorKind, CommandContext, TenantScope};
use erpx_domain::finance::{
    Account, AccountKind, AccountingPeriod, ChartOfAccounts, JournalEntry, JournalLine,
};
use erpx_domain::finance_banking::{
    BankAccount, BankMatchProposal, BankMatchResolution, BankReconciliation, BankStatementImport,
    ImportedBankTransaction,
};
use erpx_infrastructure::{
    BankAccountWriteReceipt, BankMatchDecisionWriteReceipt, BankMatchProposalWriteReceipt,
    BankReconciliationWriteReceipt, PersistenceError, SqlxFinanceBankingStore, SqlxFinanceStore,
};
use sqlx::PgPool;
use uuid::Uuid;

fn context(scope: TenantScope, key: &str) -> CommandContext {
    CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: "finance-test".into(),
            kind: ActorKind::TenantUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope,
        "finance banking test",
    )
    .expect("context")
}

fn fixture() -> (BankAccount, BankStatementImport, BankReconciliation) {
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let account_id = Uuid::new_v4();
    let fixture_suffix = Uuid::new_v4().simple().to_string();
    let scope = TenantScope::Company {
        holding_id,
        company_id,
    };
    let account = BankAccount::new(
        account_id,
        scope.clone(),
        holding_id,
        company_id,
        "THB",
        format!("provider-account-{fixture_suffix}"),
        "Operating account",
    )
    .expect("account");
    let transaction = ImportedBankTransaction::new(
        Uuid::new_v4(),
        account_id,
        format!("provider-tx-{fixture_suffix}"),
        NaiveDate::from_ymd_opt(2026, 7, 1).expect("date"),
        1000,
        "THB",
        "Customer receipt",
    )
    .expect("transaction");
    let import = BankStatementImport::new(
        Uuid::new_v4(),
        account_id,
        scope.clone(),
        holding_id,
        company_id,
        format!("statement-2026-07-{fixture_suffix}"),
        NaiveDate::from_ymd_opt(2026, 7, 1).expect("date"),
        NaiveDate::from_ymd_opt(2026, 7, 31).expect("date"),
        0,
        1000,
        vec![transaction],
        context(scope.clone(), &format!("import-{fixture_suffix}")),
    )
    .expect("import");
    let reconciliation = BankReconciliation::new(
        Uuid::new_v4(),
        scope,
        holding_id,
        company_id,
        account_id,
        import.id(),
        1000,
        1000,
        context(
            TenantScope::Company {
                holding_id,
                company_id,
            },
            &format!("reconcile-{fixture_suffix}"),
        ),
    )
    .expect("reconciliation");
    (account, import, reconciliation)
}

#[test]
fn fixture_domain_covers_matching_and_reconciliation_rules() {
    let (account, import, reconciliation) = fixture();
    reconciliation
        .validate_against(&account, &import)
        .expect("binding");
    assert_eq!(import.transactions().len(), 1);
    assert!(
        import.transactions()[0]
            .identity_key()
            .starts_with("fbt_v1_")
    );
}

#[tokio::test]
async fn store_constructor_is_lazy_and_receipts_are_typed() {
    let store = SqlxFinanceBankingStore::new(
        PgPool::connect_lazy("postgres://unused").expect("lazy"),
        PgPool::connect_lazy("postgres://unused").expect("lazy"),
        "holding-test",
    )
    .expect("store");
    assert_eq!(store.holding_ref(), "holding-test");
    let _account_receipt: Option<BankAccountWriteReceipt> = None;
    let _reconciliation_receipt: Option<BankReconciliationWriteReceipt> = None;
    let _proposal_receipt: Option<BankMatchProposalWriteReceipt> = None;
    let _decision_receipt: Option<BankMatchDecisionWriteReceipt> = None;
}

#[tokio::test]
#[ignore = "requires two disposable tenant PostgreSQL databases; URLs are never printed"]
async fn live_bank_write_boundary_is_idempotent_scoped_and_reconcilable() {
    let write_url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required");
    let read_url = std::env::var("ERPX_TEST_TENANT_READ_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_READ_DATABASE_URL is required");
    let write_pool = PgPool::connect(&write_url).await.expect("write pool");
    let read_pool = PgPool::connect(&read_url).await.expect("read pool");
    erpx_migration::migrate_tenant_write(&write_pool)
        .await
        .expect("write migration");
    erpx_migration::migrate_tenant_read(&read_pool)
        .await
        .expect("read migration");
    let (account, import, reconciliation) = fixture();
    let holding_ref = format!("holding-test-{}", Uuid::new_v4().simple());
    let store =
        SqlxFinanceBankingStore::new(write_pool.clone(), read_pool.clone(), holding_ref.clone())
            .expect("store");
    let finance_store = SqlxFinanceStore::new(write_pool.clone(), read_pool, holding_ref.clone())
        .expect("finance store");
    let registered = store
        .register_account(&account, "register-1")
        .await
        .expect("register");
    assert_eq!(registered.version, 1);
    let replay = store
        .register_account(&account, "register-1")
        .await
        .expect("register replay");
    assert!(replay.replayed);
    let imported = store
        .import_statement(&account, &import)
        .await
        .expect("import");
    assert_eq!(imported.transaction_count, 1);
    assert_eq!(
        store
            .reconcile_import(import.id())
            .await
            .expect("reconcile committed import"),
        (1, 1),
        "reconciliation must observe the persisted transaction and its single outbox event"
    );
    let missing_import = Uuid::new_v4();
    assert!(matches!(
        store.reconcile_import(missing_import).await,
        Err(PersistenceError::NotFound { entity, id })
            if entity == "bank_statement_import" && id == missing_import
    ));
    let debit_account = Account {
        id: Uuid::new_v4(),
        code: "1100".into(),
        name: "Bank".into(),
        kind: AccountKind::Asset,
        parent_id: None,
        active: true,
    };
    let credit_account = Account {
        id: Uuid::new_v4(),
        code: "4000".into(),
        name: "Revenue".into(),
        kind: AccountKind::Revenue,
        parent_id: None,
        active: true,
    };
    let chart = ChartOfAccounts::new(
        Uuid::new_v4(),
        account.holding_id(),
        account.company_id(),
        "Test chart",
        "THB",
        vec![debit_account.clone(), credit_account.clone()],
    )
    .expect("chart");
    let period = AccountingPeriod::new(
        Uuid::new_v4(),
        account.holding_id(),
        account.company_id(),
        "2026-07",
        NaiveDate::from_ymd_opt(2026, 7, 1).expect("date"),
        NaiveDate::from_ymd_opt(2026, 7, 31).expect("date"),
    )
    .expect("period");
    finance_store
        .save_chart(&chart, None)
        .await
        .expect("save chart");
    finance_store
        .save_period(&period, None)
        .await
        .expect("save period");
    let journal = JournalEntry::new(
        Uuid::new_v4(),
        account.holding_id(),
        account.company_id(),
        period.id,
        import.transactions()[0].posted_on(),
        "Bank receipt",
        vec![
            JournalLine::new(
                Uuid::new_v4(),
                debit_account.id,
                1000,
                0,
                Some("Bank receipt".into()),
            )
            .expect("debit line"),
            JournalLine::new(
                Uuid::new_v4(),
                credit_account.id,
                0,
                1000,
                Some("Bank receipt".into()),
            )
            .expect("credit line"),
        ],
        context(
            TenantScope::Company {
                holding_id: account.holding_id(),
                company_id: account.company_id(),
            },
            "journal-post-context",
        ),
    )
    .expect("journal");
    let posted = finance_store
        .post_journal(journal, &chart, &period, Utc::now())
        .await
        .expect("post journal")
        .journal;
    let stored_match_facts: (Option<String>, Option<i64>) = sqlx::query_as(
        "SELECT currency, total_minor
         FROM erpx_tenant.finance_posted_journals
         WHERE journal_id=$1 AND holding_ref=$2",
    )
    .bind(posted.id())
    .bind(&holding_ref)
    .fetch_one(&write_pool)
    .await
    .expect("stored journal match facts");
    assert_eq!(stored_match_facts, (Some("THB".into()), Some(1000)));
    let proposal_context = context(
        TenantScope::Company {
            holding_id: account.holding_id(),
            company_id: account.company_id(),
        },
        "match-proposal-context",
    );
    let store_for_concurrent_replay = store.clone();
    let (generated_first, generated_second) = tokio::join!(
        store.generate_match_proposal(import.transactions()[0].id(), proposal_context.clone()),
        store_for_concurrent_replay
            .generate_match_proposal(import.transactions()[0].id(), proposal_context),
    );
    let generated_first = generated_first.expect("first automatic match proposal");
    let generated_second = generated_second.expect("second automatic match proposal");
    assert_eq!(
        generated_first.proposal.id(),
        generated_second.proposal.id()
    );
    assert_eq!(generated_first.proposal.candidates().len(), 1);
    assert!(generated_first.proposal.candidates()[0].journal_id() == posted.id());
    assert!(matches!(
        generated_first.proposal.resolution(),
        BankMatchResolution::Matched { journal_id } if *journal_id == posted.id()
    ));
    assert_ne!(
        generated_first.receipt.replayed,
        generated_second.receipt.replayed
    );
    let proposal = generated_first.proposal;
    assert!(
        store
            .generate_match_proposal(
                import.transactions()[0].id(),
                context(
                    TenantScope::Company {
                        holding_id: account.holding_id(),
                        company_id: account.company_id(),
                    },
                    "match-proposal-context",
                ),
            )
            .await
            .expect("match proposal replay")
            .receipt
            .replayed
    );
    let conflicting_proposal = BankMatchProposal::new(
        Uuid::new_v4(),
        &account,
        &import.transactions()[0],
        proposal.candidates().to_vec(),
        context(
            TenantScope::Company {
                holding_id: account.holding_id(),
                company_id: account.company_id(),
            },
            "match-proposal-context",
        ),
    )
    .expect("conflicting proposal fixture");
    assert!(matches!(
        store.save_match_proposal(&conflicting_proposal).await,
        Err(PersistenceError::IdempotencyConflict { key })
            if key == proposal.idempotency_key()
    ));
    let proposal_events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM erpx_tenant.outbox_events
         WHERE holding_ref=$1 AND aggregate_type=$2 AND aggregate_id=$3
           AND event_type=$4",
    )
    .bind(&holding_ref)
    .bind("finance_bank_match_proposal")
    .bind(proposal.id())
    .bind("finance.bank_match_proposal_saved.v1")
    .fetch_one(&write_pool)
    .await
    .expect("proposal outbox event");
    assert_eq!(
        proposal_events, 1,
        "proposal replay must not duplicate outbox"
    );
    let decision = proposal
        .decide(
            Uuid::new_v4(),
            BankMatchResolution::Matched {
                journal_id: posted.id(),
            },
            context(
                TenantScope::Company {
                    holding_id: account.holding_id(),
                    company_id: account.company_id(),
                },
                "match-decision-context",
            ),
        )
        .expect("matched decision");
    let decision_receipt = store
        .save_match_decision(&decision)
        .await
        .expect("match decision");
    assert_eq!(decision_receipt.proposal_id, proposal.id());
    let decision_replay = store
        .save_match_decision(&decision)
        .await
        .expect("match decision replay");
    assert!(decision_replay.replayed);
    let decision_events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM erpx_tenant.outbox_events
         WHERE holding_ref=$1 AND aggregate_type=$2 AND aggregate_id=$3
           AND event_type=$4",
    )
    .bind(&holding_ref)
    .bind("finance_bank_match_decision")
    .bind(decision.id())
    .bind("finance.bank_match_decision_saved.v1")
    .fetch_one(&write_pool)
    .await
    .expect("decision outbox event");
    assert_eq!(
        decision_events, 1,
        "decision replay must not duplicate outbox"
    );
    assert!(
        sqlx::query(
            "UPDATE erpx_tenant.finance_posted_journals
             SET total_minor = total_minor + 1
             WHERE journal_id=$1 AND holding_ref=$2",
        )
        .bind(posted.id())
        .bind(&holding_ref)
        .execute(&write_pool)
        .await
        .is_err(),
        "posted journal mutation must be rejected by the database trigger"
    );
    let imported_replay = store
        .import_statement(&account, &import)
        .await
        .expect("import replay");
    assert!(imported_replay.replayed);
    assert!(
        store
            .find_transaction(import.transactions()[0].identity_key())
            .await
            .expect("transaction lookup")
            .is_some()
    );
    let created = store
        .save_reconciliation(&account, &import, &reconciliation, None)
        .await
        .expect("reconciliation create");
    assert_eq!(created.version, 1);
}

#[tokio::test]
#[ignore = "requires two disposable tenant PostgreSQL databases; URLs are never printed"]
async fn live_bank_read_projection_is_idempotent_and_rebuildable() {
    let write_url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required");
    let read_url = std::env::var("ERPX_TEST_TENANT_READ_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_READ_DATABASE_URL is required");
    let write_pool = PgPool::connect(&write_url).await.expect("write pool");
    let read_pool = PgPool::connect(&read_url).await.expect("read pool");
    erpx_migration::migrate_tenant_write(&write_pool)
        .await
        .expect("write migration");
    erpx_migration::migrate_tenant_read(&read_pool)
        .await
        .expect("read migration");
    let (account, import, reconciliation) = fixture();
    let holding_ref = format!("holding-test-{}", Uuid::new_v4().simple());
    let store = SqlxFinanceBankingStore::new(write_pool, read_pool, holding_ref).expect("store");
    store
        .register_account(&account, "register-projection")
        .await
        .expect("register");
    store
        .import_statement(&account, &import)
        .await
        .expect("import");
    store
        .save_reconciliation(&account, &import, &reconciliation, None)
        .await
        .expect("reconciliation");

    let mut started_reconciliation = BankReconciliation::new(
        reconciliation.id(),
        reconciliation.scope().clone(),
        reconciliation.holding_id(),
        reconciliation.company_id(),
        reconciliation.bank_account_id(),
        reconciliation.statement_import_id(),
        reconciliation.statement_balance_minor(),
        reconciliation.book_balance_minor(),
        context(reconciliation.scope().clone(), "reconcile-transition"),
    )
    .expect("transition reconciliation fixture");
    started_reconciliation
        .start(0)
        .expect("start reconciliation");
    let started_receipt = store
        .save_reconciliation(
            &account,
            &import,
            &started_reconciliation,
            Some(reconciliation.version()),
        )
        .await
        .expect("persist transitioned reconciliation");
    assert_eq!(started_receipt.version, 2);

    let page = store
        .read_source_event_page(None, 10)
        .await
        .expect("source page");
    assert_eq!(page.events.len(), 4);
    assert!(!page.has_more);
    let reconciliation_versions = page
        .events
        .iter()
        .filter(|event| event.aggregate_id == reconciliation.id())
        .map(|event| event.aggregate_version)
        .collect::<Vec<_>>();
    assert_eq!(reconciliation_versions, vec![1, 2]);
    for event in &page.events {
        let applied = store
            .project_event(
                event.event_id,
                &event.event_type,
                &event.aggregate_type,
                event.aggregate_id,
                event.aggregate_version,
                &event.payload,
            )
            .await
            .expect("project event");
        assert!(applied.applied);
        let replay = store
            .replay_event(
                event.event_id,
                &event.event_type,
                &event.aggregate_type,
                event.aggregate_id,
                event.aggregate_version,
                &event.payload,
            )
            .await
            .expect("replay event");
        assert!(!replay.applied);
    }
    assert!(
        store
            .get_read_row(
                "finance_bank_reconciliation",
                reconciliation.id(),
                account.scope()
            )
            .await
            .expect("reconciled read row")
            .is_some()
    );

    let mismatch = store
        .rebuild_projection_with_source_fence(Some(5), 1)
        .await
        .expect_err("mismatched source fence must fail closed");
    assert!(matches!(
        mismatch,
        PersistenceError::InvalidData(message) if message.contains("source fence")
    ));
    assert!(
        store
            .get_read_row("finance_bank_account", account.id(), account.scope())
            .await
            .expect("preserved read row")
            .is_some()
    );
    let (preparation, events_replayed) = store
        .rebuild_projection_with_source_fence(Some(4), 1)
        .await
        .expect("atomic fenced rebuild");
    assert_eq!(events_replayed, 4);
    assert_eq!(preparation.rows_cleared, 3);
    assert_eq!(preparation.checkpoints_cleared, 3);
    assert!(
        store
            .get_read_row("finance_bank_account", account.id(), account.scope())
            .await
            .expect("rebuilt read row")
            .is_some()
    );

    let first_page = store
        .list_read_rows(account.scope(), None, None, None, 2)
        .await
        .expect("bounded read page");
    assert_eq!(first_page.rows.len(), 2);
    assert!(first_page.has_more);
    let second_page = store
        .list_read_rows(
            account.scope(),
            None,
            None,
            first_page.next_cursor.as_ref(),
            2,
        )
        .await
        .expect("exclusive continuation page");
    assert_eq!(second_page.rows.len(), 1);
    assert!(!second_page.has_more);
    assert!(
        store
            .list_read_rows(
                &TenantScope::Company {
                    holding_id: Uuid::new_v4(),
                    company_id: Uuid::new_v4(),
                },
                None,
                None,
                None,
                2,
            )
            .await
            .expect("isolated company page")
            .rows
            .is_empty()
    );
}
