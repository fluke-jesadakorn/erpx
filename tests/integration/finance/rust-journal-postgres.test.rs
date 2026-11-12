#[path = "../../support/rust-postgres.rs"]
mod fixture;
#[path = "../../support/finance/journal-facts.rs"]
mod journal_fixture;

use erpx_server_finance::public::revenue_recognition::{
    RevenueRecognitionInput, recognize_revenue,
};
use erpx_server_finance::public::sales_invoice_create::SalesInvoiceCommandActor;
use erpx_server_finance::public::{
    FinanceActor, JournalDraft, JournalStatus, PgJournalStore, prepare_journal,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CommandKey, CompanyId, HoldingId, TimestampMillis, UserId,
};
use serde_json::json;

fn actor() -> FinanceActor {
    FinanceActor {
        user_id: UserId::new("user").unwrap(),
        holding_id: HoldingId::new("holding").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        assignment_id: AssignmentId::new("assignment").unwrap(),
    }
}

fn draft() -> JournalDraft {
    JournalDraft { chart_id: "chart".to_owned(), period_id: "period".to_owned(), entry_date: "2026-01-01".to_owned(), currency: "THB".to_owned(), fx_rate: Some("0.5".to_owned()), is_opening: false,
        memo: "Reviewed source journal".to_owned(), source_type: "manual".to_owned(), source_id: None,
        lines: serde_json::from_value(json!([{ "accountId": "cash", "debit": "0.03", "credit": "0" }, { "accountId": "sales", "debit": "0", "credit": "0.01" }, { "accountId": "sales", "debit": "0", "credit": "0.02" }])).unwrap() }
}

async fn prepare_pg_plan(
    connection: &mut sqlx::PgConnection,
    key: &str,
    status: JournalStatus,
) -> Result<
    erpx_server_finance::public::JournalPreparation,
    PgTransactionError<erpx_server_finance::public::JournalError>,
> {
    use erpx_server_finance::public::{
        FinanceCollation, PgJournalPreparation, embedded_currency_rules,
    };
    let actor = actor();
    let input = draft();
    let mut authority = journal_fixture::JournalFacts::default();
    let rules = embedded_currency_rules().unwrap();
    let ordering = FinanceCollation::english().unwrap();
    let mut facts = PgJournalPreparation::new(
        connection,
        &actor,
        &mut authority,
        &journal_fixture::FixedClock,
        &rules,
        &ordering,
    )?;
    prepare_journal(
        &mut facts,
        &actor,
        &input,
        &CommandKey::new(key).unwrap(),
        status,
    )
    .await
}

#[tokio::test]
async fn prepared_journal_cannot_cross_owner_transactions_on_the_same_backend() {
    use erpx_server_finance::public::JournalError;
    use std::sync::Arc;
    fixture::with_database(1, |pool| async move {
        setup(&pool).await;
        let uow = PgUnitOfWork::new(pool.clone());
        let (plan, backend) = uow
            .run::<_, JournalError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let plan = prepare_pg_plan(
                        &mut *connection,
                        "journal-transaction-fence",
                        JournalStatus::Draft,
                    )
                    .await?;
                    let backend: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                        .fetch_one(connection)
                        .await?;
                    Ok((plan, backend))
                })
            })
            .await
            .unwrap();
        let plan = Arc::new(plan);
        let refused = uow
            .run::<(), JournalError, _>(TransactionOptions::default(), move |connection| {
                let plan = plan.clone();
                Box::pin(async move {
                    assert_eq!(
                        sqlx::query_scalar::<_, i32>("SELECT pg_backend_pid()")
                            .fetch_one(&mut *connection)
                            .await?,
                        backend
                    );
                    PgJournalStore::new(
                        HoldingId::new("holding").unwrap(),
                        CompanyId::new("company").unwrap(),
                        connection,
                    )
                    .persist(&plan)
                    .await
                })
            })
            .await;
        match refused {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "JOURNAL_TRANSACTION_CHANGED")
            }
            _ => panic!("a plan from an ended transaction must be refused"),
        }
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM finance_journals")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            0
        );
    })
    .await;
}

