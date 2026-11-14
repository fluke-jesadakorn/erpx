//! Native Quality HTTP ingress over the actual Account session, current IAM
//! authority and PostgreSQL. Exercises bounded reads, the complete-history
//! page contract and the owner cancellation command through the composed
//! router, not a reference backend.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod review;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_quality::public::quality_http::{QualityHttpTarget, quality_router};
use erpx_server_shared::public::{Clock, CompanyId, HoldingId};
use erpx_service::field_service_reservation_read_http::FieldServiceReadScopeHeaders;
use erpx_service::quality::{QualityBackend, QualityConfiguration, QualityService};
use std::sync::Arc;
use tower::ServiceExt;

fn cookie(p: &review::Prepared, reviewer: bool) -> String {
    let token = if reviewer {
        &p.reviewer_token
    } else {
        &p.maker_token
    };
    format!("__Host-erpx-product={}", token.expose())
}

fn read_request(path: &str, p: &review::Prepared, reviewer: bool, company: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .header("host", "acme.example.test")
        .header("cookie", cookie(p, reviewer))
        .header("x-erpx-context-company", company)
        .header(
            "x-erpx-context-assignment",
            if reviewer {
                "finance-fixture-b"
            } else {
                "finance-fixture-a"
            },
        )
        .header("x-erpx-context-revision", "7")
        .header("x-erpx-context-assignment-version", "4")
        .body(Body::empty())
        .unwrap()
}

fn command_request(
    path: &str,
    p: &review::Prepared,
    reviewer: bool,
    key: &str,
    body: &str,
) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("host", "acme.example.test")
        .header("origin", "https://acme.example.test")
        .header("cookie", cookie(p, reviewer))
        .header("idempotency-key", key)
        .header("x-erpx-context-company", "company")
        .header(
            "x-erpx-context-assignment",
            if reviewer {
                "finance-fixture-b"
            } else {
                "finance-fixture-a"
            },
        )
        .header("x-erpx-context-revision", "7")
        .header("x-erpx-context-assignment-version", "4")
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

async fn json(response: axum::response::Response) -> serde_json::Value {
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

async fn grant_quality(p: &review::Prepared) {
    sqlx::raw_sql(
        "INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) \
         VALUES('*','custom_finance_review',2,'[\"finance.read\",\"quality.read\",\"quality.hold\",\"quality.inspect\",\"quality.release\",\"inventory.approve\",\"module.quality.read\"]','[]','[\"holding\"]','fixture',0);\
         UPDATE iam_roles SET published_revision=2 WHERE company_id='*' AND id='custom_finance_review';\
         UPDATE assignments SET modules_json='[\"finance\",\"quality\"]',version=4 WHERE id IN ('finance-fixture-a','finance-fixture-b');\
         UPDATE active_work_contexts SET assignment_version=4 WHERE assignment_id IN ('finance-fixture-a','finance-fixture-b')",
    )
    .execute(p.system.holdings[0].raw())
    .await
    .unwrap();
    let now = p.system.scenario.clock.now().get();
    let expires = now + 3_600_000;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) \
         VALUES('company','supply-quality',1,'active',0,{expires},1,'Synthetic reviewed Quality purchase','public synthetic evidence',{now}),\
                ('company','supply',1,'active',0,{expires},1,'Synthetic reviewed supply dependency','public synthetic evidence',{now}),\
                ('company','finance',1,'active',0,{expires},1,'Synthetic reviewed finance dependency','public synthetic evidence',{now});\
         INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) \
         SELECT 'quality-origin-'||package_id,company_id,package_id,'fixture-a','synthetic-request',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),{now} \
         FROM company_package_grants WHERE company_id='company' AND package_id IN ('supply-quality','supply','finance')"
    )))
    .execute(p.system.holdings[0].raw())
    .await
    .unwrap();
}

async fn seed_quality(p: &review::Prepared) {
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) \
         VALUES('branch','branch','branch','Branch','company','company','branch','active') ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) \
         VALUES('warehouse','company','branch','W1','Quality warehouse') ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode) \
         VALUES('product','company','P1','Quality product',1000,'none') ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor) \
         VALUES('warehouse','product',5000,1000) ON CONFLICT DO NOTHING;\
         INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,created_at,updated_at,quantity_scope,version) \
         VALUES('history-hold','company','branch','warehouse','product','Damaged packaging requires independent review','2026-11-10','held','maker',0,0,'whole',209),\
               ('cancel-hold','company','branch','warehouse','product','Incorrect quality scope requires independent review','2026-11-10','held','maker',0,0,'whole',1);\
         INSERT INTO inventory_quality_cancellations(id,hold_id,version,action,reason,actor_id,created_at) \
         SELECT 'history-'||g,'history-hold',g,CASE WHEN g%3=0 THEN 'approve' WHEN g%3=1 THEN 'request' ELSE 'reject' END,'Historical cancellation reason '||g,'maker',g \
         FROM generate_series(1,205) g",
    )
    .execute(p.system.holdings[0].raw())
    .await
    .unwrap();
}

/// The prepared authorizer owns the actual Account/IAM control database.
fn prepared_router(p: &review::Prepared) -> axum::Router {
    let policy = review::account::fixture::account::policy();
    let service = QualityService::new(
        p.authorizer.clone(),
        QualityConfiguration::new(
            HoldingId::new("h001").unwrap(),
            CompanyId::new("company").unwrap(),
        ),
        p.system.scenario.clock.clone(),
    );
    let backend = QualityBackend::new(
        Arc::new(policy),
        Arc::new(FieldServiceReadScopeHeaders),
        Arc::new(service),
    );
    quality_router(vec![QualityHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: Arc::new(backend),
    }])
    .unwrap()
}

