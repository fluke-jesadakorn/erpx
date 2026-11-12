//! Native People workforce commands on migrated PostgreSQL.
//!
//! Proves serializable owner SQL, command receipts/audit/outbox, person and
//! engagement version CAS, Company isolation, compensation ledgers and the
//! fail-closed Organization/Identity/Finance seams. It does not prove the
//! browser journey, staff-level history or lifecycle workflows.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_people_payroll::public::workforce::{
    CompensationMoneyPort, CompensationMoneyRequest, NoPersonAccountLinkPort, PgWorkforceCommand,
    PgWorkforceReader, StaffPlacementPort, StaffPlacementRequest, WorkforceCommandActor,
    create_compensation, create_engagement, create_person_record, person_detail, save_person,
    set_person_status, transition_compensation, transition_engagement,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, HoldingId, TimestampMillis,
};
use serde_json::json;
use sqlx::PgConnection;
use std::sync::{Arc, Mutex};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor() -> WorkforceCommandActor {
    WorkforceCommandActor {
        user_id: "owner".into(),
        holding_id: HoldingId::new("holding").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        assignment_id: "assignment".into(),
        role: "people".into(),
        permissions: vec![
            "people.manage".into(),
            "people.compensation.approve".into(),
            "people.compensation.read".into(),
        ],
        work_context: "a".repeat(64),
        expires_at: NOW.get() + 100_000,
        beta: true,
    }
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn profile(name: &str, email: &str) -> serde_json::Value {
    json!({
        "legalName": name,
        "email": email,
        "phone": "0800000000",
        "countryCode": "TH",
        "branchId": "branch",
        "jobTitle": "Engineer",
        "workerType": "employee",
        "employmentType": "salaried",
        "startDate": "2026-01-01",
        "currency": "THB",
    })
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
}

#[derive(Clone, Default)]
struct RecordingPlacement {
    log: Arc<Mutex<Vec<String>>>,
}

impl StaffPlacementPort for RecordingPlacement {
    fn apply<'a>(
        &'a mut self,
        _connection: &'a mut PgConnection,
        request: &'a StaffPlacementRequest<'a>,
    ) -> erpx_server_people_payroll::public::workforce::WorkforceCommandFuture<'a, (), BusinessError>
    {
        Box::pin(async move {
            self.log
                .lock()
                .unwrap()
                .push(format!("{}:{}", request.person_id, request.engagement_id));
            Ok(())
        })
    }
}

#[derive(Clone, Default)]
struct RecordingMoney {
    log: Arc<Mutex<Vec<(String, i64)>>>,
}

impl CompensationMoneyPort for RecordingMoney {
    fn snapshot<'a>(
        &'a mut self,
        _connection: &'a mut PgConnection,
        request: &'a CompensationMoneyRequest<'a>,
    ) -> erpx_server_people_payroll::public::workforce::WorkforceCommandFuture<'a, (), BusinessError>
    {
        Box::pin(async move {
            self.log
                .lock()
                .unwrap()
                .extend(request.amounts.iter().cloned());
            Ok(())
        })
    }
}

#[tokio::test]
async fn directory_person_create_is_atomic_and_idempotent() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let uow = PgUnitOfWork::new(pool.clone());
        let placement_log = Arc::new(Mutex::new(Vec::new()));
        let create = {
            let placement_log = placement_log.clone();
            uow.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let placement_log = placement_log.clone();
                Box::pin(async move {
                    let actor = actor();
                    let mut command = PgWorkforceCommand::new(connection, &actor);
                    let mut placement = RecordingPlacement { log: placement_log };
                    let mut link = NoPersonAccountLinkPort;
                    Ok(save_person(
                        &actor,
                        None,
                        &profile("Somchai Jaidee", "somchai@example.com"),
                        &key("person-create-1"),
                        &mut command,
                        &mut placement,
                        &mut link,
                        NOW,
                    )
                    .await?)
                })
            })
            .await
            .unwrap()
        };
        assert_eq!(create.id, "person-person-create-1");
        assert_eq!(create.version, 1);
        assert_eq!(placement_log.lock().unwrap().len(), 1);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM hr_people")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM hr_engagements WHERE status='onboarding' AND version=1"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM employees WHERE person_id='person-person-create-1'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );
        for table in ["command_receipts", "audit_events", "outbox_events"] {
            assert_eq!(
                sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!(
                    "SELECT count(*) FROM {table}"
                )))
                .fetch_one(pool.raw())
                .await
                .unwrap(),
                1,
                "{table} must have exactly one owner ledger row"
            );
        }
        // Exact replay returns the stored result and writes nothing.
        let replay = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let placement_log = placement_log.clone();
                Box::pin(async move {
                    let actor = actor();
                    let mut command = PgWorkforceCommand::new(connection, &actor);
                    let mut placement = RecordingPlacement { log: placement_log };
                    let mut link = NoPersonAccountLinkPort;
                    Ok(save_person(
                        &actor,
                        None,
                        &profile("Somchai Jaidee", "somchai@example.com"),
                        &key("person-create-1"),
                        &mut command,
                        &mut placement,
                        &mut link,
                        NOW,
                    )
                    .await?)
                })
            })
            .await
            .unwrap();
        assert_eq!(create.id, replay.id);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM hr_people")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            1
        );
        // The same key with a different request is a strict conflict.
        let error = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = actor();
                    let mut command = PgWorkforceCommand::new(connection, &actor);
                    let mut placement = RecordingPlacement::default();
                    let mut link = NoPersonAccountLinkPort;
                    save_person(
                        &actor,
                        None,
                        &profile("Another Person", "another@example.com"),
                        &key("person-create-1"),
                        &mut command,
                        &mut placement,
                        &mut link,
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap_err();
        assert_eq!(business(error).code, "IDEMPOTENCY_CONFLICT");
    })
    .await;
}

