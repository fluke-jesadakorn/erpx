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
    lock_crm_activity_sources, CrmReviewedAccountLinkRequest,
    inspect_crm_reviewed_account_link, lock_crm_reviewed_account_link_sources,
};
use erpx_server_crm::public::leads_native::{
    LeadActor, LeadChangeInput, LeadCreateInput, OpportunityChangeInput, PgCrmLeadCommands, apply_crm_qualification, apply_crm_qualification_retained, change_lead, create_lead,
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
async fn qualification_owner_effect_retains_exact_value_and_both_events() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql("INSERT INTO crm_accounts(id,company_id,kind,name,finance_party_id,reviewed_by,created_by,created_at,updated_at) VALUES('qualify-account','company','business','Reviewed','customer','reviewer','owner',1,1); UPDATE crm_leads SET account_id='qualify-account' WHERE id IN ('page-01','page-02','page-03','page-04','page-05'); INSERT INTO crm_case_events(id,lead_id,version,action,actor_id,reason,snapshot_json,created_at) VALUES('crm-event-forced-deal','page-06',1,'create','owner','Existing','{}',1)").execute(pool.raw()).await.unwrap();
        let manager = LeadActor { user_id: "owner".into(), company_id: "company".into(), can_read: true, can_sales: true, can_manage: false, can_pms: false, can_support: false, same_person_ids: vec![] };
        let input = LeadChangeInput { action: "qualify".into(), expected_version: 1, owner_id: None, due_date: None, reason: "  Confirmed customer  ".into(), customer_id: Some("customer".into()), expected_value: Some("999999999999.99".into()), close_date: Some("0000-02-29".into()) };
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_crm_reviewed_account_link_sources(&mut tx).await.unwrap();
        let link = inspect_crm_reviewed_account_link(&mut tx, CrmReviewedAccountLinkRequest { company_id: CompanyId::new("company").unwrap(), branch_id: Some("branch".into()), department_id: None, lead_id: "page-01".into(), finance_party_id: "customer".into() }).await.unwrap();
        let mut wrong_actor = manager.clone(); wrong_actor.user_id = "different".into(); wrong_actor.same_person_ids = vec!["owner".into()];
        assert_eq!(business(apply_crm_qualification(&mut tx, &wrong_actor, "page-01", "qualify-wrong", &input, "THB", &link, NOW.get()).await.unwrap_err()).code, "CRM_OWNER_REQUIRED");
        let retained = apply_crm_qualification_retained(&mut tx, &manager, "page-01", "qualify-date", &input, "THB", &link, NOW.get()).await.unwrap();
        retained.require_current(&mut tx).await.unwrap();
        let id = retained.facts().opportunity_id.clone();
        assert_eq!(id, "opportunity-qualify-date");
        assert_eq!((&*retained.facts().company_id,&*retained.facts().lead_id,&*retained.facts().owner_id,retained.facts().version,retained.facts().updated_at), ("company","page-01","owner",2,NOW.get()));
        let opportunity: (i64, String, String) = sqlx::query_as("SELECT expected_minor,currency,close_date FROM crm_opportunities WHERE id=$1").bind(&id).fetch_one(&mut *tx).await.unwrap();
        assert_eq!(opportunity, (99_999_999_999_999, "THB".into(), "0000-02-29".into()));
        let lead: (String, i64) = sqlx::query_as("SELECT status,version FROM crm_leads WHERE id='page-01'").fetch_one(&mut *tx).await.unwrap();
        assert_eq!(lead, ("qualified".into(), 2));
        let events: Vec<(String, Option<String>, i64, serde_json::Value, String, String, String)> = sqlx::query_as("SELECT id,opportunity_id,version,snapshot_json,action,actor_id,reason FROM crm_case_events WHERE lead_id='page-01' ORDER BY id").fetch_all(&mut *tx).await.unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!((&*events[0].0, events[0].1.as_deref(), events[0].2), ("crm-event-qualify-date-deal", Some(id.as_str()), 1));
        assert_eq!((&*events[1].0, events[1].1.as_deref(), events[1].2), ("crm-event-qualify-date-lead", None, 2));
        assert_eq!(events[0].3["closeDate"], "0000-02-29");
        assert_eq!(events[0].3, events[1].3);
        assert_eq!(events[0].3, serde_json::json!({"action":"qualify","expectedVersion":1,"customerId":"customer","expectedValue":"999999999999.99","closeDate":"0000-02-29","reason":"Confirmed customer"}));
        for event in &events { assert_eq!((&*event.4, &*event.5, &*event.6), ("qualify", "owner", "Confirmed customer")); }
        sqlx::query("SAVEPOINT alter_qualification").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE crm_opportunities SET close_date='0000-01-01' WHERE id=$1").bind(&id).execute(&mut *tx).await.unwrap();
        assert_eq!(business(retained.require_current(&mut tx).await.unwrap_err()).code,"CRM_QUALIFICATION_CHANGED");
        sqlx::query("ROLLBACK TO SAVEPOINT alter_qualification").execute(&mut *tx).await.unwrap();
        retained.require_current(&mut tx).await.unwrap();
        for (lead_id, key, value, expected) in [("page-02", "money-tenth", "1.1", 110_i64), ("page-03", "money-cent", "1.01", 101), ("page-04", "money-leading", "0001.01", 101)] {
            let link = inspect_crm_reviewed_account_link(&mut tx, CrmReviewedAccountLinkRequest { company_id: CompanyId::new("company").unwrap(), branch_id: Some("branch".into()), department_id: None, lead_id: lead_id.into(), finance_party_id: "customer".into() }).await.unwrap();
            let mut amount = input.clone(); amount.expected_value = Some(value.into());
            let id = apply_crm_qualification(&mut tx, &manager, lead_id, key, &amount, "THB", &link, NOW.get()).await.unwrap();
            let stored: i64 = sqlx::query_scalar("SELECT expected_minor FROM crm_opportunities WHERE id=$1").bind(id).fetch_one(&mut *tx).await.unwrap();
            assert_eq!(stored, expected, "{value}");
        }
        let failure_link = inspect_crm_reviewed_account_link(&mut tx, CrmReviewedAccountLinkRequest { company_id: CompanyId::new("company").unwrap(), branch_id: Some("branch".into()), department_id: None, lead_id: "page-05".into(), finance_party_id: "customer".into() }).await.unwrap();
        sqlx::raw_sql("CREATE FUNCTION crm_test_qualification_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.id='opportunity-trigger-drift' THEN NEW.expected_minor=NEW.expected_minor+1; END IF; RETURN NEW; END $$; CREATE TRIGGER crm_test_qualification_drift BEFORE INSERT ON crm_opportunities FOR EACH ROW EXECUTE FUNCTION crm_test_qualification_drift()").execute(&mut *tx).await.unwrap();
        let drift=apply_crm_qualification_retained(&mut tx,&manager,"page-05","trigger-drift",&input,"THB",&failure_link,NOW.get()).await.err().unwrap();
        assert_eq!(business(drift).code,"CRM_QUALIFICATION_CHANGED");
        let drift_count:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM crm_opportunities WHERE id='opportunity-trigger-drift')+(SELECT count(*) FROM crm_case_events WHERE id IN ('crm-event-trigger-drift-deal','crm-event-trigger-drift-lead'))").fetch_one(&mut *tx).await.unwrap();
        assert_eq!(drift_count,0);
        sqlx::raw_sql("CREATE FUNCTION crm_test_lead_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.id='page-05' AND NEW.status='qualified' THEN NEW.title='Trigger changed source'; END IF; RETURN NEW; END $$; CREATE TRIGGER crm_test_lead_drift BEFORE UPDATE ON crm_leads FOR EACH ROW EXECUTE FUNCTION crm_test_lead_drift()").execute(&mut *tx).await.unwrap();
        let lead_drift=apply_crm_qualification_retained(&mut tx,&manager,"page-05","lead-drift",&input,"THB",&failure_link,NOW.get()).await.err().unwrap();
        assert_eq!(business(lead_drift).code,"CRM_QUALIFICATION_CHANGED");
        let old_wrapper_drift=apply_crm_qualification(&mut tx,&manager,"page-05","wrapper-drift",&input,"THB",&failure_link,NOW.get()).await.unwrap_err();
        assert_eq!(business(old_wrapper_drift).code,"CRM_QUALIFICATION_CHANGED");
        let original:(String,String,i64,i64)=sqlx::query_as("SELECT title,status,version,(SELECT count(*) FROM crm_opportunities WHERE id='opportunity-lead-drift') FROM crm_leads WHERE id='page-05'").fetch_one(&mut *tx).await.unwrap();
        assert_eq!(original,("Pipeline page 5".into(),"open".into(),1,0));
        let wrapper_count:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM crm_opportunities WHERE id='opportunity-wrapper-drift')+(SELECT count(*) FROM crm_case_events WHERE id IN ('crm-event-wrapper-drift-deal','crm-event-wrapper-drift-lead'))").fetch_one(&mut *tx).await.unwrap();
        assert_eq!(wrapper_count,0);
        let failure = apply_crm_qualification(&mut tx, &manager, "page-05", "forced", &input, "THB", &failure_link, NOW.get()).await.unwrap_err();
        assert!(matches!(failure, PgTransactionError::Persistence(ref error) if error.sql_state() == Some("23505")));
        let after_failure: (String, i64, i64) = sqlx::query_as("SELECT status,version,(SELECT count(*) FROM crm_opportunities WHERE id='opportunity-forced') FROM crm_leads WHERE id='page-05'").fetch_one(&mut *tx).await.unwrap();
        assert_eq!(after_failure, ("open".into(), 1, 0));
        failure_link.require_current(&mut tx).await.unwrap();
        tx.rollback().await.unwrap();
        let mut wrong_unit = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *wrong_unit).await.unwrap();
        assert_eq!(business(retained.require_current(&mut wrong_unit).await.unwrap_err()).code,"CRM_SOURCE_CHANGED");
        wrong_unit.rollback().await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM crm_opportunities WHERE id='opportunity-qualify-date'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(count, 0);
        let mut committed = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *committed).await.unwrap();
        lock_crm_reviewed_account_link_sources(&mut committed).await.unwrap();
        let link = inspect_crm_reviewed_account_link(&mut committed, CrmReviewedAccountLinkRequest { company_id: CompanyId::new("company").unwrap(), branch_id: Some("branch".into()), department_id: None, lead_id: "page-01".into(), finance_party_id: "customer".into() }).await.unwrap();
        apply_crm_qualification(&mut committed, &manager, "page-01", "qualify-commit", &input, "THB", &link, NOW.get()).await.unwrap();
        committed.commit().await.unwrap();
        let stored: (String, i64, i64, String, i64) = sqlx::query_as("SELECT l.status,l.version,o.expected_minor,o.close_date,(SELECT count(*) FROM crm_case_events e WHERE e.lead_id=l.id AND e.action='qualify') FROM crm_leads l JOIN crm_opportunities o ON o.lead_id=l.id WHERE l.id='page-01' AND o.id='opportunity-qualify-commit'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(stored, ("qualified".into(), 2, 99_999_999_999_999, "0000-02-29".into(), 2));
    }).await;
}

