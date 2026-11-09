//! Native Customer 360 federated timeline over real migrated PostgreSQL.
//! These prove the retained source federation, Company/branch/department
//! isolation, per-source gates, cursor paging, serializable guards, rollback
//! and persisted readback. They are not a browser journey or a write slice.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_customer_360::public::customer_activity::{
    ExecutiveCustomerActivityQuery, executive_customer_activity,
};
use erpx_server_customer_360::public::customer_timeline::{
    CustomerTimelineActor, CustomerTimelineCursor, CustomerTimelineFuture, CustomerTimelineQuery,
    CustomerTimelineSupportEntitlement, PgCustomerTimelineRead, customer_timeline,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use sqlx::Row;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

#[allow(clippy::too_many_arguments)]
fn actor(
    user: &str,
    company: &str,
    branch: Option<&str>,
    department: Option<&str>,
    role: &str,
    assignment: &str,
    permissions: &[&str],
    modules: &[&str],
) -> CustomerTimelineActor {
    CustomerTimelineActor {
        user_id: user.into(),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: branch.map(|value| erpx_server_shared::public::RecordId::new(value).unwrap()),
        department_id: department
            .map(|value| erpx_server_shared::public::RecordId::new(value).unwrap()),
        role: role.into(),
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        modules: modules.iter().map(|value| (*value).to_owned()).collect(),
        assignment_id: assignment.into(),
        expires_at: NOW.get() + 60_000,
    }
}

fn full_actor() -> CustomerTimelineActor {
    actor(
        "user",
        "company",
        None,
        None,
        "support_agent",
        "assignment-support",
        &[
            "commerce.read",
            "support.read",
            "finance.read",
            "legal.read",
        ],
        &["field_service"],
    )
}

struct Entitlement(bool);

impl CustomerTimelineSupportEntitlement for Entitlement {
    type Error = BusinessError;
    fn support_history<'a>(
        &'a mut self,
        _company_id: &'a str,
    ) -> CustomerTimelineFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(self.0) })
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
         INSERT INTO finance_charts(id,company_id,name,base_currency,accounting_basis,status,version,created_by,created_at,updated_at) VALUES \
         ('chart','company','Chart','THB','IFRS','active',1,'owner',0,0);\
         INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,status,version,created_at,updated_at) VALUES \
         ('acc-ar','chart','company','1100','Receivable','asset','THB',0,'active',1,0,0),\
         ('acc-rev','chart','company','4000','Revenue','revenue','THB',0,'active',1,0,0);\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at) VALUES \
         ('customer','company','customer','C-001','Acme Customer','THB','active',1,10,10),\
         ('unlinked','company','customer','C-002','Unlinked Customer','THB','active',1,10,10),\
         ('foreign-customer','foreign','customer','F-001','Foreign Customer','THB','active',1,10,10);\
         INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,version,created_by,created_at,updated_at) VALUES \
         ('order-main','company','branch',NULL,'sales','SO-100','customer',NULL,'THB','2026-09-20','2026-09-23','Main', 'confirmed',100,0,1,'owner',100,100),\
         ('order-other','company','other-branch',NULL,'sales','SO-200','customer',NULL,'THB','2026-09-20','2026-09-23','Other', 'confirmed',100,0,1,'owner',200,200),\
         ('order-dept','company','branch','department','sales','SO-300','customer',NULL,'THB','2026-09-20','2026-09-23','Dept', 'confirmed',100,0,1,'owner',300,300),\
         ('order-foreign','foreign','foreign-branch',NULL,'sales','FSO-1','foreign-customer',NULL,'THB','2026-09-20','2026-09-23','Foreign', 'confirmed',100,0,1,'owner',400,400);\
         INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,status,version,created_by,created_at,updated_at) VALUES \
         ('invoice-main','company','customer','receivable','INV-100','2026-09-20','2026-10-20','THB',100,0,100,100,'acc-ar','acc-rev','posted',3,'owner',110,110);\
         INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at,version) VALUES \
         ('fulfillment-main','order-main','FUL-100','2026-09-21','owner',101,1);\
         INSERT INTO commerce_returns(id,company_id,branch_id,order_id,fulfillment_id,invoice_id,kind,number,return_date,reason,status,subtotal_minor,tax_minor,total_minor,version,requested_by,created_at,updated_at) VALUES \
         ('return-main','company','branch','order-main','fulfillment-main','invoice-main','customer_return','RET-100','2026-09-22','Synthetic return','posted',100,0,100,2,'owner',102,102);\
         INSERT INTO customer_contact_notes(id,company_id,customer_id,order_id,channel,occurred_at,body,status,version,created_by,created_at,updated_at) VALUES \
         ('note-main','company','customer','order-main','phone',103,'Synthetic contact','active',1,'owner',103,103),\
         ('note-other','company','customer','order-other','email',201,'Other contact','active',1,'owner',201,201),\
         ('note-dept','company','customer','order-dept','meeting',301,'Dept contact','active',1,'owner',301,301);\
         INSERT INTO customer_contact_note_history(id,note_id,version,action,body,reason,actor_id,created_at) VALUES \
         ('note-history-main','note-main',1,'create','Synthetic contact','Synthetic','owner',103),\
         ('note-history-other','note-other',1,'create','Other contact','Synthetic','owner',201),\
         ('note-history-dept','note-dept',1,'create','Dept contact','Synthetic','owner',301);\
         INSERT INTO customer_followups(id,company_id,order_id,owner_id,due_date,next_action,status,version,created_by,created_at,updated_at) VALUES \
         ('followup-main','company','order-main','owner','2026-09-24','Call customer','open',1,'owner',104,104);\
         INSERT INTO customer_followup_events(id,followup_id,version,action,actor_id,owner_id,due_date,reason,created_at) VALUES \
         ('followup-event-main','followup-main',1,'create','owner','owner','2026-09-24','Synthetic',104);\
         INSERT INTO support_tickets(id,company_id,branch_id,department_id,order_id,customer_id,title,issue,owner_id,due_date,status,version,created_by,created_at,updated_at) VALUES \
         ('ticket-main','company','branch',NULL,'order-main','customer','Synthetic support case','Synthetic issue','owner','2026-09-25','open',1,'owner',105,105);\
         INSERT INTO support_ticket_events(id,ticket_id,version,action,actor_id,reason,owner_id,due_date,status,created_at,evidence_json) VALUES \
         ('support-event-main','ticket-main',1,'create','owner','Synthetic','owner','2026-09-25','open',105,'{}');\
         INSERT INTO field_service_agreements(id,company_id,branch_id,department_id,customer_id,title,start_date,end_date,currency,visit_rate_minor,status,created_by,version,created_at,updated_at) VALUES \
         ('agreement-main','company','branch',NULL,'customer','Synthetic agreement','2026-09-01','2027-09-01','THB',2500,'draft','owner',1,106,106);\
         INSERT INTO field_service_agreement_events(id,agreement_id,version,action,actor_id,reason,status,created_at) VALUES \
         ('agreement-event-main','agreement-main',1,'create','owner','Synthetic','draft',106);\
         INSERT INTO field_service_orders(id,company_id,branch_id,department_id,ticket_id,ticket_version,agreement_id,agreement_version,issue,site,technician_id,status,version,created_by,created_at,updated_at) VALUES \
         ('service-order-main','company','branch',NULL,'ticket-main',1,'agreement-main',1,'Synthetic issue','Synthetic site','technician','scheduled',1,'owner',107,107);\
         INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,details_json,created_at) VALUES \
         ('service-order-event-main','service-order-main',1,'schedule','owner','Synthetic','scheduled','technician','{}',107);\
         INSERT INTO legal_items(id,company_id,branch_id,department_id,kind,title,description,classification,country,jurisdiction,language,currency,timezone,priority,due_date,owner_id,created_by,state,version,created_at,updated_at) VALUES \
         ('legal-internal','company','branch',NULL,'contract','Internal contract','Synthetic','internal','TH','TH','th','THB','Asia/Bangkok','normal','2026-10-01','owner','owner','active',1,108,108),\
         ('legal-confidential','company','branch',NULL,'contract','Confidential contract','Synthetic','confidential','TH','TH','th','THB','Asia/Bangkok','normal','2026-10-01','owner','owner','active',1,109,109);\
         INSERT INTO legal_native_references(item_id,target_module,target_id,ordinal,created_by,created_at) VALUES \
         ('legal-internal','commerce_order','order-main',0,'owner',108),\
         ('legal-confidential','commerce_order','order-main',0,'owner',109);\
         INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) VALUES \
         ('legal-event-internal','legal-internal','create','owner','Synthetic',1,108),\
         ('legal-event-confidential','legal-confidential','create','owner','Synthetic',1,109);\
         INSERT INTO legal_members(item_id,user_id,access) VALUES ('legal-confidential','user','read');\
         INSERT INTO crm_leads(id,company_id,branch_id,department_id,title,contact_name,contact_detail,source,brief,owner_id,due_date,status,version,created_by,created_at,updated_at) VALUES \
         ('lead-main','company','branch',NULL,'Synthetic opportunity','Synthetic contact','Synthetic channel','Staff inquiry','Synthetic', 'owner','2026-11-20','qualified',2,'owner',111,111);\
         INSERT INTO crm_opportunities(id,lead_id,customer_id,stage,expected_minor,currency,close_date,version,created_by,created_at,updated_at) VALUES \
         ('opportunity-main','lead-main','customer','proposal',1500,'THB','2026-12-01',1,'owner',111,111);\
         INSERT INTO crm_case_events(id,lead_id,opportunity_id,version,action,actor_id,reason,snapshot_json,created_at) VALUES \
         ('crm-event-main','lead-main',NULL,1,'create','owner','Synthetic','{}',111);\
         INSERT INTO crm_lead_activities(id,company_id,lead_id,activity_type,subject,details,owner_id,due_date,status,version,created_by,created_at,updated_at) VALUES \
         ('crm-activity-main','company','lead-main','call','Synthetic call','Synthetic','owner','2026-11-21','open',1,'owner',112,112);\
         INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy) VALUES \
         ('company','user','principal-user','active','local');\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,scope_kind,parent_id,source) VALUES \
         ('assignment-support','user','support_agent','[]','company',NULL,0,9000000000000,'holding',NULL,'manual');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn read_page(
    pool: &DatabasePool,
    actor: &CustomerTimelineActor,
    customer_id: &str,
    query: CustomerTimelineQuery,
    purchased: bool,
) -> Result<erpx_server_customer_360::public::customer_timeline::CustomerTimelinePage, BusinessError>
{
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let customer_id = customer_id.to_owned();
            let query = query.clone();
            Box::pin(async move {
                let mut reader = PgCustomerTimelineRead::new(connection);
                let mut entitlement = Entitlement(purchased);
                customer_timeline(
                    &actor,
                    &customer_id,
                    &query,
                    &mut reader,
                    &mut entitlement,
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
}

async fn paged_ids(pool: &DatabasePool, actor: &CustomerTimelineActor, size: u16) -> Vec<String> {
    let mut ids = Vec::new();
    let mut cursor = None;
    loop {
        let page = read_page(
            pool,
            actor,
            "customer",
            CustomerTimelineQuery {
                cursor: cursor.clone(),
                limit: Some(size),
            },
            true,
        )
        .await
        .unwrap();
        assert!(page.items.len() <= size as usize);
        ids.extend(page.items.iter().map(|item| item.id.clone()));
        cursor = page.next_cursor.clone();
        if cursor.is_none() {
            break;
        }
    }
    ids
}

#[tokio::test]
async fn cursor_pages_match_database_collation_across_sources() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,version,created_by,created_at,updated_at) VALUES \
             ('é','company','branch',NULL,'sales','SO-U1','customer',NULL,'THB','2026-09-20','2026-09-23','Unicode','confirmed',1,0,1,'owner',9000,9000), \
             ('z','company','branch',NULL,'sales','SO-U2','customer',NULL,'THB','2026-09-20','2026-09-23','Unicode','confirmed',1,0,1,'owner',9000,9000), \
             ('ก','company','branch',NULL,'sales','SO-U3','customer',NULL,'THB','2026-09-20','2026-09-23','Unicode','confirmed',1,0,1,'owner',9000,9000); \
             INSERT INTO customer_contact_note_history(id,note_id,version,action,body,reason,actor_id,created_at) VALUES \
             ('é','note-main',2,'correct','Unicode','Synthetic','owner',9000), \
             ('z','note-main',3,'correct','Unicode','Synthetic','owner',9000), \
             ('ก','note-main',4,'correct','Unicode','Synthetic','owner',9000)"
        ).execute(pool.raw()).await.unwrap();
        // This oracle is the legacy union's database order, not a Rust or C
        // collation approximation. Scope is the same selected Company/customer.
        let expected: Vec<String> = sqlx::query_scalar(
            "SELECT e.id FROM (\
               SELECT 'order:'||o.id id,o.updated_at::bigint at FROM commerce_orders o \
                WHERE o.company_id='company' AND o.party_id='customer' AND o.kind IN ('sales','quote') AND o.updated_at=9000 \
               UNION ALL \
               SELECT 'contact:'||h.id id,h.created_at::bigint at FROM customer_contact_note_history h \
                JOIN customer_contact_notes n ON n.id=h.note_id \
                JOIN commerce_orders o ON o.id=n.order_id AND o.company_id=n.company_id \
                WHERE o.company_id='company' AND o.party_id='customer' AND o.kind IN ('sales','quote') AND h.created_at=9000\
             ) e ORDER BY e.at DESC,e.id DESC"
        ).fetch_all(pool.raw()).await.unwrap();
        assert_eq!(expected.len(), 6);
        let actual = paged_ids(&pool, &full_actor(), 2).await;
        assert_eq!(&actual[..expected.len()], expected.as_slice());
        for id in expected {
            assert_eq!(actual.iter().filter(|item| *item == &id).count(), 1);
        }
    }).await;
}

