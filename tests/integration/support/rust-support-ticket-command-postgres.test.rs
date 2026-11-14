#![allow(dead_code)]
//! Actual Support ticket command over leased PostgreSQL, current Account/IAM
//! authority and the Workflow receipt/audit/outbox. No TS writer is invoked.
#[path = "../../support/finance/review-composition.rs"]
mod review;
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_identity_access::public::work_context::{
    ExpectedWorkSelection, HoldingWorkspaceRequest,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{AssignmentId, Clock, CommandKey, CompanyId, HoldingId};
use erpx_server_support::public::ticket::TicketChange;
use erpx_server_workflow::public::commands::CommandJson;
use erpx_service::support_ticket_command::{SupportTicketCommand, SupportTicketCreate};
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

#[tokio::test]
async fn support_ticket_create_triage_start_resolve_confirm_and_replay() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        let pool = &p.system.holdings[0];
        let now = p.system.scenario.clock.now().get();
        let expires = now + 3_600_000;
        sqlx::raw_sql(
            "INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','custom_support_agent','Synthetic Support agent',false,1,'active'),('*','custom_support_manager','Synthetic Support manager',false,1,'active') ON CONFLICT DO NOTHING;\
             INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_support_agent',1,'[\"records.read\",\"records.write\",\"support.read\",\"support.create\",\"support.work\",\"commerce.read\",\"module.support.read\",\"module.assets.read\"]','[]','[\"holding\",\"branch\",\"department\"]','synthetic-fixture',0),('*','custom_support_manager',1,'[\"records.read\",\"records.write\",\"support.read\",\"support.create\",\"support.work\",\"support.manage\",\"commerce.read\",\"module.support.read\",\"module.assets.read\"]','[]','[\"holding\",\"branch\",\"department\"]','synthetic-fixture',0) ON CONFLICT DO NOTHING;\
             INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,version,created_by,created_at,updated_at)VALUES('person-a','fixture-a','company','Synthetic Agent','TH','active',1,'synthetic-fixture',0,0),('person-b','fixture-b','company','Synthetic Manager','TH','active',1,'synthetic-fixture',0,0) ON CONFLICT DO NOTHING;\
             INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,end_date,job_title,currency,version,created_by,created_at,updated_at)VALUES('engagement-a','person-a','company','employee','salaried','active','2026-01-01',NULL,'Support','THB',1,'synthetic-fixture',0,0),('engagement-b','person-b','company','employee','salaried','active','2026-01-01',NULL,'Support','THB',1,'synthetic-fixture',0,0) ON CONFLICT DO NOTHING;INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version)VALUES('company','fixture-a','local:fixture-a','person-a','active','local',1),('company','fixture-b','local:fixture-b','person-b','active','local',1) ON CONFLICT DO NOTHING;",
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
        sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT 'support-origin-grant','company','support','fixture-a','synthetic-request',NULL,jsonb_build_object('companyId','company','packageId','support','packageVersion',1,'state','active','startsAt',0,'endsAt',$1,'version',1,'reason','Synthetic reviewed support purchase','evidence','public synthetic evidence'),$2")
            .bind(expires)
            .bind(now)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let command = SupportTicketCommand {
            authorizer: p.authorizer.clone(),
            holding: HoldingId::new("h001").unwrap(),
            company: CompanyId::new("company").unwrap(),
            catalogue: Arc::new(current_package_catalogue().unwrap()),
            clock: Arc::new(TestClock(p.system.scenario.clock.clone())),
        };
        let create_wire = CommandJson::parse(
            r#"{"kind":"general","title":"Printer offline","issue":"The front desk printer is offline."}"#,
        )
        .unwrap();
        let create_input =
            SupportTicketCreate::parse(&serde_json::from_str(create_wire.as_str()).unwrap())
                .unwrap();
        let created = command
            .create(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                create_input,
                create_wire,
                CommandKey::new("support-create-1").unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(created.as_str(), r#"{"id":"ticket-support-create-1","status":"triage","version":1}"#);
        let replay = command
            .create(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                SupportTicketCreate::General {
                    title: "Printer offline".into(),
                    issue: "The front desk printer is offline.".into(),
                },
                CommandJson::parse(
                    r#"{"kind":"general","title":"Printer offline","issue":"The front desk printer is offline."}"#,
                )
                .unwrap(),
                CommandKey::new("support-create-1").unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(replay, created);

        let triage = TicketChange::Triage {
            expected_version: 1,
            reason: "Assign the customer report".into(),
            owner_id: "fixture-a".into(),
            due_date: "2026-11-10".into(),
        };
        let triage_wire = CommandJson::parse(
            r#"{"action":"triage","expectedVersion":1,"reason":"Assign the customer report","ownerId":"fixture-a","dueDate":"2026-11-10"}"#,
        )
        .unwrap();
        command
            .change(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-fixture-b")),
                "ticket-support-create-1".into(),
                triage,
                triage_wire,
                CommandKey::new("support-triage-1").unwrap(),
            )
            .await
            .unwrap();

        let start = TicketChange::Start {
            expected_version: 2,
            reason: "Start customer assistance".into(),
        };
        command
            .change(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                "ticket-support-create-1".into(),
                start,
                CommandJson::parse(
                    r#"{"action":"start","expectedVersion":2,"reason":"Start customer assistance"}"#,
                )
                .unwrap(),
                CommandKey::new("support-start-1").unwrap(),
            )
            .await
            .unwrap();

        let resolve = TicketChange::Resolve {
            expected_version: 3,
            reason: "The printer is working again".into(),
        };
        command
            .change(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                "ticket-support-create-1".into(),
                resolve,
                CommandJson::parse(
                    r#"{"action":"resolve","expectedVersion":3,"reason":"The printer is working again"}"#,
                )
                .unwrap(),
                CommandKey::new("support-resolve-1").unwrap(),
            )
            .await
            .unwrap();

        let confirm = TicketChange::ConfirmGeneral {
            expected_version: 4,
            reason: "The manager verified the fix".into(),
        };
        command
            .change(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-fixture-b")),
                "ticket-support-create-1".into(),
                confirm,
                CommandJson::parse(
                    r#"{"action":"confirm-general","expectedVersion":4,"reason":"The manager verified the fix"}"#,
                )
                .unwrap(),
                CommandKey::new("support-confirm-1").unwrap(),
            )
            .await
            .unwrap();

        let row: (String, i64, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT status,version,confirmed_by,confirmation_reason FROM support_tickets WHERE id='ticket-support-create-1' AND company_id='company'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row.0, "closed");
        assert_eq!(row.1, 5);
        assert_eq!(row.2.as_deref(), Some("fixture-b"));
        assert_eq!(row.3.as_deref(), Some("The manager verified the fix"));
        let events: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM support_ticket_events WHERE ticket_id='ticket-support-create-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(events, 5);
        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id IN ('support-create-1','support-triage-1','support-start-1','support-resolve-1','support-confirm-1')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipts, 5);
        let audit: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM audit_events WHERE command_id IN ('support-create-1','support-triage-1','support-start-1','support-resolve-1','support-confirm-1')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(audit, 5);
        let outbox: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM outbox_events WHERE command_id IN ('support-create-1','support-triage-1','support-start-1','support-resolve-1','support-confirm-1')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(outbox, 5);

        // Stale version is rejected before any write.
        let stale = command
            .change(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                "ticket-support-create-1".into(),
                TicketChange::Start {
                    expected_version: 4,
                    reason: "Stale start".into(),
                },
                CommandJson::parse(
                    r#"{"action":"start","expectedVersion":4,"reason":"Stale start"}"#,
                )
                .unwrap(),
                CommandKey::new("support-stale-1").unwrap(),
            )
            .await;
        match stale {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "SUPPORT_STATE_CHANGED")
            }
            _ => panic!("stale Support change must fail"),
        }

        // A manager cannot perform the owner-only start action.
        command
            .create(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                SupportTicketCreate::General {
                    title: "Owner-only start".into(),
                    issue: "The assigned agent must start this case.".into(),
                },
                CommandJson::parse(
                    r#"{"kind":"general","title":"Owner-only start","issue":"The assigned agent must start this case."}"#,
                )
                .unwrap(),
                CommandKey::new("support-owner-create").unwrap(),
            )
            .await
            .unwrap();
        command
            .change(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-fixture-b")),
                "ticket-support-owner-create".into(),
                TicketChange::Triage {
                    expected_version: 1,
                    reason: "Assign the owner-only case".into(),
                    owner_id: "fixture-a".into(),
                    due_date: "2026-11-10".into(),
                },
                CommandJson::parse(
                    r#"{"action":"triage","expectedVersion":1,"reason":"Assign the owner-only case","ownerId":"fixture-a","dueDate":"2026-11-10"}"#,
                )
                .unwrap(),
                CommandKey::new("support-owner-triage").unwrap(),
            )
            .await
            .unwrap();
        let owner_start = command
            .change(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-fixture-b")),
                "ticket-support-owner-create".into(),
                TicketChange::Start {
                    expected_version: 2,
                    reason: "Manager attempts the owner action".into(),
                },
                CommandJson::parse(
                    r#"{"action":"start","expectedVersion":2,"reason":"Manager attempts the owner action"}"#,
                )
                .unwrap(),
                CommandKey::new("support-owner-start").unwrap(),
            )
            .await;
        match owner_start {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "SUPPORT_OWNER_REQUIRED")
            }
            _ => panic!("manager owner-only start must fail"),
        }

        // The resolving manager cannot confirm their own resolution.
        command
            .create(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                SupportTicketCreate::General {
                    title: "Independent confirmation".into(),
                    issue: "A different manager must confirm this resolution.".into(),
                },
                CommandJson::parse(
                    r#"{"kind":"general","title":"Independent confirmation","issue":"A different manager must confirm this resolution."}"#,
                )
                .unwrap(),
                CommandKey::new("support-independent-create").unwrap(),
            )
            .await
            .unwrap();
        command
            .change(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-fixture-b")),
                "ticket-support-independent-create".into(),
                TicketChange::Triage {
                    expected_version: 1,
                    reason: "Assign the independent case".into(),
                    owner_id: "fixture-b".into(),
                    due_date: "2026-11-10".into(),
                },
                CommandJson::parse(
                    r#"{"action":"triage","expectedVersion":1,"reason":"Assign the independent case","ownerId":"fixture-b","dueDate":"2026-11-10"}"#,
                )
                .unwrap(),
                CommandKey::new("support-independent-triage").unwrap(),
            )
            .await
            .unwrap();
        command
            .change(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-fixture-b")),
                "ticket-support-independent-create".into(),
                TicketChange::Start {
                    expected_version: 2,
                    reason: "Manager starts the case".into(),
                },
                CommandJson::parse(
                    r#"{"action":"start","expectedVersion":2,"reason":"Manager starts the case"}"#,
                )
                .unwrap(),
                CommandKey::new("support-independent-start").unwrap(),
            )
            .await
            .unwrap();
        command
            .change(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-fixture-b")),
                "ticket-support-independent-create".into(),
                TicketChange::Resolve {
                    expected_version: 3,
                    reason: "Manager resolves the case".into(),
                },
                CommandJson::parse(
                    r#"{"action":"resolve","expectedVersion":3,"reason":"Manager resolves the case"}"#,
                )
                .unwrap(),
                CommandKey::new("support-independent-resolve").unwrap(),
            )
            .await
            .unwrap();
        let self_confirm = command
            .change(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-fixture-b")),
                "ticket-support-independent-create".into(),
                TicketChange::ConfirmGeneral {
                    expected_version: 4,
                    reason: "The same manager attempts confirmation".into(),
                },
                CommandJson::parse(
                    r#"{"action":"confirm-general","expectedVersion":4,"reason":"The same manager attempts confirmation"}"#,
                )
                .unwrap(),
                CommandKey::new("support-independent-confirm").unwrap(),
            )
            .await;
        match self_confirm {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "INDEPENDENT_CONFIRMATION_REQUIRED")
            }
            _ => panic!("self-confirmation must fail"),
        }

        // Employee report: edit, add information and withdraw with replay-safe history.
        command
            .create(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                SupportTicketCreate::General {
                    title: "Own workstation issue".into(),
                    issue: "The keyboard stops responding after wake.".into(),
                },
                CommandJson::parse(
                    r#"{"kind":"general","title":"Own workstation issue","issue":"The keyboard stops responding after wake."}"#,
                )
                .unwrap(),
                CommandKey::new("support-report-create").unwrap(),
            )
            .await
            .unwrap();
        command
            .change(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                "ticket-support-report-create".into(),
                TicketChange::EditReport {
                    expected_version: 1,
                    title: "Keyboard issue after waking".into(),
                    issue: "I verified this affects only my workstation.".into(),
                    reason: "Correct the original report".into(),
                },
                CommandJson::parse(
                    r#"{"action":"edit-report","expectedVersion":1,"title":"Keyboard issue after waking","issue":"I verified this affects only my workstation.","reason":"Correct the original report"}"#,
                )
                .unwrap(),
                CommandKey::new("support-report-edit").unwrap(),
            )
            .await
            .unwrap();
        command
            .change(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                "ticket-support-report-create".into(),
                TicketChange::AddInformation {
                    expected_version: 2,
                    reason: "A restart temporarily restored the keyboard".into(),
                },
                CommandJson::parse(
                    r#"{"action":"add-information","expectedVersion":2,"reason":"A restart temporarily restored the keyboard"}"#,
                )
                .unwrap(),
                CommandKey::new("support-report-info").unwrap(),
            )
            .await
            .unwrap();
        command
            .change(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                "ticket-support-report-create".into(),
                TicketChange::WithdrawReport {
                    expected_version: 3,
                    reason: "Replacement keyboard fixed the issue".into(),
                },
                CommandJson::parse(
                    r#"{"action":"withdraw-report","expectedVersion":3,"reason":"Replacement keyboard fixed the issue"}"#,
                )
                .unwrap(),
                CommandKey::new("support-report-withdraw").unwrap(),
            )
            .await
            .unwrap();
        let report: (String, i64, String) = sqlx::query_as(
            "SELECT status,version,title FROM support_tickets WHERE id='ticket-support-report-create'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(report.0, "cancelled");
        assert_eq!(report.1, 4);
        assert_eq!(report.2, "Keyboard issue after waking");

        // Order-backed create, resolve and independent customer-note confirmation.
        sqlx::raw_sql(
            "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at)VALUES('support-customer','company','customer','SUPPORT-C','Support Customer','THB',0,0) ON CONFLICT DO NOTHING;             INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES('branch','branch','support','Support Branch','company','company','branch','active') ON CONFLICT DO NOTHING;             INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version)VALUES('warehouse','company','branch','WH','Support Warehouse','active',1) ON CONFLICT DO NOTHING;             INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,version,created_by,created_at,updated_at)VALUES('support-order','company','branch','sales','SO-SUPPORT-1','support-customer','warehouse','THB','2026-11-01','2026-11-10',1000,0,1,'fixture-a',0,0) ON CONFLICT DO NOTHING;",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        command
            .create(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                SupportTicketCreate::Order {
                    order_id: "support-order".into(),
                    source_version: 1,
                    title: "Order support".into(),
                    issue: "Customer needs order help.".into(),
                    owner_id: "fixture-a".into(),
                    due_date: "2026-11-10".into(),
                },
                CommandJson::parse(
                    r#"{"orderId":"support-order","sourceVersion":1,"title":"Order support","issue":"Customer needs order help.","ownerId":"fixture-a","dueDate":"2026-11-10"}"#,
                )
                .unwrap(),
                CommandKey::new("support-order-create").unwrap(),
            )
            .await
            .unwrap();
        command
            .change(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                "ticket-support-order-create".into(),
                TicketChange::Start {
                    expected_version: 1,
                    reason: "Start order support".into(),
                },
                CommandJson::parse(
                    r#"{"action":"start","expectedVersion":1,"reason":"Start order support"}"#,
                )
                .unwrap(),
                CommandKey::new("support-order-start").unwrap(),
            )
            .await
            .unwrap();
        command
            .change(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                "ticket-support-order-create".into(),
                TicketChange::Resolve {
                    expected_version: 2,
                    reason: "Order support resolved".into(),
                },
                CommandJson::parse(
                    r#"{"action":"resolve","expectedVersion":2,"reason":"Order support resolved"}"#,
                )
                .unwrap(),
                CommandKey::new("support-order-resolve").unwrap(),
            )
            .await
            .unwrap();
        let note_at = p.system.scenario.clock.now().get();
        sqlx::query("INSERT INTO customer_contact_notes(id,company_id,customer_id,order_id,channel,occurred_at,body,status,version,created_by,created_at,updated_at)VALUES('support-note','company','support-customer','support-order','phone',$1,'Customer confirmed the fix','active',1,'fixture-a',$1,$1)")
            .bind(note_at)
            .execute(pool.raw())
            .await
            .unwrap();
        command
            .change(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-fixture-b")),
                "ticket-support-order-create".into(),
                TicketChange::Confirm {
                    expected_version: 3,
                    reason: "Manager reviewed the customer confirmation".into(),
                    note_id: "support-note".into(),
                    note_version: 1,
                },
                CommandJson::parse(
                    r#"{"action":"confirm","expectedVersion":3,"reason":"Manager reviewed the customer confirmation","noteId":"support-note","noteVersion":1}"#,
                )
                .unwrap(),
                CommandKey::new("support-order-confirm").unwrap(),
            )
            .await
            .unwrap();
        let order: (String, i64, Option<String>) = sqlx::query_as(
            "SELECT status,version,confirmation_note_id FROM support_tickets WHERE id='ticket-support-order-create'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(order.0, "closed");
        assert_eq!(order.1, 4);
        assert_eq!(order.2.as_deref(), Some("support-note"));

        // Typed asset source create retains the actual owner snapshot.
        let mut assets_tx = pool.raw().begin().await.unwrap();
        sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at)VALUES('company','assets',1,'active',0,$1,1,'Synthetic reviewed assets purchase','public synthetic evidence',$2) ON CONFLICT DO NOTHING")
            .bind(expires)
            .bind(now)
            .execute(&mut *assets_tx)
            .await
            .unwrap();
        sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT 'support-assets-grant','company','assets','fixture-a','synthetic-request',NULL,jsonb_build_object('companyId','company','packageId','assets','packageVersion',1,'state','active','startsAt',0,'endsAt',$1,'version',1,'reason','Synthetic reviewed assets purchase','evidence','public synthetic evidence'),$2 ON CONFLICT DO NOTHING")
            .bind(expires)
            .bind(now)
            .execute(&mut *assets_tx)
            .await
            .unwrap();
        assets_tx.commit().await.unwrap();
        sqlx::query("INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,warranty_end,created_by,created_at,updated_at)VALUES('support-asset','company','branch','SUPPORT-ASSET','Support asset','synthetic','THB','2099-12-31','fixture-a',0,0) ON CONFLICT DO NOTHING")
            .execute(pool.raw())
            .await
            .unwrap();
        command
            .create(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                SupportTicketCreate::Source {
                    kind: "asset".into(),
                    id: "support-asset".into(),
                    version: 1,
                    title: "Asset support".into(),
                    issue: "The shared asset needs support.".into(),
                },
                CommandJson::parse(
                    r#"{"kind":"source","source":{"kind":"asset","id":"support-asset","version":1},"title":"Asset support","issue":"The shared asset needs support."}"#,
                )
                .unwrap(),
                CommandKey::new("support-asset-create").unwrap(),
            )
            .await
            .unwrap();
        let asset: (String, String, String) = sqlx::query_as(
            "SELECT status,source_kind,source_record_json FROM support_tickets WHERE id='ticket-support-asset-create'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(asset.0, "triage");
        assert_eq!(asset.1, "asset");
        let source: serde_json::Value = serde_json::from_str(&asset.2).unwrap();
        assert_eq!(source["kind"], "asset");
        assert_eq!(source["id"], "support-asset");
        assert_eq!(source["version"], 1);

        // Two managers race the same triage; exactly one commits and the other
        // observes the changed version before any write.
        command
            .create(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                SupportTicketCreate::General {
                    title: "Concurrent triage".into(),
                    issue: "Only one manager may triage this report.".into(),
                },
                CommandJson::parse(
                    r#"{"kind":"general","title":"Concurrent triage","issue":"Only one manager may triage this report."}"#,
                )
                .unwrap(),
                CommandKey::new("support-race-create").unwrap(),
            )
            .await
            .unwrap();
        let triage_wire = || {
            CommandJson::parse(
                r#"{"action":"triage","expectedVersion":1,"reason":"Race to triage","ownerId":"fixture-a","dueDate":"2026-11-10"}"#,
            )
            .unwrap()
        };
        let first = command.clone();
        let second = command.clone();
        let (a, b) = tokio::join!(
            first.change(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-fixture-b")),
                "ticket-support-race-create".into(),
                TicketChange::Triage {
                    expected_version: 1,
                    reason: "Race to triage".into(),
                    owner_id: "fixture-a".into(),
                    due_date: "2026-11-10".into(),
                },
                triage_wire(),
                CommandKey::new("support-race-a").unwrap(),
            ),
            second.change(
                "acme.example.test",
                p.reviewer.clone(),
                || Ok(selection("support-fixture-b")),
                "ticket-support-race-create".into(),
                TicketChange::Triage {
                    expected_version: 1,
                    reason: "Race to triage".into(),
                    owner_id: "fixture-a".into(),
                    due_date: "2026-11-10".into(),
                },
                triage_wire(),
                CommandKey::new("support-race-b").unwrap(),
            )
        );
        assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
        let conflict = a.err().or_else(|| b.err()).unwrap();
        match conflict {
            PgTransactionError::Business(error) => {
                assert_eq!(error.code, "SUPPORT_STATE_CHANGED")
            }
            _ => panic!("the losing triage must be a business conflict"),
        }
        let raced: (i64, i64) = sqlx::query_as(
            "SELECT version,(SELECT count(*) FROM support_ticket_events e WHERE e.ticket_id=t.id AND e.action='triage') FROM support_tickets t WHERE t.id='ticket-support-race-create'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(raced, (2, 1));
    })
    .await;
}


#[tokio::test]
async fn support_ticket_http_route_accepts_real_session_and_rendered_context() {
    use axum::{body::{Body, to_bytes}, http::Request};
    use erpx_service::field_service_reservation_read_http::FieldServiceReadScopeHeaders;
    use erpx_service::support_ticket_http::{SupportTicketHttpTarget, support_ticket_http_router};
    use tower::ServiceExt;
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        let pool = &p.system.holdings[0];
        let now = p.system.scenario.clock.now().get();
        let expires = now + 3_600_000;
        sqlx::raw_sql(
            "INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','custom_support_agent','Synthetic Support agent',false,1,'active'),('*','custom_support_manager','Synthetic Support manager',false,1,'active') ON CONFLICT DO NOTHING;\
             INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_support_agent',1,'[\"records.read\",\"records.write\",\"support.read\",\"support.create\",\"support.work\",\"commerce.read\",\"module.support.read\",\"module.assets.read\"]','[]','[\"holding\",\"branch\",\"department\"]','synthetic-fixture',0),('*','custom_support_manager',1,'[\"records.read\",\"records.write\",\"support.read\",\"support.create\",\"support.work\",\"support.manage\",\"commerce.read\",\"module.support.read\",\"module.assets.read\"]','[]','[\"holding\",\"branch\",\"department\"]','synthetic-fixture',0) ON CONFLICT DO NOTHING;\
             INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,version,created_by,created_at,updated_at)VALUES('person-a','fixture-a','company','Synthetic Agent','TH','active',1,'synthetic-fixture',0,0),('person-b','fixture-b','company','Synthetic Manager','TH','active',1,'synthetic-fixture',0,0) ON CONFLICT DO NOTHING;\
             INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,end_date,job_title,currency,version,created_by,created_at,updated_at)VALUES('engagement-a','person-a','company','employee','salaried','active','2026-01-01',NULL,'Support','THB',1,'synthetic-fixture',0,0),('engagement-b','person-b','company','employee','salaried','active','2026-01-01',NULL,'Support','THB',1,'synthetic-fixture',0,0) ON CONFLICT DO NOTHING;INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version)VALUES('company','fixture-a','local:fixture-a','person-a','active','local',1),('company','fixture-b','local:fixture-b','person-b','active','local',1) ON CONFLICT DO NOTHING;",
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

        let command = SupportTicketCommand {
            authorizer: p.authorizer.clone(),
            holding: HoldingId::new("h001").unwrap(),
            company: CompanyId::new("company").unwrap(),
            catalogue: Arc::new(current_package_catalogue().unwrap()),
            clock: Arc::new(TestClock(p.system.scenario.clock.clone())),
        };
        let router = support_ticket_http_router(
            Arc::new(review::account::fixture::account::policy()),
            Arc::new(FieldServiceReadScopeHeaders),
            vec![SupportTicketHttpTarget {
                host: "acme.example.test".into(),
                holding_slug: "acme".into(),
                command: Arc::new(command),
            }],
        )
        .unwrap();
        let cookie = format!("__Host-erpx-product={}", p.maker_token.expose());
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/support")
            .header("host", "acme.example.test")
            .header("cookie", cookie)
            .header("origin", "https://acme.example.test")
            .header("idempotency-key", "http-create-1")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "support-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"kind":"general","title":"HTTP printer offline","issue":"The front desk printer is offline over HTTP."}"#,
            ))
            .unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), 200);
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["data"]["id"], "ticket-http-create-1");
        assert_eq!(value["data"]["status"], "triage");
        assert_eq!(value["data"]["version"], 1);
        let status: String = sqlx::query_scalar(
            "SELECT status FROM support_tickets WHERE id='ticket-http-create-1' AND company_id='company'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(status, "triage");

        // The browser sends the ticket id with the action; the route must strip
        // it before strict TicketChange deserialization.
        let change = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/support")
            .header("host", "acme.example.test")
            .header("cookie", format!("__Host-erpx-product={}", p.maker_token.expose()))
            .header("origin", "https://acme.example.test")
            .header("idempotency-key", "http-change-1")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "support-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"id":"ticket-http-create-1","action":"edit-report","expectedVersion":1,"title":"HTTP printer offline corrected","issue":"Corrected over HTTP.","reason":"Correct the report"}"#,
            ))
            .unwrap();
        let change_response = router.clone().oneshot(change).await.unwrap();
        assert_eq!(change_response.status(), 200);
        let change_body = to_bytes(change_response.into_body(), 64 * 1024)
            .await
            .unwrap();
        let change_value: serde_json::Value = serde_json::from_slice(&change_body).unwrap();
        assert_eq!(change_value["data"]["status"], "triage");
        assert_eq!(change_value["data"]["version"], 2);
        let corrected: (String, i64) = sqlx::query_as(
            "SELECT title,version FROM support_tickets WHERE id='ticket-http-create-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(corrected.0, "HTTP printer offline corrected");
        assert_eq!(corrected.1, 2);

        let send = |key: Option<&'static str>,
                    origin: Option<&'static str>,
                    company: &'static str,
                    assignment: &'static str,
                    cookie: Option<String>| {
            let router = router.clone();
            async move {
                let mut request = Request::builder()
                    .method("POST")
                    .uri("/api/v1/holdings/acme/support")
                    .header("host", "acme.example.test")
                    .header("x-erpx-context-company", company)
                    .header("x-erpx-context-assignment", assignment)
                    .header("x-erpx-context-revision", "7")
                    .header("x-erpx-context-assignment-version", "3")
                    .header("content-type", "application/json");
                if let Some(key) = key {
                    request = request.header("idempotency-key", key);
                }
                if let Some(origin) = origin {
                    request = request.header("origin", origin);
                }
                if let Some(cookie) = cookie {
                    request = request.header("cookie", cookie);
                }
                let response = router
                    .oneshot(
                        request
                            .body(Body::from(
                                r#"{"kind":"general","title":"Rejected request","issue":"This request must be rejected."}"#,
                            ))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                response.status().as_u16()
            }
        };
        let cookie = format!("__Host-erpx-product={}", p.maker_token.expose());
        assert_eq!(
            send(
                Some("http-origin"),
                Some("https://evil.example.test"),
                "company",
                "support-fixture-a",
                Some(cookie.clone())
            )
            .await,
            403
        );
        assert_eq!(
            send(None, None, "company", "support-fixture-a", Some(cookie.clone())).await,
            422
        );
        assert_eq!(
            send(
                Some("http-cas"),
                None,
                "company",
                "support-fixture-b",
                Some(cookie.clone())
            )
            .await,
            409
        );
        assert_eq!(
            send(Some("http-company"), None, "other", "support-fixture-a", Some(cookie.clone()))
                .await,
            404
        );
        assert_eq!(
            send(Some("http-no-session"), None, "company", "support-fixture-a", None).await,
            401
        );
        let rejected: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM support_tickets WHERE id IN ('ticket-http-origin','ticket-http-cas','ticket-http-company','ticket-http-no-session')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(rejected, 0);
    })
    .await;
}

