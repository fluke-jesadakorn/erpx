//! Native Manufacturing BOM register reads on migrated PostgreSQL.
//! These tests prove owner SQL, Company isolation, serializable transaction
//! guards, rollback and persisted-source readback. They do not prove a
//! specification command, purchase, review or browser journey.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_manufacturing::public::bom::{BomActor, BomRegister, PgBomRead, bom_register};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use sqlx::Connection;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn company(value: &str) -> CompanyId {
    CompanyId::new(value).unwrap()
}

fn actor(company_id: &str, user: &str) -> BomActor {
    BomActor {
        user_id: user.into(),
        company_id: company(company_id),
        branch_id: Some(RecordId::new("branch").unwrap()),
        department_id: None,
        permissions: vec!["records.read".into()],
        modules: vec!["manufacturing".into()],
        expires_at: NOW.get() + 60_000,
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
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES \
         ('company','company','company','Company',NULL,'company',NULL,'active',1),\
         ('branch','branch','branch','Branch',NULL,'company',NULL,'active',1),\
         ('foreign','company','foreign','Foreign',NULL,'foreign',NULL,'active',1),\
         ('foreign-branch','branch','foreign-branch','Foreign branch',NULL,'foreign',NULL,'active',1);\
         INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status,version) VALUES \
         ('output','company','OUT','Bounded output','each',1000,0,'active',1),\
         ('material-a','company','MA','Material A','each',100,0,'active',1),\
         ('material-b','company','MB','Material B','each',250,0,'active',1),\
         ('foreign-output','foreign','FOUT','Foreign output','each',1000,0,'active',1),\
         ('foreign-material','foreign','FMA','Foreign material','each',100,0,'active',1);\
         INSERT INTO production_resources(id,company_id,branch_id,code,name,daily_minutes,work_weekdays_json,status,version,created_by,created_at,updated_at) VALUES \
         ('resource','company','branch','R1','Assembly line',480,'[1,2,3,4,5]','active',1,'maker',10,10),\
         ('foreign-resource','foreign','foreign-branch','R1','Foreign assembly',480,'[1,2,3,4,5]','active',1,'maker',10,10);\
         INSERT INTO manufacturing_specifications(id,company_id,name,mode,output_product_id,batch_quantity_milli,expected_yield_bps,status,created_by,reviewed_by,version,created_at,updated_at) VALUES \
         ('spec-draft','company','Draft assembly','bom','output',10000,9800,'draft','maker',NULL,1,100,100),\
         ('spec-approved','company','Approved formula','formula','output',5000,9900,'draft','maker',NULL,1,150,300),\
         ('spec-foreign','foreign','Foreign BOM','bom','foreign-output',10000,9800,'draft','maker',NULL,1,100,100);\
         INSERT INTO manufacturing_specification_lines(id,specification_id,product_id,quantity_milli,substitution_group,ordinal) VALUES \
         ('line-a','spec-draft','material-a',1500,NULL,0),\
         ('line-b','spec-draft','material-b',2500,'secondary',1),\
         ('line-approved','spec-approved','material-a',5000,NULL,0),\
         ('line-foreign','spec-foreign','foreign-material',1000,NULL,0);\
         INSERT INTO manufacturing_specification_operations(id,specification_id,resource_id,name,setup_minutes,run_minutes_per_batch,ordinal) VALUES \
         ('operation-draft','spec-draft','resource','Assemble',5,30,0),\
         ('operation-approved','spec-approved','resource','Blend',10,45,0),\
         ('operation-foreign','spec-foreign','foreign-resource','Foreign assemble',5,30,0);\
         UPDATE manufacturing_specifications SET status='approved',reviewed_by='reviewer',version=2 WHERE id='spec-approved';",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn read(
    pool: &DatabasePool,
    actor: BomActor,
) -> Result<BomRegister, PgTransactionError<BusinessError>> {
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            Box::pin(async move {
                let mut repository = PgBomRead::new(connection);
                bom_register(&actor, &mut repository, NOW).await
            })
        })
        .await
}

