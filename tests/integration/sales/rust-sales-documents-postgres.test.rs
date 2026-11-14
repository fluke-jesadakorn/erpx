//! Native Sales document list reads on migrated PostgreSQL.
//! These tests prove owner read SQL, Company/branch scope, cursor pagination,
//! serializable transaction guards, rollback and persisted-source readback.
//! They do not prove a quotation/order command, purchase, posting or browser
//! journey.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_sales::public::sales_documents::{
    PgSalesDocumentRead, SalesDocumentKind, SalesDocumentReadActor, sales_documents,
};
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

#[tokio::test]
async fn recipient_composes_current_identity_and_dated_people_in_one_unit() {
    use erpx_server_sales::public::sales_recipient::{SalesRecipientRequest, inspect_sales_recipient, prepare_sales_recipient_sources};
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','sales_recipient_test','Sales recipient',false,1,'active'); INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','sales_recipient_test',1,'[\"commerce.read\",\"commerce.sales\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0); INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES('sales-recipient-grant','sales-user','sales_recipient_test','[]','company',0,9007199254740991,1,'holding'); INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES('sales-person','sales-user','company','Sales','TH','fixture',0,0); INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES('sales-engagement','sales-person','company','employee','salaried','active','2000-01-01','Sales','THB','fixture',0,0)").execute(pool.raw()).await.unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        let locks = prepare_sales_recipient_sources(&mut tx).await.unwrap().bind().await.unwrap();
        let request = || SalesRecipientRequest { company_id: CompanyId::new("company").unwrap(), user_id: "sales-user".into(), branch_id: "branch".into(), department_id: None };
        let eligible = inspect_sales_recipient(&mut tx, &locks, request()).await.unwrap();
        assert!(eligible.is_eligible());
        assert_eq!((eligible.request().company_id.as_str(), eligible.request().user_id.as_str(), eligible.request().branch_id.as_str(), eligible.request().department_id.as_deref()), ("company", "sales-user", "branch", None));
        eligible.require_current(&mut tx).await.unwrap();
        let mut other = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        assert!(eligible.require_current(&mut other).await.is_err());
        other.rollback().await.unwrap();
        let wrong_company = SalesRecipientRequest { company_id: CompanyId::new("foreign").unwrap(), ..request() };
        let foreign = inspect_sales_recipient(&mut tx, &locks, wrong_company).await.unwrap();
        assert!(!foreign.is_eligible());
        assert_eq!(foreign.request().company_id.as_str(), "foreign");
        assert_ne!(foreign.request().company_id.as_str(), eligible.request().company_id.as_str());
        sqlx::query("UPDATE hr_engagements SET start_date='9999-01-01' WHERE id='sales-engagement'").execute(&mut *tx).await.unwrap();
        assert!(!inspect_sales_recipient(&mut tx, &locks, request()).await.unwrap().is_eligible());
        assert!(eligible.require_current(&mut tx).await.is_err());
        sqlx::query("UPDATE hr_engagements SET start_date='2000-01-01',end_date='2000-01-02' WHERE id='sales-engagement'").execute(&mut *tx).await.unwrap();
        assert!(!inspect_sales_recipient(&mut tx, &locks, request()).await.unwrap().is_eligible());
        tx.rollback().await.unwrap();
    }).await;
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

