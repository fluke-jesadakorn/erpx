//! Actual Support SLA policy, clock, alert and ticket-clock commands over
//! leased PostgreSQL, current Account/IAM authority and the Workflow ledger.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod review;
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_identity_access::public::work_context::{
    ExpectedWorkSelection, HoldingWorkspaceRequest,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{AssignmentId, Clock, CommandKey, CompanyId, HoldingId};
use erpx_server_support::public::sla::{
    AlertAcknowledge, BusinessCalendar, ClockAttach, PolicyChange, PolicyCreate, SlaPolicy,
};
use erpx_server_support::public::ticket::TicketChange;
use erpx_server_workflow::public::commands::CommandJson;
use erpx_service::support_sla_command::{SupportSlaCommand, drain_support_sla};
use erpx_service::support_ticket_command::SupportTicketCommand;
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
) -> (
    review::Prepared,
    SupportSlaCommand<TestClock>,
    SupportTicketCommand<TestClock>,
) {
    let p = review::setup_with_finance(system, false).await;
    let pool = &p.system.holdings[0];
    let now = p.system.scenario.clock.now().get();
    let expires = now + 3_600_000;
    let grant_expires = now + 30 * 24 * 60 * 60 * 1_000;
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
    sqlx::query("UPDATE assignments SET role='custom_support_manager' WHERE id='support-fixture-a'")
        .execute(pool.raw())
        .await
        .unwrap();
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at)VALUES('company','support',1,'active',0,$1,1,'Synthetic reviewed support purchase','public synthetic evidence',$2) ON CONFLICT DO NOTHING")
        .bind(grant_expires)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT 'support-sla-origin-grant','company','support','fixture-a','synthetic-request',NULL,jsonb_build_object('companyId','company','packageId','support','packageVersion',1,'state','active','startsAt',0,'endsAt',$1,'version',1,'reason','Synthetic reviewed support purchase','evidence','public synthetic evidence'),$2 ON CONFLICT DO NOTHING")
        .bind(grant_expires)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let catalogue = Arc::new(current_package_catalogue().unwrap());
    let clock = Arc::new(TestClock(p.system.scenario.clock.clone()));
    let sla = SupportSlaCommand {
        authorizer: p.authorizer.clone(),
        holding: HoldingId::new("h001").unwrap(),
        company: CompanyId::new("company").unwrap(),
        catalogue: catalogue.clone(),
        clock: clock.clone(),
    };
    let ticket = SupportTicketCommand {
        authorizer: p.authorizer.clone(),
        holding: HoldingId::new("h001").unwrap(),
        company: CompanyId::new("company").unwrap(),
        catalogue,
        clock,
    };
    (p, sla, ticket)
}

async fn seed_ticket(
    pool: &erpx_server_runtime::public::postgres::DatabasePool,
    id: &str,
    status: &str,
) {
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES('branch','branch','support','Support Branch','company','company','branch','active') ON CONFLICT DO NOTHING;\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at)VALUES('sla-customer','company','customer','SLA-C','SLA Customer','THB',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version)VALUES('sla-warehouse','company','branch','SLA-WH','SLA Warehouse','active',1) ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,version,created_by,created_at,updated_at)VALUES('sla-order','company','branch','sales','SO-SLA-1','sla-customer','sla-warehouse','THB','2026-11-01','2026-11-10',10000,0,1,'fixture-a',0,0) ON CONFLICT DO NOTHING;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query("INSERT INTO support_tickets(id,company_id,branch_id,department_id,order_id,customer_id,title,issue,owner_id,due_date,status,source_kind,version,created_by,created_at,updated_at)VALUES($1,'company','branch',NULL,'sla-order','sla-customer','SLA customer case','Customer case for the SLA clock.','fixture-a','2026-11-10',$2,'sales_order',1,'fixture-a',0,0) ON CONFLICT DO NOTHING")
        .bind(id)
        .bind(status)
        .execute(pool.raw())
        .await
        .unwrap();
}

fn policy() -> SlaPolicy {
    SlaPolicy {
        calendar: BusinessCalendar {
            timezone: "Asia/Bangkok".into(),
            weekdays: vec![1, 2, 3, 4, 5],
            opens_at_minute: 540,
            closes_at_minute: 1020,
            holidays: vec![],
        },
        target_minutes: 60,
        pause_while_waiting: true,
    }
}

