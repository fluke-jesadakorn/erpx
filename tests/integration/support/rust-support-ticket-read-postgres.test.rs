#![allow(dead_code)]
//! Native Support ticket workspace/detail reads over leased PostgreSQL with
//! current Account/IAM authority, purchase history and typed source links.
#[path = "../../support/finance/review-composition.rs"]
mod review;
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_identity_access::public::work_context::{
    ExpectedWorkSelection, HoldingWorkspaceRequest,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{AssignmentId, Clock, CompanyId, HoldingId};
use erpx_server_support::public::support_ticket_read::{
    SupportReadScope, read_customer_contact_notes, read_order_sources, read_ticket_detail,
    read_ticket_history, read_ticket_workspace,
};
use erpx_service::support_read_http::{SupportReadHttpTarget, support_read_http_router};
use erpx_service::support_ticket_read::SupportTicketReadService;
use std::sync::Arc;

#[derive(Clone)]
struct TestClock(Arc<review::account::fixture::MutableClock>);
impl Clock for TestClock {
    fn now(&self) -> erpx_server_shared::public::TimestampMillis {
        self.0.now()
    }
}

fn scope(assignment: &str, branch: Option<&str>, department: Option<&str>) -> SupportReadScope {
    scoped(assignment, "custom_support_agent", branch, department)
}

fn scoped(assignment: &str, role: &str, branch: Option<&str>, department: Option<&str>) -> SupportReadScope {
    scoped_user("fixture-a", assignment, role, branch, department)
}

fn scoped_user(
    user: &str,
    assignment: &str,
    role: &str,
    branch: Option<&str>,
    department: Option<&str>,
) -> SupportReadScope {
    SupportReadScope {
        company_id: CompanyId::new("company").unwrap(),
        branch_id: branch.map(str::to_owned),
        department_id: department.map(str::to_owned),
        user_id: user.into(),
        assignment_id: assignment.into(),
        role: role.into(),
        support_read: true,
        records_read: true,
        support_create: true,
        support_work: true,
        support_manage: role.contains("manager"),
        commerce_read: true,
    }
}

async fn serializable<'c>(
    pool: &'c erpx_server_runtime::public::postgres::DatabasePool,
) -> sqlx::Transaction<'c, sqlx::Postgres> {
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx
}

fn selection(
    assignment: &str,
    branch_slug: Option<&str>,
    department_slug: Option<&str>,
) -> HoldingWorkspaceRequest {
    HoldingWorkspaceRequest {
        company_id: CompanyId::new("company").unwrap(),
        branch_slug: branch_slug.map(str::to_owned),
        department_slug: department_slug.map(str::to_owned),
        expected: Some(ExpectedWorkSelection {
            revision: 7,
            assignment_id: AssignmentId::new(assignment).unwrap(),
            assignment_version: 3,
        }),
        mutation: false,
    }
}

async fn activate(
    pool: &erpx_server_runtime::public::postgres::DatabasePool,
    user: &str,
    assignment: &str,
) {
    let session: String = sqlx::query_scalar("SELECT session_id FROM active_work_contexts WHERE user_id=$1")
        .bind(user)
        .fetch_one(pool.raw())
        .await
        .unwrap();
    sqlx::query("UPDATE active_work_contexts SET assignment_id=$2,assignment_version=3,revision=7 WHERE session_id=$1")
        .bind(&session)
        .bind(assignment)
        .execute(pool.raw())
        .await
        .unwrap();
}

fn read_service(p: &review::Prepared) -> Arc<SupportTicketReadService<TestClock>> {
    Arc::new(SupportTicketReadService {
        authorizer: p.authorizer.clone(),
        holding: HoldingId::new("h001").unwrap(),
        company: CompanyId::new("company").unwrap(),
        catalogue: Arc::new(current_package_catalogue().unwrap()),
        clock: Arc::new(TestClock(p.system.scenario.clock.clone())),
    })
}