#[tokio::test]
async fn quality_http_reads_history_and_cancels_over_the_real_session() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        grant_quality(&p).await;
        seed_quality(&p).await;
        let app = prepared_router(&p);

        let workspace = app
            .clone()
            .oneshot(read_request(
                "/api/v1/holdings/acme/quality",
                &p,
                false,
                "company",
            ))
            .await
            .unwrap();
        let workspace_status = workspace.status();
        let workspace = json(workspace).await;
        assert_eq!(workspace_status, StatusCode::OK, "{workspace}");
        assert_eq!(workspace["data"]["holds"].as_array().unwrap().len(), 2);
        assert_eq!(workspace["data"]["canHold"], true);

        let core = app
            .clone()
            .oneshot(read_request(
                "/api/v1/holdings/acme/quality/history-hold/detail-core",
                &p,
                false,
                "company",
            ))
            .await
            .unwrap();
        let core_status = core.status();
        let core = json(core).await;
        assert_eq!(core_status, StatusCode::OK, "{core}");
        assert_eq!(core["data"]["hold"]["id"], "history-hold");
        assert_eq!(core["data"]["cancellationHistory"], "paged");
        assert!(core["data"]["cancellations"].is_null());

        let first = app
            .clone()
            .oneshot(read_request(
                "/api/v1/holdings/acme/quality/history-hold/cancellations",
                &p,
                false,
                "company",
            ))
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::OK);
        let first = json(first).await;
        let revision = first["data"]["historyRevision"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(revision.starts_with("qch1:"));
        assert_eq!(first["data"]["items"].as_array().unwrap().len(), 100);
        assert_eq!(first["data"]["items"][0]["version"], 205);
        assert_eq!(first["data"]["nextBefore"], 106);
        assert_eq!(first["data"]["headVersion"], 209);

        let second = app
            .clone()
            .oneshot(read_request(
                &format!(
                    "/api/v1/holdings/acme/quality/history-hold/cancellations?before=106&head=209&revision={revision}"
                ),
                &p,
                false,
                "company",
            ))
            .await
            .unwrap();
        assert_eq!(second.status(), StatusCode::OK);
        let second = json(second).await;
        assert_eq!(second["data"]["items"].as_array().unwrap().len(), 100);
        assert_eq!(second["data"]["items"][0]["version"], 105);
        assert_eq!(second["data"]["nextBefore"], 6);

        let stale = app
            .clone()
            .oneshot(read_request(
                &format!(
                    "/api/v1/holdings/acme/quality/history-hold/cancellations?before=106&head=210&revision={revision}"
                ),
                &p,
                false,
                "company",
            ))
            .await
            .unwrap();
        assert_eq!(stale.status(), StatusCode::CONFLICT);
        assert_eq!(
            json(stale).await["error"]["code"],
            "QUALITY_HISTORY_CHANGED"
        );

        let malformed = app
            .clone()
            .oneshot(read_request(
                "/api/v1/holdings/acme/quality/history-hold/cancellations?before=106",
                &p,
                false,
                "company",
            ))
            .await
            .unwrap();
        assert_eq!(malformed.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            json(malformed).await["error"]["code"],
            "QUALITY_HISTORY_CURSOR_INVALID"
        );

        // The incoming Company label is not authority. Move the actual selected
        // work context to another Company and prove the owner read closes.
        sqlx::query("UPDATE active_work_contexts SET company_id='foreign-company' WHERE assignment_id='finance-fixture-a'")
            .execute(p.system.holdings[0].raw())
            .await
            .unwrap();
        let denied = app
            .clone()
            .oneshot(read_request(
                "/api/v1/holdings/acme/quality",
                &p,
                false,
                "foreign-company",
            ))
            .await
            .unwrap();
        let denied_status = denied.status();
        let denied = json(denied).await;
        assert_eq!(denied_status, StatusCode::CONFLICT, "{denied}");
        assert_eq!(denied["error"]["code"], "WORK_ROLE_REQUIRED", "{denied}");
        sqlx::query("UPDATE active_work_contexts SET company_id='company' WHERE assignment_id='finance-fixture-a'")
            .execute(p.system.holdings[0].raw())
            .await
            .unwrap();
        let restored = app
            .clone()
            .oneshot(read_request(
                "/api/v1/holdings/acme/quality",
                &p,
                false,
                "company",
            ))
            .await
            .unwrap();
        assert_eq!(restored.status(), StatusCode::OK);

        sqlx::query("UPDATE assignments SET valid_from=0,valid_until=9007199254740991 WHERE id IN ('finance-fixture-a','finance-fixture-b')")
            .execute(p.system.holdings[0].raw())
            .await
            .unwrap();
        let requested = app
            .clone()
            .oneshot(command_request(
                "/api/v1/holdings/acme/quality/cancel-hold/cancel",
                &p,
                false,
                "quality-history-request",
                r#"{"expectedVersion":1,"action":"request","reason":"The hold was opened against the wrong quality scope."}"#,
            ))
            .await
            .unwrap();
        assert_eq!(requested.status(), StatusCode::OK);
        let requested = json(requested).await;
        assert_eq!(requested["data"]["status"], "cancel_pending");
        assert_eq!(requested["data"]["version"], 2);

        let self_review = app
            .clone()
            .oneshot(command_request(
                "/api/v1/holdings/acme/quality/cancel-hold/cancel",
                &p,
                false,
                "quality-history-self-review",
                r#"{"expectedVersion":2,"action":"approve","reason":"Approving my own cancellation request."}"#,
            ))
            .await
            .unwrap();
        assert_eq!(self_review.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            json(self_review).await["error"]["code"],
            "INDEPENDENT_CANCELLATION_REQUIRED"
        );

        let approved = app
            .clone()
            .oneshot(command_request(
                "/api/v1/holdings/acme/quality/cancel-hold/cancel",
                &p,
                true,
                "quality-history-approve",
                r#"{"expectedVersion":2,"action":"approve","reason":"Independent review confirms the incorrect quality scope."}"#,
            ))
            .await
            .unwrap();
        assert_eq!(approved.status(), StatusCode::OK);
        let approved = json(approved).await;
        assert_eq!(approved["data"]["status"], "cancelled");
        assert_eq!(approved["data"]["version"], 3);

        let replay = app
            .clone()
            .oneshot(command_request(
                "/api/v1/holdings/acme/quality/cancel-hold/cancel",
                &p,
                true,
                "quality-history-approve",
                r#"{"expectedVersion":2,"action":"approve","reason":"Independent review confirms the incorrect quality scope."}"#,
            ))
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::OK);
        assert_eq!(json(replay).await["data"]["version"], 3);

        let db = p.system.holdings[0].raw();
        let (receipts, audits, outbox, events): (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id IN ('quality-history-request','quality-history-approve')),\
                    (SELECT count(*) FROM audit_events WHERE command_id IN ('quality-history-request','quality-history-approve')),\
                    (SELECT count(*) FROM outbox_events WHERE command_id IN ('quality-history-request','quality-history-approve')),\
                    (SELECT count(*) FROM inventory_quality_cancellations WHERE hold_id='cancel-hold')",
        )
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(
            (receipts, audits, outbox, events),
            (2, 2, 2, 2),
            "each decision keeps one receipt, audit, outbox and owner event"
        );
        let pending_notice: (String, i64) = sqlx::query_as(
            "SELECT user_id,record_version FROM staff_notifications WHERE id='quality-history-request-fixture-b'",
        )
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(pending_notice, ("fixture-b".into(), 2));
        let pending_bridge: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM inbox_events WHERE id='legacy:quality-history-request-fixture-b'),\
                    (SELECT count(*) FROM inbox_deliveries WHERE id='quality-history-request-fixture-b')",
        )
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(pending_bridge, (1, 1));

        let detail = app
            .clone()
            .oneshot(read_request(
                "/api/v1/holdings/acme/quality/cancel-hold",
                &p,
                true,
                "company",
            ))
            .await
            .unwrap();
        assert_eq!(detail.status(), StatusCode::OK);
        assert_eq!(json(detail).await["data"]["hold"]["status"], "cancelled");

        let history = app
            .clone()
            .oneshot(read_request(
                "/api/v1/holdings/acme/quality/cancel-hold/cancellations",
                &p,
                true,
                "company",
            ))
            .await
            .unwrap();
        assert_eq!(history.status(), StatusCode::OK);
        let history = json(history).await;
        let items = history["data"]["items"].as_array().unwrap();
        assert_eq!(items.len(), 2, "{history}");
        assert_eq!(items[0]["version"], 3);
        assert_eq!(items[0]["action"], "approve");
        assert_eq!(items[0]["actorId"], "fixture-b");
        assert_eq!(items[1]["version"], 2);
        assert_eq!(items[1]["action"], "request");
        assert_eq!(items[1]["actorId"], "fixture-a");
    })
    .await;
}

