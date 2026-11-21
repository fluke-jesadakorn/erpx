//! Native Manufacturing specification commands on migrated PostgreSQL.
//!
//! Proves owner SQL, serializable transaction guards, current IAM authority,
//! independent review/CAS, persisted receipt/audit/outbox evidence, Company
//! isolation, replay and rollback. It does not prove the product route merge or
//! browser journey.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_manufacturing::public::bom::BomActor;
use erpx_server_manufacturing::public::specification::{
    APPROVE_OPERATION, CREATE_OPERATION, CreateRequest, PgSpecificationCommands, ReviewAction,
    ReviewRequest, SpecificationCommandRepository, create_specification, review_specification,
};
use erpx_server_runtime::public::http_support::serde_json;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId, RecordId, TimestampMillis};
use serde_json::Value;
use sqlx::{AssertSqlSafe, Connection, Row};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor(company: &str, user: &str) -> BomActor {
    BomActor {
        user_id: user.into(),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: Some(RecordId::new("branch").unwrap()),
        department_id: None,
        permissions: vec![
            "records.read".into(),
            "records.write".into(),
            "records.review".into(),
        ],
        modules: vec!["manufacturing".into()],
        expires_at: NOW.get() + 60_000,
    }
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn request() -> CreateRequest {
    serde_json::from_value(serde_json::json!({
        "name": "Bounded assembly",
        "mode": "bom",
        "outputProductId": "output",
        "batchQuantity": "10",
        "expectedYieldBps": 9800,
        "lines": [
            {"productId": "material-a", "quantity": "1.5", "substitutionGroup": "secondary"},
            {"productId": "material-b", "quantity": "2.5"}
        ],
        "operations": [
            {"resourceId": "resource", "name": "Assemble", "setupMinutes": 5, "runMinutesPerBatch": 30}
        ]
    }))
    .unwrap()
}

fn review(action: ReviewAction, expected_version: i64) -> ReviewRequest {
    serde_json::from_value(serde_json::json!({
        "action": action,
        "expectedVersion": expected_version,
        "reason": "Independent review of the current specification."
    }))
    .unwrap()
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
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES \
         ('company','company','company','Company',NULL,'company',NULL,'active',1),\
         ('branch','branch','branch','Branch',NULL,'company',NULL,'active',1),\
         ('other','branch','other','Other branch',NULL,'company',NULL,'active',1),\
         ('foreign','company','foreign','Foreign',NULL,'foreign',NULL,'active',1),\
         ('foreign-branch','branch','foreign-branch','Foreign branch',NULL,'foreign',NULL,'active',1);\
         INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version) VALUES \
         ('company','maker','local:maker','active','local',1),\
         ('company','reviewer','local:reviewer','active','local',1),\
         ('company','revoked','local:revoked','active','local',1),\
         ('foreign','foreign-maker','local:foreign-maker','active','local',1);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,scope_kind,scope_node_id,source) VALUES \
         ('assignment-maker','maker','holding_owner','[]','company','branch',0,9000000000000,NULL,'holding','company','manual'),\
         ('assignment-reviewer','reviewer','holding_owner','[]','company','branch',0,9000000000000,NULL,'holding','company','manual'),\
         ('assignment-revoked','revoked','holding_owner','[]','company','branch',0,9000000000000,1,'holding','company','manual'),\
         ('assignment-foreign','foreign-maker','holding_owner','[]','foreign','foreign-branch',0,9000000000000,NULL,'holding','foreign','manual');\
         INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status,version) VALUES \
         ('output','company','OUT','Bounded output','each',1000,0,'active',1),\
         ('material-a','company','MA','Material A','each',100,0,'active',1),\
         ('material-b','company','MB','Material B','each',250,0,'active',1),\
         ('inactive-output','company','IOUT','Inactive output','each',1000,0,'archived',1),\
         ('inactive-material','company','IMA','Inactive material','each',100,0,'archived',1),\
         ('foreign-output','foreign','FOUT','Foreign output','each',1000,0,'active',1),\
         ('foreign-material','foreign','FMA','Foreign material','each',100,0,'active',1);\
         INSERT INTO production_resources(id,company_id,branch_id,code,name,daily_minutes,work_weekdays_json,status,version,created_by,created_at,updated_at) VALUES \
         ('resource','company','branch','R1','Assembly line',480,'[1,2,3,4,5]','active',1,'maker',10,10),\
         ('resource-other','company','other','R2','Other line',480,'[1,2,3,4,5]','active',1,'maker',10,10),\
         ('inactive-resource','company','branch','R3','Inactive line',480,'[1,2,3,4,5]','archived',1,'maker',10,10),\
         ('foreign-resource','foreign','foreign-branch','R1','Foreign line',480,'[1,2,3,4,5]','active',1,'foreign-maker',10,10);\
         INSERT INTO manufacturing_specifications(id,company_id,name,mode,output_product_id,batch_quantity_milli,expected_yield_bps,status,created_by,reviewed_by,version,created_at,updated_at) VALUES \
         ('spec-draft','company','Seed draft','bom','output',1000,9800,'draft','maker',NULL,1,100,100),\
         ('spec-foreign','foreign','Foreign draft','bom','foreign-output',1000,9800,'draft','foreign-maker',NULL,1,100,100);\
         INSERT INTO manufacturing_specification_lines(id,specification_id,product_id,quantity_milli,substitution_group,ordinal) VALUES \
         ('line-draft','spec-draft','material-a',1000,NULL,0),\
         ('line-foreign','spec-foreign','foreign-material',1000,NULL,0);\
         INSERT INTO manufacturing_specification_operations(id,specification_id,resource_id,name,setup_minutes,run_minutes_per_batch,ordinal) VALUES \
         ('operation-draft','spec-draft','resource','Cut',0,10,0),\
         ('operation-foreign','spec-foreign','foreign-resource','Foreign cut',0,10,0);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn create(
    pool: &DatabasePool,
    company: &str,
    user: &str,
    key_value: &str,
    request: CreateRequest,
) -> Result<Value, PgTransactionError<BusinessError>> {
    let key = key(key_value);
    let actor = actor(company, user);
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let key = key.clone();
            let actor = actor.clone();
            let request = request.clone();
            Box::pin(async move {
                let mut repository = PgSpecificationCommands::new(connection);
                create_specification(&mut repository, &actor, &key, request, NOW).await
            })
        })
        .await
}

