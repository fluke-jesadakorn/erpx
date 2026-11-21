//! Actual native Finance opening-balance preview/import over leased PostgreSQL,
//! current Account/IAM authority and the caller's command receipts.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod facts;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_finance::public::finance_opening::{
    OpeningImportInput, OpeningInput, OpeningLine, OpeningSubledgerItem,
};
use erpx_server_finance::public::generic_payment::{GenericPaymentInput, prepare_generic_payment};
use erpx_server_finance::public::service_receipt::{
    ServiceReceiptAllocationInput, ServiceReceiptConfirmInput,
    flush_service_receipt_confirm_effects, prepare_service_receipt_confirm,
    read_service_receipt_review,
};
use erpx_server_finance::public::{
    FinanceActor, FinanceCollation, JournalAuthority, JournalError, JournalStatus,
    PgJournalPreparation, embedded_currency_rules, prepare_journal,
};
use erpx_server_identity_access::public::work_context::HoldingWorkspaceRequest;
use erpx_server_runtime::public::postgres::{DatabasePool, PgTransactionError};
use erpx_server_shared::public::{AssignmentId, Clock, CommandKey, CompanyId, HoldingId, UserId};
use erpx_service::finance_journal_detail::*;
use erpx_service::finance_journal_detail_http::*;
use erpx_service::finance_journal_review::FinanceJournalEnvironment;
use facts::{Prepared, account, business, command, post, reverse, setup};
use std::sync::Arc;
use tower::ServiceExt;

fn service(p: &Prepared) -> FinanceJournalDetailService<facts::TestClock> {
    FinanceJournalDetailService::new(
        p.authorizer.clone(),
        FinanceJournalReadConfiguration::new(
            FinanceJournalEnvironment::Development,
            HoldingId::new("h001").unwrap(),
            CompanyId::new("company").unwrap(),
            Arc::new(current_package_catalogue().unwrap()),
        )
        .unwrap(),
        p.system.scenario.clock.clone(),
    )
}

fn owned_maker(
    p: &Prepared,
) -> impl FnOnce() -> Result<HoldingWorkspaceRequest, erpx_server_shared::public::BusinessError>
       + Send
       + 'static {
    let request = p.maker_selection.clone();
    move || Ok(request)
}