#[tokio::test]
async fn reads_the_full_federation_and_pages_without_duplicates() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = full_actor();
        let page = read_page(
            &pool,
            &owner,
            "customer",
            CustomerTimelineQuery::default(),
            true,
        )
        .await
        .unwrap();
        assert_eq!(page.customer.name, "Acme Customer");
        assert_eq!(page.customer.code, "C-001");
        assert_eq!(page.items.len(), 17);
        assert_eq!(
            page.coverage,
            vec![
                "Sales quotations/orders",
                "Fulfillments and customer returns",
                "Internal contact history",
                "Customer follow-up history",
                "Qualified CRM lead/opportunity history",
                "CRM follow-up activities",
                "Support case history",
                "Field Service agreements and visits",
                "Finance invoice status",
                "Legal records linked to orders",
            ]
        );
        let ids = page
            .items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec![
                "contact:note-history-dept",
                "order:order-dept",
                "contact:note-history-other",
                "order:order-other",
                "crm-activity:crm-activity-main",
                "crm:crm-event-main",
                "invoice:invoice-main",
                "legal:legal-event-confidential",
                "legal:legal-event-internal",
                "service-order:service-order-event-main",
                "service-agreement:agreement-event-main",
                "support:support-event-main",
                "followup:followup-event-main",
                "contact:note-history-main",
                "return:return-main",
                "fulfillment:fulfillment-main",
                "order:order-main",
            ]
        );
        assert_eq!(page.items[0].title, "Contact · meeting");
        assert_eq!(page.items[0].href, "/commerce/orders/order-dept/notes");
        assert_eq!(page.items[0].version, 1);
        assert_eq!(page.items[1].title, "sales · SO-300");
        assert_eq!(page.items[1].href, "/commerce/orders/order-dept");
        assert_eq!(page.items[2].title, "Contact · email");
        assert_eq!(page.items[4].title, "CRM activity · call");
        assert_eq!(page.items[4].href, "/crm/lead-main#crm-activities");
        assert_eq!(page.items[5].title, "Synthetic opportunity");
        assert_eq!(page.items[6].title, "Invoice · INV-100");
        assert_eq!(page.items[6].status, "posted");
        assert_eq!(page.items[6].version, 3);
        assert_eq!(page.items[7].title, "Legal · Confidential contract");
        assert_eq!(page.items[8].title, "Legal · Internal contract");
        assert_eq!(
            page.items[9].title,
            "Field service · Synthetic support case"
        );
        assert_eq!(
            page.items[10].title,
            "Field service agreement · Synthetic agreement"
        );
        assert_eq!(page.items[11].title, "Synthetic support case");
        assert_eq!(page.items[12].title, "Customer follow-up");
        assert_eq!(page.items[13].status, "create");
        assert_eq!(page.items[14].title, "Customer return · RET-100");
        assert_eq!(page.items[14].version, 2);
        assert_eq!(page.items[15].title, "Fulfillment · FUL-100");
        assert_eq!(page.items[16].title, "sales · SO-100");
        assert_eq!(
            page.limitations,
            erpx_server_customer_360::public::customer_timeline::LIMITATIONS
        );

        let all = paged_ids(&pool, &owner, 5).await;
        assert_eq!(all.len(), 17);
        let unique = all
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique.len(), 17);
        assert_eq!(all.first().unwrap(), "contact:note-history-dept");
        assert_eq!(all.last().unwrap(), "order:order-main");
    })
    .await;
}

