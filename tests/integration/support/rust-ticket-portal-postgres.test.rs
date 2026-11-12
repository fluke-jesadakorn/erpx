#![allow(dead_code)]
//! Native Support customer ticket-portal workflow over leased PostgreSQL.
//!
//! Covers prepare -> independent decision -> response -> customer projection ->
//! acknowledgement with the real owner rows, receipts and package authority.
//! No TypeScript writer is invoked.
#[path = "../../support/finance/review-composition.rs"]
mod review;

use erpx_server_shared::public::Clock;
use erpx_server_support::public::ticket_portal::{
    PgTicketPortal, PortalActor, PortalCustomer, acknowledge, decide, prepare, respond,
};
use serde_json::json;

#[tokio::test]
async fn ticket_portal_prepare_decide_respond_project_and_acknowledge() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        let pool = &p.system.holdings[0];
        let now = p.system.scenario.clock.now().get();
        let expires = now + 3_600_000;
        sqlx::raw_sql(
            "INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','custom_support_manager','Synthetic Support manager',false,1,'active') ON CONFLICT DO NOTHING;\
             INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_support_manager',1,'[\"records.read\",\"records.write\",\"support.read\",\"support.create\",\"support.work\",\"support.manage\",\"commerce.read\",\"module.support.read\"]','[]','[\"holding\",\"branch\",\"department\"]','synthetic-fixture',0) ON CONFLICT DO NOTHING;\
             INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,version,created_by,created_at,updated_at)VALUES('person-a','fixture-a','company','Synthetic Portal Preparer','TH','active',1,'synthetic-fixture',0,0),('person-b','fixture-b','company','Synthetic Portal Reviewer','TH','active',1,'synthetic-fixture',0,0) ON CONFLICT DO NOTHING;\
             INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,end_date,job_title,currency,version,created_by,created_at,updated_at)VALUES('engagement-a','person-a','company','employee','salaried','active','2026-01-01',NULL,'Support','THB',1,'synthetic-fixture',0,0),('engagement-b','person-b','company','employee','salaried','active','2026-01-01',NULL,'Support','THB',1,'synthetic-fixture',0,0) ON CONFLICT DO NOTHING;\
             INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version)VALUES('company','fixture-a','local:fixture-a','person-a','active','local',1),('company','fixture-b','local:fixture-b','person-b','active','local',1) ON CONFLICT DO NOTHING;",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        for (user, assignment) in [
            ("fixture-a", "portal-support-a"),
            ("fixture-b", "portal-support-b"),
        ] {
            sqlx::query(
                "INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind)VALUES($1,$2,'custom_support_manager','[\"support\"]','company',$3,$4,3,'holding')",
            )
            .bind(assignment)
            .bind(user)
            .bind(0_i64)
            .bind(9_007_199_254_740_991_i64)
            .execute(pool.raw())
            .await
            .unwrap();
        }
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query(
            "INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at)VALUES\
             ('company','support',1,'active',0,$1,1,'Synthetic reviewed support purchase','public synthetic evidence',$2),\
             ('company','support-portal',1,'active',0,$1,1,'Synthetic reviewed portal purchase','public synthetic evidence',$2)\
             ON CONFLICT(company_id,package_id) DO NOTHING",
        )
        .bind(expires)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)\
             SELECT 'portal-support-grant','company','support','fixture-a','synthetic-request',NULL::jsonb,\
               jsonb_build_object('companyId','company','packageId','support','packageVersion',1,'state','active','startsAt',0,'endsAt',$1,'version',1,'reason','Synthetic reviewed support purchase','evidence','public synthetic evidence'),$2 \
             UNION ALL \
             SELECT 'portal-portal-grant','company','support-portal','fixture-a','synthetic-request',NULL::jsonb,\
               jsonb_build_object('companyId','company','packageId','support-portal','packageVersion',1,'state','active','startsAt',0,'endsAt',$1,'version',1,'reason','Synthetic reviewed portal purchase','evidence','public synthetic evidence'),$2",
        )
        .bind(expires)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO support_tickets(id,company_id,branch_id,department_id,source_kind,title,issue,owner_id,due_date,status,version,created_by,created_at,updated_at)VALUES\
             ('ticket-portal-1','company','branch',NULL,'general','Reviewed portal ticket','Private original issue','fixture-a','2026-11-20','open',3,'fixture-a',$1,$1)",
        )
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let preparer = PortalActor {
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
            user_id: "fixture-a".into(),
            assignment_id: "portal-support-a".into(),
            role: "custom_support_manager".into(),
            support_manage: true,
        };
        let reviewer = PortalActor {
            user_id: "fixture-b".into(),
            assignment_id: "portal-support-b".into(),
            ..preparer.clone()
        };
        let customer = PortalCustomer {
            user_id: "portal-customer-1".into(),
            email: "portal-customer@example.test".into(),
            holding_id: Some("h001".into()),
        };
        let authority = json!({"companyId":"company","kind":"synthetic-support-manager"}).to_string();
        let prepare_body = json!({
            "ticketId":"ticket-portal-1",
            "ticketVersion":3,
            "customerName":"Synthetic Portal Customer",
            "email":"portal-customer@example.test",
            "subject":"Reviewed customer portal update",
            "summary":"Only this reviewed summary leaves the internal Helpdesk.",
            "expiresAt":expires,
            "reason":"Current Support manager prepared the reviewed customer access"
        });
        let review_body = json!({
            "expectedVersion":1,
            "action":"approve",
            "reason":"A different current Support manager approved the reviewed access"
        });
        let response_body = json!({
            "expectedBindingVersion":2,
            "expectedResponseVersion":0,
            "message":"The reviewed customer response is published.",
            "reason":"A different current Support manager published the response"
        });
        let acknowledgement_body = json!({
            "expectedBindingVersion":2,
            "responseId":"support-portal-response-portal-respond-1",
            "responseVersion":1,
            "note":"The verified customer acknowledged the exact published response"
        });

        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        {
            let mut portal = PgTicketPortal::new(&mut tx, authority, now);
            let prepared = prepare(
                &mut portal,
                &preparer,
                &prepare_body,
                "portal-prepare-1",
            )
            .await
            .unwrap();
            assert_eq!(
                prepared,
                json!({"id":"support-portal-portal-prepare-1","status":"draft","version":1})
            );
            let same_person = decide(
                &mut portal,
                &preparer,
                "support-portal-portal-prepare-1",
                &review_body,
                "portal-approve-same-person",
            )
            .await
            .unwrap_err();
            assert_eq!(
                same_person.code,
                "INDEPENDENT_SUPPORT_PORTAL_REVIEW_REQUIRED"
            );
            let approved = decide(
                &mut portal,
                &reviewer,
                "support-portal-portal-prepare-1",
                &review_body,
                "portal-approve-1",
            )
            .await
            .unwrap();
            assert_eq!(
                approved,
                json!({"id":"support-portal-portal-prepare-1","status":"approved","version":2})
            );
            let response = respond(
                &mut portal,
                &reviewer,
                "support-portal-portal-prepare-1",
                &response_body,
                "portal-respond-1",
            )
            .await
            .unwrap();
            assert_eq!(
                response,
                json!({
                    "id":"support-portal-response-portal-respond-1",
                    "bindingId":"support-portal-portal-prepare-1",
                    "version":1,
                    "status":"published",
                    "message":"The reviewed customer response is published.",
                    "publishedAt":now
                })
            );
            let projection = portal.projection(&customer).await.unwrap();
            assert_eq!(projection.schema_available, true);
            assert_eq!(projection.items.len(), 1);
            assert_eq!(projection.items[0].id, "support-portal-portal-prepare-1");
            assert_eq!(
                projection.items[0]
                    .response
                    .as_ref()
                    .map(|response| response.message.as_str()),
                Some("The reviewed customer response is published.")
            );
            let acknowledged = acknowledge(
                &mut portal,
                &customer,
                "support-portal-portal-prepare-1",
                &acknowledgement_body,
                "portal-ack-1",
            )
            .await
            .unwrap();
            assert_eq!(
                acknowledged,
                json!({
                    "id":"support-portal-ack-portal-ack-1",
                    "bindingId":"support-portal-portal-prepare-1",
                    "responseId":"support-portal-response-portal-respond-1",
                    "responseVersion":1,
                    "status":"acknowledged",
                    "createdAt":now
                })
            );
            let replayed = prepare(
                &mut portal,
                &preparer,
                &prepare_body,
                "portal-prepare-1",
            )
            .await
            .unwrap();
            assert_eq!(replayed, prepared);
        }
        tx.commit().await.unwrap();

        let binding: (String, i64) = sqlx::query_as(
            "SELECT status,version FROM support_ticket_portal_bindings WHERE id='support-portal-portal-prepare-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(binding, ("approved".to_string(), 2));
        let responses: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint FROM support_ticket_portal_responses WHERE binding_id='support-portal-portal-prepare-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(responses, 1);
        let acks: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint FROM support_ticket_portal_acknowledgements WHERE binding_id='support-portal-portal-prepare-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(acks, 1);
        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint FROM command_receipts WHERE record_id='support-portal-portal-prepare-1' AND operation IN ('support.ticket-portal.prepare','support.ticket-portal.approve','support.ticket-portal.respond')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipts, 3);
    })
    .await;
}
