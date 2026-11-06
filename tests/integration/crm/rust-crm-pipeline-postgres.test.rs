//! Native CRM pipeline reads over real migrated PostgreSQL. These prove owner
//! read SQL, Company/branch/department isolation, cursor paging, forecast
//! aggregation, serializable transaction guards, rollback and persisted-source
//! readback. They are not a CRM command, package or browser journey.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_crm::public::activities_native::{
    ActivityActor, ActivityChangeInput, ActivityCreateInput, PgCrmActivityCommands,
    change_activity, create_activity,
};
use erpx_server_crm::public::activity_source::{
    CrmActivitySourceKind, CrmActivitySourceRequest, inspect_crm_activity_source,
    lock_crm_activity_sources,
};
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

#[tokio::test]
async fn activity_source_retains_scoped_crm_identity_in_original_unit() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql("INSERT INTO crm_accounts(id,company_id,kind,name,created_by,created_at,updated_at) VALUES('account-one','company','business','One','owner',1,1),('account-two','company','business','Two','owner',1,1); INSERT INTO crm_contacts(id,company_id,account_id,name,detail,created_by,created_at,updated_at) VALUES('contact-one','company','account-one','Contact','Detail','owner',1,1); UPDATE crm_leads SET account_id='account-one',contact_id='contact-one',department_id='department' WHERE id='page-01'; INSERT INTO crm_lead_activities(id,company_id,lead_id,activity_type,subject,details,owner_id,due_date,created_by,created_at,updated_at) VALUES('source-activity','company','page-01','call','Call customer','Discuss scope','owner','2026-11-20','owner',1,1)").execute(pool.raw()).await.unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_crm_activity_sources(&mut tx).await.unwrap();
        let request = || CrmActivitySourceRequest { company_id: CompanyId::new("company").unwrap(), branch_id: Some("branch".into()), department_id: None, source_id: "source-activity".into(), kind: CrmActivitySourceKind::Activity };
        let source = inspect_crm_activity_source(&mut tx,request()).await.unwrap();
        assert_eq!(source.finance_party(),None);
        assert!(!source.has_opportunity());
        source.require_current(&mut tx).await.unwrap();
        let foreign = CrmActivitySourceRequest { company_id: CompanyId::new("foreign").unwrap(), ..request() };
        assert_eq!(business(inspect_crm_activity_source(&mut tx,foreign).await.err().unwrap()).code,"CRM_SOURCE_NOT_FOUND");
        let wrong_branch = CrmActivitySourceRequest { branch_id: Some("other-branch".into()), ..request() };
        assert_eq!(business(inspect_crm_activity_source(&mut tx,wrong_branch).await.err().unwrap()).code,"CRM_SOURCE_NOT_FOUND");
        let department = CrmActivitySourceRequest {department_id:Some("department".into()),..request()};
        inspect_crm_activity_source(&mut tx,department).await.unwrap().require_current(&mut tx).await.unwrap();
        let wrong_department = CrmActivitySourceRequest {department_id:Some("other-department".into()),..request()};
        assert_eq!(business(inspect_crm_activity_source(&mut tx,wrong_department).await.err().unwrap()).code,"CRM_SOURCE_NOT_FOUND");
        sqlx::query("UPDATE crm_contacts SET account_id='account-two' WHERE id='contact-one'").execute(&mut *tx).await.unwrap();
        assert_eq!(business(source.require_current(&mut tx).await.unwrap_err()).code,"CRM_SOURCE_CHANGED");
        tx.rollback().await.unwrap();
        let mut other = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        assert_eq!(business(source.require_current(&mut other).await.unwrap_err()).code,"CRM_SOURCE_CHANGED");
        other.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn activity_source_waits_for_identity_writer_before_snapshot() {
    use sqlx::Connection;
    fixture::with_database(3,|pool|async move {
        setup(&pool).await;
        sqlx::raw_sql("INSERT INTO crm_accounts(id,company_id,kind,name,created_by,created_at,updated_at) VALUES('account-writer','company','business','Account','owner',1,1); INSERT INTO crm_contacts(id,company_id,account_id,name,detail,created_by,created_at,updated_at) VALUES('contact-writer','company','account-writer','Contact','Detail','owner',1,1); UPDATE crm_leads SET account_id='account-writer',contact_id='contact-writer' WHERE id='page-01'; INSERT INTO crm_lead_activities(id,company_id,lead_id,activity_type,subject,details,owner_id,due_date,created_by,created_at,updated_at) VALUES('activity-writer','company','page-01','call','Call customer','Discuss scope','owner','2026-11-20','owner',1,1)").execute(pool.raw()).await.unwrap();
        let mut writer=pool.raw().begin().await.unwrap();
        let writer_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
        sqlx::query("UPDATE crm_accounts SET finance_party_id='customer' WHERE id='account-writer'").execute(&mut *writer).await.unwrap();
        let reader_pool=pool.clone();let (send,receive)=tokio::sync::oneshot::channel();
        let reader=tokio::spawn(async move {
            let mut connection=reader_pool.raw().acquire().await.unwrap();
            let reader_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *connection).await.unwrap();
            let mut tx=connection.begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
            send.send(reader_pid).unwrap();
            lock_crm_activity_sources(&mut tx).await.unwrap();
            let request=CrmActivitySourceRequest {company_id:CompanyId::new("company").unwrap(),branch_id:Some("branch".into()),department_id:None,source_id:"activity-writer".into(),kind:CrmActivitySourceKind::Activity};
            let source=inspect_crm_activity_source(&mut tx,request).await.unwrap();
            assert_eq!(source.finance_party(),Some("customer"));
            source.require_current(&mut tx).await.unwrap();
            tx.rollback().await.unwrap();
        });
        let reader_pid=receive.await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5),async {loop {
            let blocked:bool=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(writer_pid).bind(reader_pid).fetch_one(pool.raw()).await.unwrap();
            if blocked {break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }}).await.expect("CRM source lock did not wait for account writer");
        writer.commit().await.unwrap();reader.await.unwrap();
    }).await;
}

