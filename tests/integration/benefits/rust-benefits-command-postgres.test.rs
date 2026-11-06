//! Native Benefits commands over real migrated PostgreSQL. These prove owner
//! SQL, Company scope, idempotency, CAS, independent review, payroll handoff,
//! receipt/audit/outbox readback and rollback. They are not a browser journey.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_benefits::public::command::{
    BenefitsCommandActor, PgBenefitsCommand, change, create_dependent, create_election,
    create_plan, create_window,
};
use erpx_server_benefits::public::workspace::{
    BenefitsEntitlement, BenefitsEntitlementPort, BenefitsFuture, BenefitsPeoplePort,
    BenefitsWorker,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId, TimestampMillis};
use serde_json::{Value, json};

const NOW: TimestampMillis = TimestampMillis::new(1_790_812_800_000);

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

fn actor(user: &str, company: &str, review: bool) -> BenefitsCommandActor {
    BenefitsCommandActor {
        user_id: user.into(),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: None,
        department_id: None,
        assignment_id: format!("assignment-{user}"),
        role: "benefits_administrator".into(),
        permissions: vec![
            "records.read".into(),
            "records.write".into(),
            if review {
                "benefits.review".into()
            } else {
                "benefits.read".into()
            },
        ],
        modules: vec!["benefits".into()],
        work_context: "b".repeat(64),
        expires_at: NOW.get() + 60_000,
    }
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

#[derive(Clone)]
struct People {
    worker: Option<BenefitsWorker>,
}

impl BenefitsPeoplePort for People {
    type Error = BusinessError;
    fn worker<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> BenefitsFuture<'a, Option<BenefitsWorker>, Self::Error> {
        Box::pin(async move { Ok(self.worker.clone()) })
    }
    fn employee_names<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a [String],
    ) -> BenefitsFuture<'a, Vec<(String, String)>, Self::Error> {
        Box::pin(async { Ok(Vec::new()) })
    }
}

struct Packages;

impl BenefitsEntitlementPort for Packages {
    type Error = BusinessError;
    fn entitlement<'a>(
        &'a mut self,
        _: &'a str,
    ) -> BenefitsFuture<'a, BenefitsEntitlement, Self::Error> {
        Box::pin(async {
            Ok(BenefitsEntitlement {
                enforced: true,
                history_allowed: true,
                new_work: true,
                settle_existing: true,
                revision: "1".into(),
            })
        })
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
        "INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,version,created_by,created_at,updated_at) VALUES \
         ('person-owner','owner','company','Owner Worker','TH','active',1,'owner',10,10),\
         ('person-other','other','company','Other Worker','TH','active',1,'owner',10,10),\
         ('person-foreign','foreign-owner','foreign','Foreign Worker','TH','active',1,'owner',10,10);\
         INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,version,created_by,created_at,updated_at) VALUES \
         ('engagement-owner','person-owner','company','branch','department','employee','salaried','active','2024-01-01','Engineer','THB',1,'owner',10,10),\
         ('engagement-other','person-other','company','branch',NULL,'employee','salaried','active','2024-01-01','Designer','THB',1,'owner',10,10),\
         ('engagement-foreign','person-foreign','foreign','foreign-branch',NULL,'employee','salaried','active','2024-01-01','Operator','THB',1,'owner',10,10);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn plan_input() -> Value {
    json!({
        "entity": "plan",
        "name": "Health Plus",
        "provider": "Synthetic Provider",
        "coverageType": "family",
        "currency": "thb",
        "employeeDeduction": "125.50",
        "employerCost": "300.00",
        "effectiveFrom": "2026-09-01",
        "effectiveTo": null,
    })
}

fn window_input() -> Value {
    json!({
        "entity": "window",
        "name": "October enrollment",
        "opensOn": "2026-09-01",
        "closesOn": "2026-10-31",
        "effectiveOn": "2026-11-01",
    })
}

fn dependent_input() -> Value {
    json!({
        "entity": "dependent",
        "name": "Family Member",
        "relationship": "child",
        "birthDate": "2018-02-03",
    })
}