#[tokio::test]
async fn reviewed_account_link_is_scoped_retained_and_bound_to_the_original_unit() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql("INSERT INTO crm_accounts(id,company_id,kind,name,finance_party_id,reviewed_by,created_by,created_at,updated_at) VALUES('reviewed-account','company','business','Reviewed','customer','reviewer','owner',1,1),('unreviewed-account','company','business','Unreviewed','customer',NULL,'owner',1,1); UPDATE crm_leads SET account_id='reviewed-account',department_id='department' WHERE id='page-01'").execute(pool.raw()).await.unwrap();
        let request = || CrmReviewedAccountLinkRequest { company_id: CompanyId::new("company").unwrap(), branch_id: Some("branch".into()), department_id: Some("department".into()), lead_id: "page-01".into(), finance_party_id: "customer".into() };
        let mut no_lock = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *no_lock).await.unwrap();
        assert!(inspect_crm_reviewed_account_link(&mut no_lock, request()).await.is_err());
        no_lock.rollback().await.unwrap();
        let mut savepoint = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *savepoint).await.unwrap();
        sqlx::query("SAVEPOINT before_source_locks").execute(&mut *savepoint).await.unwrap();
        lock_crm_reviewed_account_link_sources(&mut savepoint).await.unwrap();
        let released = inspect_crm_reviewed_account_link(&mut savepoint, request()).await.unwrap();
        released.require_current(&mut savepoint).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT before_source_locks").execute(&mut *savepoint).await.unwrap();
        assert_eq!(business(released.require_current(&mut savepoint).await.unwrap_err()).code, "CRM_SOURCE_CHANGED");
        savepoint.rollback().await.unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_crm_reviewed_account_link_sources(&mut tx).await.unwrap();
        let source = inspect_crm_reviewed_account_link(&mut tx, request()).await.unwrap();
        source.require_current(&mut tx).await.unwrap();
        let foreign = CrmReviewedAccountLinkRequest { company_id: CompanyId::new("foreign").unwrap(), ..request() };
        assert_eq!(business(inspect_crm_reviewed_account_link(&mut tx, foreign).await.err().unwrap()).code, "CRM_SOURCE_NOT_FOUND");
        let branch = CrmReviewedAccountLinkRequest { branch_id: Some("other-branch".into()), ..request() };
        assert_eq!(business(inspect_crm_reviewed_account_link(&mut tx, branch).await.err().unwrap()).code, "CRM_SOURCE_NOT_FOUND");
        let department = CrmReviewedAccountLinkRequest { department_id: Some("other-department".into()), ..request() };
        assert_eq!(business(inspect_crm_reviewed_account_link(&mut tx, department).await.err().unwrap()).code, "CRM_SOURCE_NOT_FOUND");
        let party = CrmReviewedAccountLinkRequest { finance_party_id: "customer-2".into(), ..request() };
        assert_eq!(business(inspect_crm_reviewed_account_link(&mut tx, party).await.err().unwrap()).code, "CRM_SOURCE_NOT_FOUND");
        sqlx::query("UPDATE crm_accounts SET reviewed_by='another-reviewer' WHERE id='reviewed-account'").execute(&mut *tx).await.unwrap();
        assert_eq!(business(source.require_current(&mut tx).await.unwrap_err()).code, "CRM_SOURCE_CHANGED");
        sqlx::query("UPDATE crm_leads SET account_id='unreviewed-account' WHERE id='page-01'").execute(&mut *tx).await.unwrap();
        assert_eq!(business(inspect_crm_reviewed_account_link(&mut tx, request()).await.err().unwrap()).code, "CRM_SOURCE_NOT_FOUND");
        tx.rollback().await.unwrap();
        let mut other = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        assert_eq!(business(source.require_current(&mut other).await.unwrap_err()).code, "CRM_SOURCE_CHANGED");
        other.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn year_zero_dates_persist_through_native_commands_and_pipeline() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql("INSERT INTO crm_accounts(id,company_id,kind,name,created_by,created_at,updated_at) VALUES('date-account','company','business','Date account','owner',1,1); INSERT INTO crm_contacts(id,company_id,account_id,name,detail,created_by,created_at,updated_at) VALUES('date-contact','company','date-account','Date contact','Detail','owner',1,1)")
            .execute(pool.raw()).await.unwrap();
        let manager = LeadActor { user_id: "owner".into(), company_id: "company".into(), can_read: true, can_sales: true, can_manage: true, can_pms: false, can_support: false, same_person_ids: vec![] };
        let input: LeadCreateInput = serde_json::from_value(serde_json::json!({
            "branchId":"branch", "title":"Year zero unique lead", "contactName":"Date contact",
            "contactDetail":"Detail", "source":"staff inquiry", "brief":"Date boundary",
            "accountId":"date-account", "contactId":"date-contact", "dueDate":"0000-02-29"
        })).unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        create_lead(&manager, "date-create", &input, &mut PgCrmLeadCommands::new(&mut tx), NOW.get()).await.unwrap();
        tx.commit().await.unwrap();
        let stored: (String, String, i64) = sqlx::query_as("SELECT owner_id,due_date,version FROM crm_leads WHERE id='lead-date-create'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(stored, ("owner".into(), "0000-02-29".into(), 1));

        let change = LeadChangeInput { action: "reassign".into(), expected_version: 1, owner_id: Some("new-owner".into()), due_date: Some("0000-01-01".into()), reason: "Manager handoff".into(), customer_id: None, expected_value: None, close_date: None };
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        change_lead(&manager, "lead-date-create", "date-reassign", &change, &mut PgCrmLeadCommands::new(&mut tx), NOW.get() + 1).await.unwrap();
        tx.commit().await.unwrap();
        let stored: (String, String, i64) = sqlx::query_as("SELECT owner_id,due_date,version FROM crm_leads WHERE id='lead-date-create'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(stored, ("new-owner".into(), "0000-01-01".into(), 2));

        let activity_actor = ActivityActor { user_id: "new-owner".into(), company_id: "company".into(), can_sales: true, can_manage: false, has_read: true, same_person_ids: vec![] };
        let activity = ActivityCreateInput { lead_id: "lead-date-create".into(), expected_version: 2, activity_type: "call".into(), subject: "Date boundary call".into(), details: "Check date persistence".into(), owner_id: "new-owner".into(), due_date: "0000-02-29".into() };
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        create_activity(&activity_actor, "date-activity", &activity, &mut PgCrmActivityCommands::new(&mut tx), NOW.get() + 2).await.unwrap();
        tx.commit().await.unwrap();
        let dates: (String, String, serde_json::Value) = sqlx::query_as("SELECT a.due_date,e.due_date,e.snapshot_json FROM crm_lead_activities a JOIN crm_lead_activity_events e ON e.activity_id=a.id WHERE a.id='crm-activity-date-activity'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!((&*dates.0, &*dates.1), ("0000-02-29", "0000-02-29"));
        assert_eq!(dates.2["dueDate"], "0000-02-29");

        let mut invalid = input.clone();
        invalid.due_date = "0000-02-30".into();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        let error = create_lead(&manager, "date-invalid", &invalid, &mut PgCrmLeadCommands::new(&mut tx), NOW.get()).await.unwrap_err();
        assert_eq!(business(error).code, "CRM_LEAD_INPUT_INVALID");
        let counts: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM crm_leads WHERE id='lead-date-invalid'), (SELECT count(*) FROM crm_case_events WHERE lead_id='lead-date-invalid')").fetch_one(&mut *tx).await.unwrap();
        assert_eq!(counts, (0, 0));
        tx.rollback().await.unwrap();
        let counts: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM crm_leads WHERE id='lead-date-invalid'), (SELECT count(*) FROM crm_case_events WHERE lead_id='lead-date-invalid')").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(counts, (0, 0));

        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        let page = crm_pipeline(&actor("company", Some("branch"), None, &["commerce.read", "commerce.sales"]), &CrmPipelineQuery { search: "Year zero unique lead".into(), ..Default::default() }, &mut PgCrmPipelineRead::new(&mut tx), &mut People, &mut Customers, &mut Packages, NOW).await.unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!((&*page.items[0].id, &*page.items[0].owner_id, &*page.items[0].due_date), ("lead-date-create", "new-owner", "0000-01-01"));
        tx.rollback().await.unwrap();

        // Qualification composition is not native yet: exercise persisted readback
        // using an existing qualified opportunity, without claiming a command handoff.
        sqlx::query("UPDATE crm_opportunities SET close_date='0000-02-29' WHERE id='opp-discovery'").execute(pool.raw()).await.unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        let page = crm_pipeline(&actor("company", Some("branch"), None, &["commerce.read", "commerce.sales"]), &CrmPipelineQuery { search: "Strong discovery lead".into(), ..Default::default() }, &mut PgCrmPipelineRead::new(&mut tx), &mut People, &mut Customers, &mut Packages, NOW).await.unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, "lead-discovery");
        assert_eq!(page.items[0].close_date.as_deref(), Some("0000-02-29"));
        tx.rollback().await.unwrap();
    }).await;
}

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