async fn wait_for_writer_block(pool: &DatabasePool, writer_pid: i32) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let blocked: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)))",
            ).bind(writer_pid).fetch_one(pool.raw()).await.unwrap();
            if blocked { break; }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await.expect("activity transaction never waited for the source writer");
}

#[tokio::test]
async fn activity_adapter_persists_actual_event_identity_and_json() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let actor = ActivityActor { user_id: "owner".into(), company_id: "company".into(),
            can_sales: true, can_manage: false, has_read: true, same_person_ids: vec![] };
        let creator = actor.clone();
        PgUnitOfWork::new(pool.clone()).run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
            let creator = creator.clone();
            Box::pin(async move {
                let mut repository = PgCrmActivityCommands::new(connection);
                let input = ActivityCreateInput { lead_id: "page-01".into(), expected_version: 1,
                    activity_type: "call".into(), subject: "  Call customer  ".into(),
                    details: "  Discuss scope  ".into(), owner_id: "owner".into(), due_date: "2026-11-20".into() };
                create_activity(&creator, "create-1", &input, &mut repository, NOW.get()).await?;
                Ok(())
            })
        }).await.unwrap();
        PgUnitOfWork::new(pool.clone()).run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            Box::pin(async move {
                let mut repository = PgCrmActivityCommands::new(connection);
                let input = ActivityChangeInput { action: "complete".into(), expected_version: 1, reason: "  Done well  ".into() };
                change_activity(&actor, "crm-activity-create-1", "complete-1", &input, &mut repository, NOW.get() + 1).await?;
                Ok(())
            })
        }).await.unwrap();
        let rows: Vec<(String, String, String, String, serde_json::Value)> = sqlx::query_as(
            "SELECT id,actor_id,owner_id,due_date,snapshot_json FROM crm_lead_activity_events WHERE activity_id=$1 ORDER BY version")
            .bind("crm-activity-create-1").fetch_all(pool.raw()).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!((&rows[0].0, &rows[1].0), (&"crm-activity-event-create-1".to_string(), &"crm-activity-event-complete-1".to_string()));
        for row in &rows { assert_eq!((&*row.1, &*row.2, &*row.3), ("owner", "owner", "2026-11-20")); }
        assert_eq!(rows[0].4["subject"], "Call customer");
        assert_eq!(rows[1].4["reason"], "Done well");
    }).await;
}