#[tokio::test]
async fn commands_persist_scoped_rows_receipts_audit_cas_and_payroll_handoff() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let owner = actor("owner", "company", true);
        let reviewer = actor("reviewer", "company", true);
        let releaser = actor("releaser", "company", true);
        let people = People {
            worker: Some(BenefitsWorker {
                person_id: "person-owner".into(),
                engagement_id: "engagement-owner".into(),
            }),
        };
        let plan_key = key("plan-command-1");
        let plan_id = "benefit-plan-plan-command-1";
        let plan_input = plan_input();
        let window_key = key("window-command-1");
        let window_id = "benefit-window-window-command-1";
        let window_input = window_input();
        let dependent_key = key("dependent-command-1");
        let dependent_id = "benefit-dependent-dependent-command-1";
        let dependent_input = dependent_input();
        let election_key = key("election-command-1");
        let election_id = "benefit-election-election-command-1";

        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), {
                let owner = owner.clone();
                let reviewer = reviewer.clone();
                let releaser = releaser.clone();
                let plan_input = plan_input.clone();
                let window_input = window_input.clone();
                let dependent_input = dependent_input.clone();
                let plan_key = plan_key.clone();
                let window_key = window_key.clone();
                let dependent_key = dependent_key.clone();
                let election_key = election_key.clone();
                move |connection| {
                    let owner = owner.clone();
                    let reviewer = reviewer.clone();
                    let releaser = releaser.clone();
                    let plan_input = plan_input.clone();
                    let window_input = window_input.clone();
                    let dependent_input = dependent_input.clone();
                    let plan_key = plan_key.clone();
                    let window_key = window_key.clone();
                    let dependent_key = dependent_key.clone();
                    let election_key = election_key.clone();
                    let mut people = people.clone();
                    Box::pin(async move {
                    let mut packages = Packages;
                    let mut command = PgBenefitsCommand::new(connection, &owner);
                    let plan = create_plan(
                        &owner,
                        &plan_input,
                        &plan_key,
                        &mut command,
                        &mut packages,
                        NOW,
                    )
                    .await?;
                    assert_eq!(plan.id, plan_id);
                    assert_eq!(plan.status, "draft");
                    assert_eq!(plan.version, 1);
                    let replay = create_plan(
                        &owner,
                        &plan_input,
                        &plan_key,
                        &mut command,
                        &mut packages,
                        NOW,
                    )
                    .await?;
                    assert_eq!(replay, plan);
                    drop(command);

                    let mut command = PgBenefitsCommand::new(connection, &reviewer);
                    let published = change(
                        &reviewer,
                        erpx_server_benefits::public::command::BenefitRecordKind::Plan,
                        plan_id,
                        &json!({
                            "entity": "plan",
                            "id": plan_id,
                            "action": "publish",
                            "expectedVersion": 1,
                            "reason": "Independent plan terms review",
                        }),
                        &key("plan-publish-1"),
                        &mut command,
                        &mut people,
                        &mut packages,
                        NOW,
                    )
                    .await?;
                    assert_eq!(published.status, "published");
                    assert_eq!(published.version, 2);
                    drop(command);

                    let mut command = PgBenefitsCommand::new(connection, &owner);
                    let window = create_window(
                        &owner,
                        &window_input,
                        &window_key,
                        &mut command,
                        &mut packages,
                        NOW,
                    )
                    .await?;
                    assert_eq!(window.id, window_id);
                    drop(command);

                    let mut command = PgBenefitsCommand::new(connection, &reviewer);
                    let opened = change(
                        &reviewer,
                        erpx_server_benefits::public::command::BenefitRecordKind::Window,
                        window_id,
                        &json!({
                            "entity": "window",
                            "id": window_id,
                            "action": "open",
                            "expectedVersion": 1,
                            "reason": "Independent window dates review",
                        }),
                        &key("window-open-1"),
                        &mut command,
                        &mut people,
                        &mut packages,
                        NOW,
                    )
                    .await?;
                    assert_eq!(opened.status, "open");
                    assert_eq!(opened.version, 2);
                    drop(command);

                    let mut command = PgBenefitsCommand::new(connection, &owner);
                    let dependent = create_dependent(
                        &owner,
                        &dependent_input,
                        &dependent_key,
                        &mut command,
                        &mut people,
                        &mut packages,
                        NOW,
                    )
                    .await?;
                    assert_eq!(dependent.id, dependent_id);
                    assert_eq!(dependent.status, "pending");
                    drop(command);

                    let mut command = PgBenefitsCommand::new(connection, &reviewer);
                    let verified = change(
                        &reviewer,
                        erpx_server_benefits::public::command::BenefitRecordKind::Dependent,
                        dependent_id,
                        &json!({
                            "entity": "dependent",
                            "id": dependent_id,
                            "action": "verify",
                            "expectedVersion": 1,
                            "reason": "Dependent identity evidence reviewed",
                        }),
                        &key("dependent-verify-1"),
                        &mut command,
                        &mut people,
                        &mut packages,
                        NOW,
                    )
                    .await?;
                    assert_eq!(verified.status, "verified");
                    assert_eq!(verified.version, 2);
                    drop(command);

                    let mut command = PgBenefitsCommand::new(connection, &owner);
                    let election = create_election(
                        &owner,
                        &json!({
                            "entity": "election",
                            "planId": plan_id,
                            "planVersion": 2,
                            "windowId": window_id,
                            "windowVersion": 2,
                            "dependentIds": [dependent_id],
                        }),
                        &election_key,
                        &mut command,
                        &mut people,
                        &mut packages,
                        NOW,
                    )
                    .await?;
                    assert_eq!(election.id, election_id);
                    assert_eq!(election.status, "elected");
                    let submitted = change(
                        &owner,
                        erpx_server_benefits::public::command::BenefitRecordKind::Election,
                        election_id,
                        &json!({
                            "entity": "election",
                            "id": election_id,
                            "action": "submit",
                            "expectedVersion": 1,
                            "reason": "Employee confirms own coverage",
                        }),
                        &key("election-submit-1"),
                        &mut command,
                        &mut people,
                        &mut packages,
                        NOW,
                    )
                    .await?;
                    assert_eq!(submitted.status, "submitted");
                    assert_eq!(submitted.version, 2);
                    drop(command);

                    let mut command = PgBenefitsCommand::new(connection, &reviewer);
                    let acknowledged = change(
                        &reviewer,
                        erpx_server_benefits::public::command::BenefitRecordKind::Election,
                        election_id,
                        &json!({
                            "entity": "election",
                            "id": election_id,
                            "action": "acknowledge",
                            "expectedVersion": 2,
                            "providerReference": "SIM-ACK-1001",
                            "reason": "Provider accepted enrollment",
                        }),
                        &key("election-acknowledge-1"),
                        &mut command,
                        &mut people,
                        &mut packages,
                        NOW,
                    )
                    .await?;
                    assert_eq!(acknowledged.status, "acknowledged");
                    assert_eq!(acknowledged.version, 3);
                    drop(command);

                    let mut command = PgBenefitsCommand::new(connection, &releaser);
                    let release_key = key("election-release-1");
                    let release_input = json!({
                        "entity": "election",
                        "id": election_id,
                        "action": "release",
                        "expectedVersion": 3,
                        "reason": "Payroll amount and provider receipt reconciled",
                    });
                    let released = change(
                        &releaser,
                        erpx_server_benefits::public::command::BenefitRecordKind::Election,
                        election_id,
                        &release_input,
                        &release_key,
                        &mut command,
                        &mut people,
                        &mut packages,
                        NOW,
                    )
                    .await?;
                    assert_eq!(released.status, "released");
                    assert_eq!(released.version, 4);
                    assert_eq!(
                        change(
                            &releaser,
                            erpx_server_benefits::public::command::BenefitRecordKind::Election,
                            election_id,
                            &release_input,
                            &release_key,
                            &mut command,
                            &mut people,
                            &mut packages,
                            NOW,
                        )
                        .await?,
                        released
                    );
                    let changed_release = change(
                        &releaser,
                        erpx_server_benefits::public::command::BenefitRecordKind::Election,
                        election_id,
                        &json!({
                            "entity": "election",
                            "id": election_id,
                            "action": "release",
                            "expectedVersion": 3,
                            "reason": "Changed release request must not replay",
                        }),
                        &release_key,
                        &mut command,
                        &mut people,
                        &mut packages,
                        NOW,
                    )
                    .await
                    .unwrap_err();
                    assert_eq!(business(changed_release).code, "IDEMPOTENCY_CONFLICT");
                    let stale_plan = change(
                        &releaser,
                        erpx_server_benefits::public::command::BenefitRecordKind::Plan,
                        plan_id,
                        &json!({
                            "entity": "plan",
                            "id": plan_id,
                            "action": "retire",
                            "expectedVersion": 1,
                            "reason": "Stale plan version must fail",
                        }),
                        &key("plan-stale-1"),
                        &mut command,
                        &mut people,
                        &mut packages,
                        NOW,
                    )
                    .await
                    .unwrap_err();
                    assert_eq!(business(stale_plan).code, "CONFLICT");
                    Ok(())
                })
                }
            })
            .await
            .unwrap();

        let plan = sqlx::query(
            "SELECT name,currency,employee_deduction_minor,employer_cost_minor,status,version,reviewed_by \
             FROM benefit_plans WHERE id=$1",
        )
        .bind("benefit-plan-plan-command-1")
        .fetch_one(pool.raw())
        .await
        .unwrap();
        use sqlx::Row;
        assert_eq!(plan.get::<String, _>("name"), "Health Plus");
        assert_eq!(plan.get::<String, _>("currency"), "THB");
        assert_eq!(plan.get::<i64, _>("employee_deduction_minor"), 12_550);
        assert_eq!(plan.get::<i64, _>("employer_cost_minor"), 30_000);
        assert_eq!(plan.get::<String, _>("status"), "published");
        assert_eq!(plan.get::<i64, _>("version"), 2);
        assert_eq!(plan.get::<String, _>("reviewed_by"), "reviewer");

        let election = sqlx::query(
            "SELECT coverage_type,dependent_ids_json,employee_deduction_minor,employer_cost_minor,\
             status,version,provider_reference,submitted_by,acknowledged_by,released_by \
             FROM benefit_elections WHERE id=$1",
        )
        .bind("benefit-election-election-command-1")
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(election.get::<String, _>("coverage_type"), "family");
        assert_eq!(
            election.get::<String, _>("dependent_ids_json"),
            "[\"benefit-dependent-dependent-command-1\"]"
        );
        assert_eq!(election.get::<i64, _>("employee_deduction_minor"), 12_550);
        assert_eq!(election.get::<i64, _>("employer_cost_minor"), 30_000);
        assert_eq!(election.get::<String, _>("status"), "released");
        assert_eq!(election.get::<i64, _>("version"), 4);
        assert_eq!(election.get::<String, _>("provider_reference"), "SIM-ACK-1001");
        assert_eq!(election.get::<String, _>("submitted_by"), "owner");
        assert_eq!(election.get::<String, _>("acknowledged_by"), "reviewer");
        assert_eq!(election.get::<String, _>("released_by"), "releaser");

        let payroll = sqlx::query(
            "SELECT employee_deduction_minor,employer_cost_minor,source_election_version,status,released_by \
             FROM benefit_payroll_inputs WHERE election_id=$1",
        )
        .bind("benefit-election-election-command-1")
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(payroll.get::<i64, _>("employee_deduction_minor"), 12_550);
        assert_eq!(payroll.get::<i64, _>("employer_cost_minor"), 30_000);
        assert_eq!(payroll.get::<i64, _>("source_election_version"), 4);
        assert_eq!(payroll.get::<String, _>("status"), "released");
        assert_eq!(payroll.get::<String, _>("released_by"), "releaser");

        let counts = sqlx::query_as::<_, (i64, i64, i64, i64)>(
            "SELECT (SELECT count(*) FROM command_receipts),(SELECT count(*) FROM audit_events),\
             (SELECT count(*) FROM outbox_events),(SELECT count(*) FROM benefit_plan_events)",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts.0, 10);
        assert_eq!(counts.1, 10);
        assert_eq!(counts.2, 10);
        assert_eq!(counts.3, 2);

        let events = sqlx::query_scalar::<_, String>(
            "SELECT action FROM benefit_election_events WHERE election_id=$1 ORDER BY version",
        )
        .bind("benefit-election-election-command-1")
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(events, vec!["elect", "submit", "acknowledge", "release"]);
    })
    .await;
}