async fn review_command(
    pool: &DatabasePool,
    company: &str,
    user: &str,
    key_value: &str,
    specification_id: &str,
    request: ReviewRequest,
) -> Result<Value, PgTransactionError<BusinessError>> {
    let key = key(key_value);
    let actor = actor(company, user);
    let specification_id = specification_id.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let key = key.clone();
            let actor = actor.clone();
            let specification_id = specification_id.clone();
            let request = request.clone();
            Box::pin(async move {
                let mut repository = PgSpecificationCommands::new(connection);
                review_specification(
                    &mut repository,
                    &actor,
                    &key,
                    &specification_id,
                    request,
                    NOW,
                )
                .await
            })
        })
        .await
}

async fn count(pool: &DatabasePool, query: &str) -> i64 {
    sqlx::query_scalar(AssertSqlSafe(query.to_owned()))
        .fetch_one(pool.raw())
        .await
        .unwrap()
}

async fn count_keyed(pool: &DatabasePool, query: &str, key: &str) -> i64 {
    sqlx::query_scalar(AssertSqlSafe(query.to_owned()))
        .bind(key)
        .fetch_one(pool.raw())
        .await
        .unwrap()
}

#[tokio::test]
async fn creates_drafts_with_ordered_components_routing_and_receipts() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let result = create(
            &pool,
            "company",
            "maker",
            "manufacturing-key-0001",
            request(),
        )
        .await
        .unwrap();
        assert_eq!(result["id"], "mfg-spec-manufacturing-key-0001");
        assert_eq!(result["status"], "draft");
        assert_eq!(result["version"], 1);

        let row = sqlx::query(
            "SELECT company_id,name,mode,output_product_id,batch_quantity_milli,\
             expected_yield_bps,status,created_by,reviewed_by,version \
             FROM manufacturing_specifications WHERE id='mfg-spec-manufacturing-key-0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("company_id"), "company");
        assert_eq!(row.get::<String, _>("name"), "Bounded assembly");
        assert_eq!(row.get::<String, _>("mode"), "bom");
        assert_eq!(row.get::<String, _>("output_product_id"), "output");
        assert_eq!(row.get::<i64, _>("batch_quantity_milli"), 10_000);
        assert_eq!(row.get::<i64, _>("expected_yield_bps"), 9_800);
        assert_eq!(row.get::<String, _>("status"), "draft");
        assert_eq!(row.get::<String, _>("created_by"), "maker");
        assert_eq!(row.get::<Option<String>, _>("reviewed_by"), None);
        assert_eq!(row.get::<i64, _>("version"), 1);

        let lines = sqlx::query(
            "SELECT product_id,quantity_milli,substitution_group,ordinal \
             FROM manufacturing_specification_lines \
             WHERE specification_id='mfg-spec-manufacturing-key-0001' ORDER BY ordinal",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].get::<String, _>("product_id"), "material-a");
        assert_eq!(lines[0].get::<i64, _>("quantity_milli"), 1_500);
        assert_eq!(
            lines[0].get::<Option<String>, _>("substitution_group"),
            Some("secondary".into())
        );
        assert_eq!(lines[0].get::<i64, _>("ordinal"), 0);
        assert_eq!(lines[1].get::<String, _>("product_id"), "material-b");
        assert_eq!(lines[1].get::<i64, _>("ordinal"), 1);

        let operations = sqlx::query(
            "SELECT resource_id,name,setup_minutes,run_minutes_per_batch,ordinal \
             FROM manufacturing_specification_operations \
             WHERE specification_id='mfg-spec-manufacturing-key-0001'",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(operations.len(), 1);
        assert_eq!(operations[0].get::<String, _>("resource_id"), "resource");
        assert_eq!(operations[0].get::<i64, _>("setup_minutes"), 5);
        assert_eq!(operations[0].get::<i64, _>("run_minutes_per_batch"), 30);

        let receipt = sqlx::query(
            "SELECT operation,actor_id,record_id,result_json,authority_json \
             FROM command_receipts WHERE command_id='manufacturing-key-0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipt.get::<String, _>("operation"), CREATE_OPERATION);
        assert_eq!(receipt.get::<String, _>("actor_id"), "maker");
        assert_eq!(
            receipt.get::<String, _>("record_id"),
            "mfg-spec-manufacturing-key-0001"
        );
        let stored: Value =
            serde_json::from_str(&receipt.get::<String, _>("result_json")).unwrap();
        assert_eq!(stored, result);
        let authority: Value =
            serde_json::from_str(&receipt.get::<String, _>("authority_json")).unwrap();
        assert_eq!(authority["companyId"], "company");
        assert_eq!(authority["branchId"], "branch");

        assert_eq!(
            count_keyed(
                &pool,
                "SELECT count(*) FROM audit_events WHERE command_id=$1",
                "manufacturing-key-0001"
            )
            .await,
            1
        );
        let audit = sqlx::query(
            "SELECT action,record_type,record_id,record_version \
             FROM audit_events WHERE command_id='manufacturing-key-0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(audit.get::<String, _>("action"), CREATE_OPERATION);
        assert_eq!(
            audit.get::<String, _>("record_type"),
            "manufacturing_specification"
        );
        assert_eq!(audit.get::<i64, _>("record_version"), 1);

        let outbox = sqlx::query(
            "SELECT event_type,aggregate_type,aggregate_id,aggregate_version,payload_json \
             FROM outbox_events WHERE command_id='manufacturing-key-0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(outbox.get::<String, _>("event_type"), CREATE_OPERATION);
        assert_eq!(
            outbox.get::<String, _>("aggregate_type"),
            "manufacturing_specification"
        );
        assert_eq!(
            outbox.get::<String, _>("aggregate_id"),
            "mfg-spec-manufacturing-key-0001"
        );
        assert_eq!(outbox.get::<i64, _>("aggregate_version"), 1);
        let payload: Value =
            serde_json::from_str(&outbox.get::<String, _>("payload_json")).unwrap();
        assert_eq!(payload["state"], "created");
        assert_eq!(payload["companyId"], "company");

        // An exact replay returns the prior receipt and writes nothing twice.
        let replay = create(
            &pool,
            "company",
            "maker",
            "manufacturing-key-0001",
            request(),
        )
        .await
        .unwrap();
        assert_eq!(replay, result);
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM manufacturing_specifications WHERE id='mfg-spec-manufacturing-key-0001'"
            )
            .await,
            1
        );
        assert_eq!(
            count_keyed(
                &pool,
                "SELECT count(*) FROM command_receipts WHERE command_id=$1",
                "manufacturing-key-0001"
            )
            .await,
            1
        );

        // A reused key with a different command is rejected before any write.
        let mut changed = serde_json::to_value(request()).unwrap();
        changed["name"] = serde_json::json!("Different assembly");
        let error = create(
            &pool,
            "company",
            "maker",
            "manufacturing-key-0001",
            serde_json::from_value(changed).unwrap(),
        )
        .await
        .unwrap_err();
        match error {
            PgTransactionError::Business(error) => {
                assert_eq!(error.code, "MANUFACTURING_IDEMPOTENCY_CONFLICT")
            }
            other => panic!("expected the idempotency conflict, got {other:?}"),
        }
    })
    .await;
}

