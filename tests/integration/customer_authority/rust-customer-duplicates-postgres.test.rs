#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

#[allow(dead_code)]
mod identity_postgres_cases;

use erpx_server_customer_authority::public::access::AccessActor;
use erpx_server_customer_authority::public::duplicates::{
    DuplicateChange, DuplicateReviewRequest, PgCustomerDuplicateReview, change_duplicate_review,
    duplicate_review_detail, duplicate_review_workspace, request_duplicate_review,
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
        assignment_id: erpx_server_shared::public::AssignmentId::new("assignment-requester").unwrap(),
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
        branch_id: None,
        ..actor()
    }
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn input() -> DuplicateReviewRequest {
    DuplicateReviewRequest {
        left_customer_id: "customer-b".into(),
        right_customer_id: "customer-a".into(),
        evidence: "  Matching records  ".into(),
    }
}

fn change(action: &str, expected: i64, canonical: Option<&str>) -> DuplicateChange {
    DuplicateChange {
        action: serde_json::from_value(serde_json::json!(action)).unwrap(),
        expected_version: expected,
        canonical_customer_id: canonical.map(str::to_owned),
        reason: "Independent duplicate review".into(),
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
        "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at) VALUES \
         ('customer-a','company','customer','CA','Customer A','THB','active',1,10,10),\
         ('customer-b','company','customer','CB','Customer B','THB','active',1,10,10);",
    )
    // fix kind for second
    .execute(pool.raw())
    .await
    .unwrap();
    // correct second party kind to customer
    sqlx::query("UPDATE finance_parties SET kind='customer' WHERE id='customer-b'")
        .execute(pool.raw())
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version) VALUES \
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
    action: &str,
    expected: i64,
    canonical: Option<&str>,
    command_key: &str,
) -> BusinessError {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let id = id.to_owned();
            let action = action.to_owned();
            let canonical = canonical.map(str::to_owned);
            let command_key = command_key.to_owned();
            Box::pin(async move {
                let mut repository = PgCustomerDuplicateReview::new(connection);
                change_duplicate_review(
                    &mut repository,
                    &actor,
                    &key(&command_key),
                    &id,
                    &change(&action, expected, canonical.as_deref()),
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
async fn duplicate_review_request_and_read() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let key_value = "customer-duplicate-req-0001";
        let result = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let actor = actor();
                let key_value = key_value.to_owned();
                move |connection| {
                    let actor = actor.clone();
                    let key_value = key_value.clone();
                    Box::pin(async move {
                        let mut repository = PgCustomerDuplicateReview::new(connection);
                        let result = request_duplicate_review(
                            &mut repository,
                            &actor,
                            &key(&key_value),
                            &input(),
                            NOW,
                        )
                        .await?;
                        // replay
                        let replay = request_duplicate_review(
                            &mut repository,
                            &actor,
                            &key(&key_value),
                            &input(),
                            NOW,
                        )
                        .await?;
                        assert_eq!(replay, result);
                        // workspace
                        let ws = duplicate_review_workspace(&mut repository, &actor, NOW).await?;
                        let rows = ws["rows"].as_array().unwrap();
                        assert_eq!(rows.len(), 1);
                        assert_eq!(rows[0]["left_customer_id"], "customer-a");
                        assert_eq!(rows[0]["right_customer_id"], "customer-b");
                        assert_eq!(ws["canCreate"], true);
                        let id = result["id"].as_str().unwrap().to_owned();
                        let detail = duplicate_review_detail(&mut repository, &actor, &id, NOW).await?;
                        assert_eq!(detail["row"]["status"], "pending");
                        let history = detail["history"].as_array().unwrap();
                        assert_eq!(history.len(), 1);
                        assert_eq!(history[0]["action"], "request");
                        Ok(result)
                    })
                }
            })
            .await
            .unwrap();
        assert_eq!(result["status"], "pending");
        assert_eq!(result["version"], 1);
        // verify DB row
        let row = sqlx::query("SELECT status,version,left_customer_id,right_customer_id,evidence FROM customer_duplicate_reviews WHERE id=$1")
            .bind(format!("customer-duplicate-{key_value}"))
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(row.get::<String, _>("status"), "pending");
        assert_eq!(row.get::<i64, _>("version"), 1);
        assert_eq!(row.get::<String, _>("left_customer_id"), "customer-a");
        assert_eq!(row.get::<String, _>("right_customer_id"), "customer-b");
        assert_eq!(row.get::<String, _>("evidence"), "Matching records");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn duplicate_decision_reopen_and_merge_are_independent_and_fully_evidenced() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let request_key = "customer-duplicate-req-merge-0001";
        let id = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let key_value = request_key.to_owned();
                move |connection| {
                    let actor = actor();
                    let key_value = key_value.clone();
                    Box::pin(async move {
                        let mut repository = PgCustomerDuplicateReview::new(connection);
                        request_duplicate_review(
                            &mut repository,
                            &actor,
                            &key(&key_value),
                            &input(),
                            NOW,
                        )
                        .await
                    })
                }
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
            "duplicate",
            1,
            Some("customer-a"),
            "customer-duplicate-self-0001",
        )
        .await;
        assert_eq!(self_error.code, "INDEPENDENT_APPROVER_REQUIRED");

        let decided = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let id = id.clone();
                move |connection| {
                    let actor = reviewer();
                    let id = id.clone();
                    Box::pin(async move {
                        let mut repository = PgCustomerDuplicateReview::new(connection);
                        let result = change_duplicate_review(
                            &mut repository,
                            &actor,
                            &key("customer-duplicate-choice-0001"),
                            &id,
                            &change("duplicate", 1, Some("customer-a")),
                            NOW,
                        )
                        .await?;
                        let detail =
                            duplicate_review_detail(&mut repository, &actor, &id, NOW).await?;
                        assert_eq!(detail["row"]["status"], "duplicate");
                        assert_eq!(detail["row"]["canMerge"], true);
                        assert_eq!(detail["history"].as_array().unwrap().len(), 2);
                        Ok(result)
                    })
                }
            })
            .await
            .unwrap();
        assert_eq!(decided["status"], "duplicate");
        assert_eq!(decided["version"], 2);

        let reopened = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let id = id.clone();
                move |connection| {
                    let actor = reviewer();
                    let id = id.clone();
                    Box::pin(async move {
                        let mut repository = PgCustomerDuplicateReview::new(connection);
                        change_duplicate_review(
                            &mut repository,
                            &actor,
                            &key("customer-duplicate-reopen-0001"),
                            &id,
                            &change("reopen", 2, None),
                            NOW,
                        )
                        .await
                    })
                }
            })
            .await
            .unwrap();
        assert_eq!(reopened["status"], "pending");
        assert_eq!(reopened["version"], 3);

        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let id = id.clone();
                move |connection| {
                    let actor = reviewer();
                    let id = id.clone();
                    Box::pin(async move {
                        let mut repository = PgCustomerDuplicateReview::new(connection);
                        change_duplicate_review(
                            &mut repository,
                            &actor,
                            &key("customer-duplicate-choice-0002"),
                            &id,
                            &change("duplicate", 3, Some("customer-a")),
                            NOW,
                        )
                        .await
                    })
                }
            })
            .await
            .unwrap();

        let merged = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let id = id.clone();
                move |connection| {
                    let actor = reviewer();
                    let id = id.clone();
                    Box::pin(async move {
                        let mut repository = PgCustomerDuplicateReview::new(connection);
                        let result = change_duplicate_review(
                            &mut repository,
                            &actor,
                            &key("customer-duplicate-merge-0001"),
                            &id,
                            &change("merge", 4, None),
                            NOW,
                        )
                        .await?;
                        let replay = change_duplicate_review(
                            &mut repository,
                            &actor,
                            &key("customer-duplicate-merge-0001"),
                            &id,
                            &change("merge", 4, None),
                            NOW,
                        )
                        .await?;
                        assert_eq!(replay, result);
                        Ok(result)
                    })
                }
            })
            .await
            .unwrap();
        assert_eq!(merged["status"], "merged");
        assert_eq!(merged["version"], 5);
        assert_eq!(merged["canonicalCustomerId"], "customer-a");
        assert_eq!(merged["mergedCustomerId"], "customer-b");

        let review = sqlx::query(
            "SELECT status,canonical_customer_id,version FROM customer_duplicate_reviews WHERE id=$1",
        )
        .bind(&id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(review.get::<String, _>("status"), "merged");
        assert_eq!(
            review.get::<Option<String>, _>("canonical_customer_id"),
            Some("customer-a".into())
        );
        assert_eq!(review.get::<i64, _>("version"), 5);

        let loser = sqlx::query(
            "SELECT status,merged_into_id,version FROM finance_parties WHERE id='customer-b'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(loser.get::<String, _>("status"), "blocked");
        assert_eq!(
            loser.get::<Option<String>, _>("merged_into_id"),
            Some("customer-a".into())
        );
        assert_eq!(loser.get::<i64, _>("version"), 2);

        let reconciliation = sqlx::query(
            "SELECT currency,invoice_count FROM customer_duplicate_merge_reconciliations WHERE review_id=$1",
        )
        .bind(&id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(reconciliation.get::<String, _>("currency"), "THB");
        assert_eq!(reconciliation.get::<i64, _>("invoice_count"), 0);

        let receipt = sqlx::query(
            "SELECT operation,result_json FROM command_receipts WHERE command_id='customer-duplicate-merge-0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipt.get::<String, _>("operation"), "customer.duplicate.merge");
        assert!(receipt
            .get::<String, _>("result_json")
            .contains("\"mergedCustomerId\":\"customer-b\""));

        let outbox = sqlx::query(
            "SELECT event_type,aggregate_type FROM outbox_events WHERE command_id='customer-duplicate-merge-0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(outbox.get::<String, _>("event_type"), "customer.duplicate.merge");
        assert_eq!(
            outbox.get::<String, _>("aggregate_type"),
            "customer_duplicate_review"
        );

        let audit = sqlx::query(
            "SELECT action,record_type,record_version FROM audit_events WHERE command_id='customer-duplicate-merge-0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(audit.get::<String, _>("action"), "customer.duplicate.merge");
        assert_eq!(
            audit.get::<String, _>("record_type"),
            "customer_duplicate_review"
        );
        assert_eq!(audit.get::<i64, _>("record_version"), 5);

        let actions = sqlx::query(
            "SELECT action FROM customer_duplicate_events WHERE review_id=$1 ORDER BY version",
        )
        .bind(&id)
        .fetch_all(pool.raw())
        .await
        .unwrap();
        let actions = actions
            .into_iter()
            .map(|row| row.get::<String, _>("action"))
            .collect::<Vec<_>>();
        assert_eq!(
            actions,
            vec!["request", "duplicate", "reopen", "duplicate", "merge"]
        );
    })
    .await;
}