async fn seed(pool: &DatabasePool) {
    sqlx::raw_sql(
        "INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,status,version,created_at,updated_at)VALUES\
         ('opening-ar','company','chart','1100','Opening receivables','asset','THB',1,'receivable','active',1,0,0),\
         ('opening-equity','company','chart','3000','Opening equity','equity','THB',0,'none','active',1,0,0),\
         ('opening-bank','company','chart','1200','Opening settlement bank','asset','THB',1,'bank','active',1,0,0) ON CONFLICT DO NOTHING;\
         UPDATE finance_accounts SET currency='THB',status='active' WHERE company_id='company' AND id IN ('cash','sales');\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at)VALUES\
         ('opening-customer','company','customer','OPN-C','Opening customer','THB','active',1,0,0),\
         ('opening-customer-b','company','customer','OPN-C2','Opening customer B','THB','active',1,0,0),\
         ('opening-supplier','company','supplier','OPN-S','Opening supplier','THB','active',1,0,0) ON CONFLICT DO NOTHING;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn import_input(input: &OpeningInput, token: &str) -> OpeningImportInput {
    OpeningImportInput {
        chart_id: input.chart_id.clone(),
        period_id: input.period_id.clone(),
        entry_date: input.entry_date.clone(),
        currency: input.currency.clone(),
        lines: input.lines.clone(),
        subledger_items: input.subledger_items.clone(),
        preview_token: token.into(),
    }
}

fn subledger_input() -> OpeningInput {
    OpeningInput {
        chart_id: "chart".into(),
        period_id: "period".into(),
        entry_date: "2026-01-01".into(),
        currency: "THB".into(),
        lines: vec![
            OpeningLine {
                account_id: "opening-ar".into(),
                description: "Opening receivables".into(),
                debit: "600.00".into(),
                credit: "0.00".into(),
                subledger_kind: "receivable".into(),
            },
            OpeningLine {
                account_id: "cash".into(),
                description: "Opening equity".into(),
                debit: "0.00".into(),
                credit: "600.00".into(),
                subledger_kind: "none".into(),
            },
        ],
        subledger_items: vec![
            OpeningSubledgerItem {
                kind: "receivable".into(),
                party_id: "opening-customer".into(),
                reference: "OPEN-AR-001".into(),
                due_date: "2026-01-15".into(),
                control_account_id: "opening-ar".into(),
                offset_account_id: "cash".into(),
                amount: "250.00".into(),
            },
            OpeningSubledgerItem {
                kind: "receivable".into(),
                party_id: "opening-customer-b".into(),
                reference: "OPEN-AR-002".into(),
                due_date: "2026-02-15".into(),
                control_account_id: "opening-ar".into(),
                offset_account_id: "cash".into(),
                amount: "350.00".into(),
            },
        ],
    }
}

fn settlement_input() -> OpeningInput {
    OpeningInput {
        chart_id: "chart".into(),
        period_id: "period".into(),
        entry_date: "2026-01-01".into(),
        currency: "THB".into(),
        lines: vec![
            OpeningLine {
                account_id: "opening-ar".into(),
                description: "Opening receivable".into(),
                debit: "250.00".into(),
                credit: "0.00".into(),
                subledger_kind: "receivable".into(),
            },
            OpeningLine {
                account_id: "opening-equity".into(),
                description: "Opening equity".into(),
                debit: "0.00".into(),
                credit: "250.00".into(),
                subledger_kind: "none".into(),
            },
        ],
        subledger_items: vec![OpeningSubledgerItem {
            kind: "receivable".into(),
            party_id: "opening-customer".into(),
            reference: "OPEN-SETTLE-001".into(),
            due_date: "2026-01-15".into(),
            control_account_id: "opening-ar".into(),
            offset_account_id: "opening-equity".into(),
            amount: "250.00".into(),
        }],
    }
}

fn finance_actor(user: &str) -> FinanceActor {
    FinanceActor {
        user_id: UserId::new(user).unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        assignment_id: AssignmentId::new("assignment-a").unwrap(),
    }
}

struct AllowJournal;

impl JournalAuthority for AllowJournal {
    type Error = PgTransactionError<JournalError>;

    async fn authorize(
        &mut self,
        _: &FinanceActor,
        _: &erpx_server_finance::public::JournalDraft,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn opening_input() -> OpeningInput {
    OpeningInput {
        chart_id: "chart".into(),
        period_id: "period".into(),
        entry_date: "2026-01-01".into(),
        currency: "THB".into(),
        lines: vec![
            OpeningLine {
                account_id: "opening-ar".into(),
                description: "Opening receivables".into(),
                debit: "1000.00".into(),
                credit: "0.00".into(),
                subledger_kind: "receivable".into(),
            },
            OpeningLine {
                account_id: "cash".into(),
                description: "Opening cash".into(),
                debit: "0.00".into(),
                credit: "1000.00".into(),
                subledger_kind: "none".into(),
            },
        ],
        subledger_items: vec![OpeningSubledgerItem {
            kind: "receivable".into(),
            party_id: "opening-customer".into(),
            reference: "OPEN-AR-1".into(),
            due_date: "2026-01-31".into(),
            control_account_id: "opening-ar".into(),
            offset_account_id: "cash".into(),
            amount: "1000.00".into(),
        }],
    }
}

#[tokio::test]
async fn opening_preview_and_import_write_journal_invoice_and_receipt() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let preview = service(&p)
            .opening_balance_preview(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                opening_input(),
            )
            .await
            .unwrap();
        assert_eq!(preview.data.debit, "1000.00");
        assert_eq!(preview.data.credit, "1000.00");
        assert_eq!(preview.data.subledger_count, 1);
        assert_eq!(preview.data.preview_token.len(), 64);
        assert_eq!(preview.data.lines[0].account_code, "1100");
        assert_eq!(preview.data.lines[0].account_name, "Opening receivables");

        let key = "opening-import-0001";
        let imported = service(&p)
            .import_opening_balance(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new(key).unwrap(),
                OpeningImportInput {
                    chart_id: "chart".into(),
                    period_id: "period".into(),
                    entry_date: "2026-01-01".into(),
                    currency: "THB".into(),
                    lines: opening_input().lines,
                    subledger_items: opening_input().subledger_items,
                    preview_token: preview.data.preview_token.clone(),
                },
            )
            .await
            .unwrap();
        assert_eq!(imported.data.id, format!("journal-{key}"));
        assert_eq!(imported.data.journal_id, format!("journal-{key}"));
        assert_eq!(imported.data.debit, "1000.00");
        assert_eq!(imported.data.status, "draft");
        assert_eq!(imported.data.subledger_count, 1);

        use sqlx::Row;
        let journal = sqlx::query(
            "SELECT chart_id,period_id,entry_date,currency,source_type,source_id,memo,is_opening,status,created_by FROM finance_journals WHERE id=$1 AND company_id='company'",
        )
        .bind(&imported.data.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(journal.try_get::<String, _>("chart_id").unwrap(), "chart");
        assert_eq!(journal.try_get::<String, _>("period_id").unwrap(), "period");
        assert_eq!(journal.try_get::<String, _>("entry_date").unwrap(), "2026-01-01");
        assert_eq!(journal.try_get::<String, _>("currency").unwrap(), "THB");
        assert_eq!(journal.try_get::<String, _>("source_type").unwrap(), "opening_import");
        assert_eq!(journal.try_get::<String, _>("source_id").unwrap(), "chart");
        assert_eq!(
            journal.try_get::<String, _>("memo").unwrap(),
            "Verified opening trial balance and subledger schedules"
        );
        assert_eq!(journal.try_get::<i64, _>("is_opening").unwrap(), 1);
        assert_eq!(journal.try_get::<String, _>("status").unwrap(), "draft");
        assert_eq!(journal.try_get::<String, _>("created_by").unwrap(), "fixture-a");

        let lines = sqlx::query(
            "SELECT account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor FROM finance_journal_lines WHERE journal_id=$1 ORDER BY ordinal",
        )
        .bind(&imported.data.id)
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].try_get::<String, _>("account_id").unwrap(), "opening-ar");
        assert_eq!(lines[0].try_get::<i64, _>("debit_minor").unwrap(), 100_000);
        assert_eq!(lines[0].try_get::<i64, _>("base_debit_minor").unwrap(), 100_000);
        assert_eq!(lines[1].try_get::<String, _>("account_id").unwrap(), "cash");
        assert_eq!(lines[1].try_get::<i64, _>("credit_minor").unwrap(), 100_000);

        let invoice = sqlx::query(
            "SELECT party_id,kind,invoice_number,invoice_date,due_date,currency,total_minor,outstanding_minor,source_type,source_id,status,journal_id,debit_account_id,credit_account_id,created_by FROM finance_invoices WHERE id=$1",
        )
        .bind(format!("{}-subledger-0", imported.data.id))
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(invoice.try_get::<String, _>("party_id").unwrap(), "opening-customer");
        assert_eq!(invoice.try_get::<String, _>("kind").unwrap(), "receivable");
        assert_eq!(invoice.try_get::<String, _>("invoice_number").unwrap(), "OPEN-AR-1");
        assert_eq!(invoice.try_get::<String, _>("invoice_date").unwrap(), "2026-01-01");
        assert_eq!(invoice.try_get::<String, _>("due_date").unwrap(), "2026-01-31");
        assert_eq!(invoice.try_get::<i64, _>("total_minor").unwrap(), 100_000);
        assert_eq!(invoice.try_get::<i64, _>("outstanding_minor").unwrap(), 100_000);
        assert_eq!(invoice.try_get::<String, _>("source_type").unwrap(), "opening_import");
        assert_eq!(invoice.try_get::<String, _>("source_id").unwrap(), imported.data.id);
        assert_eq!(invoice.try_get::<String, _>("status").unwrap(), "draft");
        assert_eq!(invoice.try_get::<String, _>("journal_id").unwrap(), imported.data.id);
        assert_eq!(invoice.try_get::<String, _>("debit_account_id").unwrap(), "opening-ar");
        assert_eq!(invoice.try_get::<String, _>("credit_account_id").unwrap(), "cash");
        assert_eq!(invoice.try_get::<String, _>("created_by").unwrap(), "fixture-a");

        let receipt = sqlx::query(
            "SELECT operation,record_id FROM command_receipts WHERE command_id=$1",
        )
        .bind(key)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipt.try_get::<String, _>("operation").unwrap(), "finance.journal.create");
        assert_eq!(receipt.try_get::<String, _>("record_id").unwrap(), imported.data.id);
        let audit: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM audit_events WHERE command_id=$1 AND action='finance.journal.create'",
        )
        .bind(key)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(audit, 1);
        let outbox: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM outbox_events WHERE command_id=$1 AND event_type='finance.journal.create'",
        )
        .bind(key)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(outbox, 1);

        // Exact replay returns the retained result without a second journal or invoice.
        let replay = service(&p)
            .import_opening_balance(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new(key).unwrap(),
                OpeningImportInput {
                    chart_id: "chart".into(),
                    period_id: "period".into(),
                    entry_date: "2026-01-01".into(),
                    currency: "THB".into(),
                    lines: opening_input().lines,
                    subledger_items: opening_input().subledger_items,
                    preview_token: preview.data.preview_token.clone(),
                },
            )
            .await
            .unwrap();
        assert_eq!(replay.data.id, imported.data.id);
        let journals: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM finance_journals WHERE company_id='company' AND is_opening=1",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(journals, 1);
        let invoices: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM finance_invoices WHERE company_id='company' AND source_type='opening_import'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(invoices, 1);

        // A second opening balance is refused while the first exists.
        match service(&p)
            .opening_balance_preview(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                opening_input(),
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "OPENING_BALANCE_EXISTS");
            }
            other => panic!("existing opening must conflict: {other:?}"),
        }
    })
    .await;
}

