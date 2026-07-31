//! Focused proof for authoritative Finance journal handoffs at the subledger
//! write boundary.

use chrono::{NaiveDate, Utc};
use erpx_domain::contracts::{ActorAttribution, ActorKind, CommandContext, TenantScope};
use erpx_domain::finance::{
    Account, AccountKind, AccountingPeriod, ChartOfAccounts, JournalEntry, JournalLine,
    PostedJournalEntry,
};
use erpx_domain::finance_subledgers::{
    Allocation, AllocationBatch, FinanceJournalHandoff, Invoice, InvoiceKind, InvoiceLine,
    PartyKind, PartyReference, Payment, PaymentKind,
};
use erpx_infrastructure::{FinanceSubledgerAggregate, PersistenceError, SqlxFinanceSubledgerStore};
use sqlx::PgPool;
use uuid::Uuid;

fn context(scope: TenantScope, key: &str) -> CommandContext {
    CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: "finance-subledger-handoff-test".into(),
            kind: ActorKind::TenantUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope,
        "finance.subledger.handoff.test",
    )
    .expect("context")
}

fn period(holding_id: Uuid, company_id: Uuid) -> AccountingPeriod {
    AccountingPeriod::new(
        Uuid::new_v4(),
        holding_id,
        company_id,
        "2026-07",
        NaiveDate::from_ymd_opt(2026, 7, 1).expect("period start"),
        NaiveDate::from_ymd_opt(2026, 7, 31).expect("period end"),
    )
    .expect("period")
}

fn invoice(
    holding_id: Uuid,
    company_id: Uuid,
    branch_id: Uuid,
    period_id: Uuid,
    id: Uuid,
    key: &str,
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
    .expect("party");
    Invoice::new(
        InvoiceKind::Payable,
        id,
        scope.clone(),
        holding_id,
        company_id,
        period_id,
        NaiveDate::from_ymd_opt(2026, 7, 10).expect("accounting date"),
        NaiveDate::from_ymd_opt(2026, 7, 10).expect("issue date"),
        NaiveDate::from_ymd_opt(2026, 7, 31).expect("due date"),
        party,
        "THB",
        vec![InvoiceLine {
            line_id: Uuid::new_v4(),
            description: "authoritative handoff fixture".into(),
            quantity: 1,
            unit_price_minor: 100,
            tax_minor: 0,
        }],
        &context(scope, key),
    )
    .expect("invoice")
}

fn posted_journal(
    holding_id: Uuid,
    company_id: Uuid,
    period: &AccountingPeriod,
    key: &str,
) -> PostedJournalEntry {
    let debit_id = Uuid::new_v4();
    let credit_id = Uuid::new_v4();
    let chart = ChartOfAccounts::new(
        Uuid::new_v4(),
        holding_id,
        company_id,
        "Handoff fixture chart",
        "THB",
        vec![
            Account {
                id: debit_id,
                code: "1000".into(),
                name: "Cash".into(),
                kind: AccountKind::Asset,
                parent_id: None,
                active: true,
            },
            Account {
                id: credit_id,
                code: "4000".into(),
                name: "Expense".into(),
                kind: AccountKind::Expense,
                parent_id: None,
                active: true,
            },
        ],
    )
    .expect("chart");
    let entry = JournalEntry::new(
        Uuid::new_v4(),
        holding_id,
        company_id,
        period.id,
        NaiveDate::from_ymd_opt(2026, 7, 10).expect("journal date"),
        "Authoritative handoff fixture",
        vec![
            JournalLine::new(Uuid::new_v4(), debit_id, 100, 0, None).expect("debit"),
            JournalLine::new(Uuid::new_v4(), credit_id, 0, 100, None).expect("credit"),
        ],
        context(
            TenantScope::Company {
                holding_id,
                company_id,
            },
            key,
        ),
    )
    .expect("journal entry");
    entry
        .post(&chart, period, Utc::now())
        .expect("posted journal")
}