#[tokio::test]
async fn foreign_company_and_reviewer_self_review_are_rejected_before_owner_effects() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = actor("owner", "company", true);
        let foreign = actor("foreign-owner", "foreign", true);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), {
                let owner = owner.clone();
                let foreign = foreign.clone();
                move |connection| {
                    let owner = owner.clone();
                    let foreign = foreign.clone();
                    Box::pin(async move {
                    let mut packages = Packages;
                    let mut people = People {
                        worker: Some(BenefitsWorker {
                            person_id: "person-owner".into(),
                            engagement_id: "engagement-owner".into(),
                        }),
                    };
                    let mut command = PgBenefitsCommand::new(connection, &owner);
                    let plan = create_plan(
                        &owner,
                        &plan_input(),
                        &key("plan-command-foreign-1"),
                        &mut command,
                        &mut packages,
                        NOW,
                    )
                    .await?;
                    let own_publish = change(
                        &owner,
                        erpx_server_benefits::public::command::BenefitRecordKind::Plan,
                        &plan.id,
                        &json!({
                            "entity": "plan",
                            "id": plan.id,
                            "action": "publish",
                            "expectedVersion": 1,
                            "reason": "Self review must fail",
                        }),
                        &key("plan-publish-self-1"),
                        &mut command,
                        &mut people,
                        &mut packages,
                        NOW,
                    )
                    .await
                    .unwrap_err();
                    assert_eq!(business(own_publish).code, "CONFLICT");
                    drop(command);

                    let mut foreign_command = PgBenefitsCommand::new(connection, &foreign);
                    let foreign_read = change(
                        &foreign,
                        erpx_server_benefits::public::command::BenefitRecordKind::Plan,
                        &plan.id,
                        &json!({
                            "entity": "plan",
                            "id": plan.id,
                            "action": "publish",
                            "expectedVersion": 1,
                            "reason": "Foreign company must not see this row",
                        }),
                        &key("plan-publish-foreign-1"),
                        &mut foreign_command,
                        &mut people,
                        &mut packages,
                        NOW,
                    )
                    .await
                    .unwrap_err();
                    assert_eq!(business(foreign_read).code, "BENEFIT_RECORD_NOT_FOUND");
                    Ok(())
                })
                }
            })
            .await
            .unwrap();

        let count = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM benefit_plans WHERE id='benefit-plan-plan-command-foreign-1' AND company_id='company'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(count, 1);
        let foreign_count = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM benefit_plans WHERE company_id='foreign'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(foreign_count, 0);
    })
    .await;
}