#[tokio::test]
async fn opening_preview_rejects_invalid_balances_and_schedules() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;

        let unbalanced = OpeningInput {
            chart_id: "chart".into(),
            period_id: "period".into(),
            entry_date: "2026-01-01".into(),
            currency: "THB".into(),
            lines: vec![
                OpeningLine {
                    account_id: "cash".into(),
                    description: "Opening cash".into(),
                    debit: "100.00".into(),
                    credit: "0.00".into(),
                    subledger_kind: "none".into(),
                },
                OpeningLine {
                    account_id: "sales".into(),
                    description: "Opening sales".into(),
                    debit: "0.00".into(),
                    credit: "90.00".into(),
                    subledger_kind: "none".into(),
                },
            ],
            subledger_items: vec![],
        };
        match service(&p)
            .opening_balance_preview("acme.example.test", p.maker.clone(), owned_maker(&p), unbalanced)
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "UNBALANCED_OPENING_BALANCE");
            }
            other => panic!("unbalanced opening must fail: {other:?}"),
        }

        let mut duplicate = opening_input();
        duplicate.subledger_items.clear();
        duplicate.lines[0].subledger_kind = "none".into();
        duplicate.lines[1].account_id = "opening-ar".into();
        match service(&p)
            .opening_balance_preview("acme.example.test", p.maker.clone(), owned_maker(&p), duplicate)
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "DUPLICATE_OPENING_ACCOUNT");
            }
            other => panic!("duplicate account must fail: {other:?}"),
        }

        let mut missing_schedule = opening_input();
        missing_schedule.subledger_items.clear();
        missing_schedule.lines[0].subledger_kind = "none".into();
        match service(&p)
            .opening_balance_preview(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                missing_schedule,
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "OPENING_CONTROL_SCHEDULE_REQUIRED");
            }
            other => panic!("missing schedule must fail: {other:?}"),
        }

        let mut mismatch = opening_input();
        mismatch.subledger_items[0].amount = "500.00".into();
        match service(&p)
            .opening_balance_preview("acme.example.test", p.maker.clone(), owned_maker(&p), mismatch)
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "OPENING_SUBLEDGER_MISMATCH");
            }
            other => panic!("schedule mismatch must fail: {other:?}"),
        }

        let mut wrong_currency = opening_input();
        wrong_currency.currency = "USD".into();
        match service(&p)
            .opening_balance_preview(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                wrong_currency,
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "OPENING_CURRENCY_MISMATCH");
            }
            other => panic!("currency mismatch must fail: {other:?}"),
        }

        // Company isolation is enforced by the owner query.
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        match erpx_server_finance::public::finance_opening::read_opening_preview(
            &mut tx,
            "another-company",
            &opening_input(),
        )
        .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "CHART_NOT_FOUND");
            }
            other => panic!("foreign Company must not see the chart: {other:?}"),
        }
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn opening_import_rejects_stale_token_and_reused_key() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let preview = service(&p)
            .opening_balance_preview(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                opening_input(),
            )
            .await
            .unwrap();
        match service(&p)
            .import_opening_balance(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("opening-stale-0001").unwrap(),
                OpeningImportInput {
                    chart_id: "chart".into(),
                    period_id: "period".into(),
                    entry_date: "2026-01-01".into(),
                    currency: "THB".into(),
                    lines: opening_input().lines,
                    subledger_items: opening_input().subledger_items,
                    preview_token: "0".repeat(64),
                },
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "OPENING_PREVIEW_STALE");
            }
            other => panic!("stale token must conflict: {other:?}"),
        }

        let key = "opening-key-0001";
        service(&p)
            .import_opening_balance(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new(key).unwrap(),
                OpeningImportInput {
                    chart_id: "chart".into(),
                    period_id: "period".into(),
                    entry_date: "2026-01-01".into(),
                    currency: "THB".into(),
                    lines: opening_input().lines,
                    subledger_items: opening_input().subledger_items,
                    preview_token: preview.data.preview_token.clone(),
                },
            )
            .await
            .unwrap();
        let mut changed = opening_input();
        changed.lines[0].description = "Changed after the first command".into();
        match service(&p)
            .import_opening_balance(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new(key).unwrap(),
                OpeningImportInput {
                    chart_id: "chart".into(),
                    period_id: "period".into(),
                    entry_date: "2026-01-01".into(),
                    currency: "THB".into(),
                    lines: changed.lines,
                    subledger_items: changed.subledger_items,
                    preview_token: preview.data.preview_token.clone(),
                },
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "IDEMPOTENCY_CONFLICT");
            }
            other => panic!("changed request must conflict: {other:?}"),
        }
    })
    .await;
}