#[tokio::test]
async fn support_sla_policy_attach_drain_acknowledge_and_ticket_clock() {
    review::account::with_system(|system| async move {
        let (p, sla, ticket_command) = support_setup(system).await;
        let pool = &p.system.holdings[0];
        seed_ticket(pool, "sla-ticket", "open").await;

        let create = PolicyCreate {
            name: "Standard support".into(),
            policy: policy(),
            reason: "Draft the standard support target".into(),
        };
        let created = sla
            .create_policy(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-fixture-b")),
                create.clone(),
                CommandJson::from_serializable(&create).unwrap(),
                CommandKey::new("sla-policy-create").unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            created.as_str(),
            r#"{"id":"support-policy-sla-policy-create","version":1}"#
        );
        let policy_id = "support-policy-sla-policy-create";
        let creator: String = sqlx::query_scalar(
            "SELECT created_by FROM support_sla_policies WHERE id='support-policy-sla-policy-create'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(creator, "fixture-b");

        // The creator cannot approve their own policy.
        let self_approve = sla
            .change_policy(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-fixture-b")),
                policy_id.into(),
                PolicyChange::Approve {
                    expected_version: 1,
                    reason: "Approve my own policy".into(),
                },
                CommandJson::parse(
                    r#"{"action":"approve","expectedVersion":1,"reason":"Approve my own policy"}"#,
                )
                .unwrap(),
                CommandKey::new("sla-self-approve").unwrap(),
            )
            .await;
        match self_approve {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "INDEPENDENT_APPROVER_REQUIRED")
            }
            _ => panic!("self approval must fail"),
        }
        sla.change_policy(
            "acme.example.test",
            p.maker.clone(),
            || Ok(selection("support-fixture-a")),
            policy_id.into(),
            PolicyChange::Approve {
                expected_version: 1,
                reason: "Independent approval".into(),
            },
            CommandJson::parse(
                r#"{"action":"approve","expectedVersion":1,"reason":"Independent approval"}"#,
            )
            .unwrap(),
            CommandKey::new("sla-policy-approve").unwrap(),
        )
        .await
        .unwrap();

        let attach = ClockAttach {
            expected_version: 1,
            policy_id: policy_id.into(),
            policy_version: 2,
            reason: "Attach the standard target".into(),
        };
        sla.attach_clock(
            "acme.example.test",
            p.reviewer.clone(),
            || Ok(selection("support-fixture-b")),
            "sla-ticket".into(),
            attach,
            CommandJson::parse(
                r#"{"action":"attach","expectedVersion":1,"policyId":"support-policy-sla-policy-create","policyVersion":2,"reason":"Attach the standard target"}"#,
            )
            .unwrap(),
            CommandKey::new("sla-attach").unwrap(),
        )
        .await
        .unwrap();
        let clock: (String, i64, String) = sqlx::query_as(
            "SELECT sla_policy_id,sla_policy_version,sla_clock_json::text FROM support_tickets WHERE id='sla-ticket'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(clock.0, policy_id);
        assert_eq!(clock.1, 2);
        let clock_value: serde_json::Value = serde_json::from_str(&clock.2).unwrap();
        assert_eq!(clock_value["state"], "running");
        let due_at = clock_value["dueAt"].as_i64().unwrap();

        let mut tx = pool.raw().begin().await.unwrap();
        let inserted = drain_support_sla(
            &mut tx,
            &CompanyId::new("company").unwrap(),
            due_at + 1,
        )
        .await
        .unwrap();
        assert_eq!(inserted, 1);
        let inserted_again = drain_support_sla(
            &mut tx,
            &CompanyId::new("company").unwrap(),
            due_at + 1,
        )
        .await
        .unwrap();
        assert_eq!(inserted_again, 0);
        tx.commit().await.unwrap();
        let alert: (String, i64, i64, i64) = sqlx::query_as(
            "SELECT id,source_version,breached_at,version FROM support_sla_alerts WHERE ticket_id='sla-ticket'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(alert.0, "support-breach-sla-ticket");
        assert_eq!(alert.1, 2);
        assert_eq!(alert.2, due_at);
        assert_eq!(alert.3, 1);
        let notices: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM staff_notifications WHERE record_type='support_sla_alert' AND record_id='support-breach-sla-ticket'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert!(notices >= 1);

        sla.acknowledge_alert(
            "acme.example.test",
            p.reviewer.clone(),
            || Ok(selection("support-fixture-b")),
            "support-breach-sla-ticket".into(),
            AlertAcknowledge {
                expected_version: 1,
                reason: "Take responsibility and follow the recovery plan".into(),
            },
            CommandJson::parse(
                r#"{"action":"acknowledge","expectedVersion":1,"reason":"Take responsibility and follow the recovery plan"}"#,
            )
            .unwrap(),
            CommandKey::new("sla-alert-ack").unwrap(),
        )
        .await
        .unwrap();
        let acknowledged: (String, Option<String>, i64) = sqlx::query_as(
            "SELECT reason,acknowledged_by,version FROM support_sla_alerts WHERE id='support-breach-sla-ticket'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(acknowledged.0, "Take responsibility and follow the recovery plan");
        assert_eq!(acknowledged.1.as_deref(), Some("fixture-b"));
        assert_eq!(acknowledged.2, 2);

        // The native ticket command advances the attached clock: start keeps
        // it running, resolve stops it and the event carries the clock.
        ticket_command
            .change(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                "sla-ticket".into(),
                TicketChange::Start {
                    expected_version: 2,
                    reason: "Start the SLA case".into(),
                },
                CommandJson::parse(
                    r#"{"action":"start","expectedVersion":2,"reason":"Start the SLA case"}"#,
                )
                .unwrap(),
                CommandKey::new("sla-ticket-start").unwrap(),
            )
            .await
            .unwrap();
        ticket_command
            .change(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                "sla-ticket".into(),
                TicketChange::Resolve {
                    expected_version: 3,
                    reason: "Resolve the SLA case".into(),
                },
                CommandJson::parse(
                    r#"{"action":"resolve","expectedVersion":3,"reason":"Resolve the SLA case"}"#,
                )
                .unwrap(),
                CommandKey::new("sla-ticket-resolve").unwrap(),
            )
            .await
            .unwrap();
        let final_clock: String = sqlx::query_scalar(
            "SELECT sla_clock_json->>'state' FROM support_tickets WHERE id='sla-ticket'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(final_clock, "stopped");
        let clock_event: String = sqlx::query_scalar(
            "SELECT evidence_json->>'clock' FROM support_ticket_events WHERE ticket_id='sla-ticket' AND action='resolve'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        let clock_event: serde_json::Value = serde_json::from_str(&clock_event).unwrap();
        assert_eq!(clock_event["state"], "stopped");

        sla.change_policy(
            "acme.example.test",
            p.reviewer.clone(),
            || Ok(selection("support-fixture-b")),
            policy_id.into(),
            PolicyChange::Retire {
                expected_version: 2,
                reason: "Retire the policy for future cases".into(),
            },
            CommandJson::parse(
                r#"{"action":"retire","expectedVersion":2,"reason":"Retire the policy for future cases"}"#,
            )
            .unwrap(),
            CommandKey::new("sla-policy-retire").unwrap(),
        )
        .await
        .unwrap();
        let retired: (String, i64) = sqlx::query_as(
            "SELECT status,version FROM support_sla_policies WHERE id=$1",
        )
        .bind(policy_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(retired, ("retired".into(), 3));

        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id IN ('sla-policy-create','sla-policy-approve','sla-attach','sla-alert-ack','sla-ticket-start','sla-ticket-resolve','sla-policy-retire')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipts, 7);
        let audit: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM audit_events WHERE command_id IN ('sla-policy-create','sla-policy-approve','sla-attach','sla-alert-ack','sla-ticket-start','sla-ticket-resolve','sla-policy-retire')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(audit, 7);
        let outbox: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM outbox_events WHERE command_id IN ('sla-policy-create','sla-policy-approve','sla-attach','sla-alert-ack','sla-ticket-start','sla-ticket-resolve','sla-policy-retire')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(outbox, 7);
    })
    .await;
}

#[tokio::test]
async fn support_sla_rejects_stale_and_ineligible_actions() {
    review::account::with_system(|system| async move {
        let (p, sla, _) = support_setup(system).await;
        let pool = &p.system.holdings[0];
        seed_ticket(pool, "sla-negative-ticket", "open").await;
        let create = PolicyCreate {
            name: "Negative support".into(),
            policy: policy(),
            reason: "Draft the negative policy".into(),
        };
        sla.create_policy(
            "acme.example.test",
            p.reviewer.clone(),
            || Ok(selection("support-fixture-b")),
            create.clone(),
            CommandJson::from_serializable(&create).unwrap(),
            CommandKey::new("sla-negative-create").unwrap(),
        )
        .await
        .unwrap();
        match sla
            .attach_clock(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-fixture-b")),
                "sla-negative-ticket".into(),
                ClockAttach {
                    expected_version: 1,
                    policy_id: "support-policy-sla-negative-create".into(),
                    policy_version: 1,
                    reason: "Attach a draft policy".into(),
                },
                CommandJson::parse(
                    r#"{"action":"attach","expectedVersion":1,"policyId":"support-policy-sla-negative-create","policyVersion":1,"reason":"Attach a draft policy"}"#,
                )
                .unwrap(),
                CommandKey::new("sla-negative-attach").unwrap(),
            )
            .await
        {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "SUPPORT_POLICY_STATE")
            }
            _ => panic!("a draft policy must not attach"),
        }
        match sla
            .change_policy(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                "support-policy-sla-negative-create".into(),
                PolicyChange::Approve {
                    expected_version: 9,
                    reason: "Stale approval".into(),
                },
                CommandJson::parse(
                    r#"{"action":"approve","expectedVersion":9,"reason":"Stale approval"}"#,
                )
                .unwrap(),
                CommandKey::new("sla-negative-approve").unwrap(),
            )
            .await
        {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "SUPPORT_POLICY_STATE")
            }
            _ => panic!("a stale policy must not approve"),
        }
    })
    .await;
}

