//! Native Payroll summary and run-list reads on migrated PostgreSQL.
//! These tests prove owner SQL, Company scope, persisted-source readback and
//! rollback; they do not prove a command, purchase or browser journey.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_people_payroll_server::public::payroll_summary::{
    PayrollReadActor, PgPayrollRead, payroll_runs, payroll_summary, payroll_work_rows,
    payroll_history, payroll_work_rows_on,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_platform_shared_server::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn legacy_history(database: &str, company: &str, cursor: Option<&str>) -> serde_json::Value {
    let token = database.strip_prefix("erpx_dev_rust_").expect("leased test database");
    let node = std::env::var("ERPX_NODE_BIN").unwrap_or_else(|_| "node".into());
    let helper = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/testing/fixtures/payroll-history-legacy-read.ts");
    let mut child = std::process::Command::new(node).arg("--conditions=react-server").arg("--import=tsx").arg(helper)
        .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped())
        .spawn().expect("start source-bound Node oracle");
    serde_json::to_writer(child.stdin.take().unwrap(), &serde_json::json!({"database":database,"token":token,"company":company,"cursor":cursor})).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "legacy reader failed: {}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).expect("legacy reader JSON")
}

#[tokio::test]
async fn history_pages_preserve_full_rows_cursor_and_company_authority() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::query("INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,period_start,period_end,pay_date,currency,mode,status,revision,totals_json,version,created_by,created_at,updated_at) SELECT 'history-'||lpad(n::text,2,'0'),'company','group','2026-12-01','2026-12-31','2026-12-31','THB','preview','draft',n,'{}',1,'user',5,5 FROM generate_series(1,35) n")
            .execute(pool.raw()).await.unwrap();
        let database: String = sqlx::query_scalar("SELECT current_database()").fetch_one(pool.raw()).await.unwrap();
        let current = actor("company", &["payroll.read"]);
        PgUnitOfWork::new(pool.clone()).run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
            let current = current.clone();
            let database = database.clone();
            Box::pin(async move {
                let first = payroll_history(c, &current, None, NOW).await?;
                assert_eq!(serde_json::to_value(&first).unwrap(), legacy_history(&database, "company", None)["page"]);
                assert_eq!(first.items.len(), 30);
                assert_eq!(first.items[0], serde_json::json!({
                    "id":"history-35", "company_id":"company", "pay_group_id":"group",
                    "country_pack_id":null, "period_start":"2026-12-01", "period_end":"2026-12-31",
                    "pay_date":"2026-12-31", "currency":"THB", "mode":"preview", "status":"draft",
                    "revision":35, "totals_json":"{}", "input_digest":null, "error_message":null,
                    "version":1, "created_by":"user", "approved_by":null, "posted_by":null,
                    "created_at":5, "updated_at":5, "scope_engagement_id":null,
                    "submitted_by":null, "submitted_digest":null, "submitted_at":null,
                    "adjustment_of_run_id":null, "page_key":"2026-12-31:5"
                }));
                assert_eq!(first.items[29]["id"], "history-06");
                assert_eq!(first.items[0]["company_id"], "company");
                assert_eq!(first.items[0]["page_key"], "2026-12-31:5");
                assert_eq!(first.items[0]["created_by"], "user");
                assert!(first.items[0].get("scope_engagement_id").is_some());
                assert!(first.items[0].get("adjustment_of_run_id").is_some());
                assert_eq!(first.next_cursor.as_deref(), Some("eyJrZXkiOiIyMDI2LTEyLTMxOjUiLCJpZCI6Imhpc3RvcnktMDYiLCJyZXZlcnNlIjpmYWxzZSwic2NvcGUiOiI2NGJkZGE2Mjc3YzIyYmQxMWJmMTNlYTRkYTYwMmI4MjY4ZGEwMzAwYzg5NGY0OWFjOGZmNjJmOWE5M2ZkNzkyIn0"));
                assert!(first.previous_cursor.is_none() && first.has_more);
                let second = payroll_history(c, &current, first.next_cursor.as_deref(), NOW).await?;
                assert_eq!(serde_json::to_value(&second).unwrap(), legacy_history(&database, "company", first.next_cursor.as_deref())["page"]);
                assert_eq!(second.items.len(), 8);
                assert_eq!(second.items[0]["id"], "history-05");
                let decorated = format!("!{}==", first.next_cursor.as_deref().unwrap());
                let decoded_like_node = payroll_history(c, &current, Some(&decorated), NOW).await?;
                assert_eq!(decoded_like_node.items, second.items);
                assert!(!second.has_more && second.next_cursor.is_none());
                let back = payroll_history(c, &current, second.previous_cursor.as_deref(), NOW).await?;
                assert_eq!(serde_json::to_value(&back).unwrap(), legacy_history(&database, "company", second.previous_cursor.as_deref())["page"]);
                assert_eq!(back.items, first.items);
                assert_eq!(back.next_cursor, first.next_cursor);
                assert!(back.previous_cursor.is_none());
                let foreign = actor("foreign", &["payroll.read"]);
                assert_eq!(business(payroll_history(c, &foreign, first.next_cursor.as_deref(), NOW).await.unwrap_err()).code, "INVALID_CURSOR");
                assert_eq!(legacy_history(&database, "foreign", first.next_cursor.as_deref())["error"]["code"], "INVALID_CURSOR");
                let denied = actor("company", &["payroll.payment.read"]);
                assert_eq!(business(payroll_history(c, &denied, Some("invalid"), NOW).await.unwrap_err()).code, "PERMISSION_REQUIRED");
                assert_eq!(business(payroll_history(c, &current, Some("invalid"), NOW).await.unwrap_err()).code, "INVALID_CURSOR");
                assert_eq!(legacy_history(&database, "company", Some("invalid"))["error"]["code"], "INVALID_CURSOR");
                Ok(())
            })
        }).await.unwrap();
    }).await;
}