#[tokio::test]
async fn company_branch_department_and_customer_scope_are_enforced() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let branch = actor(
            "user",
            "company",
            Some("branch"),
            None,
            "support_agent",
            "assignment-support",
            &[
                "commerce.read",
                "support.read",
                "finance.read",
                "legal.read",
            ],
            &["field_service"],
        );
        let page = read_page(
            &pool,
            &branch,
            "customer",
            CustomerTimelineQuery::default(),
            true,
        )
        .await
        .unwrap();
        assert_eq!(page.items.len(), 15);
        assert!(page.items.iter().all(|item| !item.id.contains("other")));
        assert!(page.items.iter().any(|item| item.id == "order:order-dept"));

        let department = actor(
            "user",
            "company",
            Some("branch"),
            Some("department"),
            "support_agent",
            "assignment-support",
            &[
                "commerce.read",
                "support.read",
                "finance.read",
                "legal.read",
            ],
            &["field_service"],
        );
        let page = read_page(
            &pool,
            &department,
            "customer",
            CustomerTimelineQuery::default(),
            true,
        )
        .await
        .unwrap();
        assert_eq!(
            page.items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "contact:note-history-dept",
                "order:order-dept",
                "invoice:invoice-main",
            ]
        );

        let foreign = actor(
            "user",
            "foreign",
            Some("foreign-branch"),
            None,
            "support_agent",
            "assignment-support",
            &["commerce.read"],
            &[],
        );
        let error = read_page(
            &pool,
            &foreign,
            "customer",
            CustomerTimelineQuery::default(),
            true,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "CUSTOMER_TIMELINE_NOT_FOUND");
        let page = read_page(
            &pool,
            &foreign,
            "foreign-customer",
            CustomerTimelineQuery::default(),
            false,
        )
        .await
        .unwrap();
        assert_eq!(
            page.items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["order:order-foreign"]
        );

        let error = read_page(
            &pool,
            &full_actor(),
            "unlinked",
            CustomerTimelineQuery::default(),
            true,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "CUSTOMER_TIMELINE_NOT_FOUND");

        let denied = actor(
            "user",
            "company",
            None,
            None,
            "employee",
            "assignment-support",
            &["finance.read"],
            &[],
        );
        let error = read_page(
            &pool,
            &denied,
            "customer",
            CustomerTimelineQuery::default(),
            true,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "CUSTOMER_TIMELINE_PERMISSION_REQUIRED");
    })
    .await;
}