#[tokio::test]
async fn support_sla_http_route_accepts_real_session_and_rendered_context() {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use erpx_service::field_service_reservation_read_http::FieldServiceReadScopeHeaders;
    use erpx_service::support_sla_http::{SupportSlaHttpTarget, support_sla_http_router};
    use tower::ServiceExt;
    review::account::with_system(|system| async move {
        let (p, sla, _) = support_setup(system).await;
        let pool = &p.system.holdings[0];
        seed_ticket(pool, "sla-http-ticket", "open").await;
        let router = support_sla_http_router(
            Arc::new(review::account::fixture::account::policy()),
            Arc::new(FieldServiceReadScopeHeaders),
            vec![SupportSlaHttpTarget {
                host: "acme.example.test".into(),
                holding_slug: "acme".into(),
                command: Arc::new(sla.clone()),
            }],
        )
        .unwrap();
        let create = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/support/sla")
            .header("host", "acme.example.test")
            .header("cookie", format!("__Host-erpx-product={}", p.reviewer_token.expose()))
            .header("origin", "https://acme.example.test")
            .header("idempotency-key", "sla-http-create")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "support-fixture-b")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"action":"create-policy","name":"HTTP support","policy":{"calendar":{"timezone":"Asia/Bangkok","weekdays":[1,2,3,4,5],"opensAtMinute":540,"closesAtMinute":1020,"holidays":[]},"targetMinutes":60,"pauseWhileWaiting":true},"reason":"Draft the HTTP policy"}"#,
            ))
            .unwrap();
        let response = router.clone().oneshot(create).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(status, 200);
        assert_eq!(value["data"]["id"], "support-policy-sla-http-create");

        // A second policy created by fixture-c is approved over HTTP by
        // fixture-b, proving the route with a real independent manager session.
        let source = PolicyCreate {
            name: "HTTP approval source".into(),
            policy: policy(),
            reason: "Draft the HTTP approval source".into(),
        };
        sla.create_policy(
            "acme.example.test",
            p.maker.clone(),
            || Ok(selection("support-fixture-a")),
            source.clone(),
            CommandJson::from_serializable(&source).unwrap(),
            CommandKey::new("sla-http-source").unwrap(),
        )
        .await
        .unwrap();
        let approve = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/support/sla")
            .header("host", "acme.example.test")
            .header("cookie", format!("__Host-erpx-product={}", p.reviewer_token.expose()))
            .header("origin", "https://acme.example.test")
            .header("idempotency-key", "sla-http-approve")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "support-fixture-b")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"id":"support-policy-sla-http-source","action":"approve","expectedVersion":1,"reason":"Approve over HTTP"}"#,
            ))
            .unwrap();
        let approve_response = router.oneshot(approve).await.unwrap();
        let approve_status = approve_response.status();
        let approve_body = to_bytes(approve_response.into_body(), 64 * 1024)
            .await
            .unwrap();
        let approve_value: serde_json::Value = serde_json::from_slice(&approve_body).unwrap();
        assert_eq!(approve_status, 200);
        assert_eq!(approve_value["data"]["version"], 2);
        let persisted: (String, i64) = sqlx::query_as(
            "SELECT status,version FROM support_sla_policies WHERE id='support-policy-sla-http-source'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(persisted, ("approved".into(), 2));
    })
    .await;
}
