//! Actual Support incident command over leased PostgreSQL, current Account/IAM
//! authority and the Workflow receipt/audit/outbox. No TS writer is invoked.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod review;
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_identity_access::public::work_context::{
    ExpectedWorkSelection, HoldingWorkspaceRequest,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{AssignmentId, Clock, CommandKey, CompanyId, HoldingId};
use erpx_server_support::public::incident::{
    IncidentChange, IncidentCreate,
};
use erpx_server_workflow::public::commands::CommandJson;
use erpx_service::support_incident_command::SupportIncidentCommand;
use std::sync::Arc;

#[derive(Clone)]
struct TestClock(Arc<review::account::fixture::MutableClock>);
impl Clock for TestClock {
    fn now(&self) -> erpx_server_shared::public::TimestampMillis {
        self.0.now()
    }
}

fn selection(assignment: &str) -> HoldingWorkspaceRequest {
    HoldingWorkspaceRequest {
        company_id: CompanyId::new("company").unwrap(),
        branch_slug: None,
        department_slug: None,
        expected: Some(ExpectedWorkSelection {
            revision: 7,
            assignment_id: AssignmentId::new(assignment).unwrap(),
            assignment_version: 3,
        }),
        mutation: true,
    }
}

async fn support_setup(
    system: review::account::System,
) -> (review::Prepared, SupportIncidentCommand<TestClock>) {
    let p = review::setup_with_finance(system, false).await;
    let pool = &p.system.holdings[0];
    let now = p.system.scenario.clock.now().get();
    let expires = now + 3_600_000;
    sqlx::raw_sql(
        "INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','custom_support_agent','Synthetic Support agent',false,1,'active'),('*','custom_support_manager','Synthetic Support manager',false,1,'active') ON CONFLICT DO NOTHING;\
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_support_agent',1,'[\"records.read\",\"records.write\",\"support.read\",\"support.create\",\"support.work\",\"commerce.read\",\"module.support.read\"]','[]','[\"holding\",\"branch\",\"department\"]','synthetic-fixture',0) ON CONFLICT DO NOTHING;\
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_support_manager',1,'[\"records.read\",\"records.write\",\"support.read\",\"support.create\",\"support.work\",\"support.manage\",\"commerce.read\",\"module.support.read\"]','[]','[\"holding\",\"branch\",\"department\"]','synthetic-fixture',0) ON CONFLICT DO NOTHING;\
         INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,version,created_by,created_at,updated_at)VALUES('person-a','fixture-a','company','Synthetic Agent','TH','active',1,'synthetic-fixture',0,0),('person-b','fixture-b','company','Synthetic Manager','TH','active',1,'synthetic-fixture',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,end_date,job_title,currency,version,created_by,created_at,updated_at)VALUES('engagement-a','person-a','company','employee','salaried','active','2026-01-01',NULL,'Support','THB',1,'synthetic-fixture',0,0),('engagement-b','person-b','company','employee','salaried','active','2026-01-01',NULL,'Support','THB',1,'synthetic-fixture',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version)VALUES('company','fixture-a','local:fixture-a','person-a','active','local',1),('company','fixture-b','local:fixture-b','person-b','active','local',1) ON CONFLICT DO NOTHING;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    for (user, assignment, role) in [
        ("fixture-a", "support-fixture-a", "custom_support_agent"),
        ("fixture-b", "support-fixture-b", "custom_support_manager"),
    ] {
        let session:String=sqlx::query_scalar("SELECT session_id FROM active_work_contexts WHERE user_id=$1")
            .bind(user)
            .fetch_one(pool.raw())
            .await
            .unwrap();
        sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind)VALUES($1,$2,$3,'[\"support\"]','company',$4,$5,3,'holding')")
            .bind(assignment)
            .bind(user)
            .bind(role)
            .bind(0_i64)
            .bind(9_007_199_254_740_991_i64)
            .execute(pool.raw())
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
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at)VALUES('company','support',1,'active',0,$1,1,'Synthetic reviewed support purchase','public synthetic evidence',$2) ON CONFLICT DO NOTHING")
        .bind(expires)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT 'support-origin-grant','company','support','fixture-a','synthetic-request',NULL,jsonb_build_object('companyId','company','packageId','support','packageVersion',1,'state','active','startsAt',0,'endsAt',$1,'version',1,'reason','Synthetic reviewed support purchase','evidence','public synthetic evidence'),$2 ON CONFLICT DO NOTHING")
        .bind(expires)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let command = SupportIncidentCommand {
        authorizer: p.authorizer.clone(),
        holding: HoldingId::new("h001").unwrap(),
        company: CompanyId::new("company").unwrap(),
        catalogue: Arc::new(current_package_catalogue().unwrap()),
        clock: Arc::new(TestClock(p.system.scenario.clock.clone())),
    };
    (p, command)
}