#[tokio::test]
async fn real_journal_facts_use_company_rates_scales_and_mandatory_authority() {
    use erpx_server_finance::public::{
        FinanceCollation, JournalError, PgJournalPreparation, embedded_currency_rules,
    };
    use std::sync::Arc;
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql("INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at) VALUES('rate-earlier','company','THB','2025-12-31',2,'user',1),('rate-future','company','THB','2027-01-01',4,'user',2);
            INSERT INTO company_currency_scales(company_id,currency,scale,configured_by,configured_at) VALUES('company','XAU',2,'user',0)")
            .execute(pool.raw()).await.unwrap();
        let currencies = Arc::new(embedded_currency_rules().unwrap());
        let collation = Arc::new(FinanceCollation::english().unwrap());
        assert_eq!(currencies.scale("XAU", Some(2)).unwrap(), 2);
        let uow = PgUnitOfWork::new(pool.clone());
        let rules = currencies.clone(); let ordering = collation.clone();
        let id = uow.run::<_, JournalError, _>(TransactionOptions::default(), move |connection| {
            let rules = rules.clone(); let ordering = ordering.clone();
            Box::pin(async move {
                let actor = actor(); let mut input = draft(); input.fx_rate = None;
                let mut authority = journal_fixture::JournalFacts::default();
                let mut facts = PgJournalPreparation::new(&mut *connection, &actor, &mut authority, &journal_fixture::FixedClock, &rules, &ordering)?;
                let plan = prepare_journal(&mut facts, &actor, &input, &CommandKey::new("journal-real-facts").unwrap(), JournalStatus::Posted).await?;
                assert_eq!(plan.fx(), 500_000);
                assert_eq!(plan.prepared_fx_evidence().unwrap().source_rate_id.as_deref(), Some("rate-earlier"));
                assert!(!plan.prepared_fx_evidence().unwrap().used_latest_fallback);
                assert_eq!(authority.calls, ["authorize"]);
                PgJournalStore::new(HoldingId::new("holding").unwrap(), CompanyId::new("company").unwrap(), connection).persist(&plan).await?;
                Ok(plan.journal_id().to_owned())
            })
        }).await.unwrap();
        assert_eq!(id, "journal-journal-real-facts");
        for (guard, expected) in [("authority", "FINANCE_FORBIDDEN"), ("rate", "MANUAL_RATE_REQUIRED"), ("scale", "FINANCE_CURRENCY_SCALE_UNSUPPORTED"), ("date", "DATE_OUTSIDE_PERIOD")] {
            let rules = currencies.clone(); let ordering = collation.clone();
            let result = uow.run::<(), JournalError, _>(TransactionOptions::default(), move |connection| {
                let rules = rules.clone(); let ordering = ordering.clone();
                Box::pin(async move {
                    let actor = actor(); let mut input = draft(); input.fx_rate = None;
                    if guard == "rate" { input.currency = "EUR".to_owned(); }
                    if guard == "scale" { input.currency = "JPY".to_owned(); }
                    if guard == "date" { input.entry_date = "2026-02-01".to_owned(); }
                    let mut authority = journal_fixture::JournalFacts { deny: guard == "authority", ..Default::default() };
                    let mut facts = PgJournalPreparation::new(connection, &actor, &mut authority, &journal_fixture::FixedClock, &rules, &ordering)?;
                    prepare_journal(&mut facts, &actor, &input, &CommandKey::new("journal-rejected-facts").unwrap(), JournalStatus::Draft).await?;
                    Ok(())
                })
            }).await;
            match result { Err(PgTransactionError::Business(error)) => assert_eq!(error.code, expected, "{guard}"), _ => panic!("expected {guard} refusal") }
        }
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM finance_journals").fetch_one(pool.raw()).await.unwrap(), 1);
    }).await;
}

async fn setup(pool: &erpx_server_runtime::public::postgres::DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql("INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES('chart','company','Primary','USD','user',0,0);
        INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES('period','company','January','2026-01-01','2026-01-31',0);
        INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,created_at,updated_at) VALUES('cash','company','chart','100','Cash','asset',0,0),('sales','company','chart','400','Sales','revenue',0,0)")
        .execute(pool.raw()).await.unwrap();
}