#[tokio::test]
async fn independent_review_and_retirement_persist_cas_and_evidence() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let error = review_command(
            &pool,
            "company",
            "maker",
            "manufacturing-review-0001",
            "spec-draft",
            review(ReviewAction::Approve, 1),
        )
        .await
        .unwrap_err();
        match error {
            PgTransactionError::Business(error) => assert_eq!(error.code, "CONFLICT"),
            other => panic!("expected the independent review conflict, got {other:?}"),
        }

        let approved = review_command(
            &pool,
            "company",
            "reviewer",
            "manufacturing-review-0002",
            "spec-draft",
            review(ReviewAction::Approve, 1),
        )
        .await
        .unwrap();
        assert_eq!(approved["status"], "approved");
        assert_eq!(approved["version"], 2);
        let row = sqlx::query(
            "SELECT status,reviewed_by,version FROM manufacturing_specifications WHERE id='spec-draft'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("status"), "approved");
        assert_eq!(row.get::<String, _>("reviewed_by"), "reviewer");
        assert_eq!(row.get::<i64, _>("version"), 2);
        assert_eq!(
            count_keyed(
                &pool,
                "SELECT count(*) FROM command_receipts WHERE command_id=$1",
                "manufacturing-review-0002"
            )
            .await,
            1
        );
        let audit = sqlx::query(
            "SELECT action,record_version FROM audit_events WHERE command_id='manufacturing-review-0002'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(audit.get::<String, _>("action"), APPROVE_OPERATION);
        assert_eq!(audit.get::<i64, _>("record_version"), 2);

        // Stale version and repeat transitions conflict without another write.
        let error = review_command(
            &pool,
            "company",
            "reviewer",
            "manufacturing-review-0003",
            "spec-draft",
            review(ReviewAction::Approve, 2),
        )
        .await
        .unwrap_err();
        match error {
            PgTransactionError::Business(error) => assert_eq!(error.code, "CONFLICT"),
            other => panic!("expected the state conflict, got {other:?}"),
        }

        // Reject is also independent and only allowed from a draft.
        let rejected = review_command(
            &pool,
            "company",
            "reviewer",
            "manufacturing-review-0004",
            "spec-foreign",
            review(ReviewAction::Reject, 1),
        )
        .await;
        assert!(rejected.is_err());

        let retired = review_command(
            &pool,
            "company",
            "maker",
            "manufacturing-retire-0001",
            "spec-draft",
            review(ReviewAction::Retire, 2),
        )
        .await
        .unwrap();
        assert_eq!(retired["status"], "retired");
        assert_eq!(retired["version"], 3);
        let row = sqlx::query(
            "SELECT status,reviewed_by,version FROM manufacturing_specifications WHERE id='spec-draft'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("status"), "retired");
        assert_eq!(row.get::<String, _>("reviewed_by"), "maker");
        assert_eq!(row.get::<i64, _>("version"), 3);

        // Retire from a draft is not a valid transition.
        let error = review_command(
            &pool,
            "company",
            "maker",
            "manufacturing-retire-0002",
            "spec-foreign",
            review(ReviewAction::Retire, 1),
        )
        .await
        .unwrap_err();
        match error {
            PgTransactionError::Business(error) => assert_eq!(error.code, "SPECIFICATION_NOT_FOUND"),
            other => panic!("expected Company isolation, got {other:?}"),
        }
    })
    .await;
}