async fn configure(pool: &erpx_server_runtime::public::postgres::DatabasePool, now: i64) {
    let expires = now + 3_600_000;
    sqlx::raw_sql(
        "INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','custom_support_agent','Synthetic Support agent',false,1,'active'),('*','custom_support_manager','Synthetic Support manager',false,1,'active'),('*','custom_support_reporter','Synthetic Support reporter',false,1,'active') ON CONFLICT DO NOTHING;\
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES\
          ('*','custom_support_agent',1,'[\"records.read\",\"records.write\",\"support.read\",\"support.create\",\"support.work\",\"commerce.read\",\"module.support.read\"]','[]','[\"holding\",\"branch\",\"department\"]','synthetic-fixture',0),\
          ('*','custom_support_manager',1,'[\"records.read\",\"records.write\",\"support.read\",\"support.create\",\"support.work\",\"support.manage\",\"commerce.read\",\"module.support.read\"]','[]','[\"holding\",\"branch\",\"department\"]','synthetic-fixture',0),\
          ('*','custom_support_reporter',1,'[\"records.read\",\"records.write\",\"support.create\",\"module.support.read\"]','[]','[\"holding\"]','synthetic-fixture',0) ON CONFLICT DO NOTHING;\
         INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,version,created_by,created_at,updated_at)VALUES\
          ('person-a','fixture-a','company','Synthetic Agent','TH','active',1,'synthetic-fixture',0,0),\
          ('person-b','fixture-b','company','Synthetic Manager','TH','active',1,'synthetic-fixture',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,end_date,job_title,currency,version,created_by,created_at,updated_at)VALUES\
          ('engagement-a','person-a','company','employee','salaried','active','2026-01-01',NULL,'Support','THB',1,'synthetic-fixture',0,0),\
          ('engagement-b','person-b','company','employee','salaried','active','2026-01-01',NULL,'Support','THB',1,'synthetic-fixture',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version)VALUES\
          ('company','fixture-a','local:fixture-a','person-a','active','local',1),\
          ('company','fixture-b','local:fixture-b','person-b','active','local',1) ON CONFLICT DO NOTHING;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    for (assignment, user, role, scope_kind, scope_node) in [
        ("support-holding-a", "fixture-a", "custom_support_agent", "holding", None),
        ("support-branch-a", "fixture-a", "custom_support_agent", "branch", Some("branch-1")),
        ("support-department-a", "fixture-a", "custom_support_agent", "department", Some("department-1")),
        ("support-reporter-a", "fixture-a", "custom_support_reporter", "holding", None),
        ("support-holding-b", "fixture-b", "custom_support_manager", "holding", None),
        ("support-branch-b", "fixture-b", "custom_support_manager", "branch", Some("branch-1")),
    ] {
        sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind,scope_node_id)VALUES($1,$2,$3,'[\"support\"]','company',$4,$5,3,$6,$7)")
            .bind(assignment)
            .bind(user)
            .bind(role)
            .bind(0_i64)
            .bind(9_007_199_254_740_991_i64)
            .bind(scope_kind)
            .bind(scope_node)
            .execute(pool.raw())
            .await
            .unwrap();
    }
    for (user, assignment) in [
        ("fixture-a", "support-holding-a"),
        ("fixture-b", "support-holding-b"),
    ] {
        let session: String = sqlx::query_scalar("SELECT session_id FROM active_work_contexts WHERE user_id=$1")
            .bind(user)
            .fetch_one(pool.raw())
            .await
            .unwrap();
        sqlx::query("UPDATE active_work_contexts SET assignment_id=$2,assignment_version=3,revision=7,expires_at=$3,updated_at=$4 WHERE session_id=$1")
            .bind(&session)
            .bind(assignment)
            .bind(expires)
            .bind(now)
            .execute(pool.raw())
            .await
            .unwrap();
    }
    let mut grants = pool.raw().begin().await.unwrap();
    for package in ["support", "assets"] {
        sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at)VALUES('company',$1,1,'active',0,$2,1,'Synthetic reviewed purchase','public synthetic evidence',$3) ON CONFLICT DO NOTHING")
            .bind(package)
            .bind(expires)
            .bind(now)
            .execute(&mut *grants)
            .await
            .unwrap();
        sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)VALUES($1,'company',$2,'fixture-a','synthetic-request',NULL,jsonb_build_object('companyId','company','packageId',$2,'packageVersion',1,'state','active','startsAt',0,'endsAt',$3,'version',1,'reason','Synthetic reviewed purchase','evidence','public synthetic evidence'),$4) ON CONFLICT DO NOTHING")
            .bind(format!("{package}-origin-grant"))
            .bind(package)
            .bind(expires)
            .bind(now)
            .execute(&mut *grants)
            .await
            .unwrap();
    }
    grants.commit().await.unwrap();
    sqlx::raw_sql(
        "INSERT INTO support_sla_policies(id,company_id,name,policy_json,status,version,created_by,approved_by,approved_at,created_at,updated_at)VALUES('policy-1','company','Working time','{\"calendar\":{\"timezone\":\"Asia/Bangkok\",\"weekdays\":[1,2,3,4,5],\"opensAtMinute\":540,\"closesAtMinute\":1020,\"holidays\":[]},\"targetMinutes\":60,\"pauseWhileWaiting\":true}','approved',1,'fixture-a','fixture-b',0,0,0);\
         INSERT INTO support_incidents(id,company_id,branch_id,department_id,title,impact,severity,owner_id,due_date,status,version,created_by,created_at,updated_at)VALUES('incident-1','company','branch-1','department-1','Shared printer outage','Several cases share the same printer','normal','fixture-b','2026-11-20','open',1,'fixture-b',0,0);\
         INSERT INTO operational_assets(id,company_id,branch_id,department_id,code,name,category,currency,cost_minor,condition,lifecycle_status,version,created_by,created_at,updated_at)VALUES('asset-1','company','branch-1','department-1','ASSET-SUPPORT','Shared lobby display','Display','THB',0,'good','registered',1,'fixture-a',0,0);\
         INSERT INTO support_tickets(id,company_id,branch_id,department_id,title,issue,owner_id,due_date,status,blocked_reason,resolution,version,created_by,created_at,updated_at,source_kind)VALUES\
          ('ticket-branch-1','company','branch-1','department-1','Printer offline','The front desk printer is offline.','fixture-a','2026-11-10','open',NULL,NULL,1,'fixture-a',100,300,'general'),\
          ('ticket-branch-2','company','branch-2','department-2','Lobby display offline','The lobby display does not show the schedule.','fixture-a','2026-11-11','in_progress',NULL,NULL,2,'fixture-a',110,310,'general'),\
          ('ticket-other-company','other','branch-9',NULL,'Foreign ticket','This row must never cross the Company.','fixture-a','2026-11-12','open',NULL,NULL,1,'fixture-a',120,320,'general'),\
          ('ticket-terminal','company','branch-1','department-1','Withdrawn report','The reporter withdrew this report before triage.',NULL,NULL,'cancelled',NULL,NULL,2,'fixture-a',130,330,'general'),\
          ('ticket-owned-report','company','branch-1','department-1','Own keyboard issue','My keyboard stops responding after wake.','fixture-a','2026-11-13','open',NULL,NULL,1,'fixture-a',140,340,'general'),\
          ('ticket-reporter-b','company','branch-1','department-1','Another reporter issue','This report belongs to another user.','fixture-b','2026-11-13','open',NULL,NULL,1,'fixture-b',141,341,'general');\
         INSERT INTO support_tickets(id,company_id,branch_id,department_id,title,issue,owner_id,due_date,status,version,created_by,created_at,updated_at,source_kind,source_record_json)VALUES\
          ('ticket-asset','company','branch-1','department-1','Asset display fault','The linked lobby display lost its schedule.','fixture-a','2026-11-14','open',1,'fixture-a',150,350,'asset','{\"kind\":\"asset\",\"id\":\"asset-1\",\"version\":1,\"companyId\":\"company\",\"branchId\":\"branch-1\",\"departmentId\":\"department-1\",\"title\":\"Shared lobby display\",\"status\":\"registered\",\"href\":\"/assets/asset-1\",\"label\":\"Asset · Shared lobby display\"}');\
         INSERT INTO support_incident_tickets(incident_id,ticket_id,linked_version,linked_by,linked_at,active)VALUES('incident-1','ticket-branch-1',1,'fixture-b',0,true);\
         INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES\
          ('branch-1','branch','branch-1','Branch One','company','company',NULL,'active'),\
          ('branch-2','branch','branch-2','Branch Two','company','company',NULL,'active'),\
          ('branch-9','branch','branch-9','Foreign Branch','company','company',NULL,'active'),\
          ('department-1','department','department-1','Department One','branch-1','company','branch-1','active'),\
          ('department-2','department','department-2','Department Two','branch-2','company','branch-2','active');\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at)VALUES('party-support','company','customer','CUS-SUP','Synthetic Support Customer','THB',0,0);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version)VALUES('warehouse-support','company','branch-1','WH-SUPPORT','Support Warehouse','active',1);\
         INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,state,subtotal_minor,tax_minor,version,created_by,created_at,updated_at)VALUES('order-support','company','branch-1','department-1','sales','SO-SUPPORT','party-support','warehouse-support','THB','2026-11-01','2026-11-10','confirmed',0,0,1,'fixture-a',0,0);\
         INSERT INTO customer_contact_notes(id,company_id,customer_id,order_id,channel,occurred_at,body,status,version,created_by,created_at,updated_at)VALUES('note-support','company','party-support','order-support','phone',250,'Customer confirmed the replacement works.', 'active',1,'fixture-a',250,250);\
         INSERT INTO support_ticket_events(id,ticket_id,version,action,actor_id,reason,owner_id,due_date,status,created_at,evidence_json)VALUES\
          ('event-1','ticket-branch-1',1,'create','fixture-a','Reported the front desk printer','fixture-a','2026-11-10','open',100,'{\"kind\":\"report\"}'),\
          ('event-2','ticket-terminal',2,'withdraw-report','fixture-a','Withdrawn before triage',NULL,NULL,'cancelled',330,'{\"action\":\"withdraw-report\"}'),\
          ('event-3','ticket-owned-report',1,'report','fixture-a','Own keyboard report','fixture-a','2026-11-13','open',140,'{\"kind\":\"report\"}');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    let expires = now + 3_600_000;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "INSERT INTO support_sla_policies(id,company_id,name,policy_json,status,version,created_by,approved_by,approved_at,created_at,updated_at)VALUES('policy-2','company','Draft resolution','{{\"calendar\":{{\"timezone\":\"Asia/Bangkok\",\"weekdays\":[1,2,3,4,5],\"opensAtMinute\":540,\"closesAtMinute\":1020,\"holidays\":[]}},\"targetMinutes\":60,\"pauseWhileWaiting\":true}}','draft',1,'fixture-a',NULL,NULL,1,1);\
         INSERT INTO support_tickets(id,company_id,branch_id,department_id,order_id,customer_id,title,issue,owner_id,due_date,status,version,created_by,created_at,updated_at,source_kind,sla_policy_id,sla_policy_version,sla_clock_json)VALUES\
          ('ticket-clock','company','branch-1','department-1',NULL,NULL,'Clock case','Resolution target is attached.','fixture-a','2026-11-16','open',1,'fixture-a',160,360,'general','policy-1',1,'{{\"snapshot\":{{\"calendar\":{{\"timezone\":\"Asia/Bangkok\",\"weekdays\":[1,2,3,4,5],\"opensAtMinute\":540,\"closesAtMinute\":1020,\"holidays\":[]}},\"targetMinutes\":60,\"pauseWhileWaiting\":true}},\"remainingMs\":3600000,\"startedAt\":0,\"dueAt\":9999999999999,\"state\":\"running\",\"breachedAt\":null}}'::jsonb),\
          ('ticket-clock-2','company','branch-2','department-2',NULL,NULL,'Other clock case','Resolution target is attached elsewhere.','fixture-a','2026-11-17','open',1,'fixture-a',161,361,'general','policy-1',1,'{{\"snapshot\":{{\"calendar\":{{\"timezone\":\"Asia/Bangkok\",\"weekdays\":[1,2,3,4,5],\"opensAtMinute\":540,\"closesAtMinute\":1020,\"holidays\":[]}},\"targetMinutes\":60,\"pauseWhileWaiting\":true}},\"remainingMs\":3600000,\"startedAt\":0,\"dueAt\":9999999999999,\"state\":\"running\",\"breachedAt\":null}}'::jsonb),\
          ('ticket-session','company','branch-1','department-1','order-support','party-support','Temporary access case','A colleague needs temporary customer context.','fixture-a','2026-11-15','open',1,'fixture-a',170,370,'sales_order',NULL,NULL,NULL),\
          ('ticket-sales-extra','company','branch-1','department-1','order-support','party-support','Unlinked customer case','This shared customer case is available to link.','fixture-a','2026-11-18','open',1,'fixture-a',171,371,'sales_order',NULL,NULL,NULL);\
         INSERT INTO support_sla_alerts(id,ticket_id,source_version,breached_at,detected_at,version,acknowledged_by,acknowledged_at,reason)VALUES\
          ('alert-1','ticket-clock',1,{breached},{breached},1,NULL,NULL,NULL),\
          ('alert-2','ticket-clock-2',1,{breached},{breached},1,NULL,NULL,NULL);\
         INSERT INTO support_sessions(id,company_id,branch_id,department_id,ticket_id,ticket_version,requested_for,reason,expires_at,status,created_by,reviewed_by,reviewed_at,revoked_by,revoked_at,version,created_at,updated_at)VALUES\
          ('session-pending','company','branch-1','department-1','ticket-session',1,'fixture-a','Temporary customer context',{expires},'pending','fixture-b',NULL,NULL,NULL,NULL,1,{created_a},{created_a}),\
          ('session-approved','company','branch-1','department-1','ticket-session',1,'fixture-a','Approved customer context',{expires},'approved','fixture-b','fixture-b',{reviewed},NULL,NULL,2,{created_b},{reviewed}),\
          ('session-expired','company','branch-1','department-1','ticket-session',1,'fixture-a','Expired customer context',1000,'pending','fixture-b',NULL,NULL,NULL,NULL,1,0,0),\
          ('session-review','company','branch-1','department-1','ticket-session',1,'fixture-a','Awaiting independent review',{expires},'pending','fixture-a',NULL,NULL,NULL,NULL,1,{created_c},{created_c});\
         INSERT INTO support_session_events(id,session_id,version,action,actor_id,reason,status,expires_at,created_at)VALUES\
          ('session-event-pending','session-pending',1,'request','fixture-b','Temporary customer context','pending',{expires},{created_a}),\
          ('session-event-approved-1','session-approved',1,'request','fixture-b','Approved customer context','pending',{expires},{created_b}),\
          ('session-event-approved-2','session-approved',2,'approve','fixture-b','Approved customer context','approved',{expires},{reviewed}),\
          ('session-event-expired','session-expired',1,'request','fixture-b','Expired customer context','pending',1000,0),\
          ('session-event-review','session-review',1,'request','fixture-a','Awaiting independent review','pending',{expires},{created_c});\
         INSERT INTO support_capabilities(id,session_id,code,status,reason,issued_by,revoked_by,revoked_at,version,created_at,updated_at)VALUES\
          ('cap-context','session-approved','customer_order_context','active','Customer order context','fixture-b',NULL,NULL,1,{reviewed},{reviewed}),\
          ('cap-evidence','session-approved','ticket_evidence_metadata','active','Evidence metadata','fixture-b',NULL,NULL,1,{reviewed},{reviewed});\
         INSERT INTO support_capability_events(id,capability_id,version,action,actor_id,reason,status,created_at)VALUES\
          ('cap-event-context','cap-context',1,'issue','fixture-b','Customer order context','active',{reviewed}),\
          ('cap-event-evidence','cap-evidence',1,'issue','fixture-b','Evidence metadata','active',{reviewed});\
         INSERT INTO support_incidents(id,company_id,branch_id,department_id,title,impact,severity,owner_id,due_date,status,version,created_by,created_at,updated_at)VALUES\
          ('incident-2','company','branch-2','department-2','Branch two outage','Only the second branch is affected','high','fixture-a','2026-11-21','open',1,'fixture-a',0,0),\
          ('incident-3','company','branch-1','department-1','Recipient incident','Shared customer cases keep their own recipients','high','fixture-a','2026-11-23','open',1,'fixture-a',0,0),\
          ('incident-foreign','other','branch-9',NULL,'Foreign incident','This row must never cross the Company','normal','fixture-b','2026-11-22','open',1,'fixture-b',0,0);\
         INSERT INTO support_incident_tickets(incident_id,ticket_id,linked_version,linked_by,linked_at,active)VALUES\
          ('incident-2','ticket-branch-2',2,'fixture-a',0,true),\
          ('incident-3','ticket-session',1,'fixture-a',0,true);\
         INSERT INTO support_incident_events(id,incident_id,version,action,actor_id,reason,created_at,evidence_json)VALUES\
          ('incident-event-1','incident-1',1,'create','fixture-b','Declared the shared incident',100,'{{\"kind\":\"incident\"}}'),\
          ('incident-event-2','incident-1',2,'start','fixture-b','Started the investigation',110,'{{\"action\":\"start\"}}'),\
          ('incident-event-3','incident-2',1,'create','fixture-a','Declared the branch two incident',120,'{{\"kind\":\"incident\"}}'),\
          ('incident-event-4','incident-3',1,'create','fixture-a','Declared the recipient incident',130,'{{\"kind\":\"incident\"}}');\
         INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at,owner_version,company_id)VALUES\
          ('upload-support-1','support_ticket','ticket-session','internal_evidence','support-available.pdf','application/pdf',128,128,'{a}','staging/support-1','final/support-1','ready','fixture-a',9007199254740991,0,0,1,'company'),\
          ('upload-support-2','support_ticket','ticket-session','internal_evidence','support-revoked.pdf','application/pdf',128,128,'{a}','staging/support-2','final/support-2','ready','fixture-a',9007199254740991,0,0,1,'company'),\
          ('upload-support-3','support_ticket','ticket-session','internal_evidence','support-hidden.pdf','application/pdf',128,128,'{a}','staging/support-3','final/support-3','ready','fixture-a',9007199254740991,0,0,1,'company');\
         INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,legal_hold,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at,revoked_at)VALUES\
          ('evidence-available','upload-support-1','support_ticket','ticket-session','internal_evidence','final/support-1',1,'support-available.pdf','application/pdf',128,'{a}','fixture-a',0,200,'company','application/pdf','available',0,0,NULL),\
          ('evidence-revoked','upload-support-2','support_ticket','ticket-session','internal_evidence','final/support-2',1,'support-revoked.pdf','application/pdf',128,'{a}','fixture-a',0,210,'company','application/pdf','available',0,0,{revoked}),\
          ('evidence-hidden','upload-support-3','support_ticket','ticket-session','internal_evidence','final/support-3',1,'support-hidden.pdf','application/pdf',128,'{a}','fixture-a',0,220,'company','application/pdf','available',0,0,NULL);\
         INSERT INTO attachment_sets(id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at,bound_at,bound_command_id)VALUES\
          ('set-available','company','branch-1','department-1','support_ticket','ticket-session','action','support-evidence:internal_evidence','internal_evidence',1,1,'internal','{{}}',1,10,'bound','fixture-a',0,0,0,'synthetic-bind'),\
          ('set-revoked','company','branch-1','department-1','support_ticket','ticket-session','action','support-evidence:internal_evidence','internal_evidence',1,1,'internal','{{}}',1,10,'bound','fixture-a',0,0,0,'synthetic-bind'),\
          ('set-hidden','company','branch-1','department-1','support_ticket','ticket-branch-1','action','support-evidence:internal_evidence','internal_evidence',1,1,'internal','{{}}',1,10,'bound','fixture-a',0,0,0,'synthetic-bind');\
         INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at,bound_at,binding_type,binding_id)VALUES\
          ('link-available','set-available','evidence-available','bound','fixture-a',0,9007199254740991,0,'support_action','ticket-session'),\
          ('link-revoked','set-revoked','evidence-revoked','bound','fixture-a',0,9007199254740991,0,'support_action','ticket-session'),\
          ('link-hidden','set-hidden','evidence-hidden','bound','fixture-a',0,9007199254740991,0,'support_action','ticket-branch-1');",
        breached = now - 300,
        expires = expires,
        reviewed = now - 500,
        created_a = now - 1_000,
        created_b = now - 2_000,
        created_c = now - 1_500,
        revoked = now - 100,
        a = "a".repeat(64),
    )))
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn ticket_reads_keep_company_branch_department_scope_authority_and_history() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        let pool = &p.system.holdings[0];
        let now = p.system.scenario.clock.now().get();
        configure(pool, now).await;

        let mut tx = serializable(pool).await;
        let rows = read_ticket_workspace(&mut tx, &scope("support-holding-a", None, None))
            .await
            .unwrap();
        let ids = rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>();
        assert!(ids.contains(&"ticket-branch-1"));
        assert!(ids.contains(&"ticket-branch-2"));
        assert!(ids.contains(&"ticket-terminal"));
        assert!(ids.contains(&"ticket-asset"));
        assert!(!ids.contains(&"ticket-other-company"));

        let detail = read_ticket_detail(&mut tx, &scope("support-holding-a", None, None), "ticket-branch-1")
            .await
            .unwrap()
            .expect("ticket detail");
        assert_eq!(detail["title"], "Printer offline");
        assert_eq!(detail["owner_eligible"], 1);
        assert_eq!(detail["reviewer_count"], 1);
        let history = read_ticket_history(&mut tx, "ticket-branch-1").await.unwrap();
        assert_eq!(history[0]["action"], "create");
        assert_eq!(history[0]["evidence_json"]["kind"], "report");

        // Selected branch/department selectors mask other rows.
        let branch = read_ticket_workspace(&mut tx, &scope("support-branch-a", Some("branch-1"), None))
            .await
            .unwrap();
        assert!(branch.iter().all(|row| row.id != "ticket-branch-2"));
        assert!(branch.iter().any(|row| row.id == "ticket-branch-1"));
        let department = read_ticket_workspace(
            &mut tx,
            &scope("support-department-a", Some("branch-1"), Some("department-1")),
        )
        .await
        .unwrap();
        assert!(department.iter().all(|row| row.id != "ticket-branch-2"));

        // Assignment scope must also match the selected branch.
        assert!(
            read_ticket_detail(
                &mut tx,
                &scope("support-branch-a", Some("branch-2"), None),
                "ticket-branch-2"
            )
            .await
            .unwrap()
            .is_none()
        );
        // Foreign Company, absent ticket and terminal rows are explicit.
        assert!(
            read_ticket_detail(
                &mut tx,
                &scope("support-holding-a", None, None),
                "ticket-other-company"
            )
            .await
            .unwrap()
            .is_none()
        );
        assert!(
            read_ticket_detail(
                &mut tx,
                &scope("support-holding-a", None, None),
                "ticket-absent"
            )
            .await
            .unwrap()
            .is_none()
        );
        let terminal = read_ticket_detail(
            &mut tx,
            &scope("support-holding-a", None, None),
            "ticket-terminal",
        )
        .await
        .unwrap()
        .expect("terminal ticket remains readable");
        assert_eq!(terminal["status"], "cancelled");

        // An employee without support.read only sees their own non-order reports.
        let own = read_ticket_workspace(&mut tx, &scoped("support-reporter-a", "custom_support_reporter", None, None))
            .await
            .unwrap();
        assert!(own.iter().any(|row| row.id == "ticket-owned-report"));
        assert!(!own.iter().any(|row| row.id == "ticket-reporter-b"));
        assert!(!own.iter().any(|row| row.id == "ticket-other-company"));

        // Revocation of the exact current assignment closes the read.
        sqlx::query("UPDATE assignments SET revoked_at=$1 WHERE id='support-holding-a'")
            .bind(now)
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(
            read_ticket_detail(
                &mut tx,
                &scope("support-holding-a", None, None),
                "ticket-branch-1"
            )
            .await
            .unwrap()
            .is_none()
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn support_http_read_returns_workspace_and_detail_and_hides_unreadable_source_links() {
    use axum::{body::{Body, to_bytes}, http::Request};
    use erpx_service::field_service_reservation_read_http::FieldServiceReadScopeHeaders;
    use tower::ServiceExt;
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        let pool = &p.system.holdings[0];
        let now = p.system.scenario.clock.now().get();
        configure(pool, now).await;
        let service = Arc::new(SupportTicketReadService {
            authorizer: p.authorizer.clone(),
            holding: HoldingId::new("h001").unwrap(),
            company: CompanyId::new("company").unwrap(),
            catalogue: Arc::new(current_package_catalogue().unwrap()),
            clock: Arc::new(TestClock(p.system.scenario.clock.clone())),
        });
        let router = support_read_http_router(
            Arc::new(review::account::fixture::account::policy()),
            review::account::fixture::account::holding_registry(),
            Arc::new(FieldServiceReadScopeHeaders),
            vec![SupportReadHttpTarget {
                host: "acme.example.test".into(),
                holding_slug: "acme".into(),
                service,
            }],
        )
        .unwrap();
        let get = |path: &'static str, cookie: Option<String>, company: &'static str, assignment: &'static str, origin: Option<&'static str>| {
            let router = router.clone();
            async move {
                let mut request = Request::builder()
                    .method("GET")
                    .uri(path)
                    .header("host", "acme.example.test")
                    .header("x-erpx-context-company", company)
                    .header("x-erpx-context-assignment", assignment)
                    .header("x-erpx-context-revision", "7")
                    .header("x-erpx-context-assignment-version", "3");
                if let Some(cookie) = cookie {
                    request = request.header("cookie", cookie);
                }
                if let Some(origin) = origin {
                    request = request.header("origin", origin);
                }
                let response = router.oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
                let status = response.status();
                let cache = response.headers().get("cache-control").and_then(|value| value.to_str().ok()).map(str::to_owned);
                let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
                (status.as_u16(), cache, serde_json::from_slice::<serde_json::Value>(&body).unwrap())
            }
        };
        let cookie = format!("__Host-erpx-product={}", p.maker_token.expose());
        let (status, cache, value) = get(
            "/api/v1/holdings/acme/support",
            Some(cookie.clone()),
            "company",
            "support-holding-a",
            Some("https://acme.example.test"),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(cache.as_deref(), Some("private, no-store"));
        assert!(value["data"]["items"].as_array().unwrap().iter().any(|row| row["id"] == "ticket-branch-1"));
        assert!(!value["data"]["items"].as_array().unwrap().iter().any(|row| row["id"] == "ticket-other-company"));
        assert_eq!(value["data"]["ownReportsOnly"], false);

        let (status, _, value) = get(
            "/api/v1/holdings/acme/support?id=ticket-branch-1",
            Some(cookie.clone()),
            "company",
            "support-holding-a",
            None,
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(value["data"]["row"]["id"], "ticket-branch-1");
        assert_eq!(value["data"]["incidents"][0]["id"], "incident-1");
        assert_eq!(value["data"]["history"][0]["action"], "create");
        assert_eq!(value["data"]["canWork"], true);
        assert_eq!(value["data"]["canManage"], false);
        assert_eq!(value["data"]["slaPolicies"], serde_json::json!([]));

        let (status, _, value) = get(
            "/api/v1/holdings/acme/support?id=ticket-branch-1",
            Some(format!("__Host-erpx-product={}", p.reviewer_token.expose())),
            "company",
            "support-holding-b",
            None,
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(value["data"]["canManage"], true);
        assert_eq!(value["data"]["canConfirm"], true);
        assert_eq!(value["data"]["slaPolicies"][0]["id"], "policy-1");

        // Incident, SLA, session and evidence GET reads share the same exact
        // Holding host, Account session and work-context authority.
        let (status, cache, value) = get(
            "/api/v1/holdings/acme/support/incidents",
            Some(cookie.clone()),
            "company",
            "support-holding-a",
            Some("https://acme.example.test"),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(cache.as_deref(), Some("private, no-store"));
        assert!(value["data"]["items"].as_array().unwrap().iter().any(|row| row["id"] == "incident-1"));
        assert!(!value["data"]["items"].as_array().unwrap().iter().any(|row| row["id"] == "incident-foreign"));
        assert_eq!(value["data"]["canManage"], false);
        let (status, _, value) = get(
            "/api/v1/holdings/acme/support/incidents?id=incident-3",
            Some(cookie.clone()),
            "company",
            "support-holding-a",
            None,
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(value["data"]["row"]["id"], "incident-3");
        assert_eq!(value["data"]["links"][0]["id"], "ticket-session");
        assert_eq!(value["data"]["recipients"].as_array().unwrap().len(), 2);
        assert_eq!(value["data"]["canWork"], true);
        let (status, _, value) = get(
            "/api/v1/holdings/acme/support/incidents?id=incident-3",
            Some(format!("__Host-erpx-product={}", p.reviewer_token.expose())),
            "company",
            "support-holding-b",
            None,
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(value["data"]["canReview"], true);
        assert_eq!(value["data"]["canManage"], true);

        let (status, _, value) = get(
            "/api/v1/holdings/acme/support/sla",
            Some(format!("__Host-erpx-product={}", p.reviewer_token.expose())),
            "company",
            "support-holding-b",
            None,
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(value["data"]["canManage"], true);
        assert_eq!(value["data"]["canCreate"], true);
        assert!(value["data"]["items"].as_array().unwrap().iter().any(|row| row["id"] == "ticket-clock"));
        assert_eq!(value["data"]["alerts"][0]["can_ack"], 1);

        let (status, _, value) = get(
            "/api/v1/holdings/acme/support/sessions",
            Some(cookie.clone()),
            "company",
            "support-holding-a",
            None,
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(value["data"]["actorId"], "fixture-a");
        assert!(value["data"]["rows"].as_array().unwrap().iter().any(|row| row["id"] == "session-approved"));
        let (status, _, value) = get(
            "/api/v1/holdings/acme/support/sessions?id=session-approved",
            Some(cookie.clone()),
            "company",
            "support-holding-a",
            None,
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(value["data"]["row"]["effective_status"], "approved");
        assert_eq!(value["data"]["context"]["order_number"], "SO-SUPPORT");
        assert_eq!(value["data"]["context"]["evidence"].as_array().unwrap().len(), 1);

        let (status, _, value) = get(
            "/api/v1/holdings/acme/support/evidence?id=ticket-session",
            Some(cookie.clone()),
            "company",
            "support-holding-a",
            None,
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(value["data"]["items"].as_array().unwrap().len(), 2);
        assert_eq!(value["data"]["canChange"], true);
        assert!(!value.to_string().contains("support-hidden.pdf"));
        let (status, _, value) = get(
            "/api/v1/holdings/acme/support/evidence?id=ticket-session",
            Some(format!("__Host-erpx-product={}", p.reviewer_token.expose())),
            "company",
            "support-holding-b",
            None,
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(value["data"]["canShare"], true);
        assert_eq!(
            get(
                "/api/v1/holdings/acme/support/evidence",
                Some(cookie.clone()),
                "company",
                "support-holding-a",
                None
            )
            .await
            .0,
            422
        );
        assert_eq!(
            get(
                "/api/v1/holdings/acme/support/evidence?id=ticket-other-company",
                Some(cookie.clone()),
                "company",
                "support-holding-a",
                None
            )
            .await
            .0,
            404
        );
        assert_eq!(
            get(
                "/api/v1/holdings/acme/support/incidents",
                Some(cookie.clone()),
                "company",
                "support-holding-a",
                Some("https://evil.example.test")
            )
            .await
            .0,
            403
        );
        assert_eq!(
            get(
                "/api/v1/holdings/acme/support/sessions",
                None,
                "company",
                "support-holding-a",
                None
            )
            .await
            .0,
            401
        );

        // A linked Asset source is hidden without the owning module authority.
        let (status, _, value) = get(
            "/api/v1/holdings/acme/support",
            Some(cookie.clone()),
            "company",
            "support-holding-a",
            None,
        )
        .await;
        assert_eq!(status, 200);
        assert!(!value["data"]["items"].as_array().unwrap().iter().any(|row| row["id"] == "ticket-asset"));
        let (status, _, value) = get(
            "/api/v1/holdings/acme/support?id=ticket-asset",
            Some(cookie.clone()),
            "company",
            "support-holding-a",
            None,
        )
        .await;
        assert_eq!(status, 404);
        assert_eq!(value["error"]["code"], "SUPPORT_TICKET_NOT_FOUND");
        assert!(!value.to_string().contains("Asset display fault"));

        // Restoring the owning module makes the same retained link readable.
        let mut role = pool.raw().begin().await.unwrap();
        sqlx::query("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)SELECT company_id,role_id,2,permissions_json||'[\"module.assets.read\"]'::jsonb,grantable_json,scopes_json,'synthetic-fixture',0 FROM iam_role_revisions WHERE company_id='*' AND role_id='custom_support_agent' AND revision=1")
            .execute(&mut *role)
            .await
            .unwrap();
        sqlx::query("UPDATE iam_roles SET published_revision=2 WHERE company_id='*' AND id='custom_support_agent'")
            .execute(&mut *role)
            .await
            .unwrap();
        role.commit().await.unwrap();
        let (status, _, value) = get(
            "/api/v1/holdings/acme/support",
            Some(cookie.clone()),
            "company",
            "support-holding-a",
            None,
        )
        .await;
        assert_eq!(status, 200);
        assert!(value["data"]["items"].as_array().unwrap().iter().any(|row| row["id"] == "ticket-asset"));
        let (status, _, value) = get(
            "/api/v1/holdings/acme/support?id=ticket-asset",
            Some(cookie.clone()),
            "company",
            "support-holding-a",
            None,
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(value["data"]["row"]["source_kind"], "asset");
        assert_eq!(value["data"]["row"]["source_record_json"], "{\"kind\":\"asset\",\"id\":\"asset-1\",\"version\":1,\"companyId\":\"company\",\"branchId\":\"branch-1\",\"departmentId\":\"department-1\",\"title\":\"Shared lobby display\",\"status\":\"registered\",\"href\":\"/assets/asset-1\",\"label\":\"Asset · Shared lobby display\"}");

        // Unauthenticated, origin, foreign Company and absent selectors fail closed.
        assert_eq!(get("/api/v1/holdings/acme/support", None, "company", "support-holding-a", None).await.0, 401);
        assert_eq!(get("/api/v1/holdings/acme/support", Some(cookie.clone()), "company", "support-holding-a", Some("https://evil.example.test")).await.0, 403);
        assert_eq!(get("/api/v1/holdings/acme/support", Some(cookie.clone()), "other", "support-holding-a", None).await.0 != 200, true);
        assert_eq!(get("/api/v1/holdings/other/support", Some(cookie.clone()), "company", "support-holding-a", None).await.0, 404);
        assert_eq!(get("/api/v1/holdings/acme/support?id=ticket-absent", Some(cookie), "company", "support-holding-a", None).await.0, 404);
    })
    .await;
}

#[tokio::test]
async fn ticket_recipients_retain_the_scope() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        let pool = &p.system.holdings[0];
        let now = p.system.scenario.clock.now().get();
        configure(pool, now).await;
        let mut tx = serializable(pool).await;
        let recipients = erpx_server_support::public::support_ticket_read::read_ticket_recipients(
            &mut tx,
            &scope("support-holding-a", None, None),
            "ticket-branch-1",
        )
        .await
        .unwrap();
        assert_eq!(recipients, vec![
            erpx_server_support::public::support_ticket_read::SupportRecipient {
                id: "fixture-a".into(),
                name: "Synthetic Agent".into(),
            },
            erpx_server_support::public::support_ticket_read::SupportRecipient {
                id: "fixture-b".into(),
                name: "Synthetic Manager".into(),
            },
        ]);
        let policies = erpx_server_support::public::support_ticket_read::read_sla_policies(
            &mut tx,
            &CompanyId::new("company").unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(policies.len(), 1);
        let sources = read_order_sources(&mut tx, &scope("support-holding-a", None, None))
            .await
            .unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].id, "order-support");
        assert_eq!(sources[0].customer_name, "Synthetic Support Customer");
        let notes = read_customer_contact_notes(
            &mut tx,
            &CompanyId::new("company").unwrap(),
            "order-support",
        )
        .await
        .unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0]["body"], "Customer confirmed the replacement works.");
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn incident_reads_keep_scope_and_current_reviewer_authority() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        let pool = &p.system.holdings[0];
        let now = p.system.scenario.clock.now().get();
        configure(pool, now).await;
        let service = read_service(&p);

        let workspace = service
            .incident_workspace("acme.example.test", p.maker.clone(), || {
                Ok(selection("support-holding-a", None, None))
            })
            .await
            .unwrap();
        let items = workspace["items"].as_array().unwrap();
        let ids = items
            .iter()
            .map(|row| row["id"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert!(ids.contains(&"incident-1"));
        assert!(ids.contains(&"incident-2"));
        assert!(ids.contains(&"incident-3"));
        assert!(!ids.contains(&"incident-foreign"));
        assert_eq!(workspace["canManage"], false);
        let tickets = workspace["tickets"].as_array().unwrap();
        assert!(tickets.iter().any(|row| row["id"] == "ticket-session"));
        assert!(tickets.iter().all(|row| row["id"] != "ticket-branch-1"));

        // Selected branch/department selectors mask other incident rows.
        activate(pool, "fixture-a", "support-branch-a").await;
        let branch = service
            .incident_workspace("acme.example.test", p.maker.clone(), || {
                Ok(selection("support-branch-a", Some("branch-1"), None))
            })
            .await
            .unwrap();
        assert!(branch["items"].as_array().unwrap().iter().any(|row| row["id"] == "incident-1"));
        assert!(branch["items"].as_array().unwrap().iter().all(|row| row["id"] != "incident-2"));
        activate(pool, "fixture-a", "support-department-a").await;
        let department = service
            .incident_workspace("acme.example.test", p.maker.clone(), || {
                Ok(selection(
                    "support-department-a",
                    Some("branch-1"),
                    Some("department-1"),
                ))
            })
            .await
            .unwrap();
        assert!(department["items"].as_array().unwrap().iter().all(|row| row["id"] != "incident-2"));
        activate(pool, "fixture-a", "support-holding-a").await;

        // The same Company as an independent manager can manage incidents.
        let manager = service
            .incident_workspace("acme.example.test", p.reviewer.clone(), || {
                Ok(selection("support-holding-b", None, None))
            })
            .await
            .unwrap();
        assert_eq!(manager["canManage"], true);

        // Detail retains links, order recipients, unlinked candidates and history.
        let detail = service
            .incident_detail(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-holding-a", None, None)),
                "incident-3".into(),
            )
            .await
            .unwrap();
        assert_eq!(detail["row"]["id"], "incident-3");
        assert_eq!(detail["row"]["owner_eligible"], 1);
        assert_eq!(detail["row"]["reviewer_eligible"], 0);
        assert_eq!(detail["row"]["reviewer_count"], 1);
        assert_eq!(detail["links"][0]["id"], "ticket-session");
        assert_eq!(detail["links"][0]["linked_version"], 1);
        let recipients = detail["recipients"].as_array().unwrap();
        assert_eq!(recipients.len(), 2);
        assert!(recipients.iter().any(|row| row["id"] == "fixture-a"));
        assert!(recipients.iter().any(|row| row["id"] == "fixture-b"));
        let candidates = detail["candidates"].as_array().unwrap();
        assert!(candidates.iter().any(|row| row["id"] == "ticket-sales-extra"));
        assert!(candidates.iter().all(|row| row["id"] != "ticket-session"));
        assert_eq!(detail["canManage"], false);
        assert_eq!(detail["canWork"], true);
        assert_eq!(detail["canReview"], false);
        assert_eq!(detail["canReopen"], false);

        let manager_detail = service
            .incident_detail(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-holding-b", None, None)),
                "incident-3".into(),
            )
            .await
            .unwrap();
        assert_eq!(manager_detail["row"]["reviewer_eligible"], 1);
        assert_eq!(manager_detail["canManage"], true);
        assert_eq!(manager_detail["canReview"], true);
        assert_eq!(manager_detail["canReopen"], true);
        assert_eq!(manager_detail["canWork"], false);

        let history = service
            .incident_detail(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-holding-a", None, None)),
                "incident-1".into(),
            )
            .await
            .unwrap();
        assert_eq!(history["history"][0]["action"], "start");
        assert_eq!(history["history"][1]["action"], "create");
        assert_eq!(history["row"]["title"], "Shared printer outage");

        // Foreign and absent incidents stay hidden.
        for id in ["incident-other-company", "incident-absent"] {
            match service
                .incident_detail(
                    "acme.example.test",
                    p.maker.clone(),
                    || Ok(selection("support-holding-a", None, None)),
                    id.into(),
                )
                .await
            {
                Err(PgTransactionError::Business(error)) => {
                    assert_eq!(error.code, "SUPPORT_INCIDENT_NOT_FOUND")
                }
                _ => panic!("{id} must not be readable"),
            }
        }
        // The selected branch masks the retained branch-two detail.
        activate(pool, "fixture-a", "support-branch-a").await;
        match service
            .incident_detail(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-branch-a", Some("branch-1"), None)),
                "incident-2".into(),
            )
            .await
        {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "SUPPORT_INCIDENT_NOT_FOUND")
            }
            _ => panic!("incident-2 must not be readable in branch one"),
        }
        activate(pool, "fixture-a", "support-holding-a").await;

        // Revocation of the exact current assignment closes the read.
        sqlx::query("UPDATE assignments SET revoked_at=$1 WHERE id='support-holding-a'")
            .bind(now)
            .execute(pool.raw())
            .await
            .unwrap();
        assert!(
            service
                .incident_detail(
                    "acme.example.test",
                    p.maker.clone(),
                    || Ok(selection("support-holding-a", None, None)),
                    "incident-1".into(),
                )
                .await
                .is_err()
        );
    })
    .await;
}

#[tokio::test]
async fn sla_reads_expose_approved_policy_clock_scope_and_purchase_gates() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        let pool = &p.system.holdings[0];
        let now = p.system.scenario.clock.now().get();
        configure(pool, now).await;
        let service = read_service(&p);

        let maker = service
            .sla_workspace("acme.example.test", p.maker.clone(), || {
                Ok(selection("support-holding-a", None, None))
            })
            .await
            .unwrap();
        assert_eq!(maker["canCreate"], false);
        assert_eq!(maker["canManage"], false);
        let policy = maker["policies"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == "policy-2")
            .expect("draft policy");
        assert_eq!(policy["status"], "draft");
        assert_eq!(policy["createdBy"], "fixture-a");
        assert_eq!(policy["canApprove"], false);
        assert_eq!(policy["policy"]["targetMinutes"], 60);
        let item = maker["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == "ticket-clock")
            .expect("clock ticket");
        assert_eq!(item["policyId"], "policy-1");
        assert_eq!(item["policyVersion"], 1);
        assert_eq!(item["ownerId"], "fixture-a");
        assert_eq!(item["clock"]["state"], "running");
        assert_eq!(item["clock"]["snapshot"]["targetMinutes"], 60);
        let alerts = maker["alerts"].as_array().unwrap();
        assert!(alerts.iter().any(|row| row["id"] == "alert-1"));
        assert!(alerts.iter().any(|row| row["id"] == "alert-2"));
        assert!(alerts.iter().all(|row| row["can_ack"] == 0));
        assert!(alerts.iter().filter(|row| row["id"] == "alert-1").all(|row| row["recipients"] == 1));

        let manager = service
            .sla_workspace("acme.example.test", p.reviewer.clone(), || {
                Ok(selection("support-holding-b", None, None))
            })
            .await
            .unwrap();
        assert_eq!(manager["canCreate"], true);
        assert_eq!(manager["canManage"], true);
        let draft = manager["policies"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == "policy-2")
            .expect("draft policy");
        assert_eq!(draft["canApprove"], true);
        let approved = manager["policies"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == "policy-1")
            .expect("approved policy");
        assert_eq!(approved["canApprove"], false);
        let alert = manager["alerts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == "alert-1")
            .expect("alert");
        assert_eq!(alert["can_ack"], 1);
        assert_eq!(alert["recipients"], 1);
        assert_eq!(alert["title"], "Clock case");
        assert_eq!(alert["detected_at"], now - 300);

        // A branch manager cannot manage Company policies and is masked to the
        // selected branch.
        activate(pool, "fixture-b", "support-branch-b").await;
        let branch = service
            .sla_workspace("acme.example.test", p.reviewer.clone(), || {
                Ok(selection("support-branch-b", Some("branch-1"), None))
            })
            .await
            .unwrap();
        assert_eq!(branch["canManage"], false);
        assert_eq!(branch["canCreate"], false);
        assert!(
            branch["policies"]
                .as_array()
                .unwrap()
                .iter()
                .all(|row| row["canApprove"] == false)
        );
        assert!(
            branch["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|row| row["id"] != "ticket-clock-2")
        );
        let branch_alerts = branch["alerts"].as_array().unwrap();
        assert!(branch_alerts.iter().any(|row| row["id"] == "alert-1"));
        assert!(branch_alerts.iter().all(|row| row["id"] != "alert-2"));
    })
    .await;
}

#[tokio::test]
async fn session_reads_keep_request_subject_reviewer_expiry_and_capability_gates() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        let pool = &p.system.holdings[0];
        let now = p.system.scenario.clock.now().get();
        configure(pool, now).await;
        let service = read_service(&p);

        let maker = service
            .session_workspace("acme.example.test", p.maker.clone(), || {
                Ok(selection("support-holding-a", None, None))
            })
            .await
            .unwrap();
        assert_eq!(maker["actorId"], "fixture-a");
        let rows = maker["rows"].as_array().unwrap();
        for id in ["session-pending", "session-approved", "session-expired", "session-review"] {
            assert!(rows.iter().any(|row| row["id"] == id), "{id}");
        }
        assert!(rows.iter().all(|row| row["can_review"] == 0));
        assert!(rows.iter().all(|row| row["target_eligible"] == 1));
        let expired = rows.iter().find(|row| row["id"] == "session-expired").unwrap();
        assert_eq!(expired["effective_status"], "expired");
        assert_eq!(expired["canReview"], false);
        let tickets = maker["tickets"].as_array().unwrap();
        assert!(tickets.iter().any(|row| row["id"] == "ticket-session"));
        assert!(tickets.iter().any(|row| row["id"] == "ticket-sales-extra"));

        let manager = service
            .session_workspace("acme.example.test", p.reviewer.clone(), || {
                Ok(selection("support-holding-b", None, None))
            })
            .await
            .unwrap();
        let manager_rows = manager["rows"].as_array().unwrap();
        let review = manager_rows
            .iter()
            .find(|row| row["id"] == "session-review")
            .expect("review session");
        assert_eq!(review["can_review"], 1);
        assert_eq!(review["canReview"], true);
        assert_eq!(review["canApprove"], true);
        assert_eq!(review["canIssueCapability"], false);
        let pending = manager_rows
            .iter()
            .find(|row| row["id"] == "session-pending")
            .unwrap();
        assert_eq!(pending["can_review"], 1);
        assert_eq!(pending["canReview"], false);
        assert_eq!(pending["canApprove"], false);
        let approved = manager_rows
            .iter()
            .find(|row| row["id"] == "session-approved")
            .unwrap();
        assert_eq!(approved["canManage"], true);
        assert_eq!(approved["canIssueCapability"], true);

        // The requested colleague reads the capability-gated context and only
        // clean evidence metadata.
        let detail = service
            .session_detail(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-holding-a", None, None)),
                "session-approved".into(),
            )
            .await
            .unwrap();
        assert_eq!(detail["row"]["id"], "session-approved");
        assert_eq!(detail["row"]["effective_status"], "approved");
        assert_eq!(detail["capabilities"].as_array().unwrap().len(), 2);
        assert_eq!(detail["history"][0]["action"], "request");
        assert_eq!(detail["history"][1]["action"], "approve");
        assert_eq!(detail["capabilityHistory"].as_array().unwrap().len(), 2);
        assert_eq!(detail["context"]["order_number"], "SO-SUPPORT");
        assert_eq!(detail["context"]["currency_code"], "THB");
        let evidence = detail["context"]["evidence"].as_array().unwrap();
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0]["id"], "evidence-available");
        assert_eq!(evidence[0]["original_name"], "support-available.pdf");
        assert!(evidence.iter().all(|row| row["id"] != "evidence-revoked"));

        let expired_detail = service
            .session_detail(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-holding-a", None, None)),
                "session-expired".into(),
            )
            .await
            .unwrap();
        assert_eq!(expired_detail["row"]["effective_status"], "expired");
        assert!(expired_detail["context"].is_null());

        let manager_detail = service
            .session_detail(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-holding-b", None, None)),
                "session-review".into(),
            )
            .await
            .unwrap();
        assert_eq!(manager_detail["row"]["canApprove"], true);
        assert!(manager_detail["context"].is_null());

        // A selected branch that owns no session hides the retained detail.
        match service
            .session_detail(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-holding-a", Some("branch-2"), None)),
                "session-approved".into(),
            )
            .await
        {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "SUPPORT_SESSION_NOT_FOUND")
            }
            _ => panic!("branch two must not expose the branch one session"),
        }
    })
    .await;
}