#[tokio::test]
async fn activity_create_cannot_cross_concurrent_opportunity_close() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        // Match the existing TS opportunity command's write order: the
        // opportunity changes first, then the owning lead version advances.
        let mut writer = pool.raw().begin().await.unwrap();
        let writer_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *writer).await.unwrap();
        sqlx::query("UPDATE crm_opportunities SET stage='lost',version=version+1 WHERE id='opp-discovery'")
            .execute(&mut *writer).await.unwrap();
        let reader_pool = pool.clone();
        let create = tokio::spawn(async move {
            let actor = ActivityActor { user_id: "owner".into(), company_id: "company".into(),
                can_sales: true, can_manage: false, has_read: true, same_person_ids: vec![] };
            PgUnitOfWork::new(reader_pool).run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let actor = actor.clone();
                Box::pin(async move {
                    let mut repository = PgCrmActivityCommands::new(connection);
                    let input = ActivityCreateInput { lead_id: "lead-discovery".into(), expected_version: 2,
                        activity_type: "call".into(), subject: "Call customer".into(), details: "Discuss scope".into(),
                        owner_id: "owner".into(), due_date: "2026-11-20".into() };
                    create_activity(&actor, "close-race", &input, &mut repository, NOW.get()).await?;
                    Ok(())
                })
            }).await
        });
        wait_for_writer_block(&pool, writer_pid).await;
        sqlx::query("UPDATE crm_leads SET version=version+1 WHERE id='lead-discovery'")
            .execute(&mut *writer).await.unwrap();
        writer.commit().await.unwrap();
        assert_eq!(business(create.await.unwrap().unwrap_err()).code, "CRM_LEAD_VERSION_CHANGED");
        let count: i64 = sqlx::query_scalar("SELECT (SELECT count(*) FROM crm_lead_activities WHERE id='crm-activity-close-race') + (SELECT count(*) FROM crm_lead_activity_events WHERE activity_id='crm-activity-close-race')")
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(count, 0);
    }).await;
}

#[tokio::test]
async fn activity_create_cannot_cross_concurrent_lead_disqualification() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let mut writer = pool.raw().begin().await.unwrap();
        let writer_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *writer).await.unwrap();
        sqlx::query("UPDATE crm_leads SET status='disqualified',version=version+1 WHERE id='page-01'")
            .execute(&mut *writer).await.unwrap();
        let reader_pool = pool.clone();
        let create = tokio::spawn(async move {
            let actor = ActivityActor { user_id: "owner".into(), company_id: "company".into(),
                can_sales: true, can_manage: false, has_read: true, same_person_ids: vec![] };
            PgUnitOfWork::new(reader_pool).run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let actor = actor.clone();
                Box::pin(async move {
                    let mut repository = PgCrmActivityCommands::new(connection);
                    let input = ActivityCreateInput { lead_id: "page-01".into(), expected_version: 1,
                        activity_type: "call".into(), subject: "Call customer".into(), details: "Discuss scope".into(),
                        owner_id: "owner".into(), due_date: "2026-11-20".into() };
                    create_activity(&actor, "lead-race", &input, &mut repository, NOW.get()).await?;
                    Ok(())
                })
            }).await
        });
        wait_for_writer_block(&pool, writer_pid).await;
        writer.commit().await.unwrap();
        assert_eq!(business(create.await.unwrap().unwrap_err()).code, "CRM_LEAD_VERSION_CHANGED");
        let count: i64 = sqlx::query_scalar("SELECT (SELECT count(*) FROM crm_lead_activities WHERE id='crm-activity-lead-race') + (SELECT count(*) FROM crm_lead_activity_events WHERE activity_id='crm-activity-lead-race')")
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(count, 0);
    }).await;
}