#[tokio::test]
async fn sources_branch_scope_and_company_isolation_fail_closed() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        for patch in [
            serde_json::json!({"outputProductId": "inactive-output"}),
            serde_json::json!({"lines": [{"productId": "inactive-material", "quantity": "1"}]}),
            serde_json::json!({"operations": [
                {"resourceId": "inactive-resource", "name": "Assemble", "setupMinutes": 0, "runMinutesPerBatch": 1}
            ]}),
            serde_json::json!({"operations": [
                {"resourceId": "resource-other", "name": "Assemble", "setupMinutes": 0, "runMinutesPerBatch": 1}
            ]}),
        ] {
            let mut body = serde_json::to_value(request()).unwrap();
            for (name, value) in patch.as_object().unwrap() {
                body[name.as_str()] = value.clone();
            }
            let error = create(
                &pool,
                "company",
                "maker",
                "manufacturing-key-0100",
                serde_json::from_value(body).unwrap(),
            )
            .await
            .unwrap_err();
            match error {
                PgTransactionError::Business(error) => {
                    assert_eq!(error.code, "MANUFACTURING_SOURCE_CHANGED")
                }
                other => panic!("expected the source guard, got {other:?}"),
            }
        }
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM manufacturing_specifications WHERE id='mfg-spec-manufacturing-key-0100'"
            )
            .await,
            0
        );

        let error = review_command(
            &pool,
            "foreign",
            "foreign-maker",
            "manufacturing-review-0100",
            "spec-draft",
            review(ReviewAction::Approve, 1),
        )
        .await
        .unwrap_err();
        match error {
            PgTransactionError::Business(error) => assert_eq!(error.code, "SPECIFICATION_NOT_FOUND"),
            other => panic!("expected foreign isolation, got {other:?}"),
        }
        let error = review_command(
            &pool,
            "company",
            "reviewer",
            "manufacturing-review-0101",
            "spec-foreign",
            review(ReviewAction::Approve, 1),
        )
        .await
        .unwrap_err();
        match error {
            PgTransactionError::Business(error) => assert_eq!(error.code, "SPECIFICATION_NOT_FOUND"),
            other => panic!("expected own-Company isolation, got {other:?}"),
        }

        let error = review_command(
            &pool,
            "company",
            "revoked",
            "manufacturing-review-0102",
            "spec-draft",
            review(ReviewAction::Approve, 1),
        )
        .await
        .unwrap_err();
        match error {
            PgTransactionError::Business(error) => {
                assert_eq!(error.code, "MANUFACTURING_ACCESS_CHANGED")
            }
            other => panic!("expected the current-authority guard, got {other:?}"),
        }
    })
    .await;
}

