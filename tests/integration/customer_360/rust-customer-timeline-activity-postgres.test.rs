//! PostgreSQL parity for the native Customer 360 timeline activity read.
//!
//! The retained TypeScript `customerTimelineEvents` projection is the oracle:
//! the same federated SQL is executed on the same leased database and its event
//! IDs are compared with the native owner port over the same actor scope. No
//! browser, composition mount or write path is involved.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_customer_360_server::public::customer_timeline::{
    CustomerTimelineSupportEntitlement, CustomerTimelineView,
};
use erpx_customer_360_server::public::customer_timeline_activity::{
    CustomerTimelineActivityQuery, PgCustomerTimelineActivityRead, customer_timeline_activity,
};
use erpx_customer_360_server::public::customer_timeline::{
    CustomerTimelineActor, CustomerTimelineFuture,
};
use erpx_platform_runtime_server::public::postgres::{
    MigrationTrack, load_migrations, migrate_database,
};
use erpx_platform_shared_server::public::{BusinessError, CompanyId, TimestampMillis};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor() -> CustomerTimelineActor {
    CustomerTimelineActor {
        user_id: "user".into(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        role: "reader".into(),
        permissions: vec!["commerce.read".into(), "finance.read".into()],
        modules: vec![],
        assignment_id: "assignment".into(),
        expires_at: NOW.get() + 60_000,
    }
}

struct Entitlement;

impl CustomerTimelineSupportEntitlement for Entitlement {
    type Error = BusinessError;
    fn support_history<'a>(
        &'a mut self,
        _company_id: &'a str,
    ) -> CustomerTimelineFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(false) })
    }
}

/// The retained TypeScript `customerTimelineEvents` projection, expressed over
/// the same base relations with the actor's source gates (Support, Field
/// Service and Legal disabled, Finance enabled). This is an independent oracle
/// for the native adapter's federated SQL.
const TS_PROJECTION: &str = r#"
WITH scoped_orders AS (
  SELECT id,company_id,party_id,kind,number,state,version,created_at,updated_at
  FROM commerce_orders WHERE company_id=$1 AND kind IN ('sales','quote')
), scoped_leads AS (
  SELECT l.id,l.title,l.status,l.version,l.created_at,l.updated_at,v.customer_id
  FROM crm_leads l JOIN crm_opportunities v ON v.lead_id=l.id WHERE l.company_id=$1
), scoped_invoices AS (
  SELECT i.id,i.party_id FROM finance_invoices i WHERE i.company_id=$1
), customers AS (
  SELECT id,name FROM finance_parties WHERE company_id=$1 AND kind='customer'
), events AS (
  SELECT 'order:'||o.id id,o.state status,o.version,o.updated_at at,o.party_id customer_id,p.name customer_name
  FROM scoped_orders o JOIN customers p ON p.id=o.party_id
  UNION ALL
  SELECT 'fulfillment:'||f.id,'recorded',f.version,f.created_at,o.party_id,p.name
  FROM commerce_fulfillments f JOIN scoped_orders o ON o.id=f.order_id JOIN customers p ON p.id=o.party_id
  UNION ALL
  SELECT 'return:'||r.id,r.status,r.version,r.updated_at,o.party_id,p.name
  FROM commerce_returns r JOIN scoped_orders o ON o.id=r.order_id AND o.company_id=r.company_id
  JOIN customers p ON p.id=o.party_id WHERE r.kind='customer_return'
  UNION ALL
  SELECT 'contact:'||h.id,h.action,h.version,h.created_at,o.party_id,p.name
  FROM customer_contact_note_history h JOIN customer_contact_notes n ON n.id=h.note_id
  JOIN scoped_orders o ON o.id=n.order_id AND o.company_id=n.company_id JOIN customers p ON p.id=o.party_id
  UNION ALL
  SELECT 'followup:'||e.id,e.action,e.version,e.created_at,o.party_id,p.name
  FROM customer_followup_events e JOIN customer_followups c ON c.id=e.followup_id
  JOIN scoped_orders o ON o.id=c.order_id AND o.company_id=c.company_id JOIN customers p ON p.id=o.party_id
  UNION ALL
  SELECT 'invoice:'||i.id,i.status,i.version,i.updated_at,i.party_id,p.name
  FROM finance_invoices i JOIN scoped_invoices si ON si.id=i.id JOIN customers p ON p.id=i.party_id
  UNION ALL
  SELECT 'crm:'||e.id,e.action,e.version,e.created_at,l.customer_id,p.name
  FROM crm_case_events e JOIN scoped_leads l ON l.id=e.lead_id JOIN customers p ON p.id=l.customer_id
  WHERE e.opportunity_id IS NULL
  UNION ALL
  SELECT 'crm-activity:'||x.id,x.status,x.version,x.updated_at,l.customer_id,p.name
  FROM crm_lead_activities x JOIN scoped_leads l ON l.id=x.lead_id JOIN customers p ON p.id=l.customer_id
) SELECT id FROM events WHERE customer_id=$2 ORDER BY at DESC,id DESC
"#;

