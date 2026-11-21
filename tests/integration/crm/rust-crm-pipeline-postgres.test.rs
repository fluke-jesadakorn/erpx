//! Native CRM pipeline reads over real migrated PostgreSQL. These prove owner
//! read SQL, Company/branch/department isolation, cursor paging, forecast
//! aggregation, serializable transaction guards, rollback and persisted-source
//! readback. They are not a CRM command, package or browser journey.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_crm::public::accounts::{
    AccountLinkFinanceActor, AccountLinkFinanceInput, PgCrmAccountCreate, link_account_finance,
};
use erpx_server_crm::public::activities_native::{
    ActivityActor, ActivityChangeInput, ActivityCreateInput, PgCrmActivityCommands,
    change_activity, create_activity,
};
use erpx_server_crm::public::activity_source::{
    CrmActivitySourceKind, CrmActivitySourceRequest, inspect_crm_activity_source, inspect_crm_activity_recipient_source,
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
        let prepared = PgCrmLeadCommands::new(&mut tx).prepare_crm_qualification(&manager, "page-01", &input).await.unwrap();
        assert_eq!((prepared.plan.action.as_str(), prepared.record.branch_id.as_str(), prepared.record.department_id.as_deref(), prepared.record.version), ("qualify", "branch", None, 1));
        let before:(String,i64,i64,Option<String>)=sqlx::query_as("SELECT status,version,(SELECT count(*) FROM public.crm_opportunities WHERE lead_id='page-01'),(SELECT opportunity_id::text FROM public.crm_case_events WHERE lead_id='page-01' AND action='qualify' LIMIT 1) FROM public.crm_leads WHERE id='page-01'").fetch_one(&mut *tx).await.unwrap();
        assert_eq!(before,("open".into(),1,0,None));
        let mut wrong_actor = manager.clone(); wrong_actor.user_id = "different".into(); wrong_actor.same_person_ids = vec!["owner".into()];
        assert_eq!(business(PgCrmLeadCommands::new(&mut tx).prepare_crm_qualification(&wrong_actor, "page-01", &input).await.err().unwrap()).code, "CRM_OWNER_REQUIRED");
        assert_eq!(business(apply_crm_qualification(&mut tx, &wrong_actor, "page-01", "qualify-wrong", &input, "THB", &link, NOW.get()).await.unwrap_err()).code, "CRM_OWNER_REQUIRED");
        let retained = apply_crm_qualification_retained(&mut tx, &manager, "page-01", "qualify-date", &input, "THB", &link, NOW.get()).await.unwrap();
        retained.require_current(&mut tx).await.unwrap();
        let owned = retained.retain_owned();
        owned.require_current(&mut tx, &link).await.unwrap();
        let id = retained.facts().opportunity_id.clone();
        assert_eq!((&*owned.facts().opportunity_id, owned.facts().version), ("opportunity-qualify-date", 2));
        assert_eq!(id, "opportunity-qualify-date");
        assert_eq!((&*retained.facts().company_id,&*retained.facts().lead_id,&*retained.facts().owner_id,retained.facts().branch_id.as_str(),retained.facts().department_id.as_deref(),retained.facts().version,retained.facts().updated_at), ("company","page-01","owner","branch",None,2,NOW.get()));
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
        assert_eq!(business(owned.require_current(&mut tx, &link).await.unwrap_err()).code,"CRM_QUALIFICATION_CHANGED");
        sqlx::query("ROLLBACK TO SAVEPOINT alter_qualification").execute(&mut *tx).await.unwrap();
        retained.require_current(&mut tx).await.unwrap();
        owned.require_current(&mut tx, &link).await.unwrap();
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
async fn native_lead_create_persists_identity_create_event_and_selected_department() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let actor = LeadActor {
            user_id: "owner".into(),
            company_id: "company".into(),
            can_read: true,
            can_sales: true,
            can_manage: false,
            can_pms: false,
            can_support: false,
            same_person_ids: vec![],
        };
        let input: LeadCreateInput = serde_json::from_value(serde_json::json!({
            "branchId": "branch", "title": "  Native create lead  ",
            "contactName": "  Ada Lovelace  ", "contactDetail": "  Contact detail  ",
            "source": "  staff inquiry  ", "brief": "  Native create brief  ",
            "dueDate": "2026-11-20"
        }))
        .unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        let plan = create_lead(
            &actor,
            "create-native",
            &input,
            Some("department"),
            &mut PgCrmLeadCommands::new(&mut tx),
            NOW.get(),
        )
        .await
        .unwrap();
        assert_eq!(
            (&*plan.branch_id, &*plan.title, &*plan.contact_name, &*plan.contact_detail, &*plan.source, &*plan.brief),
            ("branch", "Native create lead", "Ada Lovelace", "Contact detail", "staff inquiry", "Native create brief")
        );
        let account: (String, String) = sqlx::query_as(
            "SELECT kind,name FROM public.crm_accounts WHERE id='account-create-native' AND company_id='company'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(account, ("individual".into(), "Ada Lovelace".into()));
        let contact: (String, String) = sqlx::query_as(
            "SELECT name,detail FROM public.crm_contacts WHERE id='contact-create-native' AND account_id='account-create-native' AND company_id='company'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(contact, ("Ada Lovelace".into(), "Contact detail".into()));
        let lead: (String, String, String, i64, String, String) = sqlx::query_as(
            "SELECT branch_id,department_id,owner_id,version,account_id,contact_id FROM public.crm_leads WHERE id='lead-create-native' AND company_id='company'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(
            lead,
            (
                "branch".into(),
                "department".into(),
                "owner".into(),
                1,
                "account-create-native".into(),
                "contact-create-native".into()
            )
        );
        let event: (String, i64, String, String, String, serde_json::Value) = sqlx::query_as(
            "SELECT id,version,action,actor_id,reason,snapshot_json FROM public.crm_case_events WHERE lead_id='lead-create-native'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(
            (&*event.0, event.1, &*event.2, &*event.3, &*event.4),
            ("crm-event-create-native-lead", 1, "create", "owner", "Native create brief")
        );
        assert_eq!(
            event.5,
            serde_json::json!({
                "branchId": "branch",
                "title": "Native create lead",
                "contactName": "Ada Lovelace",
                "contactDetail": "Contact detail",
                "source": "staff inquiry",
                "brief": "Native create brief",
                "dueDate": "2026-11-20"
            })
        );
        tx.rollback().await.unwrap();
        let counts: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM public.crm_leads WHERE id='lead-create-native'),(SELECT count(*) FROM public.crm_accounts WHERE id='account-create-native'),(SELECT count(*) FROM public.crm_contacts WHERE id='contact-create-native'),(SELECT count(*) FROM public.crm_case_events WHERE lead_id='lead-create-native')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (0, 0, 0, 0));

        let mut committed = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *committed).await.unwrap();
        create_lead(
            &actor,
            "create-native",
            &input,
            Some("department"),
            &mut PgCrmLeadCommands::new(&mut committed),
            NOW.get(),
        )
        .await
        .unwrap();
        committed.commit().await.unwrap();
        let persisted: (String, String, String, String) = sqlx::query_as(
            "SELECT l.branch_id,l.department_id,a.kind,e.action FROM public.crm_leads l JOIN public.crm_accounts a ON a.id=l.account_id JOIN public.crm_case_events e ON e.lead_id=l.id WHERE l.id='lead-create-native'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(persisted, ("branch".into(), "department".into(), "individual".into(), "create".into()));
    })
    .await;
}