#[tokio::test]
async fn per_source_gates_hide_unavailable_owners_without_failing_the_page() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let basic = actor(
            "user",
            "company",
            None,
            None,
            "support_agent",
            "assignment-support",
            &["commerce.read"],
            &[],
        );
        let page = read_page(
            &pool,
            &basic,
            "customer",
            CustomerTimelineQuery::default(),
            false,
        )
        .await
        .unwrap();
        assert_eq!(page.items.len(), 11);
        assert!(page.items.iter().all(|item| {
            matches!(
                item.source_type.as_str(),
                "order"
                    | "fulfillment"
                    | "return"
                    | "contact"
                    | "followup"
                    | "crm"
                    | "crm_activity"
            )
        }));
        assert!(!page.coverage.contains(&"Support case history"));
        assert!(
            !page
                .coverage
                .contains(&"Field Service agreements and visits")
        );
        assert!(!page.coverage.contains(&"Finance invoice status"));
        assert!(!page.coverage.contains(&"Legal records linked to orders"));

        let no_purchase = actor(
            "user",
            "company",
            None,
            None,
            "support_agent",
            "assignment-support",
            &[
                "commerce.read",
                "support.read",
                "finance.read",
                "legal.read",
            ],
            &["field_service"],
        );
        let page = read_page(
            &pool,
            &no_purchase,
            "customer",
            CustomerTimelineQuery::default(),
            false,
        )
        .await
        .unwrap();
        assert!(page.items.iter().all(|item| item.source_type != "support"));
        assert!(!page.coverage.contains(&"Support case history"));
        assert!(page.items.iter().any(|item| item.source_type == "invoice"));
        assert!(page.items.iter().any(|item| item.source_type == "legal"));
        assert!(
            page.items
                .iter()
                .any(|item| item.source_type == "field_service")
        );

        let support_only = actor(
            "user",
            "company",
            None,
            None,
            "support_agent",
            "assignment-support",
            &["commerce.read", "support.read"],
            &[],
        );
        let page = read_page(
            &pool,
            &support_only,
            "customer",
            CustomerTimelineQuery::default(),
            true,
        )
        .await
        .unwrap();
        assert_eq!(
            page.items
                .iter()
                .filter(|item| item.source_type == "support")
                .count(),
            1
        );
        assert!(page.coverage.contains(&"Support case history"));
        assert!(!page.coverage.contains(&"Finance invoice status"));
        assert!(
            !page
                .coverage
                .contains(&"Field Service agreements and visits")
        );
        assert!(!page.coverage.contains(&"Legal records linked to orders"));
    })
    .await;
}