#[tokio::test]
async fn selected_quality_reads_use_current_hr_fallback_identity_aliases() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        grant_quality(&p).await;
        seed_quality(&p).await;
        let db = p.system.holdings[0].raw();
        // fixture-a has no IAM member here. Its person must be resolved through
        // the actual HR fallback, and these member identities are its aliases.
        sqlx::raw_sql("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES('quality-person','fixture-a','company','Synthetic person','TH','fixture',0,0); INSERT INTO iam_members(company_id,user_id,principal_id,person_id) VALUES('company','creator-alias','local:creator-alias','quality-person'),('company','inspector-alias','local:inspector-alias','quality-person'),('company','requester-alias','local:requester-alias','quality-person'); INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,last_inspector,last_result,cancel_requested_by,created_at,updated_at,quantity_scope) VALUES('alias-created','company','branch','warehouse','product','Synthetic hold','2026-11-10','held','creator-alias',NULL,NULL,NULL,0,0,'whole'),('alias-inspected','company','branch','warehouse','product','Synthetic hold','2026-11-10','awaiting_release','other-person','inspector-alias','pass',NULL,0,0,'whole'),('alias-requested','company','branch','warehouse','product','Synthetic hold','2026-11-10','cancel_pending','other-person',NULL,NULL,'requester-alias',0,0,'whole')").execute(db).await.unwrap();
        let member_person: Option<String> = sqlx::query_scalar("SELECT person_id FROM iam_members WHERE company_id='company' AND user_id='fixture-a'").fetch_optional(db).await.unwrap().flatten();
        assert!(member_person.is_none(), "fixture must use HR fallback");
        let hr_matches: i64 = sqlx::query_scalar("SELECT count(*) FROM hr_people WHERE company_id='company' AND user_id='fixture-a' AND merged_into_id IS NULL AND id='quality-person'").fetch_one(db).await.unwrap();
        let all_hr_matches: i64 = sqlx::query_scalar("SELECT count(*) FROM hr_people WHERE company_id='company' AND user_id='fixture-a' AND merged_into_id IS NULL").fetch_one(db).await.unwrap();
        assert_eq!((hr_matches, all_hr_matches), (1, 1));
        let app = prepared_router(&p);
        let reviewer_workspace=app.clone().oneshot(read_request("/api/v1/holdings/acme/quality",&p,true,"company")).await.unwrap();
        assert_eq!(reviewer_workspace.status(),StatusCode::OK);
        let reviewer_workspace=json(reviewer_workspace).await;
        let reviewer_holds=reviewer_workspace["data"]["holds"].as_array().unwrap();
        let workspace=app.clone().oneshot(read_request("/api/v1/holdings/acme/quality",&p,false,"company")).await.unwrap();
        assert_eq!(workspace.status(),StatusCode::OK);
        let workspace=json(workspace).await;
        let holds=workspace["data"]["holds"].as_array().unwrap();
        for (id,flag) in [("alias-created","canInspect"),("alias-inspected","canRelease"),("alias-requested","canReviewCancellation")] {
            let row=holds.iter().find(|row|row["id"]==id).unwrap();
            assert_eq!(row["decision"][flag],false,"{id} {row}");
            let reviewer_row=reviewer_holds.iter().find(|row|row["id"]==id).unwrap();
            assert_eq!(reviewer_row["decision"][flag],true,"unrelated actor {id} {reviewer_row}");
            for suffix in ["", "/detail-core"] {
                let response=app.clone().oneshot(read_request(&format!("/api/v1/holdings/acme/quality/{id}{suffix}"),&p,false,"company")).await.unwrap();
                let status=response.status();let body=json(response).await;
                assert_eq!(status,StatusCode::OK,"{body}");
                assert_eq!(body["data"]["hold"]["decision"][flag],false,"{id} {body}");
            }
            for suffix in ["", "/detail-core"] {
                let response=app.clone().oneshot(read_request(&format!("/api/v1/holdings/acme/quality/{id}{suffix}"),&p,true,"company")).await.unwrap();
                let status=response.status();let body=json(response).await;
                assert_eq!(status,StatusCode::OK,"{body}");
                assert_eq!(body["data"]["hold"]["decision"][flag],true,"unrelated actor {id} {body}");
            }
        }
        // A fresh selected request must observe changed actual IAM facts.
        sqlx::query("UPDATE iam_members SET person_id=NULL WHERE company_id='company' AND user_id='creator-alias'").execute(db).await.unwrap();
        let response=app.oneshot(read_request("/api/v1/holdings/acme/quality/alias-created/detail-core",&p,false,"company")).await.unwrap();
        let status=response.status();let body=json(response).await;
        assert_eq!(status,StatusCode::OK,"{body}");
        assert_eq!(body["data"]["hold"]["decision"]["canInspect"],true,"{body}");
    }).await;
}

