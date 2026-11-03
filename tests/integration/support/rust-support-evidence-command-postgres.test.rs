//! Actual Support evidence bind/revoke commands over leased PostgreSQL with the
//! Files set/link/attachment rows in the same transaction.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod review;
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_identity_access::public::work_context::{
    ExpectedWorkSelection, HoldingWorkspaceRequest,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{AssignmentId, Clock, CommandKey, CompanyId, HoldingId};
use erpx_server_support::public::support_evidence::{EvidenceBind, EvidenceRevoke};
use erpx_server_workflow::public::commands::CommandJson;
use erpx_service::support_evidence_command::SupportEvidenceCommand;
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
) -> (review::Prepared, SupportEvidenceCommand<TestClock>) {
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
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at)VALUES('company','support',1,'active',0,$1,1,'Synthetic reviewed support purchase','public synthetic evidence',$2) ON CONFLICT DO NOTHING")
        .bind(grant_expires)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT 'support-evidence-origin-grant','company','support','fixture-a','synthetic-request',NULL,jsonb_build_object('companyId','company','packageId','support','packageVersion',1,'state','active','startsAt',0,'endsAt',$1,'version',1,'reason','Synthetic reviewed support purchase','evidence','public synthetic evidence'),$2 ON CONFLICT DO NOTHING")
        .bind(grant_expires)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let command = SupportEvidenceCommand {
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
    status: &str,
    owner: &str,
) {
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES('branch','branch','support','Support Branch','company','company','branch','active') ON CONFLICT DO NOTHING;\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at)VALUES('evidence-customer','company','customer','EVID-C','Evidence Customer','THB',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version)VALUES('evidence-warehouse','company','branch','EVID-WH','Evidence Warehouse','active',1) ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,version,created_by,created_at,updated_at)VALUES('evidence-order','company','branch','sales','SO-EVID-1','evidence-customer','evidence-warehouse','THB','2026-11-01','2026-11-10',10000,0,1,'fixture-a',0,0) ON CONFLICT DO NOTHING;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query("INSERT INTO support_tickets(id,company_id,branch_id,department_id,order_id,customer_id,title,issue,owner_id,due_date,status,resolution,source_kind,version,created_by,created_at,updated_at)VALUES($1,'company','branch',NULL,'evidence-order','evidence-customer','Evidence customer case','Customer case for support evidence.',$2,'2026-11-10',$3,CASE WHEN $3='resolved' THEN 'Resolved for evidence lock' ELSE NULL END,'sales_order',1,'fixture-a',0,0) ON CONFLICT DO NOTHING")
        .bind(id)
        .bind(owner)
        .bind(status)
        .execute(pool.raw())
        .await
        .unwrap();
}