fn posted_invoice(
    holding_id: Uuid,
    company_id: Uuid,
    branch_id: Uuid,
    period: &AccountingPeriod,
    journal: &PostedJournalEntry,
    key: &str,
) -> Invoice {
    let mut value = invoice(
        holding_id,
        company_id,
        branch_id,
        period.id,
        Uuid::new_v4(),
        key,
    );
    let handoff = FinanceJournalHandoff::for_record(
        journal.id(),
        &value.metadata,
        Uuid::new_v4(),
        "payable_invoice_posted",
    )
    .expect("handoff");
    value.approve(0).expect("invoice approval");
    value.post(1, period, handoff).expect("invoice posting");
    value
}

fn settled_payment(
    holding_id: Uuid,
    company_id: Uuid,
    branch_id: Uuid,
    period: &AccountingPeriod,
    journal: &PostedJournalEntry,
    key: &str,
) -> Payment {
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
    .expect("payment party");
    let mut value = Payment::new(
        PaymentKind::VendorPayment,
        Uuid::new_v4(),
        scope.clone(),
        holding_id,
        company_id,
        period.id,
        NaiveDate::from_ymd_opt(2026, 7, 10).expect("payment date"),
        NaiveDate::from_ymd_opt(2026, 7, 10).expect("payment date"),
        party,
        "THB",
        100,
        &context(scope, key),
    )
    .expect("payment");
    value.status = erpx_domain::finance_subledgers::PaymentStatus::Settled;
    value.journal_handoff = Some(
        FinanceJournalHandoff::for_record(
            journal.id(),
            &value.metadata,
            Uuid::new_v4(),
            "vendor_payment_settled",
        )
        .expect("payment handoff"),
    );
    value
}

fn allocation(
    holding_id: Uuid,
    company_id: Uuid,
    branch_id: Uuid,
    period_id: Uuid,
    payment_id: Uuid,
    invoice_id: Uuid,
    key: &str,
) -> Allocation {
    let scope = TenantScope::Branch {
        holding_id,
        company_id,
        branch_id,
    };
    Allocation::new(
        Uuid::new_v4(),
        scope.clone(),
        holding_id,
        company_id,
        period_id,
        NaiveDate::from_ymd_opt(2026, 7, 10).expect("allocation date"),
        payment_id,
        invoice_id,
        100,
        &context(scope, key),
    )
    .expect("allocation")
}