#[tokio::test]
async fn history_safe_integer_boundaries_match_legacy_reader_acceptance() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let database: String = sqlx::query_scalar("SELECT current_database()").fetch_one(pool.raw()).await.unwrap();
        for value in [9_007_199_254_740_991_i64, -9_007_199_254_740_991, 9_007_199_254_740_992, -9_007_199_254_740_992] {
            sqlx::query("UPDATE hr_payroll_runs SET created_at=$1 WHERE id='run-nov-thb'").bind(value).execute(pool.raw()).await.unwrap();
            let legacy = legacy_history(&database, "company", None);
            let current = actor("company", &["payroll.read"]);
            let native = PgUnitOfWork::new(pool.clone()).run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
                let current = current.clone();
                Box::pin(async move { payroll_history(c, &current, None, NOW).await })
            }).await;
            if value.unsigned_abs() <= 9_007_199_254_740_991 {
                assert_eq!(serde_json::to_value(native.unwrap()).unwrap(), legacy["page"]);
            } else {
                assert_eq!(business(native.unwrap_err()).code, "DATABASE_NUMBER_OUT_OF_RANGE");
                assert_eq!(legacy["error"]["name"], "RangeError");
                assert_eq!(legacy["error"]["message"], "DATABASE_NUMBER_OUT_OF_RANGE");
            }
        }
    }).await;
}