#[tokio::test]
async fn reads_current_company_specifications_with_lines_and_routing() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let result = read(&pool, actor("company", "owner")).await.unwrap();
        assert_eq!(
            result
                .specifications
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            vec!["spec-approved", "spec-draft"]
        );
        let draft = result
            .specifications
            .iter()
            .find(|row| row.id == "spec-draft")
            .unwrap();
        assert_eq!(draft.name, "Draft assembly");
        assert_eq!(draft.mode, "bom");
        assert_eq!(draft.status, "draft");
        assert_eq!(draft.lines.len(), 2);
        assert_eq!(
            draft
                .lines
                .iter()
                .map(|line| (line.id.as_str(), line.ordinal))
                .collect::<Vec<_>>(),
            vec![("line-a", 0), ("line-b", 1)]
        );
        assert_eq!(
            draft.lines[1].substitution_group.as_deref(),
            Some("secondary")
        );
        assert_eq!(draft.operations.len(), 1);
        assert_eq!(draft.operations[0].resource_name, "Assembly line");
        assert_eq!(draft.operations[0].run_minutes_per_batch, 30);
        assert!(draft.operations[0].specification_id == "spec-draft");

        let approved = result
            .specifications
            .iter()
            .find(|row| row.id == "spec-approved")
            .unwrap();
        assert_eq!(approved.reviewed_by.as_deref(), Some("reviewer"));
        assert_eq!(approved.version, 2);
        assert_eq!(approved.lines[0].quantity_milli, 5000);

        // A read must not create or delete owner source rows.
        let specifications: i64 =
            sqlx::query_scalar("SELECT count(*) FROM manufacturing_specifications")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        let lines: i64 =
            sqlx::query_scalar("SELECT count(*) FROM manufacturing_specification_lines")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        let operations: i64 =
            sqlx::query_scalar("SELECT count(*) FROM manufacturing_specification_operations")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!((specifications, lines, operations), (3, 4, 3));
    })
    .await;
}

#[tokio::test]
async fn foreign_companies_never_share_a_bill_of_materials() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let own = read(&pool, actor("company", "owner")).await.unwrap();
        assert!(
            own.specifications
                .iter()
                .all(|row| row.company_id == "company")
        );
        let foreign = read(&pool, actor("foreign", "foreign-owner"))
            .await
            .unwrap();
        assert_eq!(foreign.specifications.len(), 1);
        assert_eq!(foreign.specifications[0].id, "spec-foreign");
        assert_eq!(foreign.specifications[0].company_id, "foreign");
        assert_eq!(
            foreign.specifications[0].lines[0].product_id,
            "foreign-material"
        );

        // A current Company read may not see the foreign specification.
        assert!(
            own.specifications
                .iter()
                .all(|row| row.id != "spec-foreign")
        );
    })
    .await;
}

#[tokio::test]
async fn read_requires_a_serializable_caller_transaction() {
    fixture::with_database(1, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let error = PgBomRead::new(&mut connection)
            .register(&company("company"))
            .await
            .unwrap_err();
        match error {
            PgTransactionError::Persistence(error) => {
                assert_eq!(error.code, "MANUFACTURING_BOM_TRANSACTION_REQUIRED")
            }
            other => panic!("expected a transaction guard, got {other:?}"),
        }
        drop(connection);

        let mut connection = pool.raw().acquire().await.unwrap();
        let mut transaction = connection
            .begin_with("BEGIN ISOLATION LEVEL READ COMMITTED")
            .await
            .unwrap();
        let error = PgBomRead::new(&mut transaction)
            .register(&company("company"))
            .await
            .unwrap_err();
        match error {
            PgTransactionError::Persistence(error) => {
                assert_eq!(error.code, "MANUFACTURING_BOM_SERIALIZABLE_REQUIRED")
            }
            other => panic!("expected a serializable guard, got {other:?}"),
        }
        transaction.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn policy_and_late_failures_roll_back_without_persisting_a_read_view() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut denied = actor("company", "owner");
        denied.permissions.clear();
        match read(&pool, denied).await {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "MANUFACTURING_BOM_PERMISSION_REQUIRED")
            }
            other => panic!("expected the policy failure, got {other:?}"),
        }

        let result = PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    sqlx::raw_sql(
                        "INSERT INTO manufacturing_specifications(id,company_id,name,mode,output_product_id,batch_quantity_milli,expected_yield_bps,status,created_by,reviewed_by,version,created_at,updated_at) VALUES \
                         ('spec-uncommitted','company','Uncommitted assembly','bom','output',1000,10000,'draft','maker',NULL,1,500,500);\
                         INSERT INTO manufacturing_specification_lines(id,specification_id,product_id,quantity_milli,substitution_group,ordinal) VALUES \
                         ('line-uncommitted','spec-uncommitted','material-a',1000,NULL,0);",
                    )
                    .execute(&mut *connection)
                    .await?;
                    let mut repository = PgBomRead::new(connection);
                    let current = bom_register(&actor("company", "owner"), &mut repository, NOW)
                        .await?;
                    assert!(
                        current
                            .specifications
                            .iter()
                            .any(|row| row.id == "spec-uncommitted")
                    );
                    Err(BusinessError::new(
                        FailureKind::Unavailable,
                        "MANUFACTURING_TEST_LATE_FAILURE",
                        "Late failure after the owner read.",
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
        let persisted: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM manufacturing_specifications WHERE id='spec-uncommitted'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(persisted, 0);
        let lines: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM manufacturing_specification_lines WHERE id='line-uncommitted'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(lines, 0);

        let readback = read(&pool, actor("company", "owner")).await.unwrap();
        assert_eq!(readback.specifications.len(), 2);
        assert!(
            readback
                .specifications
                .iter()
                .all(|row| row.id != "spec-uncommitted")
        );
    })
    .await;
}