#[tokio::test]
async fn opening_import_allows_only_one_concurrent_import_per_chart() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let definition = opening_input();
        let preview = service(&p)
            .opening_balance_preview(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                definition.clone(),
            )
            .await
            .unwrap();
        let first_service = service(&p);
        let second_service = service(&p);
        let first = first_service.import_opening_balance(
            "acme.example.test",
            p.maker.clone(),
            owned_maker(&p),
            CommandKey::new("opening-race-a").unwrap(),
            import_input(&definition, &preview.data.preview_token),
        );
        let second = second_service.import_opening_balance(
            "acme.example.test",
            p.maker.clone(),
            owned_maker(&p),
            CommandKey::new("opening-race-b").unwrap(),
            import_input(&definition, &preview.data.preview_token),
        );
        let (first, second) = tokio::join!(first, second);
        let succeeded = usize::from(first.is_ok()) + usize::from(second.is_ok());
        assert_eq!(succeeded, 1, "exactly one concurrent import may win");
        let failure = if first.is_err() { first } else { second };
        match failure {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "OPENING_BALANCE_EXISTS");
            }
            other => panic!("the losing import must see the existing opening: {other:?}"),
        }
        let journals: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM finance_journals WHERE company_id='company' AND chart_id='chart' AND is_opening=1 AND status='draft'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(journals, 1);
    })
    .await;
}