#[tokio::test]
async fn native_lead_and_opportunity_commands_write_cas_events_and_retention() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let manager = LeadActor {
            user_id: "owner".into(),
            company_id: "company".into(),
            can_read: true,
            can_sales: true,
            can_manage: true,
            can_pms: false,
            can_support: false,
            same_person_ids: vec![],
        };
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        let disqualify = LeadChangeInput {
            action: "disqualify".into(),
            expected_version: 1,
            owner_id: None,
            due_date: None,
            reason: "No budget this quarter".into(),
            customer_id: None,
            expected_value: None,
            close_date: None,
        };
        let prepared = PgCrmLeadCommands::new(&mut tx)
            .prepare_lead_change(&manager, "page-01", &disqualify)
            .await
            .unwrap();
        let retained = PgCrmLeadCommands::new(&mut tx)
            .apply_lead_change(&manager, "page-01", "command-disqualify", prepared, NOW.get())
            .await
            .unwrap();
        assert_eq!(retained.facts().version, 2);
        assert_eq!(retained.facts().status, "disqualified");
        retained.require_current(&mut tx).await.unwrap();
        let event: (String, i64, String, serde_json::Value) = sqlx::query_as(
            "SELECT id,version,action,snapshot_json FROM crm_case_events WHERE lead_id='page-01'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!((&*event.0, event.1, &*event.2), ("crm-event-command-disqualify-lead", 2, "disqualify"));
        assert_eq!(
            event.3,
            serde_json::json!({"action":"disqualify","expectedVersion":1,"reason":"No budget this quarter"})
        );

        let reopen = LeadChangeInput {
            action: "reopen".into(),
            expected_version: 2,
            owner_id: None,
            due_date: None,
            reason: "Budget restored".into(),
            customer_id: None,
            expected_value: None,
            close_date: None,
        };
        let prepared = PgCrmLeadCommands::new(&mut tx)
            .prepare_lead_change(&manager, "page-01", &reopen)
            .await
            .unwrap();
        let retained = PgCrmLeadCommands::new(&mut tx)
            .apply_lead_change(&manager, "page-01", "command-reopen", prepared, NOW.get() + 1)
            .await
            .unwrap();
        assert_eq!((retained.facts().version, retained.facts().status.as_str()), (3, "open"));
        let stale = PgCrmLeadCommands::new(&mut tx)
            .prepare_lead_change(&manager, "page-01", &reopen)
            .await;
        assert_eq!(
            business(stale.err().unwrap()).code,
            "CRM_LEAD_VERSION_CHANGED"
        );
        sqlx::query("UPDATE crm_leads SET status='disqualified' WHERE id='page-01'")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert_eq!(
            business(retained.require_current(&mut tx).await.unwrap_err()).code,
            "CRM_LEAD_CHANGED"
        );
        sqlx::query("UPDATE crm_leads SET status='open' WHERE id='page-01'")
            .execute(&mut *tx)
            .await
            .unwrap();
        retained.require_current(&mut tx).await.unwrap();
        tx.commit().await.unwrap();

        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        let lose = OpportunityChangeInput {
            action: "lose".into(),
            expected_version: 1,
            lead_version: 2,
            quote_id: None,
            quote_version: None,
            loss_reason: Some("timing".into()),
            sales_id: None,
            sales_version: None,
            reservation_id: None,
            property_id: None,
            reason: "Customer deferred".into(),
        };
        let prepared = PgCrmLeadCommands::new(&mut tx)
            .prepare_opportunity_change(&manager, "lead-discovery", &lose)
            .await
            .unwrap();
        let retained = PgCrmLeadCommands::new(&mut tx)
            .apply_opportunity_change(&manager, "lead-discovery", "command-lose", prepared, NOW.get() + 2)
            .await
            .unwrap();
        assert_eq!(
            (retained.facts().lead_version, retained.facts().opportunity_version, retained.facts().stage.as_str()),
            (3, 2, "lost")
        );
        retained.require_current(&mut tx).await.unwrap();
        let events: Vec<(String, Option<String>, i64, String)> = sqlx::query_as(
            "SELECT id,opportunity_id,version,action FROM crm_case_events WHERE lead_id='lead-discovery' ORDER BY id",
        )
        .fetch_all(&mut *tx)
        .await
        .unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!((&*events[0].0, events[0].2, &*events[0].3), ("crm-event-command-lose-deal", 2, "lose"));
        assert_eq!((&*events[1].0, events[1].2, &*events[1].3), ("crm-event-command-lose-lead", 3, "opportunity-lose"));

        let reopen = OpportunityChangeInput {
            action: "reopen".into(),
            expected_version: 2,
            lead_version: 3,
            quote_id: None,
            quote_version: None,
            loss_reason: None,
            sales_id: None,
            sales_version: None,
            reservation_id: None,
            property_id: None,
            reason: "Customer returned".into(),
        };
        let prepared = PgCrmLeadCommands::new(&mut tx)
            .prepare_opportunity_change(&manager, "lead-discovery", &reopen)
            .await
            .unwrap();
        let retained = PgCrmLeadCommands::new(&mut tx)
            .apply_opportunity_change(&manager, "lead-discovery", "command-win-reopen", prepared, NOW.get() + 3)
            .await
            .unwrap();
        assert_eq!(
            (retained.facts().lead_version, retained.facts().opportunity_version, retained.facts().stage.as_str()),
            (4, 3, "discovery")
        );
        let stored: (String, i64) = sqlx::query_as(
            "SELECT stage,version FROM crm_opportunities WHERE id='opp-discovery'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(stored, ("discovery".into(), 3));

        let mut stale = reopen.clone();
        stale.lead_version = 4;
        stale.expected_version = 2;
        let error = PgCrmLeadCommands::new(&mut tx)
            .prepare_opportunity_change(&manager, "lead-discovery", &stale)
            .await
            .err()
            .unwrap();
        assert_eq!(business(error).code, "CRM_OPPORTUNITY_VERSION_CHANGED");

        let reassign = LeadChangeInput {
            action: "reassign".into(),
            expected_version: 4,
            owner_id: Some("new-owner".into()),
            due_date: Some("2026-12-15".into()),
            reason: "Territory change".into(),
            customer_id: None,
            expected_value: None,
            close_date: None,
        };
        let prepared = PgCrmLeadCommands::new(&mut tx)
            .prepare_lead_change(&manager, "lead-discovery", &reassign)
            .await
            .unwrap();
        let retained = PgCrmLeadCommands::new(&mut tx)
            .apply_lead_change(&manager, "lead-discovery", "command-reassign", prepared, NOW.get() + 4)
            .await
            .unwrap();
        assert_eq!(
            (retained.facts().owner_id.as_str(), retained.facts().version),
            ("new-owner", 5)
        );
        retained.require_current(&mut tx).await.unwrap();

        sqlx::raw_sql(
            "INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,currency,order_date,due_date,state,subtotal_minor,tax_minor,source_id,version,created_by,created_at,updated_at) VALUES              ('quote-test','company','branch',NULL,'quote','Q-TEST','customer','THB','2026-11-01','2026-12-01','confirmed',1234,0,NULL,1,'owner',100,100),             ('sale-test','company','branch',NULL,'sales','S-TEST','customer','THB','2026-11-01','2026-12-01','confirmed',1234,0,'quote-test',1,'owner',100,100)",
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        let link = OpportunityChangeInput {
            action: "link-quote".into(),
            expected_version: 1,
            lead_version: 2,
            quote_id: Some("quote-test".into()),
            quote_version: Some(1),
            loss_reason: None,
            sales_id: None,
            sales_version: None,
            reservation_id: None,
            property_id: None,
            reason: "Approved quote".into(),
        };
        let mut prepared = PgCrmLeadCommands::new(&mut tx)
            .prepare_opportunity_change(&manager, "lead-proposal", &link)
            .await
            .unwrap();
        prepared.reference_minor = Some(1234);
        let retained = PgCrmLeadCommands::new(&mut tx)
            .apply_opportunity_change(&manager, "lead-proposal", "command-link-quote", prepared, NOW.get() + 5)
            .await
            .unwrap();
        assert_eq!(
            (retained.facts().opportunity_version, retained.facts().stage.as_str()),
            (2, "proposal")
        );
        retained.require_current(&mut tx).await.unwrap();
        let stored: (String, i64, Option<String>) = sqlx::query_as(
            "SELECT stage,expected_minor,quote_id FROM crm_opportunities WHERE id='opp-proposal'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(stored, ("proposal".into(), 1234, Some("quote-test".into())));

        let negotiate = OpportunityChangeInput {
            action: "negotiate".into(),
            expected_version: 2,
            lead_version: 3,
            quote_id: None,
            quote_version: None,
            loss_reason: None,
            sales_id: None,
            sales_version: None,
            reservation_id: None,
            property_id: None,
            reason: "Negotiating".into(),
        };
        let prepared = PgCrmLeadCommands::new(&mut tx)
            .prepare_opportunity_change(&manager, "lead-proposal", &negotiate)
            .await
            .unwrap();
        let retained = PgCrmLeadCommands::new(&mut tx)
            .apply_opportunity_change(&manager, "lead-proposal", "command-negotiate", prepared, NOW.get() + 6)
            .await
            .unwrap();
        assert_eq!(retained.facts().stage, "negotiation");
        retained.require_current(&mut tx).await.unwrap();

        sqlx::query("UPDATE commerce_orders SET state='converted' WHERE id='quote-test'")
            .execute(&mut *tx)
            .await
            .unwrap();
        let win = OpportunityChangeInput {
            action: "win".into(),
            expected_version: 3,
            lead_version: 4,
            quote_id: None,
            quote_version: None,
            loss_reason: None,
            sales_id: Some("sale-test".into()),
            sales_version: Some(1),
            reservation_id: None,
            property_id: None,
            reason: "Customer signed".into(),
        };
        let mut prepared = PgCrmLeadCommands::new(&mut tx)
            .prepare_opportunity_change(&manager, "lead-proposal", &win)
            .await
            .unwrap();
        prepared.reference_minor = Some(1234);
        let retained = PgCrmLeadCommands::new(&mut tx)
            .apply_opportunity_change(&manager, "lead-proposal", "command-win", prepared, NOW.get() + 7)
            .await
            .unwrap();
        assert_eq!(
            (retained.facts().lead_version, retained.facts().opportunity_version, retained.facts().stage.as_str()),
            (5, 4, "won")
        );
        retained.require_current(&mut tx).await.unwrap();
        let stored: (String, Option<String>, String, i64) = sqlx::query_as(
            "SELECT stage,sales_id,win_source,expected_minor FROM crm_opportunities WHERE id='opp-proposal'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(stored, ("won".into(), Some("sale-test".into()), "sales".into(), 1234));
        let events: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM crm_case_events WHERE lead_id='lead-proposal'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(events, 6);

        let mut qualify = disqualify.clone();
        qualify.action = "qualify".into();
        qualify.customer_id = Some("customer".into());
        qualify.expected_value = Some("100.00".into());
        qualify.close_date = Some("2026-12-01".into());
        let error = PgCrmLeadCommands::new(&mut tx)
            .prepare_lead_change(&manager, "page-02", &qualify)
            .await
            .err()
            .unwrap();
        assert_eq!(business(error).code, "CRM_COMMAND_NOT_NATIVE");
        tx.rollback().await.unwrap();
    })
    .await;
}