#[tokio::test]
async fn native_lead_create_uses_existing_identity_or_fails_closed() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO crm_accounts(id,company_id,kind,name,created_by,created_at,updated_at) VALUES \
             ('existing-account','company','business','Existing account','owner',1,1),\
             ('foreign-account','foreign','business','Foreign account','owner',1,1);\
             INSERT INTO crm_contacts(id,company_id,account_id,name,detail,created_by,created_at,updated_at) VALUES \
             ('existing-contact','company','existing-account','Existing contact','Detail','owner',1,1),\
             ('foreign-contact','foreign','foreign-account','Foreign contact','Detail','owner',1,1);",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let actor = LeadActor {
            user_id: "owner".into(),
            company_id: "company".into(),
            can_read: true,
            can_sales: true,
            can_manage: false,
            can_pms: false,
            can_support: false,
            same_person_ids: vec![],
        };
        let body = serde_json::json!({
            "branchId": "branch", "title": "Existing identity lead",
            "contactName": "Existing contact", "contactDetail": "Detail",
            "source": "staff inquiry", "brief": "Existing identity brief",
            "dueDate": "2026-11-20"
        });
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        let mut input: LeadCreateInput = serde_json::from_value(body.clone()).unwrap();
        input.account_id = Some("existing-account".into());
        input.contact_id = Some("existing-contact".into());
        create_lead(&actor, "create-existing", &input, None, &mut PgCrmLeadCommands::new(&mut tx), NOW.get()).await.unwrap();
        let (account_id, contact_id): (String, String) = sqlx::query_as(
            "SELECT account_id,contact_id FROM public.crm_leads WHERE id='lead-create-existing'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!((account_id.as_str(), contact_id.as_str()), ("existing-account", "existing-contact"));
        let snapshot: serde_json::Value = sqlx::query_scalar(
            "SELECT snapshot_json FROM public.crm_case_events WHERE id='crm-event-create-existing-lead'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(
            snapshot,
            serde_json::json!({
                "branchId": "branch",
                "title": "Existing identity lead",
                "contactName": "Existing contact",
                "contactDetail": "Detail",
                "source": "staff inquiry",
                "accountId": "existing-account",
                "contactId": "existing-contact",
                "brief": "Existing identity brief",
                "dueDate": "2026-11-20"
            })
        );
        let identities: i64 = sqlx::query_scalar(
            "SELECT (SELECT count(*) FROM public.crm_accounts) + (SELECT count(*) FROM public.crm_contacts)",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(identities, 4);

        for (key, account, contact, expected) in [
            ("create-missing-account", Some("missing-account"), None, "CONFLICT"),
            ("create-foreign-account", Some("foreign-account"), None, "CONFLICT"),
            ("create-wrong-contact", Some("existing-account"), Some("foreign-contact"), "CONFLICT"),
        ] {
            let mut invalid: LeadCreateInput = serde_json::from_value(body.clone()).unwrap();
            invalid.account_id = account.map(str::to_owned);
            invalid.contact_id = contact.map(str::to_owned);
            let error = create_lead(
                &actor,
                key,
                &invalid,
                None,
                &mut PgCrmLeadCommands::new(&mut tx),
                NOW.get(),
            )
            .await
            .unwrap_err();
            assert_eq!(business(error).code, expected, "{key}");
            let lead_id = format!("lead-{key}");
            let effects: (i64, i64, i64) = sqlx::query_as(
                "SELECT (SELECT count(*) FROM public.crm_leads WHERE id=$1),(SELECT count(*) FROM public.crm_accounts WHERE id=$2),(SELECT count(*) FROM public.crm_case_events WHERE id=$3)",
            )
            .bind(&lead_id)
            .bind(format!("account-{key}"))
            .bind(format!("crm-event-{key}-lead"))
            .fetch_one(&mut *tx)
            .await
            .unwrap();
            assert_eq!(effects, (0, 0, 0), "{key}");
        }
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn owner_source_creates_identity_qualifies_links_quote_and_wins() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let actor = LeadActor {
            user_id: "owner".into(),
            company_id: "company".into(),
            can_read: true,
            can_sales: true,
            can_manage: false,
            can_pms: false,
            can_support: false,
            same_person_ids: vec![],
        };
        let create: LeadCreateInput = serde_json::from_value(serde_json::json!({
            "branchId": "branch", "title": "Flow opportunity lead",
            "contactName": "Flow Customer", "contactDetail": "Reviewed customer contact",
            "source": "staff inquiry", "brief": "Qualify and win this opportunity",
            "dueDate": "2026-11-20"
        }))
        .unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        create_lead(&actor, "flow-create", &create, None, &mut PgCrmLeadCommands::new(&mut tx), NOW.get()).await.unwrap();
        lock_crm_reviewed_account_link_sources(&mut tx).await.unwrap();
        let reviewer = AccountLinkFinanceActor {
            user_id: "reviewer".into(),
            company_id: "company".into(),
            branch_id: Some("branch".into()),
            department_id: None,
            can_review: true,
        };
        let link_input: AccountLinkFinanceInput = serde_json::from_value(serde_json::json!({
            "action": "link-finance",
            "accountId": "account-flow-create",
            "customerId": "customer",
            "expectedVersion": 1,
            "evidence": "Independent Finance customer review"
        }))
        .unwrap();
        link_account_finance(
            &reviewer,
            &link_input,
            &mut PgCrmAccountCreate::new(&mut tx),
            NOW.get(),
        )
        .await
        .unwrap();
        let link = inspect_crm_reviewed_account_link(
            &mut tx,
            CrmReviewedAccountLinkRequest {
                company_id: CompanyId::new("company").unwrap(),
                branch_id: Some("branch".into()),
                department_id: None,
                lead_id: "lead-flow-create".into(),
                finance_party_id: "customer".into(),
            },
        )
        .await
        .unwrap();
        let qualify = LeadChangeInput {
            action: "qualify".into(),
            expected_version: 1,
            owner_id: None,
            due_date: None,
            reason: "Customer confirmed the opportunity".into(),
            customer_id: Some("customer".into()),
            expected_value: Some("100.00".into()),
            close_date: Some("2026-12-01".into()),
        };
        let prepared = PgCrmLeadCommands::new(&mut tx)
            .prepare_crm_qualification(&actor, "lead-flow-create", &qualify)
            .await
            .unwrap();
        assert_eq!((prepared.record.version, prepared.record.owner_id.as_str()), (1, "owner"));
        let qualified = apply_crm_qualification_retained(
            &mut tx,
            &actor,
            "lead-flow-create",
            "flow-qualify",
            &qualify,
            "THB",
            &link,
            NOW.get() + 1,
        )
        .await
        .unwrap();
        qualified.require_current(&mut tx).await.unwrap();
        let opportunity_id = qualified.facts().opportunity_id.clone();
        assert_eq!(opportunity_id, "opportunity-flow-qualify");

        sqlx::raw_sql(
            "INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,currency,order_date,due_date,state,subtotal_minor,tax_minor,source_id,version,created_by,created_at,updated_at) VALUES \
             ('flow-quote','company','branch',NULL,'quote','Q-FLOW','customer','THB','2026-11-01','2026-12-01','confirmed',10000,0,NULL,1,'owner',100,100),\
             ('flow-sale','company','branch',NULL,'sales','S-FLOW','customer','THB','2026-11-01','2026-12-01','confirmed',10000,0,'flow-quote',1,'owner',100,100)",
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        let link_quote = OpportunityChangeInput {
            action: "link-quote".into(),
            expected_version: 1,
            lead_version: 2,
            quote_id: Some("flow-quote".into()),
            quote_version: Some(1),
            loss_reason: None,
            sales_id: None,
            sales_version: None,
            reservation_id: None,
            property_id: None,
            reason: "Approved quotation linked".into(),
        };
        let mut prepared = PgCrmLeadCommands::new(&mut tx)
            .prepare_opportunity_change(&actor, "lead-flow-create", &link_quote)
            .await
            .unwrap();
        prepared.reference_minor = Some(10000);
        let linked = PgCrmLeadCommands::new(&mut tx)
            .apply_opportunity_change(&actor, "lead-flow-create", "flow-link-quote", prepared, NOW.get() + 2)
            .await
            .unwrap();
        assert_eq!((linked.facts().lead_version, linked.facts().opportunity_version, linked.facts().stage.as_str()), (3, 2, "proposal"));
        linked.require_current(&mut tx).await.unwrap();

        sqlx::query("UPDATE commerce_orders SET state='converted' WHERE id='flow-quote'")
            .execute(&mut *tx)
            .await
            .unwrap();
        let win = OpportunityChangeInput {
            action: "win".into(),
            expected_version: 2,
            lead_version: 3,
            quote_id: None,
            quote_version: None,
            loss_reason: None,
            sales_id: Some("flow-sale".into()),
            sales_version: Some(1),
            reservation_id: None,
            property_id: None,
            reason: "Confirmed source sales order wins".into(),
        };
        let mut prepared = PgCrmLeadCommands::new(&mut tx)
            .prepare_opportunity_change(&actor, "lead-flow-create", &win)
            .await
            .unwrap();
        prepared.reference_minor = Some(10000);
        let won = PgCrmLeadCommands::new(&mut tx)
            .apply_opportunity_change(&actor, "lead-flow-create", "flow-win", prepared, NOW.get() + 3)
            .await
            .unwrap();
        assert_eq!((won.facts().lead_version, won.facts().opportunity_version, won.facts().stage.as_str()), (4, 3, "won"));
        won.require_current(&mut tx).await.unwrap();
        tx.commit().await.unwrap();

        let stored: (String, i64, String, Option<String>, i64, i64) = sqlx::query_as(
            "SELECT l.status,l.version,o.stage,o.sales_id,o.expected_minor,(SELECT count(*) FROM public.crm_case_events WHERE lead_id=l.id) FROM public.crm_leads l JOIN public.crm_opportunities o ON o.lead_id=l.id WHERE l.id='lead-flow-create' AND o.id=$1",
        )
        .bind(&opportunity_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(stored, ("qualified".into(), 4, "won".into(), Some("flow-sale".into()), 10000, 7));
        let identity: (String, String, String) = sqlx::query_as(
            "SELECT a.kind,a.finance_party_id,a.reviewed_by FROM public.crm_leads l JOIN public.crm_accounts a ON a.id=l.account_id WHERE l.id='lead-flow-create'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(identity, ("individual".into(), "customer".into(), "reviewer".into()));
    })
    .await;
}

#[tokio::test]
async fn native_link_finance_requires_version_independent_review_and_selected_scope() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO crm_accounts(id,company_id,kind,name,created_by,created_at,updated_at) VALUES \
             ('link-account','company','business','Link account','creator',1,1),\
             ('creator-account','company','business','Creator account','reviewer',1,1),\
             ('branch-account','company','business','Branch account','creator',1,1),\
             ('foreign-link-account','foreign','business','Foreign account','creator',1,1);\
             INSERT INTO crm_leads(id,company_id,branch_id,department_id,title,contact_name,contact_detail,source,brief,owner_id,due_date,status,version,created_by,created_at,updated_at,account_id) VALUES \
             ('link-lead','company','branch','department','Link lead','Contact','Detail','Source','Brief','owner','2026-12-01','open',1,'creator',1,1,'link-account'),\
             ('branch-link-lead','company','other-branch',NULL,'Branch lead','Contact','Detail','Source','Brief','owner','2026-12-01','open',1,'creator',1,1,'branch-account');",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let reviewer = AccountLinkFinanceActor {
            user_id: "reviewer".into(),
            company_id: "company".into(),
            branch_id: Some("branch".into()),
            department_id: None,
            can_review: true,
        };
        let input = |account: &str| -> AccountLinkFinanceInput {
            serde_json::from_value(serde_json::json!({
                "action": "link-finance", "accountId": account,
                "customerId": "customer", "expectedVersion": 1,
                "evidence": "  Independent Finance review  "
            }))
            .unwrap()
        };
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_crm_reviewed_account_link_sources(&mut tx).await.unwrap();
        let prepared = PgCrmAccountCreate::new(&mut tx).prepare_link_finance(&reviewer, &input("link-account")).await.unwrap();
        assert_eq!((prepared.account_id.as_str(), prepared.expected_version), ("link-account", 1));
        let plan = link_account_finance(&reviewer, &input("link-account"), &mut PgCrmAccountCreate::new(&mut tx), NOW.get()).await.unwrap();
        assert_eq!((plan.account_id.as_str(), plan.customer_id.as_str(), plan.evidence.as_str()), ("link-account", "customer", "Independent Finance review"));
        let stored: (String, String, String, i64) = sqlx::query_as(
            "SELECT finance_party_id,reviewed_by,review_evidence,version FROM public.crm_accounts WHERE id='link-account'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(stored, ("customer".into(), "reviewer".into(), "Independent Finance review".into(), 2));
        let link = inspect_crm_reviewed_account_link(
            &mut tx,
            CrmReviewedAccountLinkRequest {
                company_id: CompanyId::new("company").unwrap(),
                branch_id: Some("branch".into()),
                department_id: Some("department".into()),
                lead_id: "link-lead".into(),
                finance_party_id: "customer".into(),
            },
        )
        .await
        .unwrap();
        link.require_current(&mut tx).await.unwrap();

        for account in ["link-account", "creator-account", "branch-account", "foreign-link-account", "missing-account"] {
            assert!(PgCrmAccountCreate::new(&mut tx).prepare_link_finance(&reviewer, &input(account)).await.is_err(), "{account}");
            let error = link_account_finance(&reviewer, &input(account), &mut PgCrmAccountCreate::new(&mut tx), NOW.get()).await.unwrap_err();
            assert_eq!(business(error).code, "CONFLICT", "{account}");
        }
        let mut no_review = reviewer.clone();
        no_review.can_review = false;
        assert_eq!(
            business(link_account_finance(&no_review, &input("link-account"), &mut PgCrmAccountCreate::new(&mut tx), NOW.get()).await.unwrap_err()).code,
            "CRM_ACCOUNT_REVIEW_REQUIRED"
        );
        let untouched: (Option<String>, i64) = sqlx::query_as(
            "SELECT finance_party_id,version FROM public.crm_accounts WHERE id='creator-account'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(untouched, (None, 1));
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn reviewed_account_link_uses_public_relation_under_shadow_search_path() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO crm_accounts(id,company_id,kind,name,finance_party_id,reviewed_by,created_by,created_at,updated_at) VALUES('shadow-target','company','business','Reviewed','customer','reviewer','owner',1,1); UPDATE crm_leads SET account_id='shadow-target' WHERE id='page-01'",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let request = || CrmReviewedAccountLinkRequest {
            company_id: CompanyId::new("company").unwrap(),
            branch_id: Some("branch".into()),
            department_id: None,
            lead_id: "page-01".into(),
            finance_party_id: "customer".into(),
        };
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_crm_reviewed_account_link_sources(&mut tx).await.unwrap();
        sqlx::raw_sql(
            "CREATE TEMP TABLE crm_accounts (LIKE public.crm_accounts INCLUDING DEFAULTS);\
             CREATE TEMP TABLE crm_leads (LIKE public.crm_leads INCLUDING DEFAULTS);\
             INSERT INTO pg_temp.crm_accounts(id,company_id,kind,name,finance_party_id,reviewed_by,created_by,created_at,updated_at) VALUES('shadow-target','company','business','Shadow','customer',NULL,'owner',1,1);\
             INSERT INTO pg_temp.crm_leads(id,company_id,branch_id,title,contact_name,contact_detail,source,brief,owner_id,due_date,created_by,created_at,updated_at,account_id) VALUES('page-01','company','branch','Shadow lead','Contact','Detail','Source','Brief','owner','2026-11-20','owner',1,1,'shadow-target');\
             SET LOCAL search_path TO pg_temp, public",
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        let shadowed: bool = sqlx::query_scalar(
            "SELECT to_regclass('crm_accounts')=to_regclass('pg_temp.crm_accounts') AND to_regclass('crm_leads')=to_regclass('pg_temp.crm_leads')",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert!(shadowed);
        let source = inspect_crm_reviewed_account_link(&mut tx, request()).await.unwrap();
        source.require_current(&mut tx).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT reviewed_by FROM public.crm_accounts WHERE id='shadow-target'")
                .fetch_one(&mut *tx)
                .await
                .unwrap(),
            "reviewer"
        );
        sqlx::query("SET LOCAL search_path TO pg_catalog, public").execute(&mut *tx).await.unwrap();
        assert_eq!(
            business(source.require_current(&mut tx).await.unwrap_err()).code,
            "CRM_SOURCE_CHANGED"
        );
        tx.rollback().await.unwrap();
    })
    .await;
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
        create_lead(&manager, "date-create", &input, None, &mut PgCrmLeadCommands::new(&mut tx), NOW.get()).await.unwrap();
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

        let activity_actor = ActivityActor { user_id: "new-owner".into(), company_id: "company".into(), branch_id: None, department_id: None, can_sales: true, can_manage: false, has_read: true, same_person_ids: vec![] };
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
        let error = create_lead(&manager, "date-invalid", &invalid, None, &mut PgCrmLeadCommands::new(&mut tx), NOW.get()).await.unwrap_err();
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
        let request = || CrmActivitySourceRequest { company_id: CompanyId::new("company").unwrap(), branch_id: Some("branch".into()), department_id: None, source_id: "source-activity".into(), kind: CrmActivitySourceKind::Activity };
        let mut unlocked=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *unlocked).await.unwrap();
        assert!(inspect_crm_activity_source(&mut unlocked,request()).await.is_err());
        unlocked.rollback().await.unwrap();
        let mut released=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *released).await.unwrap();
        sqlx::query("SAVEPOINT source_locks").execute(&mut *released).await.unwrap();
        lock_crm_activity_sources(&mut released).await.unwrap();
        let old_source=inspect_crm_activity_source(&mut released,request()).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT source_locks").execute(&mut *released).await.unwrap();
        assert!(old_source.require_current(&mut released).await.is_err());
        released.rollback().await.unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        sqlx::query("CREATE TEMP TABLE crm_accounts(id text)").execute(&mut *tx).await.unwrap();
        lock_crm_activity_sources(&mut tx).await.unwrap();
        let source = inspect_crm_activity_source(&mut tx,request()).await.unwrap();
        assert_eq!(source.finance_party(),None);
        assert!(!source.has_opportunity());
        source.require_current(&mut tx).await.unwrap();
        sqlx::query("UPDATE public.crm_lead_activities SET status='completed',version=version+1,owner_id='new-owner' WHERE id='source-activity'").execute(&mut *tx).await.unwrap();
        source.require_current(&mut tx).await.unwrap();
        let original_path:String=sqlx::query_scalar("SHOW search_path").fetch_one(&mut *tx).await.unwrap();
        let changed_path=if original_path=="pg_catalog, public" { "public, pg_catalog" } else { "pg_catalog, public" };
        sqlx::query("SELECT set_config('search_path',$1,true)").bind(changed_path).execute(&mut *tx).await.unwrap();
        assert!(source.require_current(&mut tx).await.is_err());
        sqlx::query("SELECT set_config('search_path',$1,true)").bind(original_path).execute(&mut *tx).await.unwrap();
        source.require_current(&mut tx).await.unwrap();
        let foreign = CrmActivitySourceRequest { company_id: CompanyId::new("foreign").unwrap(), ..request() };
        assert_eq!(business(inspect_crm_activity_source(&mut tx,foreign).await.err().unwrap()).code,"CRM_SOURCE_NOT_FOUND");
        let wrong_branch = CrmActivitySourceRequest { branch_id: Some("other-branch".into()), ..request() };
        assert_eq!(business(inspect_crm_activity_source(&mut tx,wrong_branch).await.err().unwrap()).code,"CRM_SOURCE_NOT_FOUND");
        let department = CrmActivitySourceRequest {department_id:Some("department".into()),..request()};
        inspect_crm_activity_source(&mut tx,department).await.unwrap().require_current(&mut tx).await.unwrap();
        let wrong_department = CrmActivitySourceRequest {department_id:Some("other-department".into()),..request()};
        assert_eq!(business(inspect_crm_activity_source(&mut tx,wrong_department).await.err().unwrap()).code,"CRM_SOURCE_NOT_FOUND");
        sqlx::query("UPDATE public.crm_contacts SET account_id='account-two' WHERE id='contact-one'").execute(&mut *tx).await.unwrap();
        assert_eq!(business(source.require_current(&mut tx).await.unwrap_err()).code,"CRM_SOURCE_CHANGED");
        tx.rollback().await.unwrap();
        let mut other = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        assert_eq!(business(source.require_current(&mut other).await.unwrap_err()).code,"CRM_SOURCE_CHANGED");
        other.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn activity_recipient_witness_uses_retained_lead_owner_and_selected_scope() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::query("INSERT INTO crm_lead_activities(id,company_id,lead_id,activity_type,subject,details,owner_id,due_date,created_by,created_at,updated_at) VALUES ('recipient-activity','company','lead-department','call','Call','Discuss','activity-owner','2026-11-20','owner',100,100)").execute(pool.raw()).await.unwrap();
        let request=|| CrmActivitySourceRequest { company_id:CompanyId::new("company").unwrap(),branch_id:Some("branch".into()),department_id:Some("department".into()),source_id:"recipient-activity".into(),kind:CrmActivitySourceKind::Activity };
        let mut unlocked=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *unlocked).await.unwrap();
        assert!(inspect_crm_activity_recipient_source(&mut unlocked,request()).await.is_err());
        unlocked.rollback().await.unwrap();
        let mut savepoint=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *savepoint).await.unwrap();
        sqlx::query("SAVEPOINT recipient_locks").execute(&mut *savepoint).await.unwrap();
        lock_crm_activity_sources(&mut savepoint).await.unwrap();
        let released=inspect_crm_activity_recipient_source(&mut savepoint,request()).await.unwrap();
        released.require_current(&mut savepoint).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT recipient_locks").execute(&mut *savepoint).await.unwrap();
        assert!(released.require_current(&mut savepoint).await.is_err());
        assert!(inspect_crm_activity_recipient_source(&mut savepoint,request()).await.is_err());
        savepoint.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        sqlx::query("CREATE TEMP TABLE crm_lead_activities (id text)").execute(&mut *tx).await.unwrap();
        lock_crm_activity_sources(&mut tx).await.unwrap();
        let source=inspect_crm_activity_recipient_source(&mut tx,request()).await.unwrap();
        let facts=source.facts();
        assert_eq!((&*facts.company_id,&*facts.lead_id,&*facts.lead_owner_id,&*facts.branch_id,facts.department_id.as_deref(),&*facts.activity_owner_id,facts.activity_version,facts.activity_status.as_str()),("company","lead-department","owner","branch",Some("department"),"activity-owner",1,"open"));
        source.require_current(&mut tx).await.unwrap();
        let oracle=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required"))
            .current_dir(concat!(env!("CARGO_MANIFEST_DIR"),"/../.."))
            .args(["--conditions=react-server","-e","import {salesRecipientEligibility} from './packages/server-sales/src/adapters/recipient.ts'; process.stdout.write(salesRecipientEligibility('l','l.owner_id'))"])
            .output().unwrap();
        assert!(oracle.status.success());
        let predicate=String::from_utf8(oracle.stdout).unwrap();
        // The interpolated SQL consists exclusively of the fixed owner CTE and
        // actual repository TS predicate; all source values come from witness binds.
        let sql=format!("WITH {} SELECT l.owner_id,({predicate}) AS eligible FROM crm_activity_recipient_source l",source.cte_sql());
        let row=source.bind_source(sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))).fetch_one(&mut *tx).await.unwrap();
        let owner:String=sqlx::Row::try_get(&row,"owner_id").unwrap();
        let actual:bool=sqlx::Row::try_get(&row,"eligible").unwrap();
        let legacy_sql=format!("SELECT l.owner_id,({predicate}) AS eligible FROM public.crm_lead_activities x JOIN public.crm_leads l ON l.id=x.lead_id AND l.company_id=x.company_id WHERE x.id=$1 AND x.company_id=$2 AND ($3::text IS NULL OR l.branch_id=$3) AND ($4::text IS NULL OR l.department_id=$4)");
        let legacy=source.bind_source(sqlx::query(sqlx::AssertSqlSafe(legacy_sql.as_str()))).fetch_one(&mut *tx).await.unwrap();
        assert_eq!((owner.as_str(),actual),(sqlx::Row::try_get::<String,_>(&legacy,"owner_id").unwrap().as_str(),sqlx::Row::try_get::<bool,_>(&legacy,"eligible").unwrap()));
        assert_eq!(owner,"owner");
        source.require_current(&mut tx).await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        assert!(source.require_current(&mut other).await.is_err());
        other.rollback().await.unwrap();
        let original_path:String=sqlx::query_scalar("SHOW search_path").fetch_one(&mut *tx).await.unwrap();
        let changed_path=if original_path=="pg_catalog, public" { "public, pg_catalog" } else { "pg_catalog, public" };
        sqlx::query("SELECT set_config('search_path',$1,true)").bind(changed_path).execute(&mut *tx).await.unwrap();
        assert!(source.require_current(&mut tx).await.is_err());
        sqlx::query("SELECT set_config('search_path',$1,true)").bind(original_path).execute(&mut *tx).await.unwrap();
        source.require_current(&mut tx).await.unwrap();
        let mut wrong=request();wrong.branch_id=Some("other-branch".into());
        assert!(inspect_crm_activity_recipient_source(&mut tx,wrong).await.is_err());
        sqlx::query("UPDATE crm_leads SET owner_id='changed-owner' WHERE id='lead-department'").execute(&mut *tx).await.unwrap();
        assert_eq!(business(source.require_current(&mut tx).await.unwrap_err()).code,"CRM_SOURCE_CHANGED");
        tx.rollback().await.unwrap();
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
        let actor = ActivityActor { user_id: "owner".into(), company_id: "company".into(), branch_id: None, department_id: None,
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
                let input = ActivityChangeInput { action: "complete".into(), expected_version: 1, reason: "  Done well  ".into() , owner_id: None, due_date: None, };
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
async fn activity_selected_scope_blocks_foreign_leads_and_activity_writes() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::query("INSERT INTO crm_lead_activities(id,company_id,lead_id,activity_type,subject,details,owner_id,due_date,created_by,created_at,updated_at) VALUES ('other-activity','company','lead-other-branch','call','Call','Discuss','owner','2026-11-20','owner',100,100),('dept-activity','company','lead-department','call','Call','Discuss','owner','2026-11-20','owner',100,100),('foreign-activity','foreign','lead-foreign','call','Call','Discuss','owner','2026-11-20','owner',100,100)").execute(pool.raw()).await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        let scoped=ActivityActor {user_id:"owner".into(),company_id:"company".into(),branch_id:Some("branch".into()),department_id:Some("department".into()),can_sales:true,can_manage:false,has_read:true,same_person_ids:vec![]};
        let mut repo=PgCrmActivityCommands::new(&mut tx);
        for id in ["lead-other-branch","page-01","lead-foreign"] {
            let input=ActivityCreateInput {lead_id:id.into(),expected_version:1,activity_type:"call".into(),subject:"Call".into(),details:"Discuss".into(),owner_id:"owner".into(),due_date:"2026-11-20".into()};
            assert_eq!(business(create_activity(&scoped,"scope-denied",&input,&mut repo,NOW.get()).await.unwrap_err()).code,"CRM_LEAD_NOT_FOUND");
        }
        let change=ActivityChangeInput {action:"complete".into(),expected_version:1,reason:"Done".into(),owner_id:None,due_date:None};
        for id in ["other-activity","foreign-activity"] {
            assert_eq!(business(change_activity(&scoped,id,"scope-change",&change,&mut repo,NOW.get()).await.unwrap_err()).code,"CRM_ACTIVITY_NOT_FOUND");
        }
        assert!(repo.lead(&scoped,"lead-department").await.unwrap().is_some());
        assert!(repo.activity(&scoped,"dept-activity").await.unwrap().is_some());
        let broad=ActivityActor {branch_id:None,department_id:None,..scoped.clone()};
        assert!(repo.lead(&broad,"lead-other-branch").await.unwrap().is_some());
        assert!(repo.lead(&broad,"page-01").await.unwrap().is_some());
        assert!(repo.activity(&broad,"other-activity").await.unwrap().is_some());
        assert!(repo.activity(&broad,"foreign-activity").await.unwrap().is_none());
        drop(repo);
        let effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM crm_lead_activities WHERE id='crm-activity-scope-denied'),(SELECT count(*) FROM crm_lead_activity_events WHERE id='crm-activity-event-scope-change'),(SELECT count(*) FROM crm_lead_activities WHERE id IN ('other-activity','foreign-activity') AND status <> 'open')").fetch_one(&mut *tx).await.unwrap();
        assert_eq!(effects,(0,0,0));
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn scoped_activity_waits_for_actual_lead_disqualification() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let mut writer = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *writer).await.unwrap();
        let writer_pid: i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
        let lead_actor=LeadActor { user_id:"owner".into(),company_id:"company".into(),can_read:true,can_sales:true,can_manage:false,can_pms:false,can_support:false,same_person_ids:vec![] };
        let input=LeadChangeInput { action:"disqualify".into(),expected_version:1,owner_id:None,due_date:None,reason:"No budget".into(),customer_id:None,expected_value:None,close_date:None };
        let mut lead_repo=PgCrmLeadCommands::new(&mut writer);
        let prepared=lead_repo.prepare_lead_change(&lead_actor,"lead-department",&input).await.unwrap();
        lead_repo.apply_lead_change(&lead_actor,"lead-department","scope-lead-change",prepared,NOW.get()).await.unwrap();
        drop(lead_repo);
        let (send,receive)=tokio::sync::oneshot::channel();
        let reader_pool=pool.clone();
        let reader=tokio::spawn(async move {
            let mut tx=reader_pool.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
            let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *tx).await.unwrap();
            send.send(pid).unwrap();
            let actor=ActivityActor {user_id:"owner".into(),company_id:"company".into(),branch_id:Some("branch".into()),department_id:Some("department".into()),can_sales:true,can_manage:false,has_read:true,same_person_ids:vec![]};
            let input=ActivityCreateInput {lead_id:"lead-department".into(),expected_version:1,activity_type:"call".into(),subject:"Call".into(),details:"Discuss".into(),owner_id:"owner".into(),due_date:"2026-11-20".into()};
            let result=create_activity(&actor,"scope-race",&input,&mut PgCrmActivityCommands::new(&mut tx),NOW.get()).await;
            match result {
                Err(PgTransactionError::Persistence(error)) => assert_eq!(error.sql_state(), Some("40001")),
                other => panic!("expected serialization conflict, got {other:?}"),
            }
            tx.rollback().await.unwrap();
        });
        let reader_pid=receive.await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5),async {loop {
            let blocked:bool=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(writer_pid).bind(reader_pid).fetch_one(pool.raw()).await.unwrap();
            if blocked {break;} tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }}).await.expect("selected scope activity did not wait on its actual lead writer");
        writer.commit().await.unwrap();
        reader.await.unwrap();
        let result:(String,i64) = sqlx::query_as("SELECT status,version FROM crm_leads WHERE id='lead-department'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(result,("disqualified".into(),2));
        let effects:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM crm_lead_activities WHERE id='crm-activity-scope-race'),(SELECT count(*) FROM crm_lead_activity_events WHERE activity_id='crm-activity-scope-race'),(SELECT count(*) FROM command_receipts WHERE command_id='scope-race'),(SELECT count(*) FROM audit_events WHERE command_id='scope-race'),(SELECT count(*) FROM outbox_events WHERE command_id='scope-race')").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(effects,(0,0,0,0,0));
        let lead_event:i64=sqlx::query_scalar("SELECT count(*) FROM crm_case_events WHERE lead_id='lead-department' AND action='disqualify' AND version=2").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(lead_event,1);
    }).await;
}