#[tokio::test]
async fn evidence_reads_mask_unbound_rows_and_keep_revoked_metadata_visible() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        let pool = &p.system.holdings[0];
        let now = p.system.scenario.clock.now().get();
        configure(pool, now).await;
        let service = read_service(&p);

        let maker = service
            .evidence(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-holding-a", None, None)),
                "ticket-session".into(),
            )
            .await
            .unwrap();
        let items = maker["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert!(items.iter().any(|row| row["id"] == "evidence-available"));
        let revoked = items.iter().find(|row| row["id"] == "evidence-revoked").unwrap();
        assert_eq!(revoked["revoked_at"], now - 100);
        assert_eq!(revoked["legal_hold"], 0);
        assert!(items.iter().all(|row| row["id"] != "evidence-hidden"));
        assert_eq!(maker["canChange"], true);
        assert_eq!(maker["canShare"], false);

        let manager = service
            .evidence(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-holding-b", None, None)),
                "ticket-session".into(),
            )
            .await
            .unwrap();
        assert_eq!(manager["canChange"], true);
        assert_eq!(manager["canShare"], true);

        // A resolved/cancelled case remains readable but cannot change evidence.
        let locked = service
            .evidence(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-holding-a", None, None)),
                "ticket-terminal".into(),
            )
            .await
            .unwrap();
        assert_eq!(locked["canChange"], false);
        assert_eq!(locked["canShare"], false);

        match service
            .evidence(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-holding-a", None, None)),
                "ticket-other-company".into(),
            )
            .await
        {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "SUPPORT_TICKET_NOT_FOUND");
                assert_eq!(error.kind, erpx_server_shared::public::FailureKind::NotFound);
            }
            _ => panic!("foreign evidence must stay hidden"),
        }
    })
    .await;
}