#[tokio::test]
async fn person_and_engagement_versions_are_cas_and_company_scoped() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let uow = PgUnitOfWork::new(pool.clone());
        uow.run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
            Box::pin(async move {
                let actor = actor();
                let mut command = PgWorkforceCommand::new(connection, &actor);
                let mut placement = RecordingPlacement::default();
                let mut link = NoPersonAccountLinkPort;
                save_person(
                    &actor,
                    None,
                    &profile("Somchai Jaidee", "somchai@example.com"),
                    &key("person-create-2"),
                    &mut command,
                    &mut placement,
                    &mut link,
                    NOW,
                )
                .await
            })
        })
        .await
        .unwrap();
        let updated = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = actor();
                    let mut command = PgWorkforceCommand::new(connection, &actor);
                    let mut placement = RecordingPlacement::default();
                    let mut link = NoPersonAccountLinkPort;
                    let mut raw = profile("Somchai Jaidee", "somchai@example.com");
                    raw["legalName"] = json!("Somchai J.");
                    raw["expectedVersion"] = json!(1);
                    raw["expectedEngagementVersion"] = json!(1);
                    save_person(
                        &actor,
                        Some("person-person-create-2"),
                        &raw,
                        &key("person-update-1"),
                        &mut command,
                        &mut placement,
                        &mut link,
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(updated.version, 2);
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT legal_name FROM hr_people WHERE id='person-person-create-2'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "Somchai J."
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT version FROM hr_engagements WHERE id='eng-person-create-2'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            2
        );
        let stale = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = actor();
                    let mut command = PgWorkforceCommand::new(connection, &actor);
                    let mut placement = RecordingPlacement::default();
                    let mut link = NoPersonAccountLinkPort;
                    let mut raw = profile("Somchai Jaidee", "somchai@example.com");
                    raw["expectedVersion"] = json!(1);
                    raw["expectedEngagementVersion"] = json!(1);
                    save_person(
                        &actor,
                        Some("person-person-create-2"),
                        &raw,
                        &key("person-update-2"),
                        &mut command,
                        &mut placement,
                        &mut link,
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap_err();
        assert_eq!(business(stale).code, "PERSON_CHANGED");
        let foreign = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let mut actor = actor();
                    actor.company_id = CompanyId::new("foreign").unwrap();
                    let mut command = PgWorkforceCommand::new(connection, &actor);
                    let mut placement = RecordingPlacement::default();
                    let mut link = NoPersonAccountLinkPort;
                    let mut raw = profile("Somchai Jaidee", "somchai@example.com");
                    raw["expectedVersion"] = json!(2);
                    save_person(
                        &actor,
                        Some("person-person-create-2"),
                        &raw,
                        &key("person-update-3"),
                        &mut command,
                        &mut placement,
                        &mut link,
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap_err();
        assert_eq!(business(foreign).code, "PERSON_NOT_FOUND");
    })
    .await;
}