#[tokio::test]
async fn legal_membership_and_requester_visibility_are_preserved() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let reader = actor(
            "other-user",
            "company",
            None,
            None,
            "support_agent",
            "assignment-support",
            &["commerce.read", "legal.read"],
            &[],
        );
        let page = read_page(
            &pool,
            &reader,
            "customer",
            CustomerTimelineQuery::default(),
            false,
        )
        .await
        .unwrap();
        let legal = page
            .items
            .iter()
            .filter(|item| item.source_type == "legal")
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(legal, vec!["legal:legal-event-internal"]);

        let requester = actor(
            "user",
            "company",
            None,
            None,
            "legal_requester",
            "assignment-support",
            &["commerce.read", "legal.read"],
            &[],
        );
        let page = read_page(
            &pool,
            &requester,
            "customer",
            CustomerTimelineQuery::default(),
            false,
        )
        .await
        .unwrap();
        let legal = page
            .items
            .iter()
            .filter(|item| item.source_type == "legal")
            .count();
        assert_eq!(legal, 0);
    })
    .await;
}

#[tokio::test]
async fn reads_require_a_serializable_transaction_and_do_not_persist_effects() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let before: i64 = sqlx::query_scalar("SELECT count(*) FROM customer_contact_note_history")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        let owner = full_actor();

        let mut connection = pool.raw().acquire().await.unwrap();
        let mut reader = PgCustomerTimelineRead::new(&mut connection);
        let mut entitlement = Entitlement(true);
        let error = customer_timeline(
            &owner,
            "customer",
            &CustomerTimelineQuery::default(),
            &mut reader,
            &mut entitlement,
            NOW,
        )
        .await
        .unwrap_err();
        match error {
            PgTransactionError::Persistence(error) => {
                assert_eq!(error.code, "CUSTOMER_TIMELINE_READ_TRANSACTION_REQUIRED");
            }
            other => panic!("expected a transaction guard, got {other:?}"),
        }
        drop(connection);

        let mut transaction = pool.raw().begin().await.unwrap();
        let mut reader = PgCustomerTimelineRead::new(&mut transaction);
        let error = customer_timeline(
            &owner,
            "customer",
            &CustomerTimelineQuery::default(),
            &mut reader,
            &mut entitlement,
            NOW,
        )
        .await
        .unwrap_err();
        match error {
            PgTransactionError::Persistence(error) => {
                assert_eq!(error.code, "CUSTOMER_TIMELINE_READ_SERIALIZABLE_REQUIRED");
            }
            other => panic!("expected a serializable guard, got {other:?}"),
        }
        transaction.rollback().await.unwrap();

        let mut transaction = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *transaction)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO customer_contact_notes(id,company_id,customer_id,order_id,channel,occurred_at,body,status,version,created_by,created_at,updated_at) \
             VALUES('note-rollback','company','customer','order-main','phone',500,'Rollback contact','active',1,'owner',500,500)",
        )
        .execute(&mut *transaction)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO customer_contact_note_history(id,note_id,version,action,body,reason,actor_id,created_at) \
             VALUES('note-rollback-history','note-rollback',1,'create','Rollback contact','Synthetic','owner',500)",
        )
        .execute(&mut *transaction)
        .await
        .unwrap();
        let mut reader = PgCustomerTimelineRead::new(&mut transaction);
        let page = customer_timeline(
            &owner,
            "customer",
            &CustomerTimelineQuery::default(),
            &mut reader,
            &mut entitlement,
            NOW,
        )
        .await
        .unwrap();
        assert_eq!(page.items[0].id, "contact:note-rollback-history");
        assert_eq!(page.items.len(), 18);
        transaction.rollback().await.unwrap();

        let after: i64 = sqlx::query_scalar("SELECT count(*) FROM customer_contact_note_history")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(after, before);
        let page = read_page(
            &pool,
            &owner,
            "customer",
            CustomerTimelineQuery::default(),
            true,
        )
        .await
        .unwrap();
        assert!(!page.items.iter().any(|item| item.id == "contact:note-rollback-history"));
        assert_eq!(page.items.len(), 17);
    })
    .await;
}

