//! Actual Support session and capability commands over leased PostgreSQL,
//! current Account/IAM authority and the Workflow receipt/audit/outbox.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod review;
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_identity_access::public::work_context::{
    ExpectedWorkSelection, HoldingWorkspaceRequest,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{AssignmentId, Clock, CommandKey, CompanyId, HoldingId};
use erpx_server_support::public::session::{
    CapabilityCode, SessionAction, SessionRequest,
};
use erpx_server_workflow::public::commands::CommandJson;
use erpx_service::support_session_command::SupportSessionCommand;
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
) -> (review::Prepared, SupportSessionCommand<TestClock>) {
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
        let session: String =
            sqlx::query_scalar("SELECT session_id FROM active_work_contexts WHERE user_id=$1")
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
    sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT 'support-session-origin-grant','company','support','fixture-a','synthetic-request',NULL,jsonb_build_object('companyId','company','packageId','support','packageVersion',1,'state','active','startsAt',0,'endsAt',$1,'version',1,'reason','Synthetic reviewed support purchase','evidence','public synthetic evidence'),$2 ON CONFLICT DO NOTHING")
        .bind(expires)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let command = SupportSessionCommand {
        authorizer: p.authorizer.clone(),
        holding: HoldingId::new("h001").unwrap(),
        company: CompanyId::new("company").unwrap(),
        catalogue: Arc::new(current_package_catalogue().unwrap()),
        clock: Arc::new(TestClock(p.system.scenario.clock.clone())),
    };
    (p, command)
}

async fn seed_ticket(
    pool: &erpx_server_runtime::public::postgres::DatabasePool,
    id: &str,
    source_kind: &str,
    owner: &str,
    version: i64,
) {
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES('branch','branch','support','Support Branch','company','company','branch','active') ON CONFLICT DO NOTHING;\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at)VALUES('session-customer','company','customer','SESSION-C','Session Customer','THB',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version)VALUES('session-warehouse','company','branch','SESSION-WH','Session Warehouse','active',1) ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,version,created_by,created_at,updated_at)VALUES('session-order','company','branch','sales','SO-SESSION-1','session-customer','session-warehouse','THB','2026-11-01','2026-11-10',10000,0,1,'fixture-a',0,0) ON CONFLICT DO NOTHING;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    if source_kind == "general" {
        sqlx::query("INSERT INTO support_tickets(id,company_id,branch_id,department_id,order_id,customer_id,title,issue,owner_id,due_date,status,source_kind,version,created_by,created_at,updated_at)VALUES($1,'company',NULL,NULL,NULL,NULL,'General session case','General case for the session.',NULL,NULL,'triage','general',$2,'fixture-a',0,0) ON CONFLICT DO NOTHING")
            .bind(id)
            .bind(version)
            .execute(pool.raw())
            .await
            .unwrap();
    } else {
        sqlx::query("INSERT INTO support_tickets(id,company_id,branch_id,department_id,order_id,customer_id,title,issue,owner_id,due_date,status,source_kind,version,created_by,created_at,updated_at)VALUES($1,'company','branch',NULL,'session-order','session-customer','Session customer case','Customer case for the session.',$3,'2026-11-10','open',$2,$4,'fixture-a',0,0) ON CONFLICT DO NOTHING")
            .bind(id)
            .bind(source_kind)
            .bind(owner)
            .bind(version)
            .execute(pool.raw())
            .await
            .unwrap();
    }
}

fn request(ticket: &str, requested_for: &str, expires_at: i64) -> SessionRequest {
    SessionRequest {
        ticket_id: ticket.into(),
        ticket_version: 1,
        requested_for: requested_for.into(),
        expires_at,
        reason: "Temporary customer context for the assigned case".into(),
    }
}