#[tokio::test]
async fn opening_subledger_schedules_post_and_reverse_with_independent_approval() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let definition = subledger_input();
        let preview = service(&p)
            .opening_balance_preview(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                definition.clone(),
            )
            .await
            .unwrap();
        assert_eq!(preview.data.subledger_count, 2);
        assert_eq!(preview.data.debit, "600.00");
        assert_eq!(preview.data.credit, "600.00");
        let imported = service(&p)
            .import_opening_balance(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("opening-subledger-0001").unwrap(),
                import_input(&definition, &preview.data.preview_token),
            )
            .await
            .unwrap();
        assert_eq!(imported.data.status, "draft");
        assert_eq!(imported.data.subledger_count, 2);
        let drafts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM finance_invoices WHERE company_id='company' AND source_id=$1 AND status='draft'",
        )
        .bind(&imported.data.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(drafts, 2);

        let mut changed = definition.clone();
        changed.subledger_items[0].reference = "CHANGED-REFERENCE".into();
        match service(&p)
            .import_opening_balance(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("opening-subledger-0001").unwrap(),
                import_input(&changed, &preview.data.preview_token),
            )
            .await
        {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "IDEMPOTENCY_CONFLICT");
            }
            other => panic!("changed opening schedule under the same key must conflict: {other:?}"),
        }

        let maker_post = p
            .administration()
            .review(
                "acme.example.test",
                p.maker.clone(),
                p.maker_selection.clone(),
                command(
                    &imported.data.id,
                    "opening-subledger-post-maker",
                    post(),
                ),
            )
            .await;
        business(maker_post, "INDEPENDENT_APPROVER_REQUIRED");

        let posted = p
            .review(
                &imported.data.id,
                "opening-subledger-post",
                post(),
            )
            .await
            .unwrap();
        assert!(matches!(
            posted,
            erpx_server_finance::public::journal_review::JournalReviewResult::Posted { version: 2, .. }
        ));
        let aging = service(&p)
            .finance_invoice_aging(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                "receivable".into(),
                "2026-03-01".into(),
                None,
            )
            .await
            .unwrap();
        let mut outstanding: Vec<i64> = aging
            .data
            .iter()
            .filter(|row| row["source_id"] == imported.data.id)
            .map(|row| row["outstanding_minor"].as_i64().unwrap())
            .collect();
        outstanding.sort();
        assert_eq!(outstanding, vec![25_000, 35_000]);

        let reversed = p
            .review(
                &imported.data.id,
                "opening-subledger-reverse",
                reverse(),
            )
            .await
            .unwrap();
        assert!(matches!(
            reversed,
            erpx_server_finance::public::journal_review::JournalReviewResult::Reversed { .. }
        ));
        let voided: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM finance_invoices WHERE company_id='company' AND source_id=$1 AND status='voided' AND outstanding_minor=0",
        )
        .bind(&imported.data.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(voided, 2);
    })
    .await;
}