fn actor(
    user: &str,
    company: &str,
    branch: Option<&str>,
    permissions: &[&str],
) -> SalesDocumentReadActor {
    SalesDocumentReadActor {
        user_id: user.into(),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: branch.map(|value| RecordId::new(value).unwrap()),
        department_id: None,
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        modules: vec!["sales".into()],
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
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('warehouse','company','branch','WH','Warehouse','active',1),\
         ('foreign-warehouse','foreign','foreign-branch','WH','Foreign warehouse','active',1);\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at) VALUES \
         ('customer','company','customer','C1','Acme Customer','THB','active',1,10,10),\
         ('customer-2','company','customer','C2','Strong Customer','THB','active',1,10,10),\
         ('foreign-customer','foreign','customer','C1','Foreign Customer','USD','active',1,10,10);\
         INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,source_id,version,created_by,approved_by,created_at,updated_at) VALUES \
         ('quote-draft','company','branch',NULL,'quote','QT-1','customer','warehouse','THB','2026-11-04','2026-11-18','', 'draft',1000,70,NULL,1,'owner',NULL,100,100),\
         ('quote-submitted','company','branch',NULL,'quote','QT-2','customer-2','warehouse','THB','2026-11-05','2026-11-19','', 'submitted',2000,140,NULL,2,'owner',NULL,200,200),\
         ('sales-confirmed','company','branch',NULL,'sales','SO-1','customer','warehouse','THB','2026-11-06','2026-11-20','', 'confirmed',3000,210,NULL,1,'owner',NULL,300,300),\
         ('sales-fulfilled','company','branch',NULL,'sales','SO-2','customer-2','warehouse','THB','2026-11-07','2026-11-21','', 'fulfilled',4000,280,NULL,1,'owner',NULL,400,400),\
         ('quote-other-branch','company','foreign-branch',NULL,'quote','QT-3','customer','foreign-warehouse','THB','2026-11-08','2026-11-22','', 'draft',500,35,NULL,1,'owner',NULL,500,500),\
         ('quote-department','company','branch','department','quote','QT-4','customer','warehouse','THB','2026-11-09','2026-11-23','', 'draft',600,42,NULL,1,'owner',NULL,600,600),\
         ('quote-foreign','foreign','foreign-branch',NULL,'quote','QT-F','foreign-customer','foreign-warehouse','USD','2026-11-10','2026-11-24','', 'draft',700,0,NULL,1,'owner',NULL,700,700),\
         ('purchase-company','company','branch',NULL,'purchase','PO-1','customer','warehouse','THB','2026-11-11','2026-11-25','', 'draft',800,56,NULL,1,'owner',NULL,800,800);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn setup_pages(pool: &DatabasePool) {
    setup(pool).await;
    sqlx::raw_sql(
        "INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,source_id,version,created_by,approved_by,created_at,updated_at) \
         SELECT 'page-'||lpad(n::text,2,'0'),'company','branch',NULL,'quote','QT-P'||n,'customer','warehouse','THB','2026-11-04','2026-11-18','', 'draft',1000,70,NULL,1,'owner',NULL,1000+n,1000+n \
         FROM generate_series(1,31) AS n;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn read_filters_company_kind_search_status_and_selected_branch() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = actor("owner", "company", None, &["commerce.read"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = owner.clone();
                Box::pin(async move {
                    let mut reader = PgSalesDocumentRead::new(connection);
                    let quotes = sales_documents(
                        &current,
                        &mut reader,
                        SalesDocumentKind::Quote,
                        "",
                        None,
                        None,
                        NOW,
                    )
                    .await?;
                    assert_eq!(
                        quotes
                            .items
                            .iter()
                            .map(|row| row.id.as_str())
                            .collect::<Vec<_>>(),
                        vec![
                            "quote-department",
                            "quote-other-branch",
                            "quote-submitted",
                            "quote-draft"
                        ]
                    );
                    assert!(quotes.items.iter().all(|row| row.kind == "quote"));

                    let sales = sales_documents(
                        &current,
                        &mut reader,
                        SalesDocumentKind::Sales,
                        "",
                        None,
                        None,
                        NOW,
                    )
                    .await?;
                    assert_eq!(
                        sales
                            .items
                            .iter()
                            .map(|row| row.id.as_str())
                            .collect::<Vec<_>>(),
                        vec!["sales-fulfilled", "sales-confirmed"]
                    );

                    let searched = sales_documents(
                        &current,
                        &mut reader,
                        SalesDocumentKind::Quote,
                        "strong",
                        None,
                        None,
                        NOW,
                    )
                    .await?;
                    assert_eq!(searched.items.len(), 1);
                    assert_eq!(searched.items[0].id, "quote-submitted");

                    let submitted = sales_documents(
                        &current,
                        &mut reader,
                        SalesDocumentKind::Quote,
                        "",
                        Some("submitted"),
                        None,
                        NOW,
                    )
                    .await?;
                    assert_eq!(submitted.items.len(), 1);
                    assert_eq!(submitted.items[0].id, "quote-submitted");
                    Ok(())
                })
            })
            .await
            .unwrap();

        let branch_owner = actor("owner", "company", Some("branch"), &["commerce.read"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = branch_owner.clone();
                Box::pin(async move {
                    let mut reader = PgSalesDocumentRead::new(connection);
                    let quotes = sales_documents(
                        &current,
                        &mut reader,
                        SalesDocumentKind::Quote,
                        "",
                        None,
                        None,
                        NOW,
                    )
                    .await?;
                    assert_eq!(
                        quotes
                            .items
                            .iter()
                            .map(|row| row.id.as_str())
                            .collect::<Vec<_>>(),
                        vec!["quote-department", "quote-submitted", "quote-draft"]
                    );
                    Ok(())
                })
            })
            .await
            .unwrap();

        let foreign = actor(
            "owner",
            "foreign",
            Some("foreign-branch"),
            &["commerce.read"],
        );
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = foreign.clone();
                Box::pin(async move {
                    let mut reader = PgSalesDocumentRead::new(connection);
                    let quotes = sales_documents(
                        &current,
                        &mut reader,
                        SalesDocumentKind::Quote,
                        "",
                        None,
                        None,
                        NOW,
                    )
                    .await?;
                    assert_eq!(quotes.items.len(), 1);
                    assert_eq!(quotes.items[0].id, "quote-foreign");
                    assert_eq!(quotes.items[0].company_id, "foreign");
                    Ok(())
                })
            })
            .await
            .unwrap();

        let persisted: i64 =
            sqlx::query_scalar("SELECT count(*) FROM commerce_orders WHERE company_id='company'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(persisted, 7, "reads must not create or delete order rows");
    })
    .await;
}