#[tokio::test]
async fn status_and_engagement_transition_commit_together() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let uow = PgUnitOfWork::new(pool.clone());
        uow.run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
            Box::pin(async move {
                let actor = actor();
                let mut command = PgWorkforceCommand::new(connection, &actor);
                let mut placement = RecordingPlacement::default();
                let mut link = NoPersonAccountLinkPort;
                save_person(
                    &actor,
                    None,
                    &profile("Somchai Jaidee", "somchai@example.com"),
                    &key("person-create-3"),
                    &mut command,
                    &mut placement,
                    &mut link,
                    NOW,
                )
                .await
            })
        })
        .await
        .unwrap();
        uow.run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
            Box::pin(async move {
                let actor = actor();
                let mut command = PgWorkforceCommand::new(connection, &actor);
                let mut placement = RecordingPlacement::default();
                transition_engagement(
                    &actor,
                    "eng-person-create-3",
                    &json!({"status":"active","expectedVersion":1}),
                    &key("engagement-activate-1"),
                    &mut command,
                    &mut placement,
                    NOW,
                )
                .await
            })
        })
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT status FROM hr_engagements WHERE id='eng-person-create-3'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "active"
        );
        let status = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = actor();
                    let mut command = PgWorkforceCommand::new(connection, &actor);
                    let mut placement = RecordingPlacement::default();
                    let mut link = NoPersonAccountLinkPort;
                    set_person_status(
                        &actor,
                        "person-person-create-3",
                        &json!({"status":"inactive","expectedVersion":1}),
                        &key("person-status-1"),
                        &mut command,
                        &mut placement,
                        &mut link,
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(status.status, "inactive");
        assert_eq!(status.version, 2);
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT status FROM hr_engagements WHERE id='eng-person-create-3'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "suspended"
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT status FROM employees WHERE person_id='person-person-create-3'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "inactive"
        );
        let lifecycle = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let mut actor = actor();
                    actor.beta = false;
                    let mut command = PgWorkforceCommand::new(connection, &actor);
                    let mut placement = RecordingPlacement::default();
                    let mut link = NoPersonAccountLinkPort;
                    set_person_status(
                        &actor,
                        "person-person-create-3",
                        &json!({"status":"inactive","expectedVersion":2}),
                        &key("person-status-2"),
                        &mut command,
                        &mut placement,
                        &mut link,
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap_err();
        assert_eq!(business(lifecycle).code, "LIFECYCLE_REQUIRED");
    })
    .await;
}

