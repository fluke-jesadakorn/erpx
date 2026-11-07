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
         VALUES('*','custom_finance_review',2,'[\"finance.read\",\"quality.read\",\"quality.hold\",\"quality.inspect\",\"quality.release\",\"module.quality.read\"]','[]','[\"holding\"]','fixture',0);\
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
    async fn settle<T>(mut task: tokio::task::JoinHandle<T>) -> Option<Result<T, tokio::task::JoinError>> {
        match tokio::time::timeout(Duration::from_secs(25), &mut task).await {
            Ok(result) => Some(result),
            Err(_) => { task.abort(); let _ = task.await; None }
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