async fn seed_evidence(
    pool: &erpx_server_runtime::public::postgres::DatabasePool,
    ticket: &str,
    suffix: &str,
    legal_hold: i64,
) -> (String, String) {
    let set_id = format!("evidence-set-{suffix}");
    let link_id = format!("evidence-link-{suffix}");
    let attachment_id = format!("evidence-attachment-{suffix}");
    let upload_id = format!("evidence-upload-{suffix}");
    sqlx::query("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at,owner_version,company_id)VALUES($1,'support_ticket',$2,'internal_evidence','evidence.pdf','application/pdf',128,128,$3,$4,$5,'ready','fixture-a',$6,0,0,1,'company') ON CONFLICT DO NOTHING")
        .bind(&upload_id)
        .bind(ticket)
        .bind("a".repeat(64))
        .bind(format!("staging/{suffix}"))
        .bind(format!("final/{suffix}"))
        .bind(9_007_199_254_740_991_i64)
        .execute(pool.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,legal_hold,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at)VALUES($1,$2,'support_ticket',$3,'internal_evidence',$4,1,'evidence.pdf','application/pdf',128,$5,'fixture-a',$6,0,'company','application/pdf','available',0,0) ON CONFLICT DO NOTHING")
        .bind(&attachment_id)
        .bind(&upload_id)
        .bind(ticket)
        .bind(format!("final/{suffix}"))
        .bind("a".repeat(64))
        .bind(legal_hold)
        .execute(pool.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO attachment_sets(id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at)VALUES($1,'company','branch',NULL,'support_ticket',$2,'action','support-evidence:internal_evidence','internal_evidence',1,1,'internal','{}',1,10,'draft','fixture-a',0,0) ON CONFLICT DO NOTHING")
        .bind(&set_id)
        .bind(ticket)
        .execute(pool.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at)VALUES($1,$2,$3,'draft','fixture-a',0,$4) ON CONFLICT DO NOTHING")
        .bind(&link_id)
        .bind(&set_id)
        .bind(&attachment_id)
        .bind(9_007_199_254_740_991_i64)
        .execute(pool.raw())
        .await
        .unwrap();
    (set_id, attachment_id)
}

fn bind(set_id: &str) -> EvidenceBind {
    EvidenceBind {
        expected_version: 1,
        attachment_set_ids: vec![set_id.into()],
        reason: "Internal case evidence".into(),
    }
}

#[tokio::test]
async fn support_evidence_bind_revoke_and_legal_hold() {
    review::account::with_system(|system| async move {
        let (p, command) = support_setup(system).await;
        let pool = &p.system.holdings[0];
        seed_ticket(pool, "evidence-ticket", "open", "fixture-a").await;
        let (set_id, attachment_id) =
            seed_evidence(pool, "evidence-ticket", "one", 0).await;

        let input = bind(&set_id);
        let result = command
            .bind(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                "evidence-ticket".into(),
                input.clone(),
                CommandJson::from_serializable(&input).unwrap(),
                CommandKey::new("evidence-bind-1").unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            result.as_str(),
            r#"{"id":"evidence-ticket","version":2}"#
        );
        let set: (String, String) = sqlx::query_as(
            "SELECT state,bound_command_id FROM attachment_sets WHERE id=$1",
        )
        .bind(&set_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(set, ("bound".into(), "evidence-bind-1".into()));
        let link: String = sqlx::query_scalar("SELECT state FROM attachment_links WHERE set_id=$1")
            .bind(&set_id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(link, "bound");
        let attachment_event: String = sqlx::query_scalar(
            "SELECT action FROM attachment_events WHERE set_id=$1",
        )
        .bind(&set_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(attachment_event, "set.bound");
        let ticket_event: (String, i64) = sqlx::query_as(
            "SELECT action,version FROM support_ticket_events WHERE ticket_id='evidence-ticket' ORDER BY version DESC LIMIT 1",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(ticket_event, ("attachment-bind".into(), 2));

        let revoke = EvidenceRevoke {
            expected_version: 2,
            attachment_id: attachment_id.clone(),
            reason: "Wrong simulated screenshot replaced".into(),
        };
        let revoke_wire = r#"{"action":"revoke-evidence","expectedVersion":2,"attachmentId":"__ID__","reason":"Wrong simulated screenshot replaced"}"#
            .replace("__ID__", &attachment_id);
        command
            .revoke(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                "evidence-ticket".into(),
                revoke,
                CommandJson::parse(&revoke_wire).unwrap(),
                CommandKey::new("evidence-revoke-1").unwrap(),
            )
            .await
            .unwrap();
        let revoked: Option<i64> = sqlx::query_scalar("SELECT revoked_at FROM attachments WHERE id=$1")
            .bind(&attachment_id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert!(revoked.is_some());
        let final_event: (String, i64) = sqlx::query_as(
            "SELECT action,version FROM support_ticket_events WHERE ticket_id='evidence-ticket' ORDER BY version DESC LIMIT 1",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(final_event, ("attachment-revoke".into(), 3));

        // A held attachment cannot be withdrawn.
        seed_ticket(pool, "evidence-held", "open", "fixture-a").await;
        let (held_set, held_attachment) =
            seed_evidence(pool, "evidence-held", "held", 1).await;
        let held = bind(&held_set);
        command
            .bind(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                "evidence-held".into(),
                held.clone(),
                CommandJson::from_serializable(&held).unwrap(),
                CommandKey::new("evidence-bind-held").unwrap(),
            )
            .await
            .unwrap();
        match command
            .revoke(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                "evidence-held".into(),
                EvidenceRevoke {
                    expected_version: 2,
                    attachment_id: held_attachment,
                    reason: "Held evidence must remain readable".into(),
                },
                CommandJson::parse(
                    r#"{"action":"revoke-evidence","expectedVersion":2,"attachmentId":"evidence-attachment-held","reason":"Held evidence must remain readable"}"#,
                )
                .unwrap(),
                CommandKey::new("evidence-revoke-held").unwrap(),
            )
            .await
        {
            Err(PgTransactionError::Business(error)) => assert_eq!(error.code, "CONFLICT"),
            _ => panic!("legal hold must fail"),
        }

        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id IN ('evidence-bind-1','evidence-revoke-1','evidence-bind-held','evidence-revoke-held')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipts, 3);
    })
    .await;
}

#[tokio::test]
async fn support_evidence_rejects_stale_locked_and_non_owner() {
    review::account::with_system(|system| async move {
        let (p, command) = support_setup(system).await;
        let pool = &p.system.holdings[0];
        seed_ticket(pool, "evidence-stale", "open", "fixture-a").await;
        let (set_id, _) = seed_evidence(pool, "evidence-stale", "stale", 0).await;
        match command
            .bind(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                "evidence-stale".into(),
                EvidenceBind {
                    expected_version: 9,
                    attachment_set_ids: vec![set_id.clone()],
                    reason: "Stale evidence".into(),
                },
                CommandJson::parse(
                    r#"{"action":"bind-evidence","expectedVersion":9,"attachmentSetIds":["evidence-set-stale"],"reason":"Stale evidence"}"#,
                )
                .unwrap(),
                CommandKey::new("evidence-stale-bind").unwrap(),
            )
            .await
        {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "SUPPORT_EVIDENCE_CHANGED")
            }
            _ => panic!("stale evidence must fail"),
        }
        seed_ticket(pool, "evidence-locked", "resolved", "fixture-a").await;
        let (locked_set, _) = seed_evidence(pool, "evidence-locked", "locked", 0).await;
        match command
            .bind(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                "evidence-locked".into(),
                EvidenceBind {
                    expected_version: 1,
                    attachment_set_ids: vec![locked_set.clone()],
                    reason: "Resolved evidence".into(),
                },
                CommandJson::parse(
                    r#"{"action":"bind-evidence","expectedVersion":1,"attachmentSetIds":["evidence-set-locked"],"reason":"Resolved evidence"}"#,
                )
                .unwrap(),
                CommandKey::new("evidence-locked-bind").unwrap(),
            )
            .await
        {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "SUPPORT_EVIDENCE_LOCKED")
            }
            _ => panic!("resolved evidence must fail"),
        }
        seed_ticket(pool, "evidence-foreign", "open", "fixture-b").await;
        let (foreign_set, _) = seed_evidence(pool, "evidence-foreign", "foreign", 0).await;
        match command
            .bind(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("support-fixture-a")),
                "evidence-foreign".into(),
                EvidenceBind {
                    expected_version: 1,
                    attachment_set_ids: vec![foreign_set.clone()],
                    reason: "Foreign evidence".into(),
                },
                CommandJson::parse(
                    r#"{"action":"bind-evidence","expectedVersion":1,"attachmentSetIds":["evidence-set-foreign"],"reason":"Foreign evidence"}"#,
                )
                .unwrap(),
                CommandKey::new("evidence-foreign-bind").unwrap(),
            )
            .await
        {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "SUPPORT_EVIDENCE_OWNER")
            }
            _ => panic!("non-owner evidence must fail"),
        }
    })
    .await;
}