async fn seed_ticket(pool: &erpx_server_runtime::public::postgres::DatabasePool, id: &str, source_kind: &str, version: i64) {
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES('branch','branch','support','Support Branch','company','company','branch','active') ON CONFLICT DO NOTHING;\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at)VALUES('support-customer','company','customer','SUPPORT-C','Support Customer','THB',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version)VALUES('support-warehouse','company','branch','SUPPORT-WH','Support Warehouse','active',1) ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,version,created_by,created_at,updated_at)VALUES('support-order','company','branch','sales','SO-SUPPORT-1','support-customer','support-warehouse','THB','2026-11-01','2026-11-10',10000,0,1,'fixture-a',0,0) ON CONFLICT DO NOTHING;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    if source_kind == "general" {
        sqlx::query("INSERT INTO support_tickets(id,company_id,branch_id,department_id,order_id,customer_id,title,issue,owner_id,due_date,status,source_kind,version,created_by,created_at,updated_at)VALUES($1,'company',NULL,NULL,NULL,NULL,'General customer case','General case for the incident.',NULL,NULL,'triage','general',$2,'fixture-a',0,0) ON CONFLICT DO NOTHING")
            .bind(id)
            .bind(version)
            .execute(pool.raw())
            .await
            .unwrap();
    } else {
        sqlx::query("INSERT INTO support_tickets(id,company_id,branch_id,department_id,order_id,customer_id,title,issue,owner_id,due_date,status,source_kind,version,created_by,created_at,updated_at)VALUES($1,'company','branch',NULL,'support-order','support-customer','Linked customer case','Customer case for the incident.','fixture-a','2026-11-10','open',$2,$3,'fixture-a',0,0) ON CONFLICT DO NOTHING")
            .bind(id)
            .bind(source_kind)
            .bind(version)
            .execute(pool.raw())
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn support_incident_create_start_mitigate_close_link_and_replay() {
    review::account::with_system(|system| async move {
        let (p, command) = support_setup(system).await;
        let pool = &p.system.holdings[0];
        seed_ticket(pool, "incident-source-ticket", "sales_order", 1).await;
        seed_ticket(pool, "incident-source-ticket-2", "sales_order", 1).await;

        let create_wire = CommandJson::parse(
            r#"{"ticketId":"incident-source-ticket","ticketVersion":1,"title":"Checkout outage","impact":"Customers cannot complete checkout.","severity":"critical","ownerId":"fixture-a","dueDate":"2026-11-10"}"#,
        )
        .unwrap();
        let create_input: IncidentCreate =
            serde_json::from_str(create_wire.as_str()).unwrap();
        let created = command
            .create(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-fixture-b")),
                create_input,
                create_wire,
                CommandKey::new("incident-create-1").unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            created.as_str(),
            r#"{"id":"incident-incident-create-1","status":"open","version":1}"#
        );
        let replay = command
            .create(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-fixture-b")),
                serde_json::from_str(
                    r#"{"ticketId":"incident-source-ticket","ticketVersion":1,"title":"Checkout outage","impact":"Customers cannot complete checkout.","severity":"critical","ownerId":"fixture-a","dueDate":"2026-11-10"}"#,
                )
                .unwrap(),
                CommandJson::parse(
                    r#"{"ticketId":"incident-source-ticket","ticketVersion":1,"title":"Checkout outage","impact":"Customers cannot complete checkout.","severity":"critical","ownerId":"fixture-a","dueDate":"2026-11-10"}"#,
                )
                .unwrap(),
                CommandKey::new("incident-create-1").unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(replay, created);

        let start = IncidentChange::Start {
            expected_version: 1,
            reason: "Start investigation".into(),
        };
        command
            .change(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                "incident-incident-create-1".into(),
                start,
                CommandJson::parse(
                    r#"{"action":"start","expectedVersion":1,"reason":"Start investigation"}"#,
                )
                .unwrap(),
                CommandKey::new("incident-start-1").unwrap(),
            )
            .await
            .unwrap();
        let update = IncidentChange::Update {
            expected_version: 2,
            reason: "Investigation update".into(),
        };
        command
            .change(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                "incident-incident-create-1".into(),
                update,
                CommandJson::parse(
                    r#"{"action":"update","expectedVersion":2,"reason":"Investigation update"}"#,
                )
                .unwrap(),
                CommandKey::new("incident-update-1").unwrap(),
            )
            .await
            .unwrap();
        let mitigate = IncidentChange::Mitigate {
            expected_version: 3,
            reason: "Mitigation proposed".into(),
            root_cause: "Expired certificate".into(),
            mitigation: "Rotate the certificate".into(),
        };
        command
            .change(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                "incident-incident-create-1".into(),
                mitigate,
                CommandJson::parse(
                    r#"{"action":"mitigate","expectedVersion":3,"reason":"Mitigation proposed","rootCause":"Expired certificate","mitigation":"Rotate the certificate"}"#,
                )
                .unwrap(),
                CommandKey::new("incident-mitigate-1").unwrap(),
            )
            .await
            .unwrap();
        let link = IncidentChange::Link {
            expected_version: 4,
            reason: "Link the second customer case".into(),
            ticket_id: "incident-source-ticket-2".into(),
            ticket_version: 1,
        };
        command
            .change(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-fixture-b")),
                "incident-incident-create-1".into(),
                link,
                CommandJson::parse(
                    r#"{"action":"link","expectedVersion":4,"reason":"Link the second customer case","ticketId":"incident-source-ticket-2","ticketVersion":1}"#,
                )
                .unwrap(),
                CommandKey::new("incident-link-1").unwrap(),
            )
            .await
            .unwrap();
        let unlink = IncidentChange::Unlink {
            expected_version: 5,
            reason: "Correct the extra link".into(),
            ticket_id: "incident-source-ticket-2".into(),
        };
        command
            .change(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-fixture-b")),
                "incident-incident-create-1".into(),
                unlink,
                CommandJson::parse(
                    r#"{"action":"unlink","expectedVersion":5,"reason":"Correct the extra link","ticketId":"incident-source-ticket-2"}"#,
                )
                .unwrap(),
                CommandKey::new("incident-unlink-1").unwrap(),
            )
            .await
            .unwrap();

        let self_close = IncidentChange::Close {
            expected_version: 6,
            reason: "Owner attempts to close".into(),
        };
        match command
            .change(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                "incident-incident-create-1".into(),
                self_close,
                CommandJson::parse(
                    r#"{"action":"close","expectedVersion":6,"reason":"Owner attempts to close"}"#,
                )
                .unwrap(),
                CommandKey::new("incident-self-close").unwrap(),
            )
            .await
        {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "SUPPORT_INCIDENT_MANAGE_REQUIRED")
            }
            _ => panic!("owner close must fail"),
        }

        let close = IncidentChange::Close {
            expected_version: 6,
            reason: "Independent manager confirms the mitigation".into(),
        };
        command
            .change(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-fixture-b")),
                "incident-incident-create-1".into(),
                close,
                CommandJson::parse(
                    r#"{"action":"close","expectedVersion":6,"reason":"Independent manager confirms the mitigation"}"#,
                )
                .unwrap(),
                CommandKey::new("incident-close-1").unwrap(),
            )
            .await
            .unwrap();

        let row: (String, i64, Option<String>, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT status,version,root_cause,mitigation,mitigated_by FROM support_incidents WHERE id='incident-incident-create-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row.0, "closed");
        assert_eq!(row.1, 7);
        assert_eq!(row.2.as_deref(), Some("Expired certificate"));
        assert_eq!(row.3.as_deref(), Some("Rotate the certificate"));
        assert_eq!(row.4.as_deref(), Some("fixture-a"));
        let events: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM support_incident_events WHERE incident_id='incident-incident-create-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(events, 7);
        let links: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM support_incident_tickets WHERE incident_id='incident-incident-create-1' AND active",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(links, 1);
        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id IN ('incident-create-1','incident-start-1','incident-update-1','incident-mitigate-1','incident-link-1','incident-unlink-1','incident-close-1')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipts, 7);
        let audit: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM audit_events WHERE command_id IN ('incident-create-1','incident-start-1','incident-update-1','incident-mitigate-1','incident-link-1','incident-unlink-1','incident-close-1')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(audit, 7);
        let outbox: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM outbox_events WHERE command_id IN ('incident-create-1','incident-start-1','incident-update-1','incident-mitigate-1','incident-link-1','incident-unlink-1','incident-close-1')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(outbox, 7);
    })
    .await;
}