#[tokio::test]
async fn support_session_request_approve_capability_revoke_and_replay() {
    review::account::with_system(|system| async move {
        let (p, command) = support_setup(system).await;
        let pool = &p.system.holdings[0];
        let now = p.system.scenario.clock.now().get();
        seed_ticket(pool, "session-ticket-a", "sales_order", "fixture-a", 1).await;
        seed_ticket(pool, "session-ticket-b", "sales_order", "fixture-b", 1).await;

        let input = request("session-ticket-a", "fixture-a", now + 3_600_000);
        let wire = CommandJson::from_serializable(&input).unwrap();
        let created = command
            .request(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                input.clone(),
                wire.clone(),
                CommandKey::new("session-request-1").unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            created.as_str(),
            r#"{"id":"support-session-session-request-1","status":"pending","version":1}"#
        );
        let replay = command
            .request(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                input.clone(),
                wire,
                CommandKey::new("session-request-1").unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(replay, created);

        // A manager cannot approve their own temporary access.
        let own = request("session-ticket-b", "fixture-b", now + 3_600_000);
        let own_wire = CommandJson::from_serializable(&own).unwrap();
        command
            .request(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-fixture-b")),
                own,
                own_wire,
                CommandKey::new("session-request-b").unwrap(),
            )
            .await
            .unwrap();
        match command
            .change(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-fixture-b")),
                "support-session-session-request-b".into(),
                SessionAction::Approve {
                    expected_version: 1,
                    reason: "Approve my own access".into(),
                },
                CommandJson::parse(
                    r#"{"action":"approve","expectedVersion":1,"reason":"Approve my own access"}"#,
                )
                .unwrap(),
                CommandKey::new("session-self-approve").unwrap(),
            )
            .await
        {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "INDEPENDENT_SESSION_REVIEWER_REQUIRED")
            }
            _ => panic!("self approval must fail"),
        }

        command
            .change(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-fixture-b")),
                "support-session-session-request-1".into(),
                SessionAction::Approve {
                    expected_version: 1,
                    reason: "Approve temporary customer context".into(),
                },
                CommandJson::parse(
                    r#"{"action":"approve","expectedVersion":1,"reason":"Approve temporary customer context"}"#,
                )
                .unwrap(),
                CommandKey::new("session-approve-1").unwrap(),
            )
            .await
            .unwrap();
        command
            .change(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-fixture-b")),
                "support-session-session-request-1".into(),
                SessionAction::IssueCapability {
                    expected_version: 2,
                    code: CapabilityCode::CustomerOrderContext,
                    reason: "Issue the customer order context".into(),
                },
                CommandJson::parse(
                    r#"{"action":"issue-capability","expectedVersion":2,"code":"customer_order_context","reason":"Issue the customer order context"}"#,
                )
                .unwrap(),
                CommandKey::new("session-issue-1").unwrap(),
            )
            .await
            .unwrap();
        let capability: (String, String, i64) = sqlx::query_as(
            "SELECT id,status,version FROM support_capabilities WHERE session_id='support-session-session-request-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(capability.0, "support-capability-session-issue-1");
        assert_eq!(capability.1, "active");
        assert_eq!(capability.2, 1);

        command
            .change(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-fixture-b")),
                "support-session-session-request-1".into(),
                SessionAction::RevokeCapability {
                    expected_version: 3,
                    capability_id: capability.0.clone(),
                    expected_capability_version: 1,
                    reason: "Revoke the customer order context".into(),
                },
                CommandJson::parse(
                    r#"{"action":"revoke-capability","expectedVersion":3,"capabilityId":"support-capability-session-issue-1","expectedCapabilityVersion":1,"reason":"Revoke the customer order context"}"#,
                )
                .unwrap(),
                CommandKey::new("session-revoke-cap-1").unwrap(),
            )
            .await
            .unwrap();
        command
            .change(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-fixture-b")),
                "support-session-session-request-1".into(),
                SessionAction::IssueCapability {
                    expected_version: 4,
                    code: CapabilityCode::CustomerOrderContext,
                    reason: "Reissue the customer order context".into(),
                },
                CommandJson::parse(
                    r#"{"action":"issue-capability","expectedVersion":4,"code":"customer_order_context","reason":"Reissue the customer order context"}"#,
                )
                .unwrap(),
                CommandKey::new("session-reissue-1").unwrap(),
            )
            .await
            .unwrap();
        command
            .change(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-fixture-b")),
                "support-session-session-request-1".into(),
                SessionAction::Revoke {
                    expected_version: 5,
                    reason: "Revoke the whole temporary session".into(),
                },
                CommandJson::parse(
                    r#"{"action":"revoke","expectedVersion":5,"reason":"Revoke the whole temporary session"}"#,
                )
                .unwrap(),
                CommandKey::new("session-revoke-1").unwrap(),
            )
            .await
            .unwrap();

        let session: (String, i64, String, String) = sqlx::query_as(
            "SELECT status,version,requested_for,reviewed_by FROM support_sessions WHERE id='support-session-session-request-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(session.0, "revoked");
        assert_eq!(session.1, 6);
        assert_eq!(session.2, "fixture-a");
        assert_eq!(session.3, "fixture-b");
        let events: Vec<(i64, String)> = sqlx::query_as(
            "SELECT version,action FROM support_session_events WHERE session_id='support-session-session-request-1' ORDER BY version",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            events,
            vec![
                (1, "request".into()),
                (2, "approve".into()),
                (6, "revoke".into())
            ]
        );
        let capability: (String, i64) = sqlx::query_as(
            "SELECT status,version FROM support_capabilities WHERE id='support-capability-session-issue-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(capability, ("revoked".into(), 4));
        let capability_events: Vec<(i64, String)> = sqlx::query_as(
            "SELECT version,action FROM support_capability_events WHERE capability_id='support-capability-session-issue-1' ORDER BY version",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            capability_events,
            vec![
                (1, "issue".into()),
                (2, "revoke".into()),
                (3, "reissue".into()),
                (4, "session-revoke".into())
            ]
        );
        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id IN ('session-request-1','session-request-b','session-approve-1','session-issue-1','session-revoke-cap-1','session-reissue-1','session-revoke-1')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipts, 7);
        let audit: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM audit_events WHERE command_id IN ('session-request-1','session-request-b','session-approve-1','session-issue-1','session-revoke-cap-1','session-reissue-1','session-revoke-1')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(audit, 7);
        let outbox: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM outbox_events WHERE command_id IN ('session-request-1','session-request-b','session-approve-1','session-issue-1','session-revoke-cap-1','session-reissue-1','session-revoke-1')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(outbox, 7);
        let notices: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM staff_notifications WHERE record_type='support_session' AND record_id='support-session-session-request-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert!(notices >= 2);
    })
    .await;
}

#[tokio::test]
async fn support_session_rejects_ineligible_and_invalid_requests() {
    review::account::with_system(|system| async move {
        let (p, command) = support_setup(system).await;
        let pool = &p.system.holdings[0];
        let now = p.system.scenario.clock.now().get();
        seed_ticket(pool, "session-general", "general", "fixture-a", 1).await;
        seed_ticket(pool, "session-other", "sales_order", "fixture-b", 1).await;
        let kind: String = sqlx::query_scalar(
            "SELECT source_kind FROM support_tickets WHERE id='session-general'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(kind, "general");

        let general = request("session-general", "fixture-a", now + 3_600_000);
        match command
            .request(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                general.clone(),
                CommandJson::from_serializable(&general).unwrap(),
                CommandKey::new(format!("session-general-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())).unwrap(),
            )
            .await
        {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "SUPPORT_SESSION_SOURCE_REQUIRED")
            }
            other => panic!("general source must fail: {other:?}"),
        }

        let foreign = request("session-other", "fixture-a", now + 3_600_000);
        match command
            .request(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                foreign.clone(),
                CommandJson::from_serializable(&foreign).unwrap(),
                CommandKey::new("session-foreign-key").unwrap(),
            )
            .await
        {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "SUPPORT_SESSION_TICKET_OWNER_REQUIRED")
            }
            _ => panic!("foreign ticket must fail"),
        }

        seed_ticket(pool, "session-own", "sales_order", "fixture-a", 1).await;
        let other = request("session-own", "fixture-b", now + 3_600_000);
        match command
            .request(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                other.clone(),
                CommandJson::from_serializable(&other).unwrap(),
                CommandKey::new("session-other-key").unwrap(),
            )
            .await
        {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "SUPPORT_SESSION_SELF_REQUEST_REQUIRED")
            }
            _ => panic!("requesting for another person must fail"),
        }

        let late = request("session-own", "fixture-a", now + 9 * 60 * 60 * 1_000);
        match command
            .request(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                late.clone(),
                CommandJson::from_serializable(&late).unwrap(),
                CommandKey::new("session-late-key").unwrap(),
            )
            .await
        {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "SUPPORT_SESSION_EXPIRY_INVALID")
            }
            _ => panic!("a nine hour session must fail"),
        }
    })
    .await;
}