#[tokio::test]
async fn records_request_team_sources_match_typescript_join_and_reject_invalid_fences() {
    use erpx_server_support::public::records_request_team_sources::{inspect_records_request_team_sources,lock_records_request_team_sources};
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script=r#"if(process.version!=='v26.11.1')throw Error('PINNED_NODE_REQUIRED');const fs=await import('node:fs'),people=await import('./packages/server-people_payroll/src/adapters/final-root-owner-projection-sources.ts'),s=fs.readFileSync('./packages/server-support/src/adapters/service-team-reads.ts','utf8');const match=s.match(/prepare\(`(SELECT m\.user_id,m\.responsibility,[^`]+)`\)/);if(!match)throw Error('TEAM_QUERY_CHANGED');let sql=match[1].replace('${people_payrollProjectionSources.hr_people}',people.finalRootOwnerProjectionSources.hr_people);if(sql.includes('${')||(sql.match(/\?/g)||[]).length!==2)throw Error('TEAM_QUERY_BINDS_CHANGED');let n=0;process.stdout.write(sql.replace(/\?/g,()=>'$'+(++n)));"#;
    let oracle=std::process::Command::new(std::env::var("ERPX_NODE_BIN").expect("pinned Node required")).current_dir(root).args(["--conditions=react-server","--input-type=module","-e",script]).output().unwrap();assert!(oracle.status.success(),"actual Support TS oracle: {}",String::from_utf8_lossy(&oracle.stderr));let original=String::from_utf8(oracle.stdout).unwrap();
    review::account::with_system(|system|async move{
        let p=review::setup_with_finance(system,false).await;let db=&p.system.holdings[0];
        sqlx::raw_sql("INSERT INTO service_teams(id,company_id,name,active,version,created_by,created_at,updated_at) VALUES('records-team','company','Records team',1,1,'fixture-a',1,1),('records-inactive','company','Inactive team',0,1,'fixture-a',1,1),('records-foreign','foreign','Foreign team',1,1,'fixture-a',1,1); INSERT INTO service_team_members(company_id,team_id,user_id,responsibility) VALUES('company','records-team','fixture-a','lead'),('company','records-team','fixture-b','member'),('company','records-inactive','fixture-a','lead'),('foreign','records-foreign','fixture-b','lead')").execute(db.raw()).await.unwrap();
        let mut tx=db.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();assert!(inspect_records_request_team_sources(&mut tx).await.is_err());lock_records_request_team_sources(&mut tx).await.unwrap();let source=inspect_records_request_team_sources(&mut tx).await.unwrap();
        let needle="FROM service_team_members m JOIN service_teams t";assert_eq!(original.matches(needle).count(),1);let projected=format!("WITH {} {}",source.cte_sql(),original.replacen(needle,"FROM records_request_team_members m JOIN records_request_teams t",1));
        for (team,company,count) in [("records-team","company",2),("records-inactive","company",0),("records-foreign","company",0),("missing","company",0),("records-foreign","foreign",1)] {
            let actual:Vec<(String,String,String)>=sqlx::query_as(sqlx::AssertSqlSafe(original.as_str())).bind(team).bind(company).fetch_all(&mut *tx).await.unwrap();let ours:Vec<(String,String,String)>=sqlx::query_as(sqlx::AssertSqlSafe(projected.as_str())).bind(team).bind(company).fetch_all(&mut *tx).await.unwrap();assert_eq!(actual.len(),count);assert_eq!(ours,actual);source.require_current(&mut tx).await.unwrap();
        }
        sqlx::query("SAVEPOINT team_drift").execute(&mut *tx).await.unwrap();sqlx::query("UPDATE service_teams SET active=0 WHERE id='records-team'").execute(&mut *tx).await.unwrap();let actual:Vec<(String,String,String)>=sqlx::query_as(sqlx::AssertSqlSafe(original.as_str())).bind("records-team").bind("company").fetch_all(&mut *tx).await.unwrap();let ours:Vec<(String,String,String)>=sqlx::query_as(sqlx::AssertSqlSafe(projected.as_str())).bind("records-team").bind("company").fetch_all(&mut *tx).await.unwrap();assert!(actual.is_empty());assert_eq!(ours,actual);source.require_current(&mut tx).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT team_drift").execute(&mut *tx).await.unwrap();
        let reader_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *tx).await.unwrap();let(send,receive)=tokio::sync::oneshot::channel();let other=(*db).clone();let writer=tokio::spawn(async move{let mut w=other.raw().begin().await.unwrap();sqlx::query("SET LOCAL lock_timeout='3s'").execute(&mut *w).await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *w).await.unwrap();send.send(pid).unwrap();let error=sqlx::query("UPDATE public.service_teams SET name='blocked' WHERE id='records-inactive'").execute(&mut *w).await.unwrap_err();assert_eq!(error.as_database_error().and_then(|e|e.code()).as_deref(),Some("55P03"));w.rollback().await.unwrap();});let pid=receive.await.unwrap();tokio::time::timeout(std::time::Duration::from_secs(2),async{loop{let blocked:bool=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2)) AND EXISTS(SELECT 1 FROM pg_locks WHERE pid=$2 AND relation='public.service_teams'::regclass AND mode='RowExclusiveLock' AND NOT granted)").bind(reader_pid).bind(pid).fetch_one(&mut *tx).await.unwrap();if blocked{break}tokio::task::yield_now().await;}}).await.expect("writer waits for exact team fence");writer.await.unwrap();tx.rollback().await.unwrap();
        let mut fresh=db.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_records_request_team_sources(&mut fresh).await.unwrap();assert!(source.require_current(&mut fresh).await.is_err());fresh.rollback().await.unwrap();
        let mut saved=db.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();sqlx::query("SELECT pg_current_xact_id()").execute(&mut *saved).await.unwrap();sqlx::query("SAVEPOINT team_lock").execute(&mut *saved).await.unwrap();lock_records_request_team_sources(&mut saved).await.unwrap();let held=inspect_records_request_team_sources(&mut saved).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT team_lock").execute(&mut *saved).await.unwrap();assert!(held.require_current(&mut saved).await.is_err());saved.rollback().await.unwrap();
        let mut shadow=db.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();sqlx::query("CREATE TEMP TABLE service_team_members(team_id text)").execute(&mut *shadow).await.unwrap();lock_records_request_team_sources(&mut shadow).await.unwrap();assert!(inspect_records_request_team_sources(&mut shadow).await.is_err());shadow.rollback().await.unwrap();
        let mut rls=db.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();let bypass:bool=sqlx::query_scalar("SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user").fetch_one(&mut *rls).await.unwrap();assert!(!bypass);sqlx::raw_sql("ALTER TABLE public.service_team_members ENABLE ROW LEVEL SECURITY; ALTER TABLE public.service_team_members FORCE ROW LEVEL SECURITY").execute(&mut *rls).await.unwrap();let active:bool=sqlx::query_scalar("SELECT row_security_active('public.service_team_members'::regclass)").fetch_one(&mut *rls).await.unwrap();assert!(active);lock_records_request_team_sources(&mut rls).await.unwrap();assert!(inspect_records_request_team_sources(&mut rls).await.is_err());rls.rollback().await.unwrap();
        let mut bounded=db.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_records_request_team_sources(&mut bounded).await.unwrap();let current=inspect_records_request_team_sources(&mut bounded).await.unwrap();sqlx::query("SAVEPOINT team_bytes").execute(&mut *bounded).await.unwrap();
        sqlx::query("INSERT INTO public.service_teams(id,company_id,name,active,created_by,created_at,updated_at) SELECT repeat('x',1800)||n::text,'company','Budget team',1,'fixture-a',1,1 FROM generate_series(1,1200) n").execute(&mut *bounded).await.unwrap();
        let measured:(i64,i64)=sqlx::query_as("SELECT count(*)::bigint,COALESCE(sum(bytes),0)::bigint FROM (SELECT octet_length(to_jsonb(s)::text)::bigint bytes FROM (SELECT id,company_id,active FROM public.service_teams) s UNION ALL SELECT octet_length(to_jsonb(m)::text)::bigint FROM (SELECT team_id,company_id,user_id,responsibility FROM public.service_team_members) m) bounded").fetch_one(&mut *bounded).await.unwrap();assert_eq!(measured.0,1207);assert!(measured.1>2097152);let e=inspect_records_request_team_sources(&mut bounded).await.err().expect("byte limit");assert!(matches!(e,PgTransactionError::Business(ref e) if e.code=="SUPPORT_REQUEST_TEAM_SOURCE_LIMIT"));assert!(current.require_current(&mut bounded).await.is_err());sqlx::query("ROLLBACK TO SAVEPOINT team_bytes").execute(&mut *bounded).await.unwrap();current.require_current(&mut bounded).await.unwrap();
        sqlx::query("INSERT INTO public.service_teams(id,company_id,name,active,created_by,created_at,updated_at) SELECT 'budget-team-'||n::text,'company','Budget team',1,'fixture-a',1,1 FROM generate_series(1,9994) n").execute(&mut *bounded).await.unwrap();let measured:(i64,i64)=sqlx::query_as("SELECT count(*)::bigint,COALESCE(sum(bytes),0)::bigint FROM (SELECT octet_length(to_jsonb(s)::text)::bigint bytes FROM (SELECT id,company_id,active FROM public.service_teams) s UNION ALL SELECT octet_length(to_jsonb(m)::text)::bigint FROM (SELECT team_id,company_id,user_id,responsibility FROM public.service_team_members) m) bounded").fetch_one(&mut *bounded).await.unwrap();assert_eq!(measured.0,10001);assert!(measured.1<=2097152);let e=inspect_records_request_team_sources(&mut bounded).await.err().expect("row limit");assert!(matches!(e,PgTransactionError::Business(ref e) if e.code=="SUPPORT_REQUEST_TEAM_SOURCE_LIMIT"));bounded.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn support_service_membership_sql_matches_identity_assignment_source() {
    use erpx_server_identity_access::public::sales_recipient::{inspect_sales_assignment_join,lock_sales_recipient_sources};
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script=r#"if(process.version!=='v26.11.1')throw Error('PINNED_NODE_REQUIRED');const fs=await import('node:fs'),i=await import('./packages/server-identity_access/src/adapters/final-root-owner-projection-sources.ts'),text=fs.readFileSync('./packages/server-support/src/adapters/service-access.ts','utf8');const matches=[...text.matchAll(/export const serviceMemberSql\s*=\s*`([^`]+)`/g)];if(matches.length!==1)throw Error('MEMBERSHIP_TEMPLATE_CHANGED');const marker='${identity_accessProjectionSources.assignments}',template=matches[0][1];if(template.split(marker).length!==2)throw Error('ASSIGNMENT_PROJECTION_CHANGED');let sql='SELECT '+template.replace(marker,i.finalRootOwnerProjectionSources.assignments);if(sql.includes('${')||(sql.match(/\?/g)||[]).length!==4)throw Error('MEMBERSHIP_BINDS_CHANGED');let n=0;sql=sql.replace(/\?/g,()=>'$'+(++n));process.stdout.write(JSON.stringify({sql,projection:i.finalRootOwnerProjectionSources.assignments}));"#;
    let oracle=std::process::Command::new(std::env::var("ERPX_NODE_BIN").expect("pinned Node required")).current_dir(root).args(["--conditions=react-server","--input-type=module","-e",script]).output().unwrap();assert!(oracle.status.success(),"actual Support TS oracle: {}",String::from_utf8_lossy(&oracle.stderr));let ts:serde_json::Value=serde_json::from_slice(&oracle.stdout).unwrap();let original=ts["sql"].as_str().unwrap().to_owned();let projection=ts["projection"].as_str().unwrap().to_owned();
    review::account::with_system(|system|async move {
        let p=review::setup_with_finance(system,false).await;let db=&p.system.holdings[0];
        sqlx::raw_sql("INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version) VALUES('company','member-holding','local:member-holding','active','local',1),('company','member-branch','local:member-branch','active','local',1),('company','member-dept','local:member-dept','active','local',1),('company','member-revoked','local:member-revoked','active','local',1),('company','member-future','local:member-future','active','local',1),('company','member-expired','local:member-expired','active','local',1),('company','member-clock','local:member-clock','active','local',1),('foreign','member-foreign','local:member-foreign','active','local',1); INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,revoked_at,scope_kind,scope_node_id) SELECT 'assignment-'||u,u,'custom_finance_review','[]',c,CASE WHEN u='member-future' THEN floor(extract(epoch FROM clock_timestamp())*1000)::bigint+315360000000 ELSE 0 END,CASE WHEN u='member-expired' THEN floor(extract(epoch FROM clock_timestamp())*1000)::bigint-315360000000 ELSE 9000000000000 END,CASE WHEN u='member-revoked' THEN 1 ELSE NULL END,k,n FROM (VALUES ('member-holding','company','holding',NULL),('member-branch','company','branch','branch-1'),('member-dept','company','department','dept-1'),('member-revoked','company','holding',NULL),('member-future','company','holding',NULL),('member-expired','company','holding',NULL),('member-foreign','foreign','holding',NULL)) v(u,c,k,n)").execute(db.raw()).await.unwrap();
        let mut tx=db.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();let locks=lock_sales_recipient_sources(&mut tx).await.unwrap().bind().await.unwrap();let source=inspect_sales_assignment_join(&mut tx,&locks).await.unwrap();
        let needle=format!("({projection})");assert_eq!(original.matches(&needle).count(),1);let projected=format!("WITH {} {}",source.cte_sql(),original.replacen(&needle,"sales_recipient_assignments",1));
        for (company,user,branch,department,expected) in [("company","member-holding",None,None,true),("company","member-branch",Some("branch-1"),None,true),("company","member-branch",Some("branch-other"),None,false),("company","member-branch",None,None,false),("company","member-dept",None,Some("dept-1"),true),("company","member-dept",None,None,false),("company","member-revoked",None,None,false),("company","member-future",None,None,false),("company","member-expired",None,None,false),("company","member-foreign",None,None,false),("foreign","member-foreign",None,None,true)] {
            let direct:bool=sqlx::query_scalar(sqlx::AssertSqlSafe(original.as_str())).bind(company).bind(user).bind(branch).bind(department).fetch_one(&mut *tx).await.unwrap();let native:bool=sqlx::query_scalar(sqlx::AssertSqlSafe(projected.as_str())).bind(company).bind(user).bind(branch).bind(department).fetch_one(&mut *tx).await.unwrap();assert_eq!(direct,expected,"{user}");assert_eq!(native,direct,"{user}");source.require_current(&mut tx).await.unwrap();
        }
        sqlx::query("SAVEPOINT membership_revoke").execute(&mut *tx).await.unwrap();sqlx::query("UPDATE public.assignments SET revoked_at=floor(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE id='assignment-member-holding'").execute(&mut *tx).await.unwrap();let direct:bool=sqlx::query_scalar(sqlx::AssertSqlSafe(original.as_str())).bind("company").bind("member-holding").bind(None::<&str>).bind(None::<&str>).fetch_one(&mut *tx).await.unwrap();let native:bool=sqlx::query_scalar(sqlx::AssertSqlSafe(projected.as_str())).bind("company").bind("member-holding").bind(None::<&str>).bind(None::<&str>).fetch_one(&mut *tx).await.unwrap();assert!(!direct);assert_eq!(native,direct);source.require_current(&mut tx).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT membership_revoke").execute(&mut *tx).await.unwrap();
        // This uses PostgreSQL's real advancing clock, not a mocked timestamp.
        sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES('assignment-member-clock','member-clock','custom_finance_review','[]','company',0,floor(extract(epoch FROM clock_timestamp())*1000)::bigint+3000,'holding')").execute(&mut *tx).await.unwrap();
        let was_active:bool=sqlx::query_scalar(sqlx::AssertSqlSafe(projected.as_str())).bind("company").bind("member-clock").bind(None::<&str>).bind(None::<&str>).fetch_one(&mut *tx).await.unwrap();assert!(was_active,"the freshly-created assignment must initially be active");
        tokio::time::timeout(std::time::Duration::from_secs(6),async {loop{let active:bool=sqlx::query_scalar(sqlx::AssertSqlSafe(projected.as_str())).bind("company").bind("member-clock").bind(None::<&str>).bind(None::<&str>).fetch_one(&mut *tx).await.unwrap();if !active{break}tokio::time::sleep(std::time::Duration::from_millis(20)).await;}}).await.expect("database clock must expire assignment");
        let original_expired:bool=sqlx::query_scalar(sqlx::AssertSqlSafe(original.as_str())).bind("company").bind("member-clock").bind(None::<&str>).bind(None::<&str>).fetch_one(&mut *tx).await.unwrap();assert!(!original_expired);source.require_current(&mut tx).await.unwrap();tx.rollback().await.unwrap();
    }).await;
}
