//! Native payroll run create/calculate/post on migrated PostgreSQL.
//! Proves serializable owner SQL, receipts/audit/outbox and the fail-closed
//! Finance journal seam; it does not prove the browser journey or payslips.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_people_payroll::public::payroll_run::{
    PayrollCommandActor, PayrollJournalPort, PayrollJournalRequest, PayrollPreparedJournal,
    PgPayrollCommand, calculate_payroll_run, create_payroll_run, post_payroll,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, HoldingId, TimestampMillis,
};
use serde_json::json;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor() -> PayrollCommandActor {
    PayrollCommandActor {
        user_id: "owner".into(),
        holding_id: HoldingId::new("holding").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        assignment_id: "assignment".into(),
        role: "payroll".into(),
        permissions: vec![
            "payroll.prepare".into(),
            "payroll.read".into(),
            "finance.approve".into(),
        ],
        work_context: "a".repeat(64),
        expires_at: NOW.get() + 100_000,
    }
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
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
        "INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) \
         VALUES('chart','company','Fixture chart','THB','user',0,0);\
         INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) \
         VALUES('person','user','company','Fixture person','TH','user',0,0);\
         INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,timezone,created_by,created_at,updated_at) \
         VALUES('group','company','Monthly','TH','THB','monthly','Asia/Bangkok','user',0,0);\
         INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,pay_group_id,created_by,created_at,updated_at) \
         VALUES('engagement','person','company','employee','salaried','active','2026-01-01','Engineer','THB','group','user',0,0);\
         INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,pay_frequency,base_amount_minor,hourly_rate_minor,currency,components_json,status,created_by,created_at,updated_at) \
         VALUES('compensation','engagement','2026-01-01','monthly',1000000,20000,'THB','[]','approved','user',0,0);\
         INSERT INTO hr_time_entries(id,engagement_id,company_id,work_date,regular_hours,overtime_hours,status,version,created_at,updated_at) \
         VALUES('time','engagement','company','2026-10-01',8,0,'approved',1,0,0);\
         INSERT INTO benefit_plans(id,company_id,name,provider,coverage_type,currency,employee_deduction_minor,employer_cost_minor,effective_from,effective_to,status,created_by,reviewed_by,version,created_at,updated_at) \
         VALUES('plan','company','Health Plus','Provider','employee','THB',1500,2000,'2026-01-01',NULL,'published','user','user',1,0,0);\
         INSERT INTO benefit_enrollment_windows(id,company_id,name,opens_on,closes_on,effective_on,status,created_by,reviewed_by,version,created_at,updated_at) \
         VALUES('window','company','Annual','2026-01-01','2026-01-31','2026-01-01','closed','user','user',1,0,0);\
         INSERT INTO benefit_elections(id,company_id,person_id,engagement_id,plan_id,plan_version,window_id,window_version,coverage_type,dependent_ids_json,currency,employee_deduction_minor,employer_cost_minor,effective_from,effective_to,status,created_by,submitted_by,provider_reference,acknowledged_by,released_by,version,created_at,updated_at) \
         VALUES('election','company','person','engagement','plan',1,'window',1,'employee','[]','THB',1500,2000,'2026-01-01',NULL,'released','user','user','ref','user','user',1,0,0);\
         INSERT INTO benefit_payroll_inputs(id,election_id,company_id,person_id,engagement_id,currency,employee_deduction_minor,employer_cost_minor,effective_from,effective_to,source_election_version,status,released_by,created_at) \
         VALUES('benefit-input-1','election','company','person','engagement','THB',1500,2000,'2026-01-01',NULL,1,'released','user',0);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

struct Journal;

impl PayrollJournalPort for Journal {
    fn prepare<'a>(
        &'a mut self,
        _connection: &'a mut sqlx::PgConnection,
        request: &'a PayrollJournalRequest,
    ) -> erpx_server_people_payroll::public::payroll_run::PayrollCommandFuture<
        'a,
        PayrollPreparedJournal,
        BusinessError,
    > {
        Box::pin(async move {
            assert_eq!(request.draft.source_type, "payroll");
            assert!(!request.draft.lines.is_empty());
            Ok(PayrollPreparedJournal {
                id: "journal-1".into(),
                number: "J-1".into(),
                status: "posted".into(),
                version: 1,
                debit: "10000.00".into(),
                credit: "10000.00".into(),
                fx_rate_micros: 1_000_000,
                base_currency: "THB".into(),
            })
        })
    }
}

