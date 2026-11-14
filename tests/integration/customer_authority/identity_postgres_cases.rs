#[allow(clippy::duplicate_mod)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_customer_authority::public::access::AccessActor;
use erpx_server_customer_authority::public::identity::{
    CustomerAddress, CustomerIdentity, IdentityChange, IdentityRequest, NotificationConsent,
    PgCustomerIdentity, change_customer_identity, identity_detail, identity_workspace,
    request_customer_identity_change,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, HoldingId, UserId,
};
use sqlx::Row;

const NOW: i64 = 1_800_000_000_000;

fn actor() -> AccessActor {
    AccessActor {
        user_id: UserId::new("requester").unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        assignment_id: AssignmentId::new("assignment-requester").unwrap(),
        role: Some("sales_manager".into()),
        authority_revision: Some(3),
        assignment_version: Some(1),
        permissions: vec![
            "commerce.read".into(),
            "commerce.sales".into(),
            "commerce.approve".into(),
        ],
        expires_at: NOW + 60_000,
    }
}

fn reviewer() -> AccessActor {
    AccessActor {
        user_id: UserId::new("reviewer").unwrap(),
        assignment_id: AssignmentId::new("assignment-reviewer").unwrap(),
        ..actor()
    }
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn proposed() -> CustomerIdentity {
    CustomerIdentity {
        name: "Reviewed customer name".into(),
        tax_registration: Some("TAX-NEW".into()),
        phone: Some("+66 2 555 0101".into()),
        address: Some(CustomerAddress {
            line1: "99 Demo Road".into(),
            line2: None,
            city: "Bangkok".into(),
            region: Some("Bangkok".into()),
            postal_code: "10110".into(),
            country_code: "TH".into(),
        }),
        notification_email: Some("billing@example.test".into()),
        notification_consent: NotificationConsent::Granted,
    }
}

fn request(customer: &str, version: i64) -> IdentityRequest {
    IdentityRequest {
        customer_id: customer.into(),
        expected_customer_version: version,
        proposed: proposed(),
        evidence: "  Customer supplied registration, contact and consent evidence  ".into(),
    }
}

fn apply(expected: i64) -> IdentityChange {
    IdentityChange {
        action: serde_json::from_value(serde_json::json!("apply")).unwrap(),
        expected_version: expected,
        expected_customer_version: None,
        proposed: None,
        reason: "Independent evidence review completed".into(),
    }
}

fn reject(expected: i64) -> IdentityChange {
    IdentityChange {
        action: serde_json::from_value(serde_json::json!("reject")).unwrap(),
        expected_version: expected,
        expected_customer_version: None,
        proposed: None,
        reason: "Tax reference evidence is incomplete".into(),
    }
}

fn resubmit(expected: i64, customer_version: i64) -> IdentityChange {
    IdentityChange {
        action: serde_json::from_value(serde_json::json!("resubmit")).unwrap(),
        expected_version: expected,
        expected_customer_version: Some(customer_version),
        proposed: Some(CustomerIdentity {
            name: "Corrected legal customer name".into(),
            tax_registration: Some("TAX-FINAL".into()),
            phone: None,
            address: None,
            notification_email: None,
            notification_consent: NotificationConsent::Revoked,
        }),
        reason: "Attached corrected simulated authority and consent evidence".into(),
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
        "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,tax_registration,phone,address_json,notification_email,notification_consent,created_at,updated_at) VALUES \
         ('customer-1','company','customer','C-001','Old customer name','THB','active',1,'TAX-OLD',NULL,'{}'::jsonb,NULL,'unknown',10,10),\
         ('customer-2','company','customer','C-002','Second customer','THB','active',1,NULL,NULL,'{}'::jsonb,NULL,'unknown',10,10);\
         INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version) VALUES \
         ('company','requester','local:requester','active','local',1),\
         ('company','reviewer','local:reviewer','active','local',1);\
         INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at) VALUES \
         ('person-requester','requester','company','Requester','TH','active','fixture',0,0),\
         ('person-reviewer','reviewer','company','Reviewer','TH','active','fixture',0,0);\
         INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,end_date,job_title,currency,created_by,created_at,updated_at) VALUES \
         ('engagement-requester','person-requester','company','employee','salaried','active','2020-01-01',NULL,'Sales Manager','THB','fixture',0,0),\
         ('engagement-reviewer','person-reviewer','company','employee','salaried','active','2020-01-01',NULL,'Sales Manager','THB','fixture',0,0);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,scope_kind,parent_id,source) VALUES \
         ('assignment-requester','requester','sales_manager','[]','company',NULL,0,9000000000000,'holding',NULL,'manual'),\
         ('assignment-reviewer','reviewer','sales_manager','[]','company',NULL,0,9000000000000,'holding',NULL,'manual');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn owner_error(
    pool: &DatabasePool,
    actor: AccessActor,
    id: &str,
    input: IdentityChange,
    command_key: &str,
) -> BusinessError {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let id = id.to_owned();
            let input = input.clone();
            let command_key = command_key.to_owned();
            Box::pin(async move {
                let mut repository = PgCustomerIdentity::new(connection);
                change_customer_identity(
                    &mut repository,
                    &actor,
                    &key(&command_key),
                    &id,
                    &input,
                    NOW,
                )
                .await
            })
        })
        .await
        .map_err(|error| match error {
            PgTransactionError::Business(error) => error,
            other => panic!("expected an owner business failure, got {other:?}"),
        })
        .unwrap_err()
}

