#![allow(dead_code)]
//! Actual Support ticket command over leased PostgreSQL, current Account/IAM
//! authority and the Workflow receipt/audit/outbox. No TS writer is invoked.
#[path = "../../support/finance/review-composition.rs"]
mod review;
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_identity_access::public::work_context::{
    ExpectedWorkSelection, HoldingWorkspaceRequest,
};
use erpx_server_shared::public::{AssignmentId, Clock, CommandKey, CompanyId, HoldingId};
use erpx_server_support::public::ticket::TicketChange;
use erpx_server_workflow::public::commands::CommandJson;
use erpx_service::support_ticket_command::{SupportTicketCommand, SupportTicketCreate};
use std::sync::Arc;

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
            "INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','support_agent','Synthetic Support agent',false,1,'active'),('*','support_manager','Synthetic Support manager',false,1,'active') ON CONFLICT DO NOTHING;\
             INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','support_agent',1,'[\"records.read\",\"records.write\",\"support.read\",\"support.create\",\"support.work\",\"commerce.read\"]','[]','[\"holding\",\"branch\",\"department\"]','synthetic-fixture',0),('*','support_manager',1,'[\"records.read\",\"records.write\",\"support.read\",\"support.create\",\"support.work\",\"support.manage\",\"commerce.read\"]','[]','[\"holding\",\"branch\",\"department\"]','synthetic-fixture',0) ON CONFLICT DO NOTHING;\
             INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,version,created_by,created_at,updated_at)VALUES('person-a','fixture-a','company','Synthetic Agent','TH','active',1,'synthetic-fixture',0,0),('person-b','fixture-b','company','Synthetic Manager','TH','active',1,'synthetic-fixture',0,0) ON CONFLICT DO NOTHING;\
             INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,end_date,job_title,currency,version,created_by,created_at,updated_at)VALUES('engagement-a','person-a','company','employee','salaried','active','2026-01-01',NULL,'Support','THB',1,'synthetic-fixture',0,0),('engagement-b','person-b','company','employee','salaried','active','2026-01-01',NULL,'Support','THB',1,'synthetic-fixture',0,0) ON CONFLICT DO NOTHING;",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        for (user, assignment, role) in [
            ("fixture-a", "support-fixture-a", "support_agent"),
            ("fixture-b", "support-fixture-b", "support_manager"),
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
                .bind(now - 1_000)
                .bind(expires)
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
            clock: p.system.scenario.clock.clone(),
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
    })
    .await;
}