#[tokio::test]
async fn history_page_boundaries_and_unicode_cursors_match_legacy_source() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let database: String = sqlx::query_scalar("SELECT current_database()").fetch_one(pool.raw()).await.unwrap();
        let company = "บริษัท🧾";
        let mut inserted = 0_i64;
        for count in [0_i64, 30, 31, 60, 61] {
            if count > inserted {
                sqlx::query("INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,period_start,period_end,pay_date,currency,mode,status,revision,totals_json,version,created_by,created_at,updated_at) SELECT 'รายการ-'||lpad(n::text,2,'0'),$1,'group','2026-12-01','2026-12-31','2026-12-31','THB','preview','draft',n,'{}',1,'user',5,5 FROM generate_series($2::bigint,$3::bigint) n")
                    .bind(company).bind(inserted + 1).bind(count).execute(pool.raw()).await.unwrap();
                inserted = count;
            }
            let database = database.clone();
            let current = actor(company, &["payroll.read"]);
            PgUnitOfWork::new(pool.clone()).run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
                let current = current.clone();
                let database = database.clone();
                Box::pin(async move {
                    let first = payroll_history(c, &current, None, NOW).await?;
                    assert_eq!(first.items.len(), count.min(30) as usize);
                    assert_eq!(first.has_more, count > 30);
                    assert_eq!(serde_json::to_value(&first).unwrap(), legacy_history(&database, company, None)["page"]);
                    if let Some(next) = &first.next_cursor {
                        let second = payroll_history(c, &current, Some(next), NOW).await?;
                        assert_eq!(serde_json::to_value(&second).unwrap(), legacy_history(&database, company, Some(next))["page"]);
                        let decorated = format!(" \n!{}=ignored", next.replace('-', "+").replace('_', "/"));
                        let node = legacy_history(&database, company, Some(&decorated));
                        let rust = payroll_history(c, &current, Some(&decorated), NOW).await?;
                        assert_eq!(serde_json::to_value(rust).unwrap(), node["page"]);
                        let previous = second.previous_cursor.as_deref().unwrap();
                        let back = payroll_history(c, &current, Some(previous), NOW).await?;
                        assert_eq!(back.items, first.items);
                        assert_eq!(serde_json::to_value(back).unwrap(), legacy_history(&database, company, Some(previous))["page"]);
                        if let Some(third_cursor) = &second.next_cursor {
                            let third = payroll_history(c, &current, Some(third_cursor), NOW).await?;
                            assert_eq!(serde_json::to_value(third).unwrap(), legacy_history(&database, company, Some(third_cursor))["page"]);
                        }
                    }
                    Ok(())
                })
            }).await.unwrap();
        }
    }).await;
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

