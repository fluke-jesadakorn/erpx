//! Native CRM pipeline reads over real migrated PostgreSQL. These prove owner
//! read SQL, Company/branch/department isolation, cursor paging, forecast
//! aggregation, serializable transaction guards, rollback and persisted-source
//! readback. They are not a CRM command, package or browser journey.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_crm::public::pipeline::{
    CrmPipelineActor, CrmPipelineCustomerDirectory, CrmPipelineEntitlement,
    CrmPipelineEntitlementPort, CrmPipelineFuture, CrmPipelinePeopleDirectory, CrmPipelineQuery,
    PgCrmPipelineRead, crm_pipeline,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, RecordId, TimestampMillis};
use std::collections::BTreeMap;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

fn actor(
    company: &str,
    branch: Option<&str>,
    department: Option<&str>,
    permissions: &[&str],
) -> CrmPipelineActor {
    CrmPipelineActor {
        user_id: "owner".into(),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: branch.map(|value| RecordId::new(value).unwrap()),
        department_id: department.map(|value| RecordId::new(value).unwrap()),
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        modules: vec!["crm".into()],
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
         ('branch','branch','branch','Branch','company','company',NULL,'active',1),\
         ('other-branch','branch','other-branch','Other branch','company','company',NULL,'active',1),\
         ('department','department','department','Department','branch','company','branch','active',1),\
         ('foreign','company','foreign','Foreign',NULL,'foreign',NULL,'active',1),\
         ('foreign-branch','branch','foreign-branch','Foreign branch','foreign','foreign',NULL,'active',1);\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at) VALUES \
         ('customer','company','customer','C1','Acme Customer','THB','active',1,10,10),\
         ('customer-2','company','customer','C2','Second Customer','THB','active',1,10,10),\
         ('foreign-customer','foreign','customer','C1','Foreign Customer','USD','active',1,10,10);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::raw_sql(
        "INSERT INTO crm_leads(id,company_id,branch_id,department_id,title,contact_name,contact_detail,source,brief,owner_id,due_date,status,version,created_by,created_at,updated_at) \
         SELECT 'page-'||lpad(n::text,2,'0'),'company','branch',NULL,'Pipeline page '||n,'Synthetic '||n,'Synthetic channel','Staff inquiry','Bounded pipeline row',\
         'owner','2026-11-20','open',1,'owner',100,100+n FROM generate_series(1,31) AS n;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::raw_sql(
        "INSERT INTO crm_leads(id,company_id,branch_id,department_id,title,contact_name,contact_detail,source,brief,owner_id,due_date,status,version,created_by,created_at,updated_at) VALUES \
         ('lead-discovery','company','branch',NULL,'Strong discovery lead','Strong Customer','Synthetic channel','Staff inquiry','Discovery row','owner','2026-11-20','qualified',2,'owner',100,50),\
         ('lead-proposal','company','branch',NULL,'Proposal lead','Synthetic contact','Synthetic channel','Staff inquiry','Proposal row','owner','2026-11-20','qualified',2,'owner',100,49),\
         ('lead-negotiation','company','branch',NULL,'Negotiation lead','Synthetic contact','Synthetic channel','Staff inquiry','Negotiation row','owner','2026-11-20','qualified',2,'owner',100,48),\
         ('lead-won','company','branch',NULL,'Won lead','Synthetic contact','Synthetic channel','Staff inquiry','Won row','owner','2026-11-20','qualified',2,'owner',100,47),\
         ('lead-lost','company','branch',NULL,'Lost lead','Synthetic contact','Synthetic channel','Staff inquiry','Lost row','owner','2026-11-20','qualified',2,'owner',100,46),\
         ('lead-other-branch','company','other-branch',NULL,'Other branch lead','Synthetic contact','Synthetic channel','Staff inquiry','Other branch row','owner','2026-11-20','open',1,'owner',100,45),\
         ('lead-department','company','branch','department','Department lead','Synthetic contact','Synthetic channel','Staff inquiry','Department row','owner','2026-11-20','open',1,'owner',100,44),\
         ('lead-foreign','foreign','foreign-branch',NULL,'Foreign lead','Foreign contact','Synthetic channel','Staff inquiry','Foreign row','owner','2026-11-20','open',1,'owner',100,43);\
         INSERT INTO crm_opportunities(id,lead_id,customer_id,stage,expected_minor,currency,close_date,version,created_by,created_at,updated_at) VALUES \
         ('opp-discovery','lead-discovery','customer','discovery',999,'THB','2026-12-01',1,'owner',100,50),\
         ('opp-proposal','lead-proposal','customer','proposal',1000,'THB','2026-12-01',1,'owner',100,49),\
         ('opp-negotiation','lead-negotiation','customer','negotiation',1,'THB','2026-12-01',1,'owner',100,48),\
         ('opp-won','lead-won','customer','won',5000,'THB','2026-12-01',1,'owner',100,47),\
         ('opp-lost','lead-lost','customer','lost',0,'THB','2026-12-01',1,'owner',100,46);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

struct People;
impl CrmPipelinePeopleDirectory for People {
    type Error = BusinessError;
    fn names<'a>(
        &'a mut self,
        _company_id: &'a str,
        user_ids: &'a [String],
    ) -> CrmPipelineFuture<'a, BTreeMap<String, String>, Self::Error> {
        Box::pin(async move {
            Ok(user_ids
                .iter()
                .map(|id| (id.clone(), format!("Staff {id}")))
                .collect())
        })
    }
}