#[tokio::test]
async fn compensation_create_approve_and_overlap_are_owner_facts() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let uow = PgUnitOfWork::new(pool.clone());
        uow.run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
            Box::pin(async move {
                let actor = actor();
                let mut command = PgWorkforceCommand::new(connection, &actor);
                let mut placement = RecordingPlacement::default();
                let mut link = NoPersonAccountLinkPort;
                save_person(
                    &actor,
                    None,
                    &profile("Somchai Jaidee", "somchai@example.com"),
                    &key("person-create-4"),
                    &mut command,
                    &mut placement,
                    &mut link,
                    NOW,
                )
                .await
            })
        })
        .await
        .unwrap();
        let money = RecordingMoney::default();
        let money_log = money.log.clone();
        let money_log = Arc::clone(&money_log);
        let created = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let money_log = Arc::clone(&money_log);
                Box::pin(async move {
                    let actor = actor();
                    let mut command = PgWorkforceCommand::new(connection, &actor);
                    let mut money = RecordingMoney { log: money_log };
                    create_compensation(
                        &actor,
                        &json!({
                            "engagementId":"eng-person-create-4",
                            "effectiveFrom":"2026-01-01",
                            "payFrequency":"monthly",
                            "currency":"THB",
                            "baseAmount":"10000.00",
                            "components":[{"code":"BONUS","label":"Bonus","kind":"earning","amount":"1000.00"}]
                        }),
                        &key("comp-create-1"),
                        &mut command,
                        &mut money,
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(created.status, "draft");
        assert_eq!(money.log.lock().unwrap().len(), 4);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT base_amount_minor FROM hr_compensation_revisions WHERE id='comp-comp-create-1'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1_000_000
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT components_json FROM hr_compensation_revisions WHERE id='comp-comp-create-1'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            r#"[{"amount":"1000.00","code":"BONUS","kind":"earning","label":"Bonus","taxable":false}]"#
        );
        let mut reviewer = actor();
        reviewer.user_id = "reviewer".into();
        let reviewer_for_approve = reviewer.clone();
        let approved = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let reviewer = reviewer_for_approve.clone();
                Box::pin(async move {
                    let mut command = PgWorkforceCommand::new(connection, &reviewer);
                    transition_compensation(
                        &reviewer,
                        "comp-comp-create-1",
                        &json!({"status":"approved","expectedVersion":1}),
                        &key("comp-approve-1"),
                        &mut command,
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(approved.status, "approved");
        assert_eq!(approved.version, 2);
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT approved_by FROM hr_compensation_revisions WHERE id='comp-comp-create-1'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "reviewer"
        );
        // A second overlapping approved revision is refused.
        uow.run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
            Box::pin(async move {
                let actor = actor();
                let mut command = PgWorkforceCommand::new(connection, &actor);
                let mut money = RecordingMoney::default();
                create_compensation(
                    &actor,
                    &json!({
                        "engagementId":"eng-person-create-4",
                        "effectiveFrom":"2026-06-01",
                        "payFrequency":"monthly",
                        "currency":"THB",
                        "baseAmount":"12000.00"
                    }),
                    &key("comp-create-2"),
                    &mut command,
                    &mut money,
                    NOW,
                )
                .await
            })
        })
        .await
        .unwrap();
        let overlap = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let reviewer = reviewer.clone();
                Box::pin(async move {
                    let mut command = PgWorkforceCommand::new(connection, &reviewer);
                    transition_compensation(
                        &reviewer,
                        "comp-comp-create-2",
                        &json!({"status":"approved","expectedVersion":1}),
                        &key("comp-approve-2"),
                        &mut command,
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap_err();
        assert_eq!(business(overlap).code, "COMPENSATION_PERIOD_OVERLAP");
    })
    .await;
}

#[tokio::test]
async fn private_person_detail_redacts_phone_for_directory_managers() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let uow = PgUnitOfWork::new(pool.clone());
        uow.run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
            Box::pin(async move {
                let actor = actor();
                let mut command = PgWorkforceCommand::new(connection, &actor);
                let mut placement = RecordingPlacement::default();
                let mut link = NoPersonAccountLinkPort;
                save_person(
                    &actor,
                    None,
                    &profile("Somchai Jaidee", "somchai@example.com"),
                    &key("person-create-5"),
                    &mut command,
                    &mut placement,
                    &mut link,
                    NOW,
                )
                .await
            })
        })
        .await
        .unwrap();
        let manager = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let mut actor = actor();
                    actor.permissions = vec!["people.manage".into()];
                    let mut command = PgWorkforceCommand::new(connection, &actor);
                    Ok(
                        person_detail(&actor, "person-person-create-5", None, &mut command, NOW)
                            .await?,
                    )
                })
            })
            .await
            .unwrap();
        assert!(!manager.private_access);
        assert!(manager.phone.is_none());
        assert!(manager.compensation.is_none());
        let private = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let mut actor = actor();
                    actor.user_id = "other-reader".into();
                    actor.permissions = vec!["people.compensation.read".into()];
                    let mut command = PgWorkforceCommand::new(connection, &actor);
                    Ok(
                        person_detail(&actor, "person-person-create-5", None, &mut command, NOW)
                            .await?,
                    )
                })
            })
            .await
            .unwrap();
        assert!(private.private_access);
        assert_eq!(private.phone.as_deref(), Some("0800000000"));
        let reader = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let mut reader = PgWorkforceReader::new(connection);
                    Ok(reader
                        .person(
                            &CompanyId::new("company").unwrap(),
                            "person-person-create-5",
                            None,
                        )
                        .await?)
                })
            })
            .await
            .unwrap();
        assert_eq!(reader.unwrap().legal_name, "Somchai Jaidee");
    })
    .await;
}

#[tokio::test]
async fn standalone_person_receives_its_first_engagement() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let uow = PgUnitOfWork::new(pool.clone());
        uow.run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
            Box::pin(async move {
                let actor = actor();
                let mut command = PgWorkforceCommand::new(connection, &actor);
                create_person_record(
                    &actor,
                    &json!({
                        "legalName":"Somsri Jaidee",
                        "email":"somsri@example.com",
                        "countryCode":"TH"
                    }),
                    &key("person-record-1"),
                    &mut command,
                    NOW,
                )
                .await
            })
        })
        .await
        .unwrap();
        let engagement = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = actor();
                    let mut command = PgWorkforceCommand::new(connection, &actor);
                    create_engagement(
                        &actor,
                        &json!({
                            "personId":"person-person-record-1",
                            "branchId":"branch",
                            "workerType":"employee",
                            "employmentType":"salaried",
                            "startDate":"2026-02-01",
                            "jobTitle":"Analyst",
                            "currency":"THB"
                        }),
                        &key("engagement-create-1"),
                        &mut command,
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(engagement.status, "onboarding");
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT person_id FROM hr_engagements WHERE id='eng-engagement-create-1'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "person-person-record-1"
        );
    })
    .await;
}