#[tokio::test]
async fn selected_quality_alias_fence_serializes_both_writer_orders() {
    use std::time::Duration;
    async fn settle<T>(
        mut task: tokio::task::JoinHandle<T>,
    ) -> Option<Result<T, tokio::task::JoinError>> {
        match tokio::time::timeout(Duration::from_secs(25), &mut task).await {
            Ok(result) => Some(result),
            Err(_) => {
                task.abort();
                let _ = task.await;
                None
            }
        }
    }
    review::account::with_system(|system| async move {
        let p=review::setup_with_finance(system,false).await;
        grant_quality(&p).await;
        seed_quality(&p).await;
        let db=p.system.holdings[0].raw().clone();
        sqlx::raw_sql("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES('alias-race-person','fixture-a','company','Synthetic person','TH','fixture',0,0); INSERT INTO iam_members(company_id,user_id,principal_id,person_id) VALUES('company','alias-race','local:alias-race','alias-race-person'); INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,created_at,updated_at,quantity_scope) VALUES('alias-race-hold','company','branch','warehouse','product','Synthetic hold','2026-11-10','held','alias-race',0,0,'whole')").execute(&db).await.unwrap();
        let app=prepared_router(&p);
        let path="/api/v1/holdings/acme/quality/alias-race-hold/detail-core";
        // Reader first: stop it at the Quality lock after it has acquired IAM.
        let mut blocker=db.begin().await.unwrap();
        sqlx::query("LOCK TABLE inventory_quality_holds IN ROW EXCLUSIVE MODE").execute(&mut *blocker).await.unwrap();
        let blocker_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *blocker).await.unwrap();
        let request=read_request(path,&p,false,"company");let router=app.clone();
        let reader=tokio::spawn(async move { tokio::time::timeout(Duration::from_secs(20),router.oneshot(request)).await });
        let reader_pid=tokio::time::timeout(Duration::from_secs(5),async {
            loop {
                let pids:Vec<i32>=sqlx::query_scalar("SELECT l.pid FROM pg_locks l WHERE l.relation='inventory_quality_holds'::regclass AND l.mode='ShareRowExclusiveLock' AND NOT l.granted AND $1=ANY(pg_blocking_pids(l.pid)) AND EXISTS(SELECT 1 FROM pg_locks i WHERE i.pid=l.pid AND i.relation='iam_members'::regclass AND i.mode='ShareLock' AND i.granted)").bind(blocker_pid).fetch_all(&db).await.unwrap();
                if pids.len()==1 {break pids[0];} tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await;
        let (send,receive)=tokio::sync::oneshot::channel();let writer_db=db.clone();
        let writer=tokio::spawn(async move {
            let mut tx=writer_db.begin().await.unwrap();
            sqlx::query("SET LOCAL statement_timeout='15s'").execute(&mut *tx).await.unwrap();
            let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *tx).await.unwrap();let _=send.send(pid);
            sqlx::query("UPDATE iam_members SET person_id=NULL WHERE company_id='company' AND user_id='alias-race'").execute(&mut *tx).await.unwrap();
            tx.commit().await.unwrap();
        });
        let writer_pid=tokio::time::timeout(Duration::from_secs(5),receive).await.ok().and_then(Result::ok);
        let blocked=if let (Ok(reader_pid),Some(writer_pid))=(reader_pid.as_ref(),writer_pid) {
            tokio::time::timeout(Duration::from_secs(5),async {loop {
                let yes:bool=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(*reader_pid).bind(writer_pid).fetch_one(&mut *blocker).await.unwrap();
                if yes {break;}tokio::time::sleep(Duration::from_millis(10)).await;
            }}).await.is_ok()
        } else {false};
        let rollback_result=blocker.rollback().await;
        let (reader_result,writer_result)=tokio::join!(settle(reader),settle(writer));
        rollback_result.unwrap();
        assert!(reader_pid.is_ok() && blocked,"expected exact Quality reader and IAM writer lock chain");
        writer_result.expect("writer timed out").expect("writer task failed");
        let old=reader_result.expect("reader timed out").expect("reader task failed").expect("request timed out").expect("request failed");
        assert_eq!(old.status(),StatusCode::OK);assert_eq!(json(old).await["data"]["hold"]["decision"]["canInspect"],false);
        let fresh=app.clone().oneshot(read_request(path,&p,false,"company")).await.unwrap();
        assert_eq!(fresh.status(),StatusCode::OK);assert_eq!(json(fresh).await["data"]["hold"]["decision"]["canInspect"],true);
        // Writer first: the selected reader must wait on that writer's actual PID
        // before taking its snapshot and must observe its committed new alias.
        let mut writer=db.begin().await.unwrap();
        let writer_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
        sqlx::query("UPDATE iam_members SET person_id='alias-race-person' WHERE company_id='company' AND user_id='alias-race'").execute(&mut *writer).await.unwrap();
        let router=app.clone();let request=read_request(path,&p,false,"company");
        let reader=tokio::spawn(async move {tokio::time::timeout(Duration::from_secs(20),router.oneshot(request)).await});
        let blocked_reader=tokio::time::timeout(Duration::from_secs(5),async {loop {
            let pids:Vec<i32>=sqlx::query_scalar("SELECT l.pid FROM pg_locks l WHERE l.relation='iam_members'::regclass AND l.mode='ShareLock' AND NOT l.granted AND $1=ANY(pg_blocking_pids(l.pid))").bind(writer_pid).fetch_all(&db).await.unwrap();
            if pids.len()==1 {break pids[0];}tokio::time::sleep(Duration::from_millis(10)).await;
        }}).await;
        let commit_result=writer.commit().await;
        let reader_result=settle(reader).await;
        commit_result.unwrap();
        assert!(blocked_reader.is_ok(),"expected exact IAM writer blocker");
        let response=reader_result.expect("reader timed out").expect("reader task failed").expect("request timed out").expect("request failed");
        assert_eq!(response.status(),StatusCode::OK);assert_eq!(json(response).await["data"]["hold"]["decision"]["canInspect"],false);
    }).await;
}

#[tokio::test]
async fn quality_http_release_and_scrap_routes_drive_native_owner_commands() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        grant_quality(&p).await;
        seed_quality(&p).await;
        let db = p.system.holdings[0].raw();
        let now = p.system.scenario.clock.now().get();
        let expires = now + 3_600_000;
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "UPDATE assignments SET valid_from=0,valid_until=9007199254740991 WHERE id IN ('finance-fixture-a','finance-fixture-b');\
             INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,last_inspector,last_result,held_quantity_milli,created_at,updated_at,quantity_scope,version) \
             VALUES('http-release-hold','company','branch','warehouse','product','Release the passing batch','2026-11-10','awaiting_release','fixture-a','fixture-c','pass',NULL,{now},{now},'whole',1),\
                   ('http-scrap-hold','company','branch','warehouse','product','Scrap the failed batch','2026-11-10','held','fixture-a','fixture-c','fail',5000,{now},{now},'whole',1);\
             INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at) \
             VALUES('http-scrap-set','company','branch','quality_hold','http-scrap-hold','action','disposition','disposition_evidence',1,1,'internal','{{}}',0,10,'draft','fixture-b',{now},{now});\
             INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at,owner_version,company_id,detected_media_type,attachment_set_id) \
             VALUES('http-scrap-upload','quality_hold','http-scrap-hold','disposition_evidence','scrap.pdf','application/pdf',12,12,repeat('f',64),'http-scrap/staging','http-scrap/object','ready','fixture-b',{expires},{now},{now},1,'company','application/pdf','http-scrap-set');\
             INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at) \
             VALUES('http-scrap-file','http-scrap-upload','quality_hold','http-scrap-hold','disposition_evidence','http-scrap/object',1,'scrap.pdf','application/pdf',12,repeat('f',64),'fixture-b',{now},'company','application/pdf','available',{now},{now});\
             INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) \
             VALUES('http-scrap-link','http-scrap-set','http-scrap-file','draft','fixture-b',{now},{expires})"
        )))
        .execute(db)
        .await
        .unwrap();
        let app = prepared_router(&p);

        let release_key = "http-quality-release";
        let release_body = r#"{"expectedVersion":1,"reason":"Independent release review confirms delivery."}"#;
        let released = app
            .clone()
            .oneshot(command_request(
                "/api/v1/holdings/acme/quality/http-release-hold/release",
                &p,
                true,
                release_key,
                release_body,
            ))
            .await
            .unwrap();
        let status = released.status();
        let released = json(released).await;
        assert_eq!(status, StatusCode::OK, "{released}");
        assert_eq!(released["data"]["id"], "http-release-hold");
        assert_eq!(released["data"]["status"], "released");
        assert_eq!(released["data"]["version"], 2);
        let response = app
            .clone()
            .oneshot(command_request(
                "/api/v1/holdings/acme/quality/http-release-hold/release",
                &p,
                true,
                release_key,
                release_body,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(json(response).await["data"]["version"], 2);
        let self_release = app
            .clone()
            .oneshot(command_request(
                "/api/v1/holdings/acme/quality/http-scrap-hold/release",
                &p,
                false,
                "http-quality-release-self",
                r#"{"expectedVersion":1,"reason":"The creator must not release this hold."}"#,
            ))
            .await
            .unwrap();
        assert_eq!(self_release.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            json(self_release).await["error"]["code"],
            "INDEPENDENT_RELEASE_REQUIRED"
        );
        let stale = app
            .clone()
            .oneshot(command_request(
                "/api/v1/holdings/acme/quality/http-release-hold/release",
                &p,
                true,
                "http-quality-release-stale",
                r#"{"expectedVersion":9,"reason":"A stale release must not apply."}"#,
            ))
            .await
            .unwrap();
        assert_eq!(stale.status(), StatusCode::CONFLICT);

        let scrap_key = "http-quality-scrap";
        let scrap_body = r#"{"expectedVersion":1,"attachmentSetIds":["http-scrap-set"],"dispositionDate":"2026-11-10","quantity":"2.5","reason":"Controlled scrap after the failed inspection."}"#;
        let scrapped = app
            .clone()
            .oneshot(command_request(
                "/api/v1/holdings/acme/quality/http-scrap-hold/disposition",
                &p,
                true,
                scrap_key,
                scrap_body,
            ))
            .await
            .unwrap();
        let status = scrapped.status();
        let scrapped = json(scrapped).await;
        assert_eq!(status, StatusCode::OK, "{scrapped}");
        let count_id = format!("quality-scrap-{scrap_key}");
        assert_eq!(scrapped["data"]["id"], "http-scrap-hold");
        assert_eq!(scrapped["data"]["status"], "disposition_pending");
        assert_eq!(scrapped["data"]["version"], 2);
        assert_eq!(scrapped["data"]["countId"], count_id);

        let replay = app
            .clone()
            .oneshot(command_request(
                "/api/v1/holdings/acme/quality/http-scrap-hold/disposition",
                &p,
                true,
                scrap_key,
                scrap_body,
            ))
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::OK);
        assert_eq!(json(replay).await["data"]["countId"], count_id);

        let self_scrap = app
            .clone()
            .oneshot(command_request(
                "/api/v1/holdings/acme/quality/http-scrap-hold/disposition",
                &p,
                false,
                "http-quality-scrap-self",
                r#"{"expectedVersion":2,"dispositionDate":"2026-11-10","quantity":"1","reason":"The creator must not request disposal."}"#,
            ))
            .await
            .unwrap();
        assert_eq!(self_scrap.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            json(self_scrap).await["error"]["code"],
            "INDEPENDENT_DISPOSITION_REQUIRED"
        );
        let mismatch = app
            .clone()
            .oneshot(command_request(
                "/api/v1/holdings/acme/quality/http-scrap-hold/disposition",
                &p,
                true,
                "http-quality-scrap-mismatch",
                r#"{"expectedVersion":2,"dispositionDate":"2026-11-10","quantity":"9","reason":"Scrap more than the held quantity."}"#,
            ))
            .await
            .unwrap();
        assert_eq!(mismatch.status(), StatusCode::CONFLICT);

        let (hold, count, line, lot_lines, event, set_state, link_state, receipt, audit, outbox) =
            sqlx::query_as::<_, (String, String, i64, i64, String, String, String, i64, i64, i64)>(
                "SELECT h.status,c.status,\
                        (SELECT count(*) FROM inventory_count_lines l WHERE l.count_id=c.id),\
                        (SELECT count(*) FROM inventory_count_lot_lines l JOIN inventory_count_lines x ON x.id=l.count_line_id WHERE x.count_id=c.id),\
                        e.action,s.state,a.state,\
                        (SELECT count(*) FROM command_receipts WHERE command_id=$2),\
                        (SELECT count(*) FROM audit_events WHERE command_id=$2),\
                        (SELECT count(*) FROM outbox_events WHERE command_id=$2) \
                 FROM inventory_quality_holds h JOIN inventory_counts c ON c.quality_hold_id=h.id \
                 JOIN inventory_quality_disposition_events e ON e.hold_id=h.id AND e.count_id=c.id \
                 JOIN attachment_sets s ON s.target_id=h.id AND s.id='http-scrap-set' \
                 JOIN attachment_links a ON a.set_id=s.id \
                 WHERE h.id='http-scrap-hold' AND c.id=$1",
            )
            .bind(&count_id)
            .bind(scrap_key)
            .fetch_one(db)
            .await
            .unwrap();
        assert_eq!(
            (hold.as_str(), count.as_str(), line, lot_lines, event.as_str()),
            ("disposition_pending", "submitted", 1, 0, "request")
        );
        assert_eq!((set_state.as_str(), link_state.as_str()), ("bound", "bound"));
        assert_eq!((receipt, audit, outbox), (1, 1, 1));
        let notification: (String, String, i64, String) = sqlx::query_as(
            "SELECT user_id,record_type,record_version,message FROM staff_notifications WHERE id=$1",
        )
        .bind(format!("{scrap_key}-fixture-a"))
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(
            notification,
            (
                "fixture-a".into(),
                "stock_count".into(),
                1,
                "Quality scrap requires independent Finance review".into()
            )
        );
    })
    .await;
}