#[tokio::test]
async fn native_activity_matches_retained_ts_projection_over_real_postgres() {
    fixture::with_database(5, |pool| async move {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let files = load_migrations(root, MigrationTrack::HoldingWrite)
            .await
            .unwrap();
        migrate_database(&pool, MigrationTrack::HoldingWrite, &files)
            .await
            .unwrap();
        sqlx::raw_sql(
            "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES \
             ('company','company','company','Company',NULL,'company',NULL,'active',1),\
             ('branch','branch','branch','Branch','company','company',NULL,'active',1),\
             ('foreign','company','foreign','Foreign',NULL,'foreign',NULL,'active',1),\
             ('foreign-branch','branch','foreign-branch','Foreign branch','foreign','foreign',NULL,'active',1);\
             INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES \
             ('warehouse','company','branch','WH-1','Main warehouse'),\
             ('foreign-warehouse','foreign','foreign-branch','FW-1','Foreign warehouse');\
             INSERT INTO finance_charts(id,company_id,name,base_currency,accounting_basis,status,version,created_by,created_at,updated_at) VALUES \
             ('chart','company','Chart','THB','IFRS','active',1,'owner',0,0);\
             INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,status,version,created_at,updated_at) VALUES \
             ('acc-ar','chart','company','1100','Receivable','asset','THB',0,'active',1,0,0),\
             ('acc-rev','chart','company','4000','Revenue','revenue','THB',0,'active',1,0,0);\
             INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at) VALUES \
             ('customer','company','customer','C-001','Acme Customer','THB','active',1,10,10),\
             ('foreign-customer','foreign','customer','F-001','Foreign Customer','THB','active',1,10,10);\
             INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,version,created_by,created_at,updated_at) VALUES \
             ('order-main','company','branch',NULL,'sales','SO-100','customer','warehouse','THB','2026-09-20','2026-09-23','Main','confirmed',100,0,1,'owner',100,100),\
             ('order-foreign','foreign','foreign-branch',NULL,'sales','FSO-1','foreign-customer','foreign-warehouse','THB','2026-09-20','2026-09-23','Foreign','confirmed',100,0,1,'owner',400,400);\
             INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,status,version,created_by,created_at,updated_at) VALUES \
             ('invoice-main','company','customer','receivable','INV-100','2026-09-20','2026-10-20','THB',100,0,100,100,'acc-ar','acc-rev','posted',3,'owner',110,110);\
             INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at,version) VALUES \
             ('fulfillment-main','order-main','FUL-100','2026-09-21','owner',101,1);\
             INSERT INTO commerce_returns(id,company_id,branch_id,order_id,fulfillment_id,invoice_id,kind,number,return_date,reason,status,subtotal_minor,tax_minor,total_minor,version,requested_by,created_at,updated_at) VALUES \
             ('return-main','company','branch','order-main','fulfillment-main','invoice-main','customer_return','RET-100','2026-09-22','Synthetic return','posted',100,0,100,2,'owner',102,102);\
             INSERT INTO crm_leads(id,company_id,branch_id,department_id,title,contact_name,contact_detail,source,brief,owner_id,due_date,status,version,created_by,created_at,updated_at) VALUES \
             ('lead-main','company','branch',NULL,'Synthetic opportunity','Synthetic contact','Synthetic channel','Staff inquiry','Synthetic','owner','2026-11-20','qualified',2,'owner',111,111);\
             INSERT INTO crm_opportunities(id,lead_id,customer_id,stage,expected_minor,currency,close_date,version,created_by,created_at,updated_at) VALUES \
             ('opportunity-main','lead-main','customer','proposal',1500,'THB','2026-12-01',1,'owner',111,111);\
             INSERT INTO crm_case_events(id,lead_id,opportunity_id,version,action,actor_id,reason,snapshot_json,created_at) VALUES \
             ('crm-event-main','lead-main',NULL,1,'create','owner','Synthetic','{}',111);\
             INSERT INTO crm_lead_activities(id,company_id,lead_id,activity_type,subject,details,owner_id,due_date,status,version,created_by,created_at,updated_at) VALUES \
             ('crm-activity-main','company','lead-main','call','Synthetic call','Synthetic','owner','2026-11-21','open',1,'owner',112,112);"
        )
        .execute(pool.raw())
        .await
        .unwrap();

        // Oracle: the retained TypeScript projection's event IDs.
        let expected: Vec<String> = sqlx::query_scalar(TS_PROJECTION)
            .bind("company")
            .bind("customer")
            .fetch_all(pool.raw())
            .await
            .unwrap();

        let query = CustomerTimelineActivityQuery {
            from: 0,
            to: 1_000,
            customer_id: Some("customer".into()),
            query: None,
        };
        let actual: Vec<String> = {
            let mut tx = pool
                .raw()
                .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
                .await
                .unwrap();
            let result = {
                let mut reader = PgCustomerTimelineActivityRead::new(&mut tx);
                customer_timeline_activity(
                    &actor(),
                    &query,
                    &mut reader,
                    &mut Entitlement,
                    NOW,
                )
                .await
                .unwrap()
            };
            tx.rollback().await.unwrap();
            result.into_iter().map(|row| row.id).collect()
        };
        assert_eq!(actual, expected);
        assert!(!actual.is_empty());
        // Finance source is enabled for this actor; the foreign company and
        // customer are excluded by Company isolation.
        assert!(actual.contains(&"invoice:invoice-main".to_owned()));
        assert!(actual.iter().all(|id| !id.contains("foreign")));
        // The native port exposes the exact source view derived from the actor.
        assert!(!CustomerTimelineView {
            support: false,
            field_service: false,
            finance: true,
            legal: false,
        }
        .coverage()
        .is_empty());
    })
    .await;
}