#[tokio::test]
async fn persisted_source_rows_are_returned_unchanged() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let before: String = sqlx::query_scalar(
            "SELECT encode(sha256(convert_to((SELECT jsonb_agg(to_jsonb(o) ORDER BY o.id) FROM commerce_orders o)::text,'UTF8')),'hex')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        let page = read_page(
            &pool,
            &full_actor(),
            "customer",
            CustomerTimelineQuery::default(),
            true,
        )
        .await
        .unwrap();
        assert_eq!(page.items.len(), 17);
        let after: String = sqlx::query_scalar(
            "SELECT encode(sha256(convert_to((SELECT jsonb_agg(to_jsonb(o) ORDER BY o.id) FROM commerce_orders o)::text,'UTF8')),'hex')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(before, after);
        let row = sqlx::query("SELECT state,version FROM commerce_orders WHERE id='order-main'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(row.get::<String, _>("state"), "confirmed");
        assert_eq!(row.get::<i64, _>("version"), 1);
    })
    .await;
}


async fn read_activity(
    pool: &DatabasePool,
    actor: &CustomerTimelineActor,
    query: ExecutiveCustomerActivityQuery,
    purchased: bool,
) -> Result<Vec<erpx_server_customer_360::public::customer_activity::ExecutiveCustomerActivityRow>, BusinessError> {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let query = query.clone();
            Box::pin(async move {
                let mut reader = PgCustomerTimelineRead::new(connection);
                let mut entitlement = Entitlement(purchased);
                executive_customer_activity(&actor, &query, &mut reader, &mut entitlement, NOW).await
            })
        })
        .await
        .map_err(|error| match error {
            PgTransactionError::Business(error) => error,
            other => panic!("expected an owner business failure, got {other:?}"),
        })
}

