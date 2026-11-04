//! Native Payroll summary and run-list reads on migrated PostgreSQL.
//! These tests prove owner SQL, Company scope, persisted-source readback and
//! rollback; they do not prove a command, purchase or browser journey.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_people_payroll::public::payroll_summary::{
    PayrollReadActor, PgPayrollRead, payroll_runs, payroll_summary,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

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