#[tokio::test]
async fn support_evidence_http_route_accepts_real_session_and_rendered_context() {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use erpx_service::field_service_reservation_read_http::FieldServiceReadScopeHeaders;
    use erpx_service::support_evidence_http::{
        SupportEvidenceHttpTarget, support_evidence_http_router,
    };
    use tower::ServiceExt;
    review::account::with_system(|system| async move {
        let (p, command) = support_setup(system).await;
        let pool = &p.system.holdings[0];
        seed_ticket(pool, "evidence-http", "open", "fixture-a").await;
        let (set_id, attachment_id) = seed_evidence(pool, "evidence-http", "http", 0).await;
        let router = support_evidence_http_router(
            Arc::new(review::account::fixture::account::policy()),
            Arc::new(FieldServiceReadScopeHeaders),
            vec![SupportEvidenceHttpTarget {
                host: "acme.example.test".into(),
                holding_slug: "acme".into(),
                command: Arc::new(command),
            }],
        )
        .unwrap();
        let bind = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/support/evidence")
            .header("host", "acme.example.test")
            .header("cookie", format!("__Host-erpx-product={}", p.maker_token.expose()))
            .header("origin", "https://acme.example.test")
            .header("idempotency-key", "evidence-http-bind")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "support-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .header("content-type", "application/json")
            .body(Body::from(format!(
                r#"{{"id":"evidence-http","action":"bind-evidence","expectedVersion":1,"attachmentSetIds":["{set_id}"],"reason":"HTTP evidence"}}"#
            )))
            .unwrap();
        let response = router.clone().oneshot(bind).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(status, 200);
        assert_eq!(value["data"]["version"], 2);
        let revoke = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/support/evidence")
            .header("host", "acme.example.test")
            .header("cookie", format!("__Host-erpx-product={}", p.maker_token.expose()))
            .header("origin", "https://acme.example.test")
            .header("idempotency-key", "evidence-http-revoke")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "support-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .header("content-type", "application/json")
            .body(Body::from(format!(
                r#"{{"id":"evidence-http","action":"revoke-evidence","expectedVersion":2,"attachmentId":"{attachment_id}","reason":"HTTP withdrawal"}}"#
            )))
            .unwrap();
        let revoke_response = router.oneshot(revoke).await.unwrap();
        let revoke_status = revoke_response.status();
        let revoke_body = to_bytes(revoke_response.into_body(), 64 * 1024)
            .await
            .unwrap();
        let revoke_value: serde_json::Value = serde_json::from_slice(&revoke_body).unwrap();
        assert_eq!(revoke_status, 200);
        assert_eq!(revoke_value["data"]["version"], 3);
        let persisted: (i64, Option<i64>) = sqlx::query_as(
            "SELECT t.version,a.revoked_at FROM support_tickets t JOIN attachments a ON a.id=$1 WHERE t.id='evidence-http'",
        )
        .bind(&attachment_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(persisted.0, 3);
        assert!(persisted.1.is_some());
    })
    .await;
}