#[tokio::test]
async fn cursor_pagination_preserves_order_without_duplicates() {
    fixture::with_database(2, |pool| async move {
        setup_pages(&pool).await;
        let owner = actor("owner", "company", Some("branch"), &["commerce.read"]);
        let ids = PgUnitOfWork::new(pool.clone())
            .run::<Vec<String>, BusinessError, _>(
                TransactionOptions::default(),
                move |connection| {
                    let current = owner.clone();
                    Box::pin(async move {
                        let mut ids = Vec::new();
                        let mut reader = PgSalesDocumentRead::new(connection);
                        let first = sales_documents(
                            &current,
                            &mut reader,
                            SalesDocumentKind::Quote,
                            "",
                            None,
                            None,
                            NOW,
                        )
                        .await?;
                        assert_eq!(first.items.len(), 30);
                        assert!(first.has_more);
                        let cursor = first.next_cursor.clone().expect("first page cursor");
                        ids.extend(first.items.iter().map(|row| row.id.clone()));
                        let second = sales_documents(
                            &current,
                            &mut reader,
                            SalesDocumentKind::Quote,
                            "",
                            None,
                            Some(&cursor),
                            NOW,
                        )
                        .await?;
                        assert!(!second.has_more);
                        assert!(second.next_cursor.is_none());
                        ids.extend(second.items.iter().map(|row| row.id.clone()));
                        Ok(ids)
                    })
                },
            )
            .await
            .unwrap();
        assert_eq!(ids.len(), 34);
        let unique = ids.iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique.len(), ids.len());
        assert_eq!(ids.first().map(String::as_str), Some("page-31"));
        assert!(ids.contains(&"quote-draft".to_owned()));
    })
    .await;
}

#[tokio::test]
async fn transaction_guards_rollback_and_persisted_readback_hold() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = actor("owner", "company", None, &["commerce.read"]);
        let mut outside = pool.raw().acquire().await.unwrap();
        let error = PgSalesDocumentRead::new(&mut outside)
            .page(
                &CompanyId::new("company").unwrap(),
                None,
                None,
                SalesDocumentKind::Quote,
                "",
                None,
                None,
                31,
            )
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref value)
                if value.code == "SALES_DOCUMENTS_READ_TRANSACTION_REQUIRED"
        ));
        drop(outside);

        let mut default_isolation = pool.raw().begin().await.unwrap();
        let error = PgSalesDocumentRead::new(&mut default_isolation)
            .page(
                &CompanyId::new("company").unwrap(),
                None,
                None,
                SalesDocumentKind::Quote,
                "",
                None,
                None,
                31,
            )
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref value)
                if value.code == "SALES_DOCUMENTS_READ_SERIALIZABLE_REQUIRED"
        ));
        default_isolation.rollback().await.unwrap();

        let before: i64 = sqlx::query_scalar("SELECT count(*) FROM commerce_orders")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        let read_owner = owner.clone();
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = read_owner.clone();
                Box::pin(async move {
                    let mut reader = PgSalesDocumentRead::new(connection);
                    let page = sales_documents(
                        &current,
                        &mut reader,
                        SalesDocumentKind::Quote,
                        "",
                        None,
                        None,
                        NOW,
                    )
                    .await?;
                    assert_eq!(page.items.len(), 4);
                    Ok(())
                })
            })
            .await
            .unwrap();
        let after: i64 = sqlx::query_scalar("SELECT count(*) FROM commerce_orders")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(before, after);

        let rollback_owner = owner.clone();
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = rollback_owner.clone();
                Box::pin(async move {
                    sqlx::raw_sql(
                        "INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,source_id,version,created_by,approved_by,created_at,updated_at) VALUES \
                         ('rollback-order','company','branch',NULL,'quote','QT-R','customer','warehouse','THB','2026-11-04','2026-11-18','', 'draft',1000,70,NULL,1,'owner',NULL,9000,9000);",
                    )
                    .execute(&mut *connection)
                    .await?;
                    let mut reader = PgSalesDocumentRead::new(connection);
                    let page = sales_documents(
                        &current,
                        &mut reader,
                        SalesDocumentKind::Quote,
                        "",
                        None,
                        None,
                        NOW,
                    )
                    .await?;
                    assert_eq!(page.items.len(), 5);
                    assert_eq!(page.items[0].id, "rollback-order");
                    Err(PgTransactionError::Business(BusinessError::invalid(
                        "OWNER_ABORT",
                        "Rollback fixture",
                    )))
                })
            })
            .await
            .unwrap_err();
        let persisted: i64 =
            sqlx::query_scalar("SELECT count(*) FROM commerce_orders WHERE id='rollback-order'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(persisted, 0);
    })
    .await;
}

#[tokio::test]
async fn missing_permission_fails_before_reading_the_source() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let denied = actor("owner", "company", None, &[]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = denied.clone();
                Box::pin(async move {
                    let mut reader = PgSalesDocumentRead::new(connection);
                    let error = business(
                        sales_documents(
                            &current,
                            &mut reader,
                            SalesDocumentKind::Quote,
                            "",
                            None,
                            None,
                            NOW,
                        )
                        .await
                        .unwrap_err(),
                    );
                    assert_eq!(error.kind, FailureKind::Forbidden);
                    assert_eq!(error.code, "SALES_PERMISSION_REQUIRED");
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}