#[tokio::test]
async fn opening_receivable_settlement_blocks_the_opening_reversal() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let definition = settlement_input();
        let preview = service(&p)
            .opening_balance_preview(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                definition.clone(),
            )
            .await
            .unwrap();
        let imported = service(&p)
            .import_opening_balance(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("opening-settlement-0001").unwrap(),
                import_input(&definition, &preview.data.preview_token),
            )
            .await
            .unwrap();
        p.review(&imported.data.id, "opening-settlement-post", post())
            .await
            .unwrap();
        let invoice_id = format!("{}-subledger-0", imported.data.id);

        sqlx::raw_sql(
            "INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,opening_minor,closing_minor,status,source_hash,version,imported_by,created_at)VALUES\
             ('opening-statement','company','opening-bank','OPEN-SETTLE-STMT-001','2026-01-31',0,25000,'imported','opening-settlement-source',1,'fixture-a',0);\
             INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,description,amount_minor,currency,match_status,version)VALUES\
             ('opening-bank-row','opening-statement','company','2026-01-20','OPEN-SETTLE-BANK-001','Opening receivable receipt',25000,'THB','unmatched',1)",
        )
        .execute(pool.raw())
        .await
        .unwrap();

        let clock = p.system.scenario.clock.clone();
        let payment_id = "opening-receipt-001";
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        let maker = finance_actor("fixture-a");
        prepare_generic_payment(
            &mut tx,
            &maker,
            payment_id,
            &GenericPaymentInput {
                kind: "receipt".into(),
                party_id: "opening-customer".into(),
                payment_number: "OPEN-RECEIPT-001".into(),
                payment_date: "2026-01-20".into(),
                currency: "THB".into(),
                amount: "250.00".into(),
                bank_account: "opening-bank".into(),
                settlement_invoice_id: invoice_id.clone(),
            },
            clock.as_ref(),
        )
        .await
        .unwrap()
        .persist(&mut tx)
        .await
        .unwrap();
        let approver = finance_actor("fixture-b");
        read_service_receipt_review(&mut tx, &approver, payment_id)
            .await
            .unwrap()
            .apply(&mut tx, &approver, true, 1, false, false, clock.now().get())
            .await
            .unwrap();
        let treasury = finance_actor("fixture-c");
        read_service_receipt_review(&mut tx, &treasury, payment_id)
            .await
            .unwrap()
            .apply(&mut tx, &treasury, false, 2, false, false, clock.now().get())
            .await
            .unwrap();
        let confirm = prepare_service_receipt_confirm(
            &mut tx,
            &treasury,
            payment_id,
            &invoice_id,
            "opening-receipt-confirm",
            &ServiceReceiptConfirmInput {
                expected_version: 3,
                period_id: "period".into(),
                bank_transaction_id: "opening-bank-row".into(),
                bank_version: 1,
                bank_reference: "OPEN-SETTLE-BANK-001".into(),
                proof: "Exact bank evidence matches the opening receivable".into(),
                fx_rate: None,
                fx_gain_account_id: None,
                fx_loss_account_id: None,
                allocations: vec![ServiceReceiptAllocationInput {
                    invoice_id: invoice_id.clone(),
                    amount: "250.00".into(),
                }],
                attachment_set_ids: Vec::new(),
            },
            clock.as_ref(),
        )
        .await
        .unwrap();
        let rules = embedded_currency_rules().unwrap();
        let collation = FinanceCollation::english().unwrap();
        let mut authority = AllowJournal;
        let mut facts = PgJournalPreparation::new(
            &mut tx,
            &treasury,
            &mut authority,
            clock.as_ref(),
            &rules,
            &collation,
        )
        .unwrap();
        let primary = prepare_journal(
            &mut facts,
            &treasury,
            confirm.primary_draft(),
            &CommandKey::new("opening-receipt-confirm-journal").unwrap(),
            JournalStatus::Posted,
        )
        .await
        .unwrap();
        let retained = confirm
            .apply(&mut tx, &primary, None, clock.as_ref())
            .await
            .unwrap();
        flush_service_receipt_confirm_effects(&mut tx)
            .await
            .unwrap();
        retained.require_finalized(&mut tx).await.unwrap();
        tx.commit().await.unwrap();

        let (status, outstanding): (String, i64) = sqlx::query_as(
            "SELECT status,outstanding_minor FROM finance_payments WHERE id=$1 AND company_id='company'",
        )
        .bind(payment_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!((status.as_str(), outstanding), ("settled", 0));
        let (allocations, amount): (i64, i64) = sqlx::query_as(
            "SELECT count(*)::bigint,coalesce(sum(amount_minor),0)::bigint FROM finance_allocations WHERE payment_id=$1 AND company_id='company'",
        )
        .bind(payment_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!((allocations, amount), (1, 25_000));
        let (invoice_status, invoice_outstanding): (String, i64) = sqlx::query_as(
            "SELECT status,outstanding_minor FROM finance_invoices WHERE id=$1 AND company_id='company'",
        )
        .bind(&invoice_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!((invoice_status.as_str(), invoice_outstanding), ("paid", 0));

        match p
            .review(
                &imported.data.id,
                "opening-settlement-reverse",
                reverse(),
            )
            .await
        {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "OPENING_SUBLEDGER_SETTLED");
            }
            other => panic!("a settled opening receivable must block reversal: {other:?}"),
        }
    })
    .await;
}