#[tokio::test]
async fn failed_plan_event_insert_rolls_back_row_receipt_audit_and_outbox() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::query(
            "INSERT INTO benefit_plans(id,company_id,name,provider,coverage_type,currency,\
             employee_deduction_minor,employer_cost_minor,effective_from,status,created_by,\
             version,created_at,updated_at) VALUES('blocker-plan','company','Blocker','Provider',\
             'employee','THB',0,0,'2026-01-01','draft','owner',1,1,1)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO benefit_plan_events(id,plan_id,version,action,actor_id,reason,status,created_at) \
             VALUES('benefit-plan-event-plan-command-rollback-1','blocker-plan',1,'create','owner','Blocker','draft',1)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let owner = actor("owner", "company", true);
        let failed = PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), {
                let owner = owner.clone();
                move |connection| {
                    let owner = owner.clone();
                    Box::pin(async move {
                    let mut packages = Packages;
                    let mut command = PgBenefitsCommand::new(connection, &owner);
                    create_plan(
                        &owner,
                        &plan_input(),
                        &key("plan-command-rollback-1"),
                        &mut command,
                        &mut packages,
                        NOW,
                    )
                    .await?;
                    Ok(())
                })
                }
            })
            .await;
        assert!(failed.is_err());
        let plan = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM benefit_plans WHERE id='benefit-plan-plan-command-rollback-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        let receipt = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM command_receipts WHERE command_id='plan-command-rollback-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        let audit = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_events WHERE command_id='plan-command-rollback-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        let outbox = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM outbox_events WHERE command_id='plan-command-rollback-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!((plan, receipt, audit, outbox), (0, 0, 0, 0));
    })
    .await;
}