struct Customers;
impl CrmPipelineCustomerDirectory for Customers {
    type Error = BusinessError;
    fn names<'a>(
        &'a mut self,
        _company_id: &'a str,
        customer_ids: &'a [String],
    ) -> CrmPipelineFuture<'a, BTreeMap<String, String>, Self::Error> {
        Box::pin(async move {
            Ok(customer_ids
                .iter()
                .map(|id| (id.clone(), format!("Customer {id}")))
                .collect())
        })
    }
}

struct Packages;
impl CrmPipelineEntitlementPort for Packages {
    type Error = BusinessError;
    fn entitlement<'a>(
        &'a mut self,
        _company_id: &'a str,
    ) -> CrmPipelineFuture<'a, CrmPipelineEntitlement, Self::Error> {
        Box::pin(async {
            Ok(CrmPipelineEntitlement {
                enforced: true,
                history_allowed: true,
                new_work: true,
                settle_existing: true,
                revision: "7".into(),
            })
        })
    }
}

#[tokio::test]
async fn read_filters_company_branch_department_search_and_stage() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = actor("company", None, None, &["commerce.read", "commerce.sales"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = owner.clone();
                Box::pin(async move {
                    let mut reader = PgCrmPipelineRead::new(connection);
                    let mut people = People;
                    let mut customers = Customers;
                    let mut packages = Packages;
                    let page = crm_pipeline(
                        &current,
                        &CrmPipelineQuery::default(),
                        &mut reader,
                        &mut people,
                        &mut customers,
                        &mut packages,
                        NOW,
                    )
                    .await?;
                    assert_eq!(page.items.len(), 30);
                    assert!(page.has_more);
                    assert_eq!(page.items[0].id, "page-31");
                    assert!(page.items.iter().all(|row| row.company_id == "company"));
                    assert_eq!(page.items[0].owner_name.as_deref(), Some("Staff owner"));
                    assert_eq!(page.forecast["THB"].unweighted_minor, "2000");
                    assert_eq!(page.forecast["THB"].weighted_minor, "751");
                    assert_eq!(page.forecast["THB"].won_minor, "5000");
                    assert_eq!(page.purchase.unwrap().revision, "7");
                    Ok(())
                })
            })
            .await
            .unwrap();

        let searched = actor("company", None, None, &["commerce.read"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = searched.clone();
                Box::pin(async move {
                    let mut reader = PgCrmPipelineRead::new(connection);
                    let mut people = People;
                    let mut customers = Customers;
                    let mut packages = Packages;
                    let page = crm_pipeline(
                        &current,
                        &CrmPipelineQuery {
                            search: "strong".into(),
                            ..CrmPipelineQuery::default()
                        },
                        &mut reader,
                        &mut people,
                        &mut customers,
                        &mut packages,
                        NOW,
                    )
                    .await?;
                    assert_eq!(page.items.len(), 1);
                    assert_eq!(page.items[0].id, "lead-discovery");
                    assert_eq!(
                        page.items[0].customer_name.as_deref(),
                        Some("Customer customer")
                    );
                    Ok(())
                })
            })
            .await
            .unwrap();

        let staged = actor("company", None, None, &["commerce.read"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = staged.clone();
                Box::pin(async move {
                    let mut reader = PgCrmPipelineRead::new(connection);
                    let mut people = People;
                    let mut customers = Customers;
                    let mut packages = Packages;
                    let page = crm_pipeline(
                        &current,
                        &CrmPipelineQuery {
                            stage: Some("discovery".into()),
                            ..CrmPipelineQuery::default()
                        },
                        &mut reader,
                        &mut people,
                        &mut customers,
                        &mut packages,
                        NOW,
                    )
                    .await?;
                    assert_eq!(page.items.len(), 1);
                    assert_eq!(page.items[0].stage.as_deref(), Some("discovery"));
                    Ok(())
                })
            })
            .await
            .unwrap();

        let branch = actor("company", Some("branch"), None, &["commerce.read"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = branch.clone();
                Box::pin(async move {
                    let mut reader = PgCrmPipelineRead::new(connection);
                    let mut people = People;
                    let mut customers = Customers;
                    let mut packages = Packages;
                    let page = crm_pipeline(
                        &current,
                        &CrmPipelineQuery::default(),
                        &mut reader,
                        &mut people,
                        &mut customers,
                        &mut packages,
                        NOW,
                    )
                    .await?;
                    assert!(
                        page.items
                            .iter()
                            .all(|row| row.branch_id.as_deref() == Some("branch"))
                    );
                    let cursor = page.next_cursor.clone().expect("branch second page cursor");
                    let second = crm_pipeline(
                        &current,
                        &CrmPipelineQuery {
                            cursor: Some(cursor),
                            ..CrmPipelineQuery::default()
                        },
                        &mut reader,
                        &mut people,
                        &mut customers,
                        &mut packages,
                        NOW,
                    )
                    .await?;
                    assert!(!second.items.iter().any(|row| row.id == "lead-other-branch"));
                    assert!(
                        second
                            .items
                            .iter()
                            .all(|row| row.branch_id.as_deref() == Some("branch"))
                    );
                    Ok(())
                })
            })
            .await
            .unwrap();

        let department = actor(
            "company",
            Some("branch"),
            Some("department"),
            &["commerce.read"],
        );
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = department.clone();
                Box::pin(async move {
                    let mut reader = PgCrmPipelineRead::new(connection);
                    let mut people = People;
                    let mut customers = Customers;
                    let mut packages = Packages;
                    let page = crm_pipeline(
                        &current,
                        &CrmPipelineQuery::default(),
                        &mut reader,
                        &mut people,
                        &mut customers,
                        &mut packages,
                        NOW,
                    )
                    .await?;
                    assert_eq!(page.items.len(), 1);
                    assert_eq!(page.items[0].id, "lead-department");
                    Ok(())
                })
            })
            .await
            .unwrap();

        let foreign = actor("foreign", Some("foreign-branch"), None, &["commerce.read"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = foreign.clone();
                Box::pin(async move {
                    let mut reader = PgCrmPipelineRead::new(connection);
                    let mut people = People;
                    let mut customers = Customers;
                    let mut packages = Packages;
                    let page = crm_pipeline(
                        &current,
                        &CrmPipelineQuery::default(),
                        &mut reader,
                        &mut people,
                        &mut customers,
                        &mut packages,
                        NOW,
                    )
                    .await?;
                    assert_eq!(page.items.len(), 1);
                    assert_eq!(page.items[0].id, "lead-foreign");
                    assert!(page.forecast.is_empty());
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn cursor_pages_are_unique_and_forecast_is_scope_bounded() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = actor("company", None, None, &["commerce.read"]);
        let ids = PgUnitOfWork::new(pool.clone())
            .run::<Vec<String>, BusinessError, _>(
                TransactionOptions::default(),
                move |connection| {
                    let current = owner.clone();
                    Box::pin(async move {
                        let mut ids = Vec::new();
                        let mut reader = PgCrmPipelineRead::new(connection);
                        let mut people = People;
                        let mut customers = Customers;
                        let mut packages = Packages;
                        let first = crm_pipeline(
                            &current,
                            &CrmPipelineQuery::default(),
                            &mut reader,
                            &mut people,
                            &mut customers,
                            &mut packages,
                            NOW,
                        )
                        .await?;
                        assert_eq!(first.items.len(), 30);
                        let cursor = first.next_cursor.clone().expect("first page cursor");
                        ids.extend(first.items.iter().map(|row| row.id.clone()));
                        let second = crm_pipeline(
                            &current,
                            &CrmPipelineQuery {
                                cursor: Some(cursor),
                                ..CrmPipelineQuery::default()
                            },
                            &mut reader,
                            &mut people,
                            &mut customers,
                            &mut packages,
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
        assert_eq!(ids.len(), 38);
        let unique = ids.iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique.len(), ids.len());
        assert_eq!(ids.first().map(String::as_str), Some("page-31"));
        assert!(ids.contains(&"lead-won".to_owned()));
    })
    .await;
}

#[tokio::test]
async fn transaction_guards_rollback_and_persisted_readback_hold() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = actor("company", None, None, &["commerce.read"]);
        let mut outside = pool.raw().acquire().await.unwrap();
        let error = PgCrmPipelineRead::new(&mut outside)
            .page(&CompanyId::new("company").unwrap(), None, None, "", None, None, 31)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref value)
                if value.code == "CRM_PIPELINE_READ_TRANSACTION_REQUIRED"
        ));
        drop(outside);

        let mut default_isolation = pool.raw().begin().await.unwrap();
        let error = PgCrmPipelineRead::new(&mut default_isolation)
            .page(&CompanyId::new("company").unwrap(), None, None, "", None, None, 31)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref value)
                if value.code == "CRM_PIPELINE_READ_SERIALIZABLE_REQUIRED"
        ));
        default_isolation.rollback().await.unwrap();

        let before: i64 = sqlx::query_scalar("SELECT count(*) FROM crm_leads")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        let read_owner = owner.clone();
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = read_owner.clone();
                Box::pin(async move {
                    let mut reader = PgCrmPipelineRead::new(connection);
                    let mut people = People;
                    let mut customers = Customers;
                    let mut packages = Packages;
                    let page = crm_pipeline(
                        &current,
                        &CrmPipelineQuery::default(),
                        &mut reader,
                        &mut people,
                        &mut customers,
                        &mut packages,
                        NOW,
                    )
                    .await?;
                    assert_eq!(page.items.len(), 30);
                    Ok(())
                })
            })
            .await
            .unwrap();
        let after: i64 = sqlx::query_scalar("SELECT count(*) FROM crm_leads")
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
                        "INSERT INTO crm_leads(id,company_id,branch_id,department_id,title,contact_name,contact_detail,source,brief,owner_id,due_date,status,version,created_by,created_at,updated_at) VALUES \
                         ('rollback-lead','company','branch',NULL,'Rollback lead','Synthetic contact','Synthetic channel','Staff inquiry','Rollback row','owner','2026-11-20','open',1,'owner',100,9000);",
                    )
                    .execute(&mut *connection)
                    .await?;
                    let mut reader = PgCrmPipelineRead::new(connection);
                    let mut people = People;
                    let mut customers = Customers;
                    let mut packages = Packages;
                    let page = crm_pipeline(
                        &current,
                        &CrmPipelineQuery::default(),
                        &mut reader,
                        &mut people,
                        &mut customers,
                        &mut packages,
                        NOW,
                    )
                    .await?;
                    assert_eq!(page.items.len(), 30);
                    assert_eq!(page.items[0].id, "rollback-lead");
                    Err(PgTransactionError::Business(BusinessError::invalid(
                        "OWNER_ABORT",
                        "Rollback fixture",
                    )))
                })
            })
            .await
            .unwrap_err();
        let persisted: i64 =
            sqlx::query_scalar("SELECT count(*) FROM crm_leads WHERE id='rollback-lead'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(persisted, 0);
    })
    .await;
}