#[tokio::test]
async fn activity_reassign_stages_cas_event_and_rollback_in_original_unit() {
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;
        sqlx::query("INSERT INTO crm_lead_activities(id,company_id,lead_id,activity_type,subject,details,owner_id,due_date,status,outcome,created_by,created_at,updated_at) VALUES('reassign-source','company','lead-department','call','Call','Discuss','old-owner','2026-11-20','open','prior','maker',1,1)").execute(pool.raw()).await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        sqlx::raw_sql("CREATE TEMP TABLE crm_lead_activities (LIKE public.crm_lead_activities INCLUDING DEFAULTS); CREATE TEMP TABLE crm_leads (LIKE public.crm_leads INCLUDING DEFAULTS); INSERT INTO pg_temp.crm_lead_activities SELECT * FROM public.crm_lead_activities WHERE id='reassign-source'; INSERT INTO pg_temp.crm_leads SELECT * FROM public.crm_leads WHERE id='lead-department'; UPDATE pg_temp.crm_lead_activities SET status='completed' WHERE id='reassign-source'; SET LOCAL search_path TO pg_temp, public").execute(&mut *tx).await.unwrap();
        let shadowed:bool=sqlx::query_scalar("SELECT to_regclass('crm_lead_activities')=to_regclass('pg_temp.crm_lead_activities') AND to_regclass('crm_leads')=to_regclass('pg_temp.crm_leads')").fetch_one(&mut *tx).await.unwrap();assert!(shadowed);
        let actor=ActivityActor {user_id:"manager".into(),company_id:"company".into(),branch_id:Some("branch".into()),department_id:Some("department".into()),can_sales:true,can_manage:true,has_read:true,same_person_ids:vec![]};
        let input:ActivityChangeInput=serde_json::from_value(serde_json::json!({"action":"reassign","expectedVersion":1,"ownerId":"new-owner","dueDate":"2026-12-05","reason":"  Handoff  ","unknown":"discard"})).unwrap();
        let mut nonmanager=actor.clone();nonmanager.can_manage=false;
        assert_eq!(business(PgCrmActivityCommands::new(&mut tx).prepare_reassign(&nonmanager,"reassign-source","denied",&input).await.err().unwrap()).code,"CRM_ACTIVITY_OWNER_REQUIRED");
        let mut wrong=actor.clone();wrong.branch_id=Some("other-branch".into());
        assert_eq!(business(PgCrmActivityCommands::new(&mut tx).prepare_reassign(&wrong,"reassign-source","wrong",&input).await.err().unwrap()).code,"CRM_ACTIVITY_NOT_FOUND");
        let mut stale=input.clone();stale.expected_version=2;
        assert_eq!(business(PgCrmActivityCommands::new(&mut tx).prepare_reassign(&actor,"reassign-source","stale",&stale).await.err().unwrap()).code,"CONFLICT");
        let prepared:erpx_server_crm::public::activities_native::PreparedCrmActivityReassign=PgCrmActivityCommands::new(&mut tx).prepare_reassign(&actor,"reassign-source","reassign-1",&input).await.unwrap();
        assert_eq!((prepared.owner_id(),prepared.due_date(),prepared.expected_version()),("new-owner","2026-12-05",1));
        let retained:erpx_server_crm::public::activities_native::RetainedCrmActivityReassign=PgCrmActivityCommands::new(&mut tx).apply_reassign(prepared,1234).await.unwrap();
        retained.require_current(&mut tx).await.unwrap();
        let notification:erpx_server_crm::public::activities_native::CrmReassignNotificationFacts=retained.notification_facts();
        assert_eq!((notification.command_key.as_str(),notification.company_id.as_str(),notification.recipient.as_str(),notification.activity_id.as_str(),notification.version,notification.status,notification.updated_at),("reassign-1","company","new-owner","reassign-source",2,"open",1234));
        let row:(String,String,String,Option<String>,i64,i64)=sqlx::query_as("SELECT owner_id,due_date,status,outcome,version,updated_at FROM public.crm_lead_activities WHERE id='reassign-source'").fetch_one(&mut *tx).await.unwrap();
        assert_eq!(row,("new-owner".into(),"2026-12-05".into(),"open".into(),None,2,1234));
        let event:(String,i64,String,String,String,String,String,serde_json::Value)=sqlx::query_as("SELECT id,version,action,actor_id,owner_id,due_date,note,snapshot_json FROM public.crm_lead_activity_events WHERE activity_id='reassign-source'").fetch_one(&mut *tx).await.unwrap();
        assert_eq!(event,("crm-activity-event-reassign-1".into(),2,"reassign".into(),"manager".into(),"new-owner".into(),"2026-12-05".into(),"Handoff".into(),serde_json::json!({"action":"reassign","expectedVersion":1,"ownerId":"new-owner","dueDate":"2026-12-05","reason":"Handoff"})));
        // The TypeScript application writer is retired. Keep the schema oracle:
        // the native event snapshot must equal the actual TS `crmActivityChangeSchema`
        // normalization of the same raw reassign input.
        let oracle=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required"))
            .current_dir(concat!(env!("CARGO_MANIFEST_DIR"),"/../.."))
            .args(["--conditions=react-server","-e",r#"const m=await import('./packages/server-crm/src/domain/activities.ts'); const p=m.crmActivityChangeSchema.safeParse({action:'reassign',expectedVersion:1,ownerId:'new-owner',dueDate:'2026-12-05',reason:'  Handoff  ',unknown:'discard'}); process.stdout.write(JSON.stringify(p.success?p.data:null))"#])
            .output().unwrap();
        assert!(oracle.status.success(),"TS activity schema oracle failed");
        let oracle:serde_json::Value=serde_json::from_slice(&oracle.stdout).unwrap();
        assert_eq!(event.7,oracle);
        let mut other=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();assert!(retained.require_current(&mut other).await.is_err());other.rollback().await.unwrap();
        tx.rollback().await.unwrap();
        let after:(String,String,i64,i64)=sqlx::query_as("SELECT owner_id,due_date,version,(SELECT count(*) FROM public.crm_lead_activity_events WHERE activity_id='reassign-source') FROM public.crm_lead_activities WHERE id='reassign-source'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(after,("old-owner".into(),"2026-11-20".into(),1,0));
    }).await;
}

#[tokio::test]
async fn activity_reassign_competing_writers_wait_on_actual_unit() {
    fixture::with_database(3,|pool|async move {
        setup(&pool).await;
        sqlx::query("INSERT INTO crm_lead_activities(id,company_id,lead_id,activity_type,subject,details,owner_id,due_date,created_by,created_at,updated_at) VALUES('reassign-race','company','lead-department','call','Call','Discuss','old-owner','2026-11-20','maker',1,1)").execute(pool.raw()).await.unwrap();
        let mut first=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *first).await.unwrap();
        let first_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *first).await.unwrap();
        let actor=ActivityActor {user_id:"manager".into(),company_id:"company".into(),branch_id:Some("branch".into()),department_id:Some("department".into()),can_sales:true,can_manage:true,has_read:true,same_person_ids:vec![]};
        let input:ActivityChangeInput=serde_json::from_value(serde_json::json!({"action":"reassign","expectedVersion":1,"ownerId":"new-owner","dueDate":"2026-12-05","reason":"Handoff"})).unwrap();
        let prepared=PgCrmActivityCommands::new(&mut first).prepare_reassign(&actor,"reassign-race","race-first",&input).await.unwrap();
        PgCrmActivityCommands::new(&mut first).apply_reassign(prepared,1234).await.unwrap();
        let other_pool=pool.clone();let (send,recv)=tokio::sync::oneshot::channel();
        let second=tokio::spawn(async move {
            let mut tx=other_pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
            let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *tx).await.unwrap();send.send(pid).unwrap();
            let result=PgCrmActivityCommands::new(&mut tx).prepare_reassign(&actor,"reassign-race","race-second",&input).await;
            match result {Err(PgTransactionError::Persistence(error))=>assert_eq!(error.sql_state(),Some("40001")),Err(PgTransactionError::Business(error))=>assert_eq!(error.code,"CONFLICT"),Ok(_)=>panic!("stale reassign prepared")};
            tx.rollback().await.unwrap();
        });
        let second_pid=recv.await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5),async {loop {let blocked:bool=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(first_pid).bind(second_pid).fetch_one(pool.raw()).await.unwrap();if blocked {break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.expect("competing reassign did not wait for actual writer");
        first.commit().await.unwrap();second.await.unwrap();
        let state:(String,i64,i64,i64)=sqlx::query_as("SELECT owner_id,version,(SELECT count(*) FROM crm_lead_activity_events WHERE activity_id='reassign-race'),(SELECT count(*) FROM crm_lead_activity_events WHERE id='crm-activity-event-race-second') FROM crm_lead_activities WHERE id='reassign-race'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(state,("new-owner".into(),2,1,0));
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
            let actor = ActivityActor { user_id: "owner".into(), company_id: "company".into(), branch_id: None, department_id: None,
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
        assert_eq!(business(create.await.unwrap().unwrap_err()).code, "CRM_ACTIVITY_SOURCE_CLOSED");
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
            let actor = ActivityActor { user_id: "owner".into(), company_id: "company".into(), branch_id: None, department_id: None,
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
        assert_eq!(business(create.await.unwrap().unwrap_err()).code, "CRM_ACTIVITY_SOURCE_CLOSED");
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
            let actor = ActivityActor { user_id: "owner".into(), company_id: "company".into(), branch_id: None, department_id: None, can_sales: true,
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
        assert_eq!(business(create.await.unwrap().unwrap_err()).code, "CONFLICT");
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
        let actor = ActivityActor { user_id: "owner".into(), company_id: "company".into(), branch_id: None, department_id: None, can_sales: true,
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
        assert_eq!(business(error).code, "CRM_QUALIFICATION_COMPOSITION_REQUIRED");
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn pms_win_effect_records_confirmed_reservation_amount_and_both_events() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO crm_accounts(id,company_id,kind,name,created_by,version,created_at,updated_at) VALUES ('win-account','company','individual','Hotel guest','owner',1,1,1);\
             INSERT INTO crm_contacts(id,company_id,account_id,name,detail,created_by,version,created_at,updated_at) VALUES ('win-contact','company','win-account','Hotel guest','Synthetic channel','owner',1,1,1);\
             UPDATE crm_leads SET contact_id='win-contact' WHERE id='lead-discovery';\
             INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,state,config_json,version,created_by,updated_at) VALUES ('win-property','company','branch','Hotel','2026-12-01','open','{}',1,'owner',1);\
             INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES \
               ('win-guest','win-property','guest','{}',1,1),\
               ('win-reservation','win-property','reservation','{\"guestId\":\"win-guest\",\"state\":\"confirmed\"}',1,1),\
               ('win-stay','win-property','stay','{\"reservationId\":\"win-reservation\",\"prices\":[{\"net\":777}]}',1,1);\
             INSERT INTO crm_guest_links(id,company_id,contact_id,guest_id,property_id,evidence,reviewed_by,created_at,sharing_state,sharing_evidence,sharing_by,sharing_at,version) VALUES ('win-link','company','win-contact','win-guest','win-property','Reviewed identity','reviewer',1,'granted','Hotel history consent','reviewer',1,2);",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let actor = LeadActor { user_id: "owner".into(), company_id: "company".into(), can_read: true, can_sales: true, can_manage: false, can_pms: true, can_support: false, same_person_ids: vec![] };
        let input: OpportunityChangeInput = serde_json::from_value(serde_json::json!({
            "action": "win-pms",
            "expectedVersion": 1,
            "leadVersion": 2,
            "reservationId": "win-reservation",
            "propertyId": "win-property",
            "reason": "Confirmed hotel reservation"
        }))
        .unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        let mut prepared = PgCrmLeadCommands::new(&mut tx).prepare_opportunity_change(&actor, "lead-discovery", &input).await.unwrap();
        prepared.reference_minor = Some(777);
        let retained = PgCrmLeadCommands::new(&mut tx).apply_opportunity_change(&actor, "lead-discovery", "command-win-pms", prepared, NOW.get()).await.unwrap();
        retained.require_current(&mut tx).await.unwrap();
        assert_eq!((retained.facts().lead_version, retained.facts().opportunity_version, retained.facts().stage.as_str()), (3, 2, "won"));
        let stored: (String, String, i64, String) = sqlx::query_as("SELECT stage,win_source,expected_minor,reservation_id FROM crm_opportunities WHERE id='opp-discovery'").fetch_one(&mut *tx).await.unwrap();
        assert_eq!(stored, ("won".into(), "pms".into(), 777, "win-reservation".into()));
        let events: Vec<(String, Option<String>, i64)> = sqlx::query_as("SELECT action,opportunity_id,version FROM crm_case_events WHERE lead_id='lead-discovery' AND action IN ('opportunity-win-pms','win-pms') ORDER BY action").fetch_all(&mut *tx).await.unwrap();
        assert_eq!(events.len(), 2);
        assert!(events.contains(&("opportunity-win-pms".into(), None, 3)));
        assert!(events.contains(&("win-pms".into(), Some("opp-discovery".into()), 2)));
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn lead_and_opportunity_require_exact_owner_even_for_identity_alias() {
    // Execute the retained application against controlled repository facts to
    // obtain its actual ownership decisions. The native checks below use the
    // real migrated PostgreSQL owner rows, not this oracle repository.
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let module=root.join("packages/server-crm/src/application/leads.ts");
    let script=r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const m=await import(process.argv[1]);const row={id:'lead',owner_id:'owner',status:'open',stage:'discovery',opportunity_id:'opp'};const repo={records:async()=>[row],replay:async()=>null,commit:async()=>({ok:true})};const app=m.createCRMApplication({actorId:'alias',canRead:true,canSales:true,canApprove:false,canPms:false,canSupport:false,clock:{now:()=>1},authorize:()=>{},repository:async()=>repo,activityWork:async()=>[]});const result=[];for(const run of [()=>app.changeLead('lead',{action:'disqualify',expectedVersion:1,reason:'Not ready'},'test-command'),()=>app.changeOpportunity('lead',{action:'lose',expectedVersion:1,leadVersion:2,lossReason:'other',reason:'Not ready'},'test-command')]){try{await run();result.push('allowed')}catch(e){result.push(e.code)}}process.stdout.write(JSON.stringify(result));"#;
    let oracle=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).current_dir(root).args(["--conditions=react-server","-e",script]).arg(module).output().unwrap();
    assert!(oracle.status.success(),"actual CRM application oracle: {}",String::from_utf8_lossy(&oracle.stderr));
    let expected:Vec<String>=serde_json::from_slice(&oracle.stdout).unwrap();assert_eq!(expected,["CRM_OWNER_REQUIRED","CRM_OWNER_REQUIRED"]);
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let alias=LeadActor {user_id:"alias".into(),company_id:"company".into(),can_read:true,can_sales:true,can_manage:false,can_pms:false,can_support:false,same_person_ids:vec!["owner".into()]};
        let exact=LeadActor {user_id:"owner".into(),..alias.clone()};
        let lead:LeadChangeInput=serde_json::from_value(serde_json::json!({"action":"disqualify","expectedVersion":1,"reason":"Not ready"})).unwrap();
        let opportunity:OpportunityChangeInput=serde_json::from_value(serde_json::json!({"action":"lose","expectedVersion":1,"leadVersion":2,"lossReason":"other","reason":"Not ready"})).unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        let lead_error=PgCrmLeadCommands::new(&mut tx).prepare_lead_change(&alias,"page-01",&lead).await.err().unwrap();
        let opp_error=PgCrmLeadCommands::new(&mut tx).prepare_opportunity_change(&alias,"lead-discovery",&opportunity).await.err().unwrap();
        assert_eq!(vec![business(lead_error).code.to_owned(),business(opp_error).code.to_owned()],expected);
        assert!(PgCrmLeadCommands::new(&mut tx).prepare_lead_change(&exact,"page-01",&lead).await.is_ok());
        assert!(PgCrmLeadCommands::new(&mut tx).prepare_opportunity_change(&exact,"lead-discovery",&opportunity).await.is_ok());
        let effects:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM crm_case_events WHERE lead_id IN ('page-01','lead-discovery')),(SELECT count(*) FROM command_receipts)").fetch_one(&mut *tx).await.unwrap();assert_eq!(effects,(0,0));
        tx.rollback().await.unwrap();
    }).await;
}