#[tokio::test]
async fn create_calculate_and_post_retain_receipts_audit_outbox_and_readback() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let create = json!({
            "payGroupId": "group",
            "periodStart": "2026-10-01",
            "periodEnd": "2026-10-02",
            "payDate": "2026-10-02",
            "currency": "THB",
            "mode": "preview"
        });
        let current = actor();
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let actor = current.clone();
                let create = create.clone();
                Box::pin(async move {
                    let created = {
                        let mut repository = PgPayrollCommand::new(&mut *connection, &actor);
                        create_payroll_run(
                            &actor,
                            &create,
                            &key("create-key-1"),
                            &mut repository,
                            NOW,
                        )
                        .await
                        .map_err(business)?
                    };
                    let run_id = created["id"].as_str().unwrap().to_owned();
                    let calculated = {
                        let mut repository = PgPayrollCommand::new(&mut *connection, &actor);
                        calculate_payroll_run(
                            &actor,
                            &run_id,
                            &json!({}),
                            &key("calculate-key-1"),
                            &mut repository,
                            NOW,
                        )
                        .await
                        .map_err(business)?
                    };
                    assert_eq!(calculated["status"], "preview");
                    assert_eq!(calculated["totals"]["gross"], 1_000_000);
                    assert_eq!(calculated["totals"]["deductions"], 1_500);
                    assert_eq!(calculated["totals"]["employer"], 2_000);
                    assert_eq!(calculated["totals"]["net"], 998_500);
                    let item_snapshot: String = sqlx::query_scalar(
                        "SELECT input_snapshot_json FROM hr_payroll_items WHERE run_id=$1",
                    )
                    .bind(&run_id)
                    .fetch_one(&mut *connection)
                    .await
                    .unwrap();
                    let item_snapshot: serde_json::Value =
                        serde_json::from_str(&item_snapshot).unwrap();
                    assert_eq!(
                        item_snapshot["benefitInputs"][0]["id"],
                        "benefit-input-1"
                    );
                    assert_eq!(
                        item_snapshot["benefitInputs"][0]["employee_deduction_minor"],
                        1_500
                    );
                    assert_eq!(
                        item_snapshot["benefitInputs"][0]["employer_cost_minor"],
                        2_000
                    );
                    sqlx::query(
                        "UPDATE hr_payroll_runs SET status='approved',approved_by='reviewer' WHERE id=$1",
                    )
                    .bind(&run_id)
                    .execute(&mut *connection)
                    .await
                    .unwrap();
                    sqlx::query(
                        "INSERT INTO hr_payroll_account_mappings(id,company_id,pay_group_id,chart_id,accounts_json,status,version,created_by,created_at,updated_at) \
                         VALUES('mapping','company','group','chart',$1,'approved',1,'reviewer',0,0)",
                    )
                    .bind(
                        json!({
                            "chartId": "chart",
                            "expenseAccountId": "expense",
                            "netPayableAccountId": "net",
                            "employerExpenseAccountId": "employer-expense",
                            "employerContributionAccountId": "employer-payable",
                            "socialSecurityPayableAccountId": "sso-payable",
                            "taxPayableAccountId": "tax-payable",
                            "benefitPayableAccountId": "benefit-payable"
                        })
                        .to_string(),
                    )
                    .execute(&mut *connection)
                    .await
                    .unwrap();
                    let mut journal = Journal;
                    let posted = {
                        let mut repository = PgPayrollCommand::new(&mut *connection, &actor);
                        post_payroll(
                            &actor,
                            &run_id,
                            &json!({
                                "chartId": "chart",
                                "periodId": "period",
                                "entryDate": "2026-10-02",
                                "accountMappingId": "mapping",
                                "expectedVersion": 2
                            }),
                            &key("post-key-1"),
                            &mut repository,
                            &mut journal,
                            NOW,
                        )
                        .await
                        .map_err(business)?
                    };
                    assert_eq!(posted["status"], "posted");
                    assert_eq!(posted["journalId"], "journal-1");
                    let receipts: i64 = sqlx::query_scalar(
                        "SELECT count(*) FROM command_receipts WHERE command_id IN ('create-key-1','calculate-key-1','post-key-1')",
                    )
                    .fetch_one(&mut *connection)
                    .await
                    .unwrap();
                    let audits: i64 = sqlx::query_scalar(
                        "SELECT count(*) FROM audit_events WHERE command_id IN ('create-key-1','calculate-key-1','post-key-1')",
                    )
                    .fetch_one(&mut *connection)
                    .await
                    .unwrap();
                    let outbox: i64 = sqlx::query_scalar(
                        "SELECT count(*) FROM outbox_events WHERE command_id IN ('create-key-1','calculate-key-1','post-key-1')",
                    )
                    .fetch_one(&mut *connection)
                    .await
                    .unwrap();
                    let postings: i64 = sqlx::query_scalar(
                        "SELECT count(*) FROM hr_payroll_postings WHERE run_id=$1",
                    )
                    .bind(&run_id)
                    .fetch_one(&mut *connection)
                    .await
                    .unwrap();
                    let status: String = sqlx::query_scalar(
                        "SELECT status FROM hr_payroll_runs WHERE id=$1",
                    )
                    .bind(&run_id)
                    .fetch_one(&mut *connection)
                    .await
                    .unwrap();
                    assert_eq!(receipts, 3);
                    assert_eq!(audits, 3);
                    assert_eq!(outbox, 3);
                    assert_eq!(postings, 1);
                    assert_eq!(status, "posted");
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}