#[tokio::test(flavor = "multi_thread")]
async fn identity_request_apply_reject_resubmit_and_evidence() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let id = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let actor = actor();
                Box::pin(async move {
                    let mut repository = PgCustomerIdentity::new(connection);
                    let result = request_customer_identity_change(
                        &mut repository,
                        &actor,
                        &key("customer-identity-req-0001"),
                        &request("customer-1", 1),
                        NOW,
                    )
                    .await?;
                    let replay = request_customer_identity_change(
                        &mut repository,
                        &actor,
                        &key("customer-identity-req-0001"),
                        &request("customer-1", 1),
                        NOW,
                    )
                    .await?;
                    assert_eq!(replay, result);
                    Ok(result)
                })
            })
            .await
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();

        let self_error = owner_error(
            &pool,
            actor(),
            &id,
            apply(1),
            "customer-identity-self-0001",
        )
        .await;
        assert_eq!(self_error.code, "INDEPENDENT_APPROVER_REQUIRED");

        let applied = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let id = id.clone();
                move |connection| {
                    let actor = reviewer();
                    let id = id.clone();
                    Box::pin(async move {
                        let mut repository = PgCustomerIdentity::new(connection);
                        change_customer_identity(
                            &mut repository,
                            &actor,
                            &key("customer-identity-apply-0001"),
                            &id,
                            &apply(1),
                            NOW,
                        )
                        .await
                    })
                }
            })
            .await
            .unwrap();
        assert_eq!(applied["status"], "applied");
        assert_eq!(applied["version"], 2);

        let customer = sqlx::query(
            "SELECT name,tax_registration,phone,address_json,notification_email,\
             notification_consent,version FROM finance_parties WHERE id='customer-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(customer.get::<String, _>("name"), "Reviewed customer name");
        assert_eq!(customer.get::<i64, _>("version"), 2);
        assert_eq!(
            customer.get::<String, _>("notification_consent"),
            "granted"
        );

        let second = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let actor = actor();
                Box::pin(async move {
                    let mut repository = PgCustomerIdentity::new(connection);
                    request_customer_identity_change(
                        &mut repository,
                        &actor,
                        &key("customer-identity-req-0002"),
                        &request("customer-2", 1),
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();

        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let second = second.clone();
                move |connection| {
                    let actor = reviewer();
                    let second = second.clone();
                    Box::pin(async move {
                        let mut repository = PgCustomerIdentity::new(connection);
                        change_customer_identity(
                            &mut repository,
                            &actor,
                            &key("customer-identity-reject-0001"),
                            &second,
                            &reject(1),
                            NOW,
                        )
                        .await
                    })
                }
            })
            .await
            .unwrap();

        let resubmitted = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let second = second.clone();
                move |connection| {
                    let actor = actor();
                    let second = second.clone();
                    Box::pin(async move {
                        let mut repository = PgCustomerIdentity::new(connection);
                        change_customer_identity(
                            &mut repository,
                            &actor,
                            &key("customer-identity-resubmit-0001"),
                            &second,
                            &resubmit(2, 1),
                            NOW,
                        )
                        .await
                    })
                }
            })
            .await
            .unwrap();
        assert_eq!(resubmitted["status"], "pending");
        assert_eq!(resubmitted["version"], 3);

        let detail = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let second = second.clone();
                move |connection| {
                    let actor = reviewer();
                    let second = second.clone();
                    Box::pin(async move {
                        let mut repository = PgCustomerIdentity::new(connection);
                        identity_detail(&mut repository, &actor, &second, NOW).await
                    })
                }
            })
            .await
            .unwrap();
        assert_eq!(detail["row"]["status"], "pending");
        assert_eq!(detail["row"]["version"], 3);
        let actions = detail["history"]
            .as_array()
            .unwrap()
            .iter()
            .map(|event| event["action"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(actions, vec!["request", "reject", "resubmit"]);

        let workspace = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let actor = reviewer();
                Box::pin(async move {
                    let mut repository = PgCustomerIdentity::new(connection);
                    identity_workspace(&mut repository, &actor, NOW).await
                })
            })
            .await
            .unwrap();
        let pending = workspace["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"].as_str() == Some(second.as_str()))
            .unwrap();
        assert_eq!(pending["canReview"], true);
        assert_eq!(pending["canResubmit"], false);

        let receipt = sqlx::query(
            "SELECT operation,result_json FROM command_receipts WHERE command_id='customer-identity-apply-0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            receipt.get::<String, _>("operation"),
            "customer.identity.apply"
        );
        assert!(receipt
            .get::<String, _>("result_json")
            .contains("\"status\":\"applied\""));

        let outbox = sqlx::query(
            "SELECT event_type,aggregate_type FROM outbox_events WHERE command_id='customer-identity-apply-0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            outbox.get::<String, _>("event_type"),
            "customer.identity.apply"
        );
        assert_eq!(
            outbox.get::<String, _>("aggregate_type"),
            "customer_identity_change"
        );

        let audit = sqlx::query(
            "SELECT action,record_type,record_version FROM audit_events WHERE command_id='customer-identity-apply-0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(audit.get::<String, _>("action"), "customer.identity.apply");
        assert_eq!(
            audit.get::<String, _>("record_type"),
            "customer_identity_change"
        );
        assert_eq!(audit.get::<i64, _>("record_version"), 2);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn identity_apply_fails_closed_when_the_customer_master_changed() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let id = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let actor = actor();
                Box::pin(async move {
                    let mut repository = PgCustomerIdentity::new(connection);
                    request_customer_identity_change(
                        &mut repository,
                        &actor,
                        &key("customer-identity-stale-0001"),
                        &request("customer-1", 1),
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        sqlx::query(
            "UPDATE finance_parties SET name='Concurrent master edit',version=version+1 WHERE id='customer-1'",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let error = owner_error(
            &pool,
            reviewer(),
            &id,
            apply(1),
            "customer-identity-stale-apply-0001",
        )
        .await;
        assert_eq!(error.code, "CUSTOMER_IDENTITY_SOURCE_CHANGED");
        let row = sqlx::query("SELECT status,version FROM customer_identity_changes WHERE id=$1")
            .bind(&id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(row.get::<String, _>("status"), "pending");
        assert_eq!(row.get::<i64, _>("version"), 1);
        let receipts = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM command_receipts WHERE command_id='customer-identity-stale-apply-0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipts, 0);
    })
    .await;
}