#[tokio::test]
async fn journal_joins_owner_commit_and_rolls_back_with_source_and_receipt() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql("CREATE TABLE source_probe(id TEXT PRIMARY KEY); CREATE TABLE receipt_probe(id TEXT PRIMARY KEY)").execute(pool.raw()).await.unwrap();
        let uow = PgUnitOfWork::new(pool.clone());
        let result = uow.run::<_, erpx_server_finance::public::JournalError, _>(TransactionOptions::default(), |connection| Box::pin(async move {
            let plan = prepare_pg_plan(&mut *connection, "journal-atomic-key", JournalStatus::Posted).await?;
            sqlx::query("INSERT INTO source_probe(id) VALUES('source')").execute(&mut *connection).await?;
            PgJournalStore::new(HoldingId::new("holding").unwrap(), CompanyId::new("company").unwrap(), &mut *connection).persist(&plan).await?;
            sqlx::query("INSERT INTO receipt_probe(id) VALUES('receipt')").execute(&mut *connection).await?;
            Ok(plan.result().id.clone())
        })).await.unwrap();
        assert_eq!(result, "journal-journal-atomic-key");
        let totals: (i64, i64, i64, i64) = sqlx::query_as("SELECT sum(debit_minor)::bigint,sum(credit_minor)::bigint,sum(base_debit_minor)::bigint,sum(base_credit_minor)::bigint FROM finance_journal_lines").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(totals, (3, 3, 2, 2));
        let rollback = uow.run::<(), erpx_server_finance::public::JournalError, _>(TransactionOptions::default(), |connection| Box::pin(async move {
            let plan = prepare_pg_plan(&mut *connection, "journal-rollback-key", JournalStatus::Draft).await?;
            PgJournalStore::new(HoldingId::new("holding").unwrap(), CompanyId::new("company").unwrap(), &mut *connection).persist(&plan).await?;
            sqlx::query("INSERT INTO source_probe(id) VALUES('rolled-back')").execute(&mut *connection).await?;
            sqlx::query("INSERT INTO receipt_probe(id) VALUES('receipt')").execute(connection).await?;
            Ok(())
        })).await;
        assert!(rollback.is_err());
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM finance_journals").fetch_one(pool.raw()).await.unwrap(), 1);
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM source_probe").fetch_one(pool.raw()).await.unwrap(), 1);
    }).await;
}

#[tokio::test]
async fn journal_rejects_unscoped_transactions_and_changed_posting_facts() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let plan = prepare_journal(&mut journal_fixture::JournalFacts::default(), &actor(), &draft(), &CommandKey::new("journal-guard-key").unwrap(), JournalStatus::Draft).await.unwrap();
        let mut connection = pool.raw().acquire().await.unwrap();
        assert!(PgJournalStore::new(HoldingId::new("holding").unwrap(), CompanyId::new("company").unwrap(), &mut connection).persist(&plan).await.is_err());
        drop(connection);
        let uow = PgUnitOfWork::new(pool.clone());
        for guard in ["foreign-company", "foreign-holding", "account", "period"] {
            let result = uow.run::<(), erpx_server_finance::public::JournalError, _>(TransactionOptions::default(), move |connection| Box::pin(async move {
                let plan = prepare_pg_plan(&mut *connection, "journal-guard-key", JournalStatus::Draft).await?;
                if guard == "account" { sqlx::query("UPDATE finance_accounts SET status='inactive' WHERE id='sales'").execute(&mut *connection).await?; }
                if guard == "period" { sqlx::query("UPDATE finance_periods SET state='closed' WHERE id='period'").execute(&mut *connection).await?; }
                let holding = if guard == "foreign-holding" { "foreign" } else { "holding" };
                let company = if guard == "foreign-company" { "foreign" } else { "company" };
                PgJournalStore::new(HoldingId::new(holding).unwrap(), CompanyId::new(company).unwrap(), connection).persist(&plan).await
            })).await;
            assert!(matches!(result, Err(PgTransactionError::Business(_))), "{guard}");
        }
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM finance_journals").fetch_one(pool.raw()).await.unwrap(), 0);
        assert_eq!(sqlx::query_scalar::<_, String>("SELECT status FROM finance_accounts WHERE id='sales'").fetch_one(pool.raw()).await.unwrap(), "active");
    }).await;
}

fn revenue_command_actor(company: &str) -> SalesInvoiceCommandActor {
    SalesInvoiceCommandActor {
        user_id: "user".into(),
        assignment_id: "assignment".into(),
        role: "finance".into(),
        session_id: "session".into(),
        revision: 1,
        assignment_version: 1,
        company_id: company.into(),
        branch_id: None,
        department_id: None,
        scope_kind: "holding".into(),
        scope_node_id: "holding".into(),
        iam_json: "{}".into(),
    }
}