/// The bare Quality POST is the native manual hold-create command: purchase
/// admission, one receipt-keyed hold and replay all share the composed unit.
#[tokio::test]
async fn quality_http_hold_create_is_native_purchase_gated_and_replayable() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        grant_quality(&p).await;
        seed_quality(&p).await;
        let db = p.system.holdings[0].raw();
        sqlx::query("UPDATE assignments SET valid_from=0,valid_until=9007199254740991 WHERE id IN ('finance-fixture-a','finance-fixture-b')")
            .execute(db)
            .await
            .unwrap();
        let app = prepared_router(&p);
        let version: i64 = sqlx::query_scalar(
            "SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'",
        )
        .fetch_one(db)
        .await
        .unwrap();
        let key = "http-quality-hold-create";
        let body = format!(
            r#"{{"warehouseId":"warehouse","productId":"product","expectedStockVersion":{version},"dueDate":"2026-11-10","reason":"Damaged carton requires independent review"}}"#
        );
        let created = app
            .clone()
            .oneshot(command_request(
                "/api/v1/holdings/acme/quality",
                &p,
                false,
                key,
                &body,
            ))
            .await
            .unwrap();
        let status = created.status();
        let created = json(created).await;
        assert_eq!(status, StatusCode::OK, "{created}");
        assert_eq!(created["data"]["id"], format!("quality-{key}"));
        assert_eq!(created["data"]["status"], "held");
        assert_eq!(created["data"]["version"], 1);
        let saved: (String, String, i64, i64, Option<String>, String) = sqlx::query_as(
            "SELECT status,branch_id,version,held_quantity_milli,spec_id,quantity_scope \
             FROM inventory_quality_holds WHERE id=$1",
        )
        .bind(format!("quality-{key}"))
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(
            saved,
            (
                "held".into(),
                "branch".into(),
                1,
                5000,
                None,
                "whole".into()
            ),
            "the native hold derives its branch and held quantity from current stock"
        );
        let stock: (i64, i64) = sqlx::query_as(
            "SELECT version,quantity_milli FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'",
        )
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(
            stock,
            (version + 1, 5000),
            "the hold trigger advances only the stock version"
        );
        let retained: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),\
                    (SELECT count(*) FROM audit_events WHERE command_id=$1),\
                    (SELECT count(*) FROM outbox_events WHERE command_id=$1)",
        )
        .bind(key)
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(retained, (1, 1, 1));

        let replay = app
            .clone()
            .oneshot(command_request(
                "/api/v1/holdings/acme/quality",
                &p,
                false,
                key,
                &body,
            ))
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::OK);
        assert_eq!(json(replay).await["data"]["version"], 1);
        let replayed: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM inventory_quality_holds WHERE id=$1),\
                    (SELECT count(*) FROM command_receipts WHERE command_id=$2)",
        )
        .bind(format!("quality-{key}"))
        .bind(key)
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(
            replayed,
            (1, 1),
            "a replay keeps exactly one hold and one receipt"
        );

        let now = p.system.scenario.clock.now().get();
        let mut draining = db.begin().await.unwrap();
        let before: serde_json::Value = sqlx::query_scalar(
            "SELECT jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence) \
             FROM company_package_grants WHERE company_id='company' AND package_id='supply-quality'",
        )
        .fetch_one(&mut *draining)
        .await
        .unwrap();
        sqlx::query("UPDATE company_package_grants SET state='draining',version=version+1 WHERE company_id='company' AND package_id='supply-quality'")
            .execute(&mut *draining)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) \
             SELECT 'quality-hold-draining','company',package_id,'fixture-a','synthetic-draining',$1,\
                    jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$2 \
             FROM company_package_grants WHERE company_id='company' AND package_id='supply-quality'",
        )
        .bind(before)
        .bind(now)
        .execute(&mut *draining)
        .await
        .unwrap();
        draining.commit().await.unwrap();
        let draining = app
            .clone()
            .oneshot(command_request(
                "/api/v1/holdings/acme/quality",
                &p,
                false,
                "http-quality-hold-draining",
                &body,
            ))
            .await
            .unwrap();
        assert_eq!(draining.status(), StatusCode::FORBIDDEN);
        assert_eq!(json(draining).await["error"]["code"], "PACKAGE_DRAINING");

        sqlx::query("DELETE FROM company_package_grants WHERE company_id='company' AND package_id='supply-quality'")
            .execute(db)
            .await
            .unwrap();
        let basic = app
            .clone()
            .oneshot(command_request(
                "/api/v1/holdings/acme/quality",
                &p,
                false,
                "http-quality-hold-supply-only",
                &body,
            ))
            .await
            .unwrap();
        assert_eq!(basic.status(), StatusCode::FORBIDDEN);
        assert_eq!(json(basic).await["error"]["code"], "PACKAGE_REQUIRED");

        sqlx::query("DELETE FROM company_package_grants WHERE company_id='company'")
            .execute(db)
            .await
            .unwrap();
        let unpurchased = app
            .clone()
            .oneshot(command_request(
                "/api/v1/holdings/acme/quality",
                &p,
                false,
                "http-quality-hold-unpurchased",
                &body,
            ))
            .await
            .unwrap();
        assert_eq!(unpurchased.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            json(unpurchased).await["error"]["code"],
            "PACKAGE_REQUIRED"
        );
        let refused: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM inventory_quality_holds WHERE id IN ('quality-http-quality-hold-draining','quality-http-quality-hold-supply-only','quality-http-quality-hold-unpurchased')",
        )
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(refused, 0, "refused creates persist no hold");
    })
    .await;
}

