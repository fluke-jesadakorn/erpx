#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_customer_authority::public::access::AccessActor;
use erpx_server_customer_authority::public::duplicates::{
    DuplicateReviewRequest, PgCustomerDuplicateReview, duplicate_review_detail,
    duplicate_review_workspace, request_duplicate_review,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId, HoldingId, UserId};
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
        permissions: vec!["commerce.read".into(), "commerce.sales".into()],
        expires_at: NOW + 60_000,
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