fn activity_query(limit: usize) -> ExecutiveCustomerActivityQuery {
    ExecutiveCustomerActivityQuery {
        from: 0,
        to: 1_000_000,
        query: None,
        cursor: None,
        limit,
        record_id: None,
    }
}

#[tokio::test]
async fn executive_activity_federates_cross_customer_rows_and_honors_gates() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let full = full_actor();
        let rows = read_activity(&pool, &full, activity_query(100), true)
            .await
            .unwrap();
        let ids = rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec![
                "contact:note-history-dept",
                "order:order-dept",
                "contact:note-history-other",
                "order:order-other",
                "crm-activity:crm-activity-main",
                "crm:crm-event-main",
                "invoice:invoice-main",
                "legal:legal-event-confidential",
                "legal:legal-event-internal",
                "service-order:service-order-event-main",
                "service-agreement:agreement-event-main",
                "support:support-event-main",
                "followup:followup-event-main",
                "contact:note-history-main",
                "return:return-main",
                "fulfillment:fulfillment-main",
                "order:order-main",
            ]
        );
        assert!(rows.iter().all(|row| row.customer_name == "Acme Customer"));
        assert!(rows.iter().all(|row| row.customer_id == "customer"));

        let unpurchased = read_activity(&pool, &full, activity_query(100), false)
            .await
            .unwrap();
        assert!(!unpurchased.iter().any(|row| row.id.starts_with("support:")));
        assert!(unpurchased.iter().any(|row| row.id.starts_with("invoice:")));

        let restricted = actor(
            "user",
            "company",
            None,
            None,
            "sales_agent",
            "assignment-support",
            &["commerce.read"],
            &[],
        );
        let restricted_rows = read_activity(&pool, &restricted, activity_query(100), false)
            .await
            .unwrap();
        let restricted_ids = restricted_rows
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>();
        assert!(!restricted_ids.iter().any(|id| id.starts_with("invoice:")));
        assert!(!restricted_ids.iter().any(|id| id.starts_with("legal:")));
        assert!(!restricted_ids.iter().any(|id| id.starts_with("support:")));
        assert!(!restricted_ids.iter().any(|id| id.starts_with("service-")));
        assert_eq!(restricted_ids.len(), 11);

        let mut search = activity_query(100);
        search.query = Some("Confidential".into());
        let searched = read_activity(&pool, &full, search, true).await.unwrap();
        assert_eq!(
            searched.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            vec!["legal:legal-event-confidential"]
        );

        let mut record = activity_query(100);
        record.record_id = Some("order:order-main".into());
        let read = read_activity(&pool, &full, record, true).await.unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].id, "order:order-main");

        let mut cursor = None;
        let mut paged = Vec::new();
        loop {
            let mut query = activity_query(5);
            query.cursor = cursor.clone();
            let page = read_activity(&pool, &full, query, true).await.unwrap();
            paged.extend(page.iter().map(|row| row.id.clone()));
            match page.last() {
                Some(last) if page.len() > 5 => cursor = Some(CustomerTimelineCursor { at: last.at, id: last.id.clone() }),
                _ => break,
            }
            if page.len() <= 5 {
                break;
            }
        }
        assert_eq!(paged.len(), 17);
        assert_eq!(paged.iter().collect::<std::collections::BTreeSet<_>>().len(), 17);

        let branch = actor(
            "user",
            "company",
            Some("branch"),
            None,
            "support_agent",
            "assignment-support",
            &["commerce.read", "support.read", "finance.read", "legal.read"],
            &["field_service"],
        );
        let branch_rows = read_activity(&pool, &branch, activity_query(100), true)
            .await
            .unwrap();
        let branch_ids = branch_rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>();
        assert!(!branch_ids.contains(&"order:order-other"));
        assert!(!branch_ids.contains(&"contact:note-history-other"));
        assert!(branch_ids.contains(&"order:order-dept"));
    });
}