async fn insert_posted_journal(
    pool: &PgPool,
    holding_ref: &str,
    posted: &PostedJournalEntry,
    row_holding_id: Uuid,
    row_company_id: Uuid,
    row_period_id: Uuid,
) {
    let payload = serde_json::to_value(posted).expect("posted payload");
    let total_minor: i64 = posted.lines().iter().map(|line| line.debit_minor).sum();
    let mut tx = pool.begin().await.expect("journal insert transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(holding_ref)
        .execute(&mut *tx)
        .await
        .expect("journal holding scope");
    sqlx::query(
        "INSERT INTO erpx_tenant.finance_posted_journals
            (journal_id, holding_id, company_id, period_id, holding_ref, version,
             idempotency_key, posted_at, currency, total_minor, payload)
         VALUES ($1,$2,$3,$4,$5,1,$6,$7,'THB',$8,$9)",
    )
    .bind(posted.id())
    .bind(row_holding_id)
    .bind(row_company_id)
    .bind(row_period_id)
    .bind(holding_ref)
    .bind(posted.idempotency_key())
    .bind(posted.posted_at())
    .bind(total_minor)
    .bind(payload)
    .execute(&mut *tx)
    .await
    .expect("posted journal insert");
    tx.commit().await.expect("journal insert commit");
}

async fn insert_subledger_aggregate(
    pool: &PgPool,
    holding_ref: &str,
    aggregate: &FinanceSubledgerAggregate,
    version: i64,
) {
    let payload = match aggregate {
        FinanceSubledgerAggregate::Invoice(value) => {
            serde_json::to_value(value).expect("invoice payload")
        }
        FinanceSubledgerAggregate::Payment(value) => {
            serde_json::to_value(value).expect("payment payload")
        }
        FinanceSubledgerAggregate::Allocation(value) => {
            serde_json::to_value(value).expect("allocation payload")
        }
        FinanceSubledgerAggregate::CashReceipt(value) => {
            serde_json::to_value(value).expect("cash receipt payload")
        }
        FinanceSubledgerAggregate::CashDisbursement(value) => {
            serde_json::to_value(value).expect("cash disbursement payload")
        }
        FinanceSubledgerAggregate::CashReconciliation(value) => {
            serde_json::to_value(value).expect("cash reconciliation payload")
        }
        FinanceSubledgerAggregate::SubledgerEntry(value) => {
            serde_json::to_value(value).expect("subledger entry payload")
        }
    };
    let branch_id = match aggregate.scope() {
        TenantScope::Branch { branch_id, .. } => Some(*branch_id),
        _ => None,
    };
    let mut tx = pool.begin().await.expect("subledger insert transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(holding_ref)
        .execute(&mut *tx)
        .await
        .expect("subledger holding scope");
    sqlx::query(
        "INSERT INTO erpx_tenant.finance_subledger_aggregates
            (aggregate_id, aggregate_type, holding_ref, company_id, branch_id,
             period_id, accounting_date, reference, version, idempotency_key, payload)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
    )
    .bind(aggregate.aggregate_id())
    .bind(aggregate.aggregate_type())
    .bind(holding_ref)
    .bind(aggregate.metadata().company_id)
    .bind(branch_id)
    .bind(aggregate.metadata().period_id)
    .bind(aggregate.metadata().accounting_date)
    .bind(&aggregate.metadata().reference)
    .bind(version)
    .bind(&aggregate.metadata().idempotency_key)
    .bind(payload)
    .execute(&mut *tx)
    .await
    .expect("subledger aggregate insert");
    tx.commit().await.expect("subledger insert commit");
}

async fn subledger_counts(
    pool: &PgPool,
    holding_ref: &str,
    aggregate_id: Uuid,
    request_key: &str,
) -> (i64, i64, i64, i64) {
    let mut tx = pool.begin().await.expect("count transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(holding_ref)
        .execute(&mut *tx)
        .await
        .expect("count holding scope");
    let counts = sqlx::query_as::<_, (i64, i64, i64, i64)>(
        "SELECT
            (SELECT count(*) FROM erpx_tenant.finance_subledger_aggregates
              WHERE holding_ref=$1 AND aggregate_id=$2),
            (SELECT count(*) FROM erpx_tenant.aggregate_snapshots
              WHERE holding_ref=$1 AND aggregate_id=$2),
            (SELECT count(*) FROM erpx_tenant.outbox_events
              WHERE holding_ref=$1 AND aggregate_id=$2),
            (SELECT count(*) FROM erpx_tenant.finance_subledger_command_idempotency
              WHERE holding_ref=$1 AND idempotency_key=$3)",
    )
    .bind(holding_ref)
    .bind(aggregate_id)
    .bind(request_key)
    .fetch_one(&mut *tx)
    .await
    .expect("subledger counts");
    tx.commit().await.expect("count commit");
    counts
}

#[test]
fn draft_invoice_is_accepted_without_a_journal_handoff() {
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let draft = invoice(
        holding_id,
        company_id,
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        "draft-handoff-unit",
    );
    FinanceSubledgerAggregate::Invoice(draft)
        .validate()
        .expect("draft without a handoff remains valid");
}

#[test]
fn posted_invoice_without_or_with_record_mismatched_handoff_is_rejected() {
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let accounting_period = period(holding_id, company_id);
    let mut value = invoice(
        holding_id,
        company_id,
        Uuid::new_v4(),
        accounting_period.id,
        Uuid::new_v4(),
        "missing-handoff-unit",
    );
    value.status = erpx_domain::finance_subledgers::InvoiceStatus::Posted;
    assert!(
        FinanceSubledgerAggregate::Invoice(value)
            .validate()
            .is_err()
    );

    let mut mismatched = invoice(
        holding_id,
        company_id,
        Uuid::new_v4(),
        accounting_period.id,
        Uuid::new_v4(),
        "mismatched-handoff-unit",
    );
    let other = invoice(
        holding_id,
        company_id,
        Uuid::new_v4(),
        accounting_period.id,
        Uuid::new_v4(),
        "other-handoff-unit",
    );
    mismatched.journal_handoff = Some(
        FinanceJournalHandoff::for_record(
            Uuid::new_v4(),
            &other.metadata,
            Uuid::new_v4(),
            "payable_invoice_posted",
        )
        .expect("mismatched handoff shape"),
    );
    assert!(
        FinanceSubledgerAggregate::Invoice(mismatched)
            .validate()
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires one disposable tenant PostgreSQL write database"]
async fn live_subledger_handoff_is_authoritative_atomic_and_idempotent() {
    let url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("write pool");
    erpx_migration::migrate_tenant_write(&pool)
        .await
        .expect("tenant-write migrations");

    let holding_ref = format!("holding-finance-handoff-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let branch_id = Uuid::new_v4();
    let primary_period = period(holding_id, company_id);
    let journal = posted_journal(
        holding_id,
        company_id,
        &primary_period,
        "handoff-journal-valid",
    );
    insert_posted_journal(
        &pool,
        &holding_ref,
        &journal,
        holding_id,
        company_id,
        primary_period.id,
    )
    .await;

    let store =
        SqlxFinanceSubledgerStore::new(pool.clone(), pool.clone(), &holding_ref).expect("store");
    let valid = posted_invoice(
        holding_id,
        company_id,
        branch_id,
        &primary_period,
        &journal,
        "handoff-subledger-valid",
    );
    let valid_id = valid.metadata.id;
    let valid_key = valid.metadata.idempotency_key.clone();
    let first = store
        .save(&FinanceSubledgerAggregate::Invoice(valid.clone()), None)
        .await
        .expect("valid authoritative handoff");
    assert!(!first.replayed);
    let replay = store
        .save(&FinanceSubledgerAggregate::Invoice(valid), None)
        .await
        .expect("exact replay");
    assert!(replay.replayed);
    assert_eq!(replay.version, first.version);
    assert_eq!(
        subledger_counts(&pool, &holding_ref, valid_id, &valid_key).await,
        (1, 1, 1, 1)
    );

    let missing = posted_invoice(
        holding_id,
        company_id,
        branch_id,
        &primary_period,
        &posted_journal(
            holding_id,
            company_id,
            &primary_period,
            "unused-missing-fixture",
        ),
        "handoff-subledger-missing",
    );
    let missing_journal_id = missing
        .journal_handoff
        .as_ref()
        .expect("missing handoff")
        .journal_id;
    let missing_id = missing.metadata.id;
    let missing_key = missing.metadata.idempotency_key.clone();
    let missing_result = store
        .save(&FinanceSubledgerAggregate::Invoice(missing), None)
        .await;
    assert!(matches!(
        missing_result,
        Err(PersistenceError::NotFound {
            entity: "finance posted journal",
            id
        }) if id == missing_journal_id
    ));
    assert_eq!(
        subledger_counts(&pool, &holding_ref, missing_id, &missing_key).await,
        (0, 0, 0, 0)
    );

    let other_company_id = Uuid::new_v4();
    let other_period = period(holding_id, other_company_id);
    let other_journal = posted_journal(
        holding_id,
        other_company_id,
        &other_period,
        "handoff-journal-mismatched",
    );
    insert_posted_journal(
        &pool,
        &holding_ref,
        &other_journal,
        holding_id,
        other_company_id,
        other_period.id,
    )
    .await;
    let mut mismatched = invoice(
        holding_id,
        company_id,
        branch_id,
        primary_period.id,
        Uuid::new_v4(),
        "handoff-subledger-mismatched",
    );
    mismatched.journal_handoff = Some(
        FinanceJournalHandoff::for_record(
            other_journal.id(),
            &mismatched.metadata,
            Uuid::new_v4(),
            "payable_invoice_posted",
        )
        .expect("mismatched authoritative handoff shape"),
    );
    mismatched.approve(0).expect("mismatched approval");
    mismatched.status = erpx_domain::finance_subledgers::InvoiceStatus::Posted;
    let mismatched_id = mismatched.metadata.id;
    let mismatched_key = mismatched.metadata.idempotency_key.clone();
    let mismatch_result = store
        .save(&FinanceSubledgerAggregate::Invoice(mismatched), None)
        .await;
    assert!(matches!(
        mismatch_result,
        Err(PersistenceError::InvalidData(message))
            if message == "Finance journal handoff does not match the authoritative posted journal"
    ));
    assert_eq!(
        subledger_counts(&pool, &holding_ref, mismatched_id, &mismatched_key).await,
        (0, 0, 0, 0)
    );

    let mut allocation_payment = settled_payment(
        holding_id,
        company_id,
        branch_id,
        &primary_period,
        &journal,
        "handoff-allocation-payment",
    );
    let mut allocation_invoice = invoice(
        holding_id,
        company_id,
        branch_id,
        primary_period.id,
        Uuid::new_v4(),
        "handoff-allocation-invoice",
    );
    allocation_payment.party = allocation_invoice.party.clone();
    let missing_allocation_journal = posted_journal(
        holding_id,
        company_id,
        &primary_period,
        "unused-allocation-missing-fixture",
    );
    allocation_invoice.journal_handoff = Some(
        FinanceJournalHandoff::for_record(
            missing_allocation_journal.id(),
            &allocation_invoice.metadata,
            Uuid::new_v4(),
            "payable_invoice_posted",
        )
        .expect("allocation invoice handoff"),
    );
    allocation_invoice.status = erpx_domain::finance_subledgers::InvoiceStatus::Posted;
    let payment_id = allocation_payment.metadata.id;
    let invoice_id = allocation_invoice.metadata.id;
    let allocation_payment_key = allocation_payment.metadata.idempotency_key.clone();
    let allocation_invoice_key = allocation_invoice.metadata.idempotency_key.clone();
    insert_subledger_aggregate(
        &pool,
        &holding_ref,
        &FinanceSubledgerAggregate::Payment(allocation_payment.clone()),
        1,
    )
    .await;
    insert_subledger_aggregate(
        &pool,
        &holding_ref,
        &FinanceSubledgerAggregate::Invoice(allocation_invoice.clone()),
        1,
    )
    .await;
    let allocation = allocation(
        holding_id,
        company_id,
        branch_id,
        primary_period.id,
        payment_id,
        invoice_id,
        "handoff-allocation-batch",
    );
    let allocation_id = allocation.metadata.id;
    let allocation_key = allocation.metadata.idempotency_key.clone();
    let batch = AllocationBatch::new(payment_id, vec![allocation]).expect("allocation batch");
    let allocation_result = store
        .save_allocation_batch(&batch, &allocation_payment, &[allocation_invoice], 1)
        .await;
    assert!(matches!(
        allocation_result,
        Err(PersistenceError::NotFound {
            entity: "finance posted journal",
            id
        }) if id == missing_allocation_journal.id()
    ));
    assert_eq!(
        subledger_counts(&pool, &holding_ref, payment_id, &allocation_payment_key).await,
        (1, 0, 0, 0)
    );
    assert_eq!(
        subledger_counts(&pool, &holding_ref, invoice_id, &allocation_invoice_key).await,
        (1, 0, 0, 0)
    );
    assert_eq!(
        subledger_counts(&pool, &holding_ref, allocation_id, &allocation_key).await,
        (0, 0, 0, 0)
    );
}