#[tokio::test]
async fn transaction_guards_and_late_failures_roll_back_every_owner_effect() {
    fixture::with_database(1, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let error = PgSpecificationCommands::new(&mut connection)
            .current_authority(&actor("company", "maker"), true, NOW.get())
            .await
            .unwrap_err();
        match error {
            PgTransactionError::Persistence(error) => {
                assert_eq!(error.code, "MANUFACTURING_COMMAND_TRANSACTION_REQUIRED")
            }
            other => panic!("expected a transaction guard, got {other:?}"),
        }
        drop(connection);

        let mut connection = pool.raw().acquire().await.unwrap();
        let mut transaction = connection
            .begin_with("BEGIN ISOLATION LEVEL READ COMMITTED")
            .await
            .unwrap();
        let error = PgSpecificationCommands::new(&mut transaction)
            .current_authority(&actor("company", "maker"), true, NOW.get())
            .await
            .unwrap_err();
        match error {
            PgTransactionError::Persistence(error) => {
                assert_eq!(error.code, "MANUFACTURING_COMMAND_SERIALIZABLE_REQUIRED")
            }
            other => panic!("expected a serializable guard, got {other:?}"),
        }
        transaction.rollback().await.unwrap();
        drop(connection);

        let result = PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let key = key("manufacturing-key-0200");
                    let mut repository = PgSpecificationCommands::new(connection);
                    create_specification(&mut repository, &actor("company", "maker"), &key, request(), NOW)
                        .await?;
                    Err(BusinessError::new(
                        erpx_server_shared::public::FailureKind::Unavailable,
                        "MANUFACTURING_TEST_LATE_FAILURE",
                        "Late failure after specification creation.",
                    )
                    .into())
                })
            })
            .await;
        match result {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "MANUFACTURING_TEST_LATE_FAILURE")
            }
            other => panic!("expected the controlled late failure, got {other:?}"),
        }
        for (table, query) in [
            (
                "specification",
                "SELECT count(*) FROM manufacturing_specifications WHERE id='mfg-spec-manufacturing-key-0200'",
            ),
            (
                "line",
                "SELECT count(*) FROM manufacturing_specification_lines WHERE specification_id='mfg-spec-manufacturing-key-0200'",
            ),
            (
                "operation",
                "SELECT count(*) FROM manufacturing_specification_operations WHERE specification_id='mfg-spec-manufacturing-key-0200'",
            ),
            (
                "receipt",
                "SELECT count(*) FROM command_receipts WHERE command_id='manufacturing-key-0200'",
            ),
            (
                "audit",
                "SELECT count(*) FROM audit_events WHERE command_id='manufacturing-key-0200'",
            ),
            (
                "outbox",
                "SELECT count(*) FROM outbox_events WHERE command_id='manufacturing-key-0200'",
            ),
        ] {
            assert_eq!(count(&pool, query).await, 0, "{table}");
        }

        // A rollback after the receipt never leaves a replayable receipt.
        let result = PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let key = key("manufacturing-key-0201");
                    let mut repository = PgSpecificationCommands::new(connection);
                    create_specification(&mut repository, &actor("company", "maker"), &key, request(), NOW)
                        .await?;
                    Err(BusinessError::new(
                        erpx_server_shared::public::FailureKind::Unavailable,
                        "MANUFACTURING_TEST_POST_RECEIPT_FAILURE",
                        "Late failure after the receipt.",
                    )
                    .into())
                })
            })
            .await;
        assert!(result.is_err());
        assert_eq!(
            count_keyed(
                &pool,
                "SELECT count(*) FROM command_receipts WHERE command_id=$1",
                "manufacturing-key-0201"
            )
            .await,
            0
        );
    })
    .await;
}