#[tokio::test]
async fn missing_permission_and_package_history_fail_before_reading() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let denied = actor("company", None, None, &[]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = denied.clone();
                Box::pin(async move {
                    let mut reader = PgCrmPipelineRead::new(connection);
                    let mut people = People;
                    let mut customers = Customers;
                    let mut packages = Packages;
                    let error = business(
                        crm_pipeline(
                            &current,
                            &CrmPipelineQuery::default(),
                            &mut reader,
                            &mut people,
                            &mut customers,
                            &mut packages,
                            NOW,
                        )
                        .await
                        .unwrap_err(),
                    );
                    assert_eq!(error.code, "CRM_PERMISSION_REQUIRED");
                    Ok(())
                })
            })
            .await
            .unwrap();

        struct DeniedPackage;
        impl CrmPipelineEntitlementPort for DeniedPackage {
            type Error = BusinessError;
            fn entitlement<'a>(
                &'a mut self,
                _company_id: &'a str,
            ) -> CrmPipelineFuture<'a, CrmPipelineEntitlement, Self::Error> {
                Box::pin(async {
                    Ok(CrmPipelineEntitlement {
                        enforced: true,
                        history_allowed: false,
                        new_work: false,
                        settle_existing: false,
                        revision: "9".into(),
                    })
                })
            }
        }
        let owner = actor("company", None, None, &["commerce.read"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = owner.clone();
                Box::pin(async move {
                    let mut reader = PgCrmPipelineRead::new(connection);
                    let mut people = People;
                    let mut customers = Customers;
                    let mut packages = DeniedPackage;
                    let error = business(
                        crm_pipeline(
                            &current,
                            &CrmPipelineQuery::default(),
                            &mut reader,
                            &mut people,
                            &mut customers,
                            &mut packages,
                            NOW,
                        )
                        .await
                        .unwrap_err(),
                    );
                    assert_eq!(error.code, "PACKAGE_REQUIRED");
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}