#[tokio::test]
async fn support_session_http_route_accepts_real_session_and_rendered_context() {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use erpx_service::field_service_reservation_read_http::FieldServiceReadScopeHeaders;
    use erpx_service::support_session_http::{
        SupportSessionHttpTarget, support_session_http_router,
    };
    use tower::ServiceExt;
    review::account::with_system(|system| async move {
        let (p, command) = support_setup(system).await;
        let pool = &p.system.holdings[0];
        let now = p.system.scenario.clock.now().get();
        seed_ticket(pool, "session-http-ticket", "sales_order", "fixture-a", 1).await;
        let router = support_session_http_router(
            Arc::new(review::account::fixture::account::policy()),
            Arc::new(FieldServiceReadScopeHeaders),
            vec![SupportSessionHttpTarget {
                host: "acme.example.test".into(),
                holding_slug: "acme".into(),
                command: Arc::new(command),
            }],
        )
        .unwrap();
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/support/sessions")
            .header("host", "acme.example.test")
            .header("cookie", format!("__Host-erpx-product={}", p.maker_token.expose()))
            .header("origin", "https://acme.example.test")
            .header("idempotency-key", "session-http-request")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "support-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .header("content-type", "application/json")
            .body(Body::from(format!(
                r#"{{"ticketId":"session-http-ticket","ticketVersion":1,"requestedFor":"fixture-a","expiresAt":{},"reason":"Temporary HTTP customer context"}}"#,
                now + 3_600_000
            )))
            .unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(status, 200);
        assert_eq!(
            value["data"]["id"],
            "support-session-session-http-request"
        );
        assert_eq!(value["data"]["status"], "pending");

        let change = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/support/sessions")
            .header("host", "acme.example.test")
            .header("cookie", format!("__Host-erpx-product={}", p.reviewer_token.expose()))
            .header("origin", "https://acme.example.test")
            .header("idempotency-key", "session-http-approve")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "support-fixture-b")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"id":"support-session-session-http-request","action":"approve","expectedVersion":1,"reason":"Approve the HTTP session"}"#,
            ))
            .unwrap();
        let change_response = router.oneshot(change).await.unwrap();
        let change_status = change_response.status();
        let change_body = to_bytes(change_response.into_body(), 64 * 1024)
            .await
            .unwrap();
        let change_value: serde_json::Value = serde_json::from_slice(&change_body).unwrap();
        assert_eq!(change_status, 200);
        assert_eq!(change_value["data"]["status"], "approved");
        assert_eq!(change_value["data"]["version"], 2);
        let persisted: (String, i64) = sqlx::query_as(
            "SELECT status,version FROM support_sessions WHERE id='support-session-session-http-request'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(persisted, ("approved".into(), 2));
    })
    .await;
}