#[tokio::test]
async fn service_ticket_sources_retain_their_distinct_owner_locks() {
    use erpx_server_support::public::field_service_workspace::*;
    use erpx_server_support::public::field_service::{lock_service_ticket_sources,require_service_ticket_sources_held};
    review::account::with_system(|system| async move {
        let p=review::setup_with_finance(system,false).await;let pool=&p.system.holdings[0];
        configure(pool,p.system.scenario.clock.now().get()).await;
        // General cases legitimately have neither a customer nor a branch.
        sqlx::query("INSERT INTO support_tickets(id,company_id,branch_id,customer_id,title,issue,owner_id,due_date,status,version,created_by,created_at,updated_at,source_kind) VALUES('service-null-ticket','company',NULL,NULL,'General request','Synthetic general issue','fixture-a','2026-11-10','open',1,'fixture-a',0,0,'general')").execute(pool.raw()).await.unwrap();
        let company=CompanyId::new("company").unwrap();
        let scope=ServiceWorkspaceTicketScope{company_id:company.clone(),branch:None,department:None,assignment_kind:"holding".into(),assignment_node:None,include_picker:false,ids:vec!["ticket-session".into(),"service-null-ticket".into()]};
        let ids=vec!["ticket-session".to_owned()];
        let mut unlocked=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *unlocked).await.unwrap();
        assert!(read_service_workspace_tickets(&mut unlocked,scope.clone()).await.is_err());assert!(read_service_support_settlement(&mut unlocked,&company,&ids).await.is_err());assert!(require_service_workspace_tickets_held(&mut unlocked).await.is_err());assert!(require_service_ticket_sources_held(&mut unlocked).await.is_err());unlocked.rollback().await.unwrap();
        let mut insufficient=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *insufficient).await.unwrap();lock_service_workspace_tickets(&mut insufficient).await.unwrap();
        require_service_workspace_tickets_held(&mut insufficient).await.unwrap();assert!(require_service_ticket_sources_held(&mut insufficient).await.is_err());let workspace=read_service_workspace_tickets(&mut insufficient,scope.clone()).await.unwrap();workspace.require_current(&mut insufficient).await.unwrap();
        assert!(read_service_support_settlement(&mut insufficient,&company,&ids).await.is_err());insufficient.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();sqlx::query("SELECT pg_current_xact_id()").execute(&mut *tx).await.unwrap();sqlx::query("SAVEPOINT before_support_locks").execute(&mut *tx).await.unwrap();
        lock_service_support_settlement(&mut tx).await.unwrap();
        let count:i64=sqlx::query_scalar("SELECT count(DISTINCT relation) FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND mode='ShareLock' AND relation IN ('support_tickets'::regclass,'support_ticket_events'::regclass,'support_sla_alerts'::regclass)").fetch_one(&mut *tx).await.unwrap();assert_eq!(count,3);
        let workspace=read_service_workspace_tickets(&mut tx,scope).await.unwrap();let settlement=read_service_support_settlement(&mut tx,&company,&ids).await.unwrap();
        assert!(workspace.data().display.iter().any(|t|t.id==ids[0]));assert!(settlement.data().tickets.iter().any(|t|t.id==ids[0]));workspace.require_current(&mut tx).await.unwrap();settlement.require_current(&mut tx).await.unwrap();
        let general=workspace.data().display.iter().find(|t|t.id=="service-null-ticket").unwrap();assert_eq!(general.source_kind,"general");assert_eq!(general.customer_id,None);assert_eq!(general.branch_id,None);
        sqlx::query("ROLLBACK TO SAVEPOINT before_support_locks").execute(&mut *tx).await.unwrap();assert!(workspace.require_current(&mut tx).await.is_err());assert!(settlement.require_current(&mut tx).await.is_err());assert!(require_service_workspace_tickets_held(&mut tx).await.is_err());assert!(require_service_ticket_sources_held(&mut tx).await.is_err());
        lock_service_ticket_sources(&mut tx).await.unwrap();require_service_workspace_tickets_held(&mut tx).await.unwrap();require_service_ticket_sources_held(&mut tx).await.unwrap();tx.rollback().await.unwrap();
        let mut strong=pool.raw().begin().await.unwrap();sqlx::query("LOCK TABLE support_tickets,support_ticket_events IN EXCLUSIVE MODE").execute(&mut *strong).await.unwrap();require_service_workspace_tickets_held(&mut strong).await.unwrap();require_service_ticket_sources_held(&mut strong).await.unwrap();strong.rollback().await.unwrap();
    }).await;
}