fn actor(company: &str, permissions: &[&str]) -> PayrollReadActor {
    PayrollReadActor {
        company_id: CompanyId::new(company).unwrap(),
        branch_id: None,
        department_id: None,
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        expires_at: NOW.get() + 100_000,
    }
}

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES \
         ('person','user','company','Fixture person','TH','user',0,0),\
         ('person-2',NULL,'company','Fixture person two','TH','user',0,0),\
         ('foreign-person',NULL,'foreign','Foreign person','TH','user',0,0);\
         INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES \
         ('engagement','person','company','employee','salaried','active','2026-01-01','Fixture','THB','user',0,0),\
         ('engagement-2','person-2','company','employee','salaried','active','2026-01-01','Fixture two','THB','user',0,0),\
         ('foreign-engagement','foreign-person','foreign','employee','salaried','active','2026-01-01','Foreign','USD','user',0,0);\
         INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,timezone,created_by,created_at,updated_at) VALUES \
         ('group','company','Monthly','TH','THB','monthly','Asia/Bangkok','user',0,0),\
         ('foreign-group','foreign','Foreign monthly','US','USD','monthly','UTC','user',0,0);\
         INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,period_start,period_end,pay_date,currency,mode,status,revision,totals_json,version,created_by,created_at,updated_at) VALUES \
         ('run-oct-thb','company','group','2026-10-01','2026-10-31','2026-10-31','THB','production','posted',1,'{\"gross\":1500000}',1,'user',1,1),\
         ('run-oct-usd','company','group','2026-10-01','2026-10-31','2026-10-31','USD','production','posted',2,'{\"gross\":10000}',1,'user',2,2),\
         ('run-nov-thb','company','group','2026-11-01','2026-11-30','2026-11-30','THB','preview','preview',1,'{}',1,'user',3,3),\
         ('run-foreign','foreign','foreign-group','2026-10-01','2026-10-31','2026-10-31','USD','production','posted',1,'{}',1,'user',4,4);\
         INSERT INTO hr_payroll_items(id,run_id,engagement_id,worker_type,input_snapshot_json,result_json,gross_minor,deductions_minor,employer_cost_minor,net_minor,currency,status,created_at) VALUES \
         ('item-1','run-oct-thb','engagement','employee','{}','{}',1000000,100000,50000,900000,'THB','posted',1),\
         ('item-2','run-oct-thb','engagement-2','employee','{}','{}',500000,50000,25000,450000,'THB','posted',2),\
         ('item-3','run-oct-usd','engagement','employee','{}','{}',10000,1000,500,9000,'USD','posted',3),\
         ('item-4','run-nov-thb','engagement','employee','{}','{}',999,99,49,899,'THB','calculated',4),\
         ('item-5','run-foreign','foreign-engagement','employee','{}','{}',777,77,7,700,'USD','posted',5);\
         INSERT INTO hr_payment_batches(id,company_id,source_type,source_id,currency,status,format_code,total_minor,version,created_by,created_at,updated_at) VALUES \
         ('batch','company','payroll','run-oct-thb','THB','settled','generic-csv',1350000,1,'user',5,5),\
         ('batch-cancelled','company','payroll','run-oct-thb','THB','cancelled','generic-csv',1,1,'user',6,6);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn summary_and_runs_are_company_scoped_posted_and_read_back_persisted_rows() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let read_actor = actor("company", &["payroll.read"]);
        let transaction_actor = read_actor.clone();
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = transaction_actor.clone();
                Box::pin(async move {
                    let mut reader = PgPayrollRead::new(connection);
                    let rows = payroll_summary(&current, &mut reader, None, NOW).await?;
                    assert_eq!(rows.len(), 2);
                    assert_eq!(rows[0].pay_month, "2026-10");
                    assert_eq!(rows[0].currency, "THB");
                    assert_eq!(rows[0].run_count, 1);
                    assert_eq!(rows[0].headcount, 2);
                    assert_eq!(rows[0].gross_minor, 1_500_000);
                    assert_eq!(rows[0].deductions_minor, 150_000);
                    assert_eq!(rows[0].employer_cost_minor, 75_000);
                    assert_eq!(rows[0].net_minor, 1_350_000);
                    assert_eq!(rows[1].pay_month, "2026-10");
                    assert_eq!(rows[1].currency, "USD");
                    assert_eq!(rows[1].run_count, 1);
                    assert_eq!(rows[1].headcount, 1);
                    assert_eq!(rows[1].gross_minor, 10_000);
                    assert_eq!(rows[1].net_minor, 9_000);

                    let october =
                        payroll_summary(&current, &mut reader, Some("2026-10"), NOW).await?;
                    assert_eq!(october, rows);
                    assert!(
                        payroll_summary(&current, &mut reader, Some("2026-11"), NOW)
                            .await?
                            .is_empty()
                    );
                    let invalid = payroll_summary(&current, &mut reader, Some("2026-13"), NOW)
                        .await
                        .unwrap_err();
                    assert_eq!(business(invalid).code, "INVALID_PAY_MONTH");

                    let full = payroll_runs(&current, &mut reader, NOW).await?;
                    assert_eq!(full.len(), 3);
                    assert_eq!(full[0].id, "run-nov-thb");
                    assert_eq!(full[0].status, "preview");
                    assert!(full.iter().all(|row| row.id != "run-foreign"));
                    assert!(full.iter().all(|row| row.totals_json.is_some()));
                    assert!(
                        full.iter()
                            .all(|row| row.pay_group_id.as_deref() == Some("group")),
                        "the full run list projects the selected pay group for the period lookup"
                    );

                    let payment_only_actor = actor("company", &["payroll.payment.read"]);
                    let payment_only = payroll_runs(&payment_only_actor, &mut reader, NOW).await?;
                    assert_eq!(payment_only.len(), 2);
                    assert!(
                        payment_only
                            .iter()
                            .all(|row| row.status == "posted" && row.totals_json.is_none())
                    );
                    let batch = payment_only
                        .iter()
                        .find(|row| row.id == "run-oct-thb")
                        .unwrap();
                    assert_eq!(batch.payment_batch_id.as_deref(), Some("batch"));
                    assert_eq!(batch.payment_status.as_deref(), Some("settled"));
                    assert_eq!(batch.net_payable_minor, Some(1_350_000));
                    assert!(
                        payment_only.iter().all(|row| row.pay_group_id.is_none()),
                        "a payment-only reader never receives the pay group"
                    );

                    let work = payroll_work_rows(&current, &mut reader, NOW).await?;
                    assert_eq!(
                        work.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
                        vec!["run-nov-thb", "run-oct-usd", "run-oct-thb"]
                    );
                    assert!(work.iter().all(|row| row.created_by == "user"));
                    assert!(work.iter().all(|row| row.pay_group_id == "group"));
                    assert_eq!(
                        work.iter().map(|row| row.updated_at).collect::<Vec<_>>(),
                        vec![3, 2, 1]
                    );
                    Ok(())
                })
            })
            .await
            .unwrap();

        let foreign_actor = actor("foreign", &["payroll.read"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = foreign_actor.clone();
                Box::pin(async move {
                    let mut reader = PgPayrollRead::new(connection);
                    let rows = payroll_summary(&current, &mut reader, None, NOW).await?;
                    assert_eq!(rows.len(), 1);
                    assert_eq!(rows[0].company_id, "foreign");
                    let runs = payroll_runs(&current, &mut reader, NOW).await?;
                    assert_eq!(runs.len(), 1);
                    assert_eq!(runs[0].id, "run-foreign");
                    Ok(())
                })
            })
            .await
            .unwrap();

        let persisted: i64 =
            sqlx::query_scalar("SELECT count(*) FROM hr_payroll_runs WHERE company_id='company'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(persisted, 3, "reads must not create or delete payroll runs");
    })
    .await;
}