#[tokio::test]
async fn support_incident_rejects_general_source_and_stale_version() {
    review::account::with_system(|system| async move {
        let (p, command) = support_setup(system).await;
        let pool = &p.system.holdings[0];
        seed_ticket(pool, "incident-general-ticket", "general", 1).await;
        let kind:String=sqlx::query_scalar("SELECT source_kind FROM support_tickets WHERE id='incident-general-ticket'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(kind, "general");
        let create_wire = CommandJson::parse(
            r#"{"ticketId":"incident-general-ticket","ticketVersion":1,"title":"General issue","impact":"A general issue is not a shared customer incident.","severity":"normal","ownerId":"fixture-a","dueDate":"2026-11-10"}"#,
        )
        .unwrap();
        let input: IncidentCreate = serde_json::from_str(create_wire.as_str()).unwrap();
        match command
            .create(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-fixture-b")),
                input,
                create_wire,
                CommandKey::new(format!("incident-general-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())).unwrap(),
            )
            .await
        {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "SUPPORT_INCIDENT_SOURCE_REQUIRED")
            }
            _ => panic!("general source must fail"),
        }
    })
    .await;
}

#[tokio::test]
async fn support_incident_http_route_accepts_real_session_and_rendered_context() {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use erpx_service::field_service_reservation_read_http::FieldServiceReadScopeHeaders;
    use erpx_service::support_incident_http::{
        SupportIncidentHttpTarget, support_incident_http_router,
    };
    use tower::ServiceExt;
    review::account::with_system(|system| async move {
        let (p, command) = support_setup(system).await;
        let pool = &p.system.holdings[0];
        seed_ticket(pool, "incident-http-source", "sales_order", 1).await;
        let router = support_incident_http_router(
            Arc::new(review::account::fixture::account::policy()),
            Arc::new(FieldServiceReadScopeHeaders),
            vec![SupportIncidentHttpTarget {
                host: "acme.example.test".into(),
                holding_slug: "acme".into(),
                command: Arc::new(command),
            }],
        )
        .unwrap();
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/support/incidents")
            .header("host", "acme.example.test")
            .header("cookie", format!("__Host-erpx-product={}", p.reviewer_token.expose()))
            .header("origin", "https://acme.example.test")
            .header("idempotency-key", "incident-http-create")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "support-fixture-b")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"action":"create","ticketId":"incident-http-source","ticketVersion":1,"title":"HTTP checkout outage","impact":"Customers cannot check out over HTTP.","severity":"high","ownerId":"fixture-a","dueDate":"2026-11-10"}"#,
            ))
            .unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        assert_eq!(status, 200);
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["data"]["id"], "incident-incident-http-create");
        assert_eq!(value["data"]["status"], "open");
        assert_eq!(value["data"]["version"], 1);

        let change = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/support/incidents")
            .header("host", "acme.example.test")
            .header("cookie", format!("__Host-erpx-product={}", p.maker_token.expose()))
            .header("origin", "https://acme.example.test")
            .header("idempotency-key", "incident-http-start")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "support-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"id":"incident-incident-http-create","action":"start","expectedVersion":1,"reason":"Start the HTTP incident"}"#,
            ))
            .unwrap();
        let change_response = router.oneshot(change).await.unwrap();
        let change_status = change_response.status();
        let change_body = to_bytes(change_response.into_body(), 64 * 1024)
            .await
            .unwrap();
        assert_eq!(change_status, 200);
        let change_value: serde_json::Value = serde_json::from_slice(&change_body).unwrap();
        assert_eq!(change_value["data"]["status"], "investigating");
        assert_eq!(change_value["data"]["version"], 2);
        let persisted: (String, i64) = sqlx::query_as(
            "SELECT status,version FROM support_incidents WHERE id='incident-incident-http-create'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(persisted, ("investigating".into(), 2));
    })
    .await;
}