fn revenue_finance_actor(company: &str) -> FinanceActor {
    FinanceActor {
        user_id: UserId::new("user").unwrap(),
        holding_id: HoldingId::new("holding").unwrap(),
        company_id: CompanyId::new(company).unwrap(),
        assignment_id: AssignmentId::new("assignment").unwrap(),
    }
}

struct RevenueClock(i64);
impl Clock for RevenueClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(self.0)
    }
}

async fn seed_revenue(
    pool: &DatabasePool,
    company: &str,
    id: &str,
    allocated_minor: i64,
    recognized_minor: i64,
    revenue_account: &str,
    deferred_account: &str,
    currency: &str,
) {
    sqlx::query(
        "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,\
         created_at,updated_at) VALUES($1,$2,'customer',$3,'Revenue customer',$4,'active',1,0,0) ON CONFLICT DO NOTHING",
    )
    .bind(format!("customer-{company}"))
    .bind(company)
    .bind(format!("RC-{company}"))
    .bind(currency)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO finance_revenue_contracts(id,company_id,customer_id,contract_number,\
         currency,transaction_price_minor,created_by,created_at,updated_at) \
         VALUES($1,$2,$3,$4,$5,$6,'fixture',0,0)",
    )
    .bind(format!("contract-{id}"))
    .bind(company)
    .bind(format!("customer-{company}"))
    .bind(format!("C-{id}"))
    .bind(currency)
    .bind(allocated_minor)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO finance_revenue_obligations(id,contract_id,description,allocated_minor,\
         recognition_method,start_date,end_date,recognized_minor,revenue_account_id,\
         deferred_account_id,status) \
         VALUES($1,$2,'Implementation',$3,'point_in_time','2026-01-01',NULL,$4,$5,$6,$7)",
    )
    .bind(id)
    .bind(format!("contract-{id}"))
    .bind(allocated_minor)
    .bind(recognized_minor)
    .bind(revenue_account)
    .bind(deferred_account)
    .bind(if recognized_minor == 0 {
        "pending"
    } else if recognized_minor == allocated_minor {
        "complete"
    } else {
        "active"
    })
    .execute(pool.raw())
    .await
    .unwrap();
}

fn revenue_error_code(
    result: Result<impl std::fmt::Debug, PgTransactionError<BusinessError>>,
) -> &'static str {
    match result {
        Err(PgTransactionError::Business(error)) => error.code,
        Err(PgTransactionError::Persistence(error)) => {
            panic!("expected a business denial, got persistence {error:?}")
        }
        Ok(value) => panic!("expected a business denial, got {value:?}"),
    }
}

#[tokio::test]
async fn revenue_recognition_posts_a_balanced_journal_advances_obligation_and_replays() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::query(
            "INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,created_at,updated_at) \
             VALUES('deferred','company','chart','200','Deferred revenue','liability',0,0) \
             ON CONFLICT DO NOTHING",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        seed_revenue(
            &pool,
            "company",
            "revenue-obligation-1",
            100_00,
            0,
            "sales",
            "deferred",
            "USD",
        )
        .await;
        let uow = PgUnitOfWork::new(pool.clone());
        let input = RevenueRecognitionInput {
            amount: "40.00".into(),
            period_id: "period".into(),
            entry_date: "2026-01-10".into(),
        };
        let key = CommandKey::new("revenue-recognize-0001").unwrap();
        let result = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                let key = key.clone();
                let input = input.clone();
                Box::pin(async move {
                    recognize_revenue(
                        connection,
                        &revenue_command_actor("company"),
                        &revenue_finance_actor("company"),
                        &RevenueClock(1_800_000_000_000),
                        &key,
                        "revenue-obligation-1",
                        &input,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(result.id, "revenue-obligation-1");
        assert_eq!(result.journal_id, "journal-revenue-recognize-0001");
        assert_eq!(result.recognized, "40.00");

        let journal: (String, Option<String>, String, String, String, String) = sqlx::query_as(
            "SELECT source_type,source_id,memo,status,currency,posted_by \
             FROM finance_journals WHERE id=$1",
        )
        .bind(&result.journal_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            journal,
            (
                "revenue_recognition".into(),
                Some("revenue-obligation-1".into()),
                "Revenue recognition Implementation".into(),
                "posted".into(),
                "USD".into(),
                "user".into(),
            )
        );
        let lines: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT account_id,debit_minor,credit_minor FROM finance_journal_lines \
             WHERE journal_id=$1 ORDER BY ordinal",
        )
        .bind(&result.journal_id)
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            lines,
            vec![
                ("deferred".into(), 40_00, 0),
                ("sales".into(), 0, 40_00)
            ]
        );
        let obligation: (i64, String) = sqlx::query_as(
            "SELECT recognized_minor,status FROM finance_revenue_obligations WHERE id=$1",
        )
        .bind("revenue-obligation-1")
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(obligation, (40_00, "active".into()));
        for (table, column) in [
            ("command_receipts", "operation"),
            ("audit_events", "action"),
            ("outbox_events", "event_type"),
        ] {
            let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                "SELECT count(*) FROM {table} WHERE command_id=$1 AND {column}='finance.revenue.recognize'"
            )))
            .bind(key.as_str())
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(count, 1, "{table}");
        }

        let replay = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                let key = key.clone();
                let input = input.clone();
                Box::pin(async move {
                    recognize_revenue(
                        connection,
                        &revenue_command_actor("company"),
                        &revenue_finance_actor("company"),
                        &RevenueClock(1_800_000_000_000),
                        &key,
                        "revenue-obligation-1",
                        &input,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(replay, result);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM finance_journals")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT recognized_minor FROM finance_revenue_obligations WHERE id='revenue-obligation-1'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            40_00
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM command_receipts WHERE command_id='revenue-recognize-0001'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );
    })
    .await;
}