#[tokio::test]
async fn policy_negatives_transaction_guards_and_rollback_preserve_the_source() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;

        let mut outside = pool.raw().acquire().await.unwrap();
        let error = PgPayrollRead::new(&mut outside)
            .summary(&CompanyId::new("company").unwrap(), None)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref value)
                if value.code == "PAYROLL_READ_TRANSACTION_REQUIRED"
        ));
        drop(outside);

        let read_actor = actor("company", &["payroll.read"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = read_actor.clone();
                Box::pin(async move {
                    let mut reader = PgPayrollRead::new(connection);
                    let mut denied = current.clone();
                    denied.permissions.clear();
                    let error = business(
                        payroll_summary(&denied, &mut reader, None, NOW)
                            .await
                            .unwrap_err(),
                    );
                    assert_eq!(error.kind, FailureKind::Forbidden);
                    assert_eq!(error.code, "PERMISSION_REQUIRED");

                    let mut branch = current.clone();
                    branch.branch_id = Some(RecordId::new("branch").unwrap());
                    let error =
                        business(payroll_runs(&branch, &mut reader, NOW).await.unwrap_err());
                    assert_eq!(error.code, "COMPANY_PAYROLL_SCOPE_REQUIRED");

                    let mut expired = current.clone();
                    expired.expires_at = NOW.get();
                    let error = business(
                        payroll_summary(&expired, &mut reader, None, NOW)
                            .await
                            .unwrap_err(),
                    );
                    assert_eq!(error.kind, FailureKind::Conflict);
                    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");

                    let payment_only = actor("company", &["payroll.payment.read"]);
                    let error = business(
                        payroll_summary(&payment_only, &mut reader, None, NOW)
                            .await
                            .unwrap_err(),
                    );
                    assert_eq!(error.code, "PERMISSION_REQUIRED");
                    Ok(())
                })
            })
            .await
            .unwrap();

        let rollback_actor = actor("company", &["payroll.read"]);
        let rollback = PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = rollback_actor.clone();
                Box::pin(async move {
                    sqlx::raw_sql(
                        "INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,period_start,period_end,pay_date,currency,mode,status,revision,totals_json,version,created_by,created_at,updated_at) VALUES \
                         ('run-rollback','company','group','2026-09-01','2026-09-30','2026-09-30','THB','production','posted',1,'{}',1,'user',7,7);\
                         INSERT INTO hr_payroll_items(id,run_id,engagement_id,worker_type,input_snapshot_json,result_json,gross_minor,deductions_minor,employer_cost_minor,net_minor,currency,status,created_at) VALUES \
                         ('item-rollback','run-rollback','engagement','employee','{}','{}',100,10,5,90,'THB','posted',7);",
                    )
                    .execute(&mut *connection)
                    .await?;
                    let mut reader = PgPayrollRead::new(connection);
                    let rows = payroll_summary(&current, &mut reader, Some("2026-09"), NOW).await?;
                    assert_eq!(rows.len(), 1);
                    assert_eq!(rows[0].net_minor, 90);
                    Err(PgTransactionError::Business(BusinessError::invalid(
                        "OWNER_ABORT",
                        "Rollback fixture",
                    )))
                })
            })
            .await;
        assert!(rollback.is_err());
        let disappeared: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM hr_payroll_runs WHERE id='run-rollback'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(disappeared, 0);

        let mut other = pool.raw().begin().await.unwrap();
        let error = PgPayrollRead::new(&mut other)
            .summary(&CompanyId::new("company").unwrap(), None)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref value)
                if value.code == "PAYROLL_READ_SERIALIZABLE_REQUIRED"
        ));
        other.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn in_process_work_rows_require_the_callers_serializable_transaction_and_company_scope() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;

        let mut outside = pool.raw().acquire().await.unwrap();
        let error = payroll_work_rows_on(
            &mut outside,
            &actor("company", &["payroll.read"]),
            NOW,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "PAYROLL_READ_TRANSACTION_REQUIRED");
        drop(outside);

        let mut plain = pool.raw().begin().await.unwrap();
        let error = payroll_work_rows_on(&mut plain, &actor("company", &["payroll.read"]), NOW)
            .await
            .unwrap_err();
        assert_eq!(error.code, "PAYROLL_READ_SERIALIZABLE_REQUIRED");
        plain.rollback().await.unwrap();

        let read_actor = actor("company", &["payroll.read"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = read_actor.clone();
                Box::pin(async move {
                    let rows = payroll_work_rows_on(&mut *connection, &current, NOW).await?;
                    assert_eq!(
                        rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
                        vec!["run-nov-thb", "run-oct-usd", "run-oct-thb"]
                    );
                    let serialized = serde_json::to_value(&rows[0]).unwrap();
                    assert_eq!(serialized["pay_group_id"], "group");
                    assert_eq!(serialized["created_by"], "user");
                    assert_eq!(serialized["updated_at"], 3);

                    let mut denied = current.clone();
                    denied.permissions = vec!["payroll.payment.read".into()];
                    let error = payroll_work_rows_on(&mut *connection, &denied, NOW)
                        .await
                        .unwrap_err();
                    assert_eq!(error.code, "PERMISSION_REQUIRED");
                    assert_eq!(error.kind, FailureKind::Forbidden);

                    let mut branch = current.clone();
                    branch.branch_id = Some(RecordId::new("branch").unwrap());
                    let error = payroll_work_rows_on(&mut *connection, &branch, NOW)
                        .await
                        .unwrap_err();
                    assert_eq!(error.code, "COMPANY_PAYROLL_SCOPE_REQUIRED");

                    let mut expired = current.clone();
                    expired.expires_at = NOW.get();
                    let error = payroll_work_rows_on(&mut *connection, &expired, NOW)
                        .await
                        .unwrap_err();
                    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}