#[tokio::test]
async fn activity_create_waits_for_concurrent_qualification_insert() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let mut writer = pool.raw().begin().await.unwrap();
        let writer_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
        // Qualification inserts the opportunity before changing the lead.
        sqlx::query("INSERT INTO crm_opportunities(id,lead_id,customer_id,expected_minor,currency,close_date,created_by,created_at,updated_at) VALUES('opp-new','page-01','customer',100,'THB','2026-12-01','owner',100,100)")
            .execute(&mut *writer).await.unwrap();
        let reader_pool = pool.clone();
        let create = tokio::spawn(async move {
            let actor = ActivityActor { user_id: "owner".into(), company_id: "company".into(), can_sales: true,
                can_manage: false, has_read: true, same_person_ids: vec![] };
            PgUnitOfWork::new(reader_pool).run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let actor = actor.clone();
                Box::pin(async move {
                    let mut repository = PgCrmActivityCommands::new(connection);
                    let input = ActivityCreateInput { lead_id: "page-01".into(), expected_version: 1,
                        activity_type: "call".into(), subject: "Call customer".into(), details: "Discuss scope".into(),
                        owner_id: "owner".into(), due_date: "2026-11-20".into() };
                    create_activity(&actor, "qualify-race", &input, &mut repository, NOW.get()).await?;
                    Ok(())
                })
            }).await
        });
        wait_for_writer_block(&pool, writer_pid).await;
        sqlx::query("UPDATE crm_leads SET status='qualified',version=version+1 WHERE id='page-01'")
            .execute(&mut *writer).await.unwrap();
        writer.commit().await.unwrap();
        assert_eq!(business(create.await.unwrap().unwrap_err()).code, "CRM_LEAD_VERSION_CHANGED");
        let count: i64 = sqlx::query_scalar("SELECT (SELECT count(*) FROM crm_lead_activities WHERE id='crm-activity-qualify-race') + (SELECT count(*) FROM crm_lead_activity_events WHERE activity_id='crm-activity-qualify-race')")
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(count, 0);
    }).await;
}

#[tokio::test]
async fn opportunity_close_waits_for_prior_activity_create() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let mut creator = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *creator).await.unwrap();
        let creator_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *creator).await.unwrap();
        let actor = ActivityActor { user_id: "owner".into(), company_id: "company".into(), can_sales: true,
            can_manage: false, has_read: true, same_person_ids: vec![] };
        let input = ActivityCreateInput { lead_id: "lead-discovery".into(), expected_version: 2,
            activity_type: "call".into(), subject: "Call customer".into(), details: "Discuss scope".into(),
            owner_id: "owner".into(), due_date: "2026-11-20".into() };
        create_activity(&actor, "prior-create", &input, &mut PgCrmActivityCommands::new(&mut creator), NOW.get()).await.unwrap();
        let writer_pool = pool.clone();
        let close = tokio::spawn(async move {
            let mut writer = writer_pool.raw().begin().await.unwrap();
            sqlx::query("UPDATE crm_opportunities SET stage='lost',version=version+1 WHERE id='opp-discovery'")
                .execute(&mut *writer).await.unwrap();
            sqlx::query("UPDATE crm_leads SET version=version+1 WHERE id='lead-discovery'")
                .execute(&mut *writer).await.unwrap();
            writer.commit().await.unwrap();
        });
        wait_for_writer_block(&pool, creator_pid).await;
        creator.commit().await.unwrap();
        close.await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT (SELECT count(*) FROM crm_lead_activities WHERE id='crm-activity-prior-create') + (SELECT count(*) FROM crm_lead_activity_events WHERE activity_id='crm-activity-prior-create')")
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(count, 2);
        let stage: String = sqlx::query_scalar("SELECT stage FROM crm_opportunities WHERE id='opp-discovery'")
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(stage, "lost");
    }).await;
}

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