/// Alert acknowledgement keeps the actual recipient assertion, receipt replay
/// and Commercial settlement purchase admission inside one composed unit.
#[tokio::test]
async fn quality_alert_acknowledgement_is_native_purchase_gated_and_replayable() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        grant_quality(&p).await;
        seed_quality(&p).await;
        let db = p.system.holdings[0].raw();
        let now = p.system.scenario.clock.now().get();
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "UPDATE assignments SET valid_from=0,valid_until=9007199254740991 WHERE id IN ('finance-fixture-a','finance-fixture-b');\
             INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,created_at,updated_at,quantity_scope,version) \
             VALUES('http-alert-hold','company','branch','warehouse','product','Acknowledge the current escalation','2026-11-10','held','maker',{now},{now},'whole',1),\
                   ('http-alert-creator-hold','company','branch','warehouse','product','Creator cannot acknowledge own escalation','2026-11-10','held','fixture-b',{now},{now},'whole',1),\
                   ('http-alert-terminal-hold','company','branch','warehouse','product','Terminal hold escalation','2026-11-10','cancelled','maker',{now},{now},'whole',1),\
                   ('http-alert-stale-hold','company','branch','warehouse','product','Stale escalation version','2026-11-10','held','maker',{now},{now},'whole',1),\
                   ('http-alert-purchase-hold','company','branch','warehouse','product','Unpurchased escalation','2026-11-10','held','maker',{now},{now},'whole',1),\
                   ('http-alert-unknown-hold','company','branch','warehouse','product','Unknown escalation scope','2026-11-10','held','maker',{now},{now},'whole',1);\
             INSERT INTO inventory_quality_alerts(id,hold_id,phase,source_version,due_date,detected_at) \
             VALUES('http-alert-a','http-alert-hold','due_soon',1,'2026-11-10',{now}),\
                   ('http-alert-creator','http-alert-creator-hold','due_soon',1,'2026-11-10',{now}),\
                   ('http-alert-terminal','http-alert-terminal-hold','overdue',1,'2026-11-10',{now}),\
                   ('http-alert-stale','http-alert-stale-hold','overdue',1,'2026-11-10',{now}),\
                   ('http-alert-purchase','http-alert-purchase-hold','due_soon',1,'2026-11-10',{now})"
        )))
        .execute(db)
        .await
        .unwrap();
        let app = prepared_router(&p);
        let key = "http-quality-alert-ack";
        let body = r#"{"expectedVersion":1,"reason":"Independent quality manager accepted the escalation"}"#;
        let acknowledged = app
            .clone()
            .oneshot(command_request(
                "/api/v1/holdings/acme/quality/http-alert-a/acknowledge",
                &p,
                true,
                key,
                body,
            ))
            .await
            .unwrap();
        let status = acknowledged.status();
        let acknowledged = json(acknowledged).await;
        assert_eq!(status, StatusCode::OK, "{acknowledged}");
        assert_eq!(acknowledged["data"]["id"], "http-alert-a");
        assert_eq!(acknowledged["data"]["version"], 2);
        assert_eq!(acknowledged["data"]["acknowledgedBy"], "fixture-b");
        let saved: (String, i64, i64, String) = sqlx::query_as(
            "SELECT acknowledged_by,version,acknowledged_at,reason FROM inventory_quality_alerts WHERE id='http-alert-a'",
        )
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(saved.0, "fixture-b");
        assert_eq!(saved.1, 2);
        assert_eq!(saved.2, now);
        assert_eq!(
            saved.3,
            "Independent quality manager accepted the escalation"
        );
        let ledger: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),\
                    (SELECT count(*) FROM audit_events WHERE command_id=$1),\
                    (SELECT count(*) FROM outbox_events WHERE command_id=$1)",
        )
        .bind(key)
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(ledger, (1, 1, 1));

        let replay = app
            .clone()
            .oneshot(command_request(
                "/api/v1/holdings/acme/quality/http-alert-a/acknowledge",
                &p,
                true,
                key,
                body,
            ))
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::OK);
        let replay = json(replay).await;
        assert_eq!(replay["data"]["version"], 2);
        assert_eq!(replay["data"]["acknowledgedBy"], "fixture-b");
        let repeated: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM inventory_quality_alerts WHERE id='http-alert-a' AND version=2 AND acknowledged_by='fixture-b'",
        )
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(repeated, 1);

        let conflict = app
            .clone()
            .oneshot(command_request(
                "/api/v1/holdings/acme/quality/http-alert-a/acknowledge",
                &p,
                true,
                key,
                r#"{"expectedVersion":1,"reason":"A different acknowledgement body"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(conflict.status(), StatusCode::CONFLICT);
        assert_eq!(
            json(conflict).await["error"]["code"],
            "IDEMPOTENCY_CONFLICT"
        );

        let already = app
            .clone()
            .oneshot(command_request(
                "/api/v1/holdings/acme/quality/http-alert-a/acknowledge",
                &p,
                true,
                "http-quality-alert-already",
                body,
            ))
            .await
            .unwrap();
        assert_eq!(already.status(), StatusCode::CONFLICT);
        assert_eq!(
            json(already).await["error"]["code"],
            "QUALITY_ALERT_ACKNOWLEDGED"
        );

        for (path, key, reason) in [
            (
                "/api/v1/holdings/acme/quality/http-alert-stale/acknowledge",
                "http-quality-alert-stale",
                r#"{"expectedVersion":2,"reason":"A stale escalation version"}"#,
            ),
            (
                "/api/v1/holdings/acme/quality/http-alert-creator/acknowledge",
                "http-quality-alert-creator",
                r#"{"expectedVersion":1,"reason":"The creator cannot acknowledge their own escalation"}"#,
            ),
            (
                "/api/v1/holdings/acme/quality/http-alert-terminal/acknowledge",
                "http-quality-alert-terminal",
                r#"{"expectedVersion":1,"reason":"A terminal hold escalation cannot be acknowledged"}"#,
            ),
        ] {
            let refused = app
                .clone()
                .oneshot(command_request(path, &p, true, key, reason))
                .await
                .unwrap();
            assert_eq!(refused.status(), StatusCode::CONFLICT, "{path}");
            assert_eq!(json(refused).await["error"]["code"], "CONFLICT", "{path}");
        }

        let missing = app
            .clone()
            .oneshot(command_request(
                "/api/v1/holdings/acme/quality/http-alert-missing/acknowledge",
                &p,
                true,
                "http-quality-alert-missing",
                body,
            ))
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            json(missing).await["error"]["code"],
            "QUALITY_ALERT_NOT_FOUND"
        );

        sqlx::query("DELETE FROM company_package_grants WHERE company_id='company' AND package_id='supply-quality'")
            .execute(db)
            .await
            .unwrap();
        let unpurchased = app
            .clone()
            .oneshot(command_request(
                "/api/v1/holdings/acme/quality/http-alert-purchase/acknowledge",
                &p,
                true,
                "http-quality-alert-purchase",
                body,
            ))
            .await
            .unwrap();
        assert_eq!(unpurchased.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            json(unpurchased).await["error"]["code"],
            "PACKAGE_REQUIRED"
        );
        let refused: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM inventory_quality_alerts WHERE id IN ('http-alert-stale','http-alert-creator','http-alert-terminal','http-alert-purchase') AND acknowledged_by IS NULL AND version=1",
        )
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(refused, 4, "refused acknowledgements persist no alert write");

    })
    .await;
}