#[tokio::test]
async fn opening_preview_and_import_http_routes_accept_real_session() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        seed(&p.system.holdings[0]).await;
        let app = finance_journal_read_router(
            Arc::new(account::fixture::account::policy()),
            account::fixture::account::holding_registry(),
            Arc::new(FinanceJournalReadScopeHeaders),
            vec![FinanceJournalReadTarget {
                host: "acme.example.test".into(),
                holding_slug: "acme".into(),
                service: Arc::new(service(&p)),
            }],
        )
        .unwrap();
        let body = serde_json::to_vec(&opening_input()).unwrap();
        let preview = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/finance/opening-balances/preview")
            .header("host", "acme.example.test")
            .header("cookie", format!("__Host-erpx-product={}", p.maker_token.expose()))
            .header("origin", "https://acme.example.test")
            .header("content-type", "application/json")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .body(Body::from(body))
            .unwrap();
        let response = app.clone().oneshot(preview).await.unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 2 * 1024 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(status, StatusCode::OK, "{value}");
        assert_eq!(value["data"]["debit"], "1000.00");
        let token = value["data"]["previewToken"].as_str().unwrap().to_owned();
        assert_eq!(token.len(), 64);

        let import_body = serde_json::to_vec(&serde_json::json!({
            "chartId": "chart",
            "periodId": "period",
            "entryDate": "2026-01-01",
            "currency": "THB",
            "lines": opening_input().lines,
            "subledgerItems": opening_input().subledger_items,
            "previewToken": token,
        }))
        .unwrap();
        let import = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/finance/opening-balances/import")
            .header("host", "acme.example.test")
            .header("cookie", format!("__Host-erpx-product={}", p.maker_token.expose()))
            .header("origin", "https://acme.example.test")
            .header("content-type", "application/json")
            .header("idempotency-key", "opening-http-0001")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .body(Body::from(import_body.clone()))
            .unwrap();
        let response = app.clone().oneshot(import).await.unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 2 * 1024 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(status, StatusCode::CREATED, "{value}");
        assert_eq!(value["data"]["id"], "journal-opening-http-0001");

        let missing = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/finance/opening-balances/import")
            .header("host", "acme.example.test")
            .header("cookie", format!("__Host-erpx-product={}", p.maker_token.expose()))
            .header("origin", "https://acme.example.test")
            .header("content-type", "application/json")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .body(Body::from(import_body))
            .unwrap();
        assert_eq!(
            app.oneshot(missing).await.unwrap().status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    })
    .await;
}
