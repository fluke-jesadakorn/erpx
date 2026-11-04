#![allow(dead_code)]
//! Native Support ticket workspace/detail reads over leased PostgreSQL with
//! current Account/IAM authority, purchase history and typed source links.
#[path = "../../support/finance/review-composition.rs"]
mod review;
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_shared::public::{Clock, CompanyId, HoldingId};
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
    SupportReadScope {
        company_id: CompanyId::new("company").unwrap(),
        branch_id: branch.map(str::to_owned),
        department_id: department.map(str::to_owned),
        user_id: "fixture-a".into(),
        assignment_id: assignment.into(),
        role: role.into(),
        support_read: true,
        records_read: true,
        support_create: true,
        support_work: true,
        support_manage: false,
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