#[tokio::test]
async fn revenue_recognition_completes_bounds_and_company_isolation() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::query(
            "INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,created_at,updated_at) \
             VALUES('deferred','company','chart','200','Deferred revenue','liability',0,0) \
             ON CONFLICT DO NOTHING",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        seed_revenue(
            &pool,
            "company",
            "revenue-obligation-full",
            100_00,
            40_00,
            "sales",
            "deferred",
            "USD",
        )
        .await;
        seed_revenue(
            &pool,
            "foreign",
            "revenue-obligation-foreign",
            100_00,
            0,
            "sales",
            "deferred",
            "USD",
        )
        .await;
        seed_revenue(
            &pool,
            "company",
            "revenue-obligation-inactive",
            100_00,
            0,
            "sales",
            "deferred",
            "USD",
        )
        .await;
        let uow = PgUnitOfWork::new(pool.clone());
        let run = |obligation: &'static str, amount: &'static str, key: &'static str| {
            let uow = &uow;
            async move {
                let key = CommandKey::new(key).unwrap();
                uow.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let key = key.clone();
                    Box::pin(async move {
                        recognize_revenue(
                            connection,
                            &revenue_command_actor("company"),
                            &revenue_finance_actor("company"),
                            &RevenueClock(1_800_000_000_000),
                            &key,
                            obligation,
                            &RevenueRecognitionInput {
                                amount: amount.into(),
                                period_id: "period".into(),
                                entry_date: "2026-01-10".into(),
                            },
                        )
                        .await
                    })
                })
                .await
            }
        };
        let completed = run("revenue-obligation-full", "60.00", "revenue-recognize-full").await.unwrap();
        assert_eq!(completed.recognized, "60.00");
        let obligation: (i64, String) = sqlx::query_as(
            "SELECT recognized_minor,status FROM finance_revenue_obligations \
             WHERE id='revenue-obligation-full'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(obligation, (100_00, "complete".into()));
        let journals: i64 = sqlx::query_scalar("SELECT count(*) FROM finance_journals")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(
            revenue_error_code(run("revenue-obligation-full", "0.01", "revenue-recognize-exceeds").await),
            "REVENUE_EXCEEDS_ALLOCATION"
        );
        assert_eq!(
            revenue_error_code(run("revenue-obligation-full", "0.00", "revenue-recognize-zero").await),
            "REVENUE_EXCEEDS_ALLOCATION"
        );
        assert_eq!(
            revenue_error_code(run(
                "revenue-obligation-foreign",
                "10.00",
                "revenue-recognize-foreign",
            )
            .await),
            "OBLIGATION_NOT_FOUND"
        );
        sqlx::query("UPDATE finance_accounts SET status='inactive' WHERE id='sales'")
            .execute(pool.raw())
            .await
            .unwrap();
        assert_eq!(
            revenue_error_code(run(
                "revenue-obligation-inactive",
                "10.00",
                "revenue-recognize-inactive",
            )
            .await),
            "ACCOUNT_INVALID"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM finance_journals")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            journals
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT recognized_minor FROM finance_revenue_obligations \
                 WHERE id='revenue-obligation-inactive'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            0
        );
    })
    .await;
}
