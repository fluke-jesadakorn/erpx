//! Native Quality specification and inspection commands over the actual
//! Account session, current IAM authority, Commercial purchase and PostgreSQL.
//! The composed router is exercised; no reference backend or mock is used.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod review;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_files::public::quality_inspection_evidence::{
    inspect_quality_inspection_evidence_sets, lock_quality_inspection_evidence_sources,
};
use erpx_server_quality::public::quality_http::{QualityHttpTarget, quality_router};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{Clock, CompanyId, HoldingId, RecordId};
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

fn request(
    method: &str,
    path: &str,
    p: &review::Prepared,
    reviewer: bool,
    key: Option<&str>,
    body: Option<&str>,
) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "acme.example.test")
        .header("cookie", cookie(p, reviewer))
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
        .header("x-erpx-context-assignment-version", "4");
    if let Some(key) = key {
        builder = builder
            .header("origin", "https://acme.example.test")
            .header("idempotency-key", key)
            .header("content-type", "application/json");
    }
    builder
        .body(body.map_or_else(Body::empty, |body| Body::from(body.to_owned())))
        .unwrap()
}

async fn json(response: axum::response::Response) -> serde_json::Value {
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

async fn grant_quality(p: &review::Prepared) {
    sqlx::raw_sql(
        "INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) \
         VALUES('*','custom_finance_review',2,'[\"finance.read\",\"quality.read\",\"quality.hold\",\"quality.inspect\",\"quality.release\",\"inventory.approve\",\"module.quality.read\"]','[]','[\"holding\",\"branch\"]','fixture',0);\
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
         SELECT 'quality-command-origin-'||package_id,company_id,package_id,'fixture-a','synthetic-request',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),{now} \
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
         VALUES('warehouse','product',5000,1000) ON CONFLICT DO NOTHING",
    )
    .execute(p.system.holdings[0].raw())
    .await
    .unwrap();
}

fn prepared_router(p: &review::Prepared) -> axum::Router {
    let service = QualityService::new(
        p.authorizer.clone(),
        QualityConfiguration::new(
            HoldingId::new("h001").unwrap(),
            CompanyId::new("company").unwrap(),
        ),
        p.system.scenario.clock.clone(),
    );
    let backend = QualityBackend::new(
        Arc::new(review::account::fixture::account::policy()),
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
async fn quality_inspection_attachment_evidence_and_staff_handoff_are_native() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        grant_quality(&p).await;
        seed_quality(&p).await;
        let app = prepared_router(&p);
        let db = p.system.holdings[0].raw();
        sqlx::raw_sql(
            "UPDATE assignments SET valid_from=0,valid_until=9007199254740991 WHERE id IN ('finance-fixture-a','finance-fixture-b');\
             INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) \
             VALUES('quality-evidence-manager','fixture-manager','quality_manager','[]','company',0,9007199254740991,1,'holding')",
        )
        .execute(db)
        .await
        .unwrap();
        let now = p.system.scenario.clock.now().get();
        let expires = now + 3_600_000;
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,created_at,updated_at,quantity_scope,version) \
             VALUES('evidence-hold','company','branch','warehouse','product','Inspect damaged packaging','2026-11-10','held','fixture-a',{now},{now},'whole',1);\
             INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at,owner_version,company_id,detected_media_type) \
             VALUES('evidence-upload','quality_hold','evidence-hold','quality_inspection_evidence','evidence.pdf','application/pdf',12,12,'evidence-checksum','evidence/staging','evidence/object','ready','fixture-b',{expires},{now},{now},1,'company','application/pdf');\
             INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at) \
             VALUES('evidence-file','evidence-upload','quality_hold','evidence-hold','quality_inspection_evidence','evidence/object',1,'evidence.pdf','application/pdf',12,'evidence-checksum','fixture-b',{now},'company','application/pdf','available',{now},{now})"
        )))
        .execute(db)
        .await
        .unwrap();

        let key = "quality-command-evidence";
        let body = r#"{"expectedVersion":1,"result":"fail","evidence":"Packaging is torn across the seal.","attachmentIds":["evidence-file"],"attachmentSetIds":[]}"#;
        let inspected = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/evidence-hold/inspect",
                &p,
                true,
                Some(key),
                Some(body),
            ))
            .await
            .unwrap();
        let status = inspected.status();
        let inspected = json(inspected).await;
        assert_eq!(status, StatusCode::OK, "{inspected}");
        assert_eq!(inspected["data"]["status"], "held");
        assert_eq!(inspected["data"]["result"], "fail");
        assert_eq!(inspected["data"]["version"], 2);

        let refs: Vec<(String, i64)> = sqlx::query_as(
            "SELECT attachment_id,ordinal FROM inventory_quality_inspection_attachments WHERE inspection_id=$1 ORDER BY ordinal",
        )
        .bind(format!("inspection-{key}"))
        .fetch_all(db)
        .await
        .unwrap();
        assert_eq!(refs, vec![("evidence-file".into(), 0)]);
        let attachment: (String, String, bool) = sqlx::query_as(
            "SELECT source_state,owner_id,revoked_at IS NULL FROM attachments WHERE id='evidence-file'",
        )
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(attachment, ("available".into(), "evidence-hold".into(), true));
        let notice: (String, i64, String, String) = sqlx::query_as(
            "SELECT user_id,record_version,record_type,message FROM staff_notifications WHERE id=$1",
        )
        .bind(format!("{key}-fixture-b"))
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(
            notice,
            (
                "fixture-b".into(),
                2,
                "quality_hold".into(),
                "Quality handoff: review the source record".into()
            )
        );
        let bridge: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM inbox_events WHERE id=$1),(SELECT count(*) FROM inbox_deliveries WHERE id=$2)",
        )
        .bind(format!("legacy:{key}-fixture-b"))
        .bind(format!("{key}-fixture-b"))
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(bridge, (1, 1));

        let replay = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/evidence-hold/inspect",
                &p,
                true,
                Some(key),
                Some(body),
            ))
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::OK);
        let replay = json(replay).await;
        assert_eq!(replay["data"]["version"], 2);
        let stable: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM inventory_quality_inspection_attachments WHERE inspection_id=$1),(SELECT count(*) FROM staff_notifications WHERE id=$2),(SELECT count(*) FROM inbox_deliveries WHERE id=$2)",
        )
        .bind(format!("inspection-{key}"))
        .bind(format!("{key}-fixture-b"))
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(stable, (1, 1, 1));

        let reused = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/evidence-hold/inspect",
                &p,
                true,
                Some("quality-command-evidence-reuse"),
                Some(
                    r#"{"expectedVersion":2,"result":"fail","evidence":"Retry must refuse the bound file.","attachmentIds":["evidence-file"]}"#,
                ),
            ))
            .await
            .unwrap();
        let status = reused.status();
        let reused = json(reused).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{reused}");
        assert_eq!(reused["error"]["code"], "QUALITY_ATTACHMENT_INVALID");

        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,created_at,updated_at,quantity_scope,version) \
             VALUES('evidence-set-hold','company','branch','warehouse','product','Inspect set evidence','2026-11-10','held','fixture-a',{now},{now},'whole',1);\
             INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at) \
             VALUES('evidence-set','company','branch','quality_hold','evidence-set-hold','action','inspection','quality_inspection_evidence',1,1,'internal','{{}}',0,10,'draft','fixture-b',{now},{now});\
             INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at,owner_version,company_id,detected_media_type,attachment_set_id) \
             VALUES('set-upload-a','quality_hold','evidence-set-hold','quality_inspection_evidence','set-a.pdf','application/pdf',12,12,repeat('a',64),'set-a/staging','set-a/object','ready','fixture-b',{expires},{now},{now},1,'company','application/pdf','evidence-set'),\
                   ('set-upload-b','quality_hold','evidence-set-hold','quality_inspection_evidence','set-b.pdf','application/pdf',12,12,repeat('b',64),'set-b/staging','set-b/object','ready','fixture-b',{expires},{now},{now},1,'company','application/pdf','evidence-set');\
             INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at) \
             VALUES('set-file-a','set-upload-a','quality_hold','evidence-set-hold','quality_inspection_evidence','set-a/object',1,'set-a.pdf','application/pdf',12,repeat('a',64),'fixture-b',{now},'company','application/pdf','available',{now},{now}),\
                   ('set-file-b','set-upload-b','quality_hold','evidence-set-hold','quality_inspection_evidence','set-b/object',1,'set-b.pdf','application/pdf',12,repeat('b',64),'fixture-b',{now},'company','application/pdf','available',{now},{now});\
             INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) \
             VALUES('set-link-a','evidence-set','set-file-a','draft','fixture-b',{now_two},{expires}),\
                   ('set-link-b','evidence-set','set-file-b','draft','fixture-b',{now_one},{expires})",
            now_two = now + 2,
            now_one = now + 1
        )))
        .execute(db)
        .await
        .unwrap();
        let set_key = "quality-command-evidence-set";
        let set_body = r#"{"expectedVersion":1,"result":"fail","evidence":"Scanned set evidence is torn across the seal.","attachmentSetIds":["evidence-set"]}"#;
        let inspected = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/evidence-set-hold/inspect",
                &p,
                true,
                Some(set_key),
                Some(set_body),
            ))
            .await
            .unwrap();
        let status = inspected.status();
        let inspected = json(inspected).await;
        assert_eq!(status, StatusCode::OK, "{inspected}");
        assert_eq!(inspected["data"]["version"], 2);
        let set_refs: Vec<(String, i64)> = sqlx::query_as(
            "SELECT attachment_id,ordinal FROM inventory_quality_inspection_attachments WHERE inspection_id=$1 ORDER BY ordinal",
        )
        .bind(format!("inspection-{set_key}"))
        .fetch_all(db)
        .await
        .unwrap();
        assert_eq!(
            set_refs,
            vec![("set-file-b".into(), 0), ("set-file-a".into(), 1)],
            "stable set membership order"
        );
        let set_state: String =
            sqlx::query_scalar("SELECT state FROM attachment_sets WHERE id='evidence-set'")
                .fetch_one(db)
                .await
                .unwrap();
        assert_eq!(set_state, "draft", "membership resolution does not mutate Files");
        let set_notice: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM staff_notifications WHERE id=$1 AND record_version=2",
        )
        .bind(format!("{set_key}-fixture-b"))
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(set_notice, 1);

        let mut tx = db.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        lock_quality_inspection_evidence_sources(&mut tx)
            .await
            .unwrap();
        let witness = inspect_quality_inspection_evidence_sets(
            &mut tx,
            &CompanyId::new("company").unwrap(),
            &RecordId::new("branch").unwrap(),
            "evidence-set-hold",
            1,
            &["evidence-set".into()],
        )
        .await
        .unwrap()
        .expect("current set membership");
        assert_eq!(
            witness.attachment_ids(),
            ["set-file-b".to_string(), "set-file-a".to_string()]
        );
        sqlx::query(
            "INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) \
             VALUES('set-link-late','evidence-set','evidence-file','draft','fixture-b',$1,$2)",
        )
        .bind(now + 3)
        .bind(expires)
        .execute(&mut *tx)
        .await
        .unwrap();
        match witness.require_current(&mut tx).await {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "QUALITY_ATTACHMENT_CHANGED")
            }
            _ => panic!("late membership change was accepted"),
        }
        tx.rollback().await.unwrap();

        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at) \
             VALUES('evidence-stale-set','company','branch','quality_hold','evidence-set-hold','action','inspection','quality_inspection_evidence',1,1,'internal','{{}}',0,10,'draft','fixture-a',{now},{now}),\
                   ('evidence-hold-set','company','branch','quality_hold','evidence-hold','action','inspection','quality_inspection_evidence',2,1,'internal','{{}}',0,10,'draft','fixture-b',{now},{now});\
             INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) \
             VALUES('set-link-stale','evidence-stale-set','set-file-a','draft','fixture-b',{expires},{expires})"
        )))
        .execute(db)
        .await
        .unwrap();
        let stale = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/evidence-set-hold/inspect",
                &p,
                true,
                Some("quality-command-evidence-set-stale"),
                Some(
                    r#"{"expectedVersion":2,"result":"fail","evidence":"Stale set evidence must fail.","attachmentSetIds":["evidence-stale-set"]}"#,
                ),
            ))
            .await
            .unwrap();
        let status = stale.status();
        let stale = json(stale).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{stale}");
        assert_eq!(stale["error"]["code"], "QUALITY_ATTACHMENT_INVALID");
        let inspections: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM inventory_quality_inspections WHERE hold_id='evidence-set-hold'",
        )
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(inspections, 1);

        let foreign = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/evidence-set-hold/inspect",
                &p,
                true,
                Some("quality-command-evidence-set-foreign"),
                Some(
                    r#"{"expectedVersion":2,"result":"fail","evidence":"Another hold set must fail.","attachmentSetIds":["evidence-hold-set"]}"#,
                ),
            ))
            .await
            .unwrap();
        let status = foreign.status();
        let foreign = json(foreign).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{foreign}");
        assert_eq!(foreign["error"]["code"], "QUALITY_ATTACHMENT_INVALID");

        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,created_at,updated_at,quantity_scope,version) \
             VALUES('evidence-mixed-hold','company','branch','warehouse','product','Inspect mixed set evidence','2026-11-10','held','fixture-a',{now},{now},'whole',1);\
             INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at) \
             VALUES('evidence-mixed-set','company','branch','quality_hold','evidence-mixed-hold','action','inspection','quality_inspection_evidence',1,1,'internal','{{}}',0,10,'draft','fixture-b',{now},{now});\
             INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at,owner_version,company_id,detected_media_type,attachment_set_id) \
             VALUES('mixed-upload-a','quality_hold','evidence-mixed-hold','quality_inspection_evidence','mixed-a.pdf','application/pdf',12,12,repeat('c',64),'mixed-a/staging','mixed-a/object','ready','fixture-b',{expires},{now},{now},1,'company','application/pdf','evidence-mixed-set'),\
                   ('mixed-upload-b','quality_hold','evidence-mixed-hold','quality_inspection_evidence','mixed-b.pdf','application/pdf',12,12,repeat('d',64),'mixed-b/staging','mixed-b/object','ready','fixture-b',{expires},{now},{now},1,'company','application/pdf','evidence-mixed-set');\
             INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at) \
             VALUES('mixed-file-a','mixed-upload-a','quality_hold','evidence-mixed-hold','quality_inspection_evidence','mixed-a/object',1,'mixed-a.pdf','application/pdf',12,repeat('c',64),'fixture-b',{now},'company','application/pdf','available',{now},{now}),\
                   ('mixed-file-b','mixed-upload-b','quality_hold','evidence-mixed-hold','quality_inspection_evidence','mixed-b/object',1,'mixed-b.pdf','application/pdf',12,repeat('d',64),'fixture-b',{now},'company','application/pdf','available',{now},{now});\
             INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) \
             VALUES('mixed-link-a','evidence-mixed-set','mixed-file-a','draft','fixture-b',{now},{expires}),\
                   ('mixed-link-b','evidence-mixed-set','mixed-file-b','draft','fixture-b',{now_one},{expires})",
            now_one = now + 1
        )))
        .execute(db)
        .await
        .unwrap();
        let mixed_key = "quality-command-evidence-mixed";
        let mixed = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/evidence-mixed-hold/inspect",
                &p,
                true,
                Some(mixed_key),
                Some(
                    r#"{"expectedVersion":1,"result":"fail","evidence":"Direct and set evidence overlap once.","attachmentIds":["mixed-file-a"],"attachmentSetIds":["evidence-mixed-set"]}"#,
                ),
            ))
            .await
            .unwrap();
        let status = mixed.status();
        let mixed = json(mixed).await;
        assert_eq!(status, StatusCode::OK, "{mixed}");
        let mixed_refs: Vec<(String, i64)> = sqlx::query_as(
            "SELECT attachment_id,ordinal FROM inventory_quality_inspection_attachments WHERE inspection_id=$1 ORDER BY ordinal",
        )
        .bind(format!("inspection-{mixed_key}"))
        .fetch_all(db)
        .await
        .unwrap();
        assert_eq!(
            mixed_refs,
            vec![("mixed-file-a".into(), 0), ("mixed-file-b".into(), 1)],
            "combined direct and set ids are de-duplicated in stable order"
        );

        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,created_at,updated_at,quantity_scope,version) \
             VALUES('evidence-pass-hold','company','branch','warehouse','product','Inspect released packaging','2026-11-10','held','fixture-a',{now},{now},'whole',1)"
        )))
        .execute(db)
        .await
        .unwrap();
        let passed = app
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/evidence-pass-hold/inspect",
                &p,
                true,
                Some("quality-command-evidence-pass"),
                Some(
                    r#"{"expectedVersion":1,"result":"pass","evidence":"Packaging passes independent inspection."}"#,
                ),
            ))
            .await
            .unwrap();
        let status = passed.status();
        let passed = json(passed).await;
        assert_eq!(status, StatusCode::OK, "{passed}");
        assert_eq!(passed["data"]["status"], "awaiting_release");
        let handoff: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM staff_notifications WHERE id='quality-command-evidence-pass-fixture-manager'),\
                    (SELECT count(*) FROM staff_notifications WHERE id='quality-command-evidence-pass-fixture-b'),\
                    (SELECT count(*) FROM staff_notifications WHERE id='quality-command-evidence-pass-fixture-a')",
        )
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(handoff, (1, 0, 0));
    })
    .await;
}

#[tokio::test]
async fn quality_specification_and_inspection_commands_are_native_and_replayable() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        grant_quality(&p).await;
        seed_quality(&p).await;
        let app = prepared_router(&p);
        let create_key = "quality-spec-command-create";
        let created = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/specifications",
                &p,
                false,
                Some(create_key),
                Some(
                    r#"{"productId":"product","name":"Cold chain acceptance","metrics":[{"code":"temp","label":"Temperature","unit":"C","minimum":"2","maximum":"8"}]}"#,
                ),
            ))
            .await
            .unwrap();
        let status = created.status();
        let created = json(created).await;
        assert_eq!(status, StatusCode::OK, "{created}");
        assert_eq!(created["data"]["id"], format!("quality-spec-{create_key}"));
        assert_eq!(created["data"]["status"], "draft");
        assert_eq!(created["data"]["version"], 1);

        let spec_id = format!("quality-spec-{create_key}");
        let updated = app
            .clone()
            .oneshot(request(
                "POST",
                &format!("/api/v1/holdings/acme/quality/specifications/{spec_id}"),
                &p,
                false,
                None,
                None,
            ))
            .await
            .unwrap();
        assert_eq!(updated.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let updated = app
            .clone()
            .oneshot(request(
                "POST",
                &format!("/api/v1/holdings/acme/quality/specifications/{spec_id}"),
                &p,
                false,
                Some("quality-spec-command-update"),
                Some(
                    r#"{"productId":"product","name":"Cold chain acceptance v2","expectedVersion":1,"metrics":[{"code":"temp","label":"Temperature","unit":"C","minimum":"1","maximum":"7"},{"code":"seal","label":"Seal","unit":"ok","minimum":"1","maximum":""}]}"#,
                ),
            ))
            .await
            .unwrap();
        let status = updated.status();
        let updated = json(updated).await;
        assert_eq!(status, StatusCode::OK, "{updated}");
        assert_eq!(updated["data"]["version"], 2);
        let metric_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM inventory_quality_spec_metrics WHERE spec_id=$1",
        )
        .bind(&spec_id)
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(metric_count, 2);

        let self_activation = app
            .clone()
            .oneshot(request(
                "POST",
                &format!("/api/v1/holdings/acme/quality/specifications/{spec_id}"),
                &p,
                false,
                Some("quality-spec-command-self-activate"),
                Some(
                    r#"{"expectedVersion":2,"action":"activate","reason":"Self approval must be refused."}"#,
                ),
            ))
            .await
            .unwrap();
        assert_eq!(self_activation.status(), StatusCode::FORBIDDEN);

        let activated = app
            .clone()
            .oneshot(request(
                "POST",
                &format!("/api/v1/holdings/acme/quality/specifications/{spec_id}"),
                &p,
                true,
                Some("quality-spec-command-activate"),
                Some(
                    r#"{"expectedVersion":2,"action":"activate","reason":"Independent review confirms the limits."}"#,
                ),
            ))
            .await
            .unwrap();
        let status = activated.status();
        let activated = json(activated).await;
        assert_eq!(status, StatusCode::OK, "{activated}");
        assert_eq!(activated["data"]["status"], "active");
        assert_eq!(activated["data"]["version"], 3);

        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,created_at,updated_at,quantity_scope,version,spec_id,spec_version) \
             VALUES('command-inspect-hold','company','branch','warehouse','product','Inspect the released batch','2026-11-10','held','fixture-a',0,0,'whole',1,'{spec_id}',3)"
        )))
        .execute(p.system.holdings[0].raw())
        .await
        .unwrap();
        let metric_id = format!("{spec_id}-metric-0");
        let metric_id2 = format!("{spec_id}-metric-1");
        let inspect_key = "quality-command-inspect";
        let inspected = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/command-inspect-hold/inspect",
                &p,
                true,
                Some(inspect_key),
                Some(&format!(
                    r#"{{"expectedVersion":1,"measurements":[{{"metricId":"{metric_id}","value":"5"}},{{"metricId":"{metric_id2}","value":"1"}}],"attachmentIds":[],"attachmentSetIds":[],"evidence":"Measured at chilled storage."}}"#
                )),
            ))
            .await
            .unwrap();
        let status = inspected.status();
        let inspected = json(inspected).await;
        assert_eq!(status, StatusCode::OK, "{inspected}");
        assert_eq!(inspected["data"]["id"], "command-inspect-hold");
        assert_eq!(inspected["data"]["status"], "awaiting_release");
        assert_eq!(inspected["data"]["result"], "pass");
        assert_eq!(inspected["data"]["version"], 2);

        let replay = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/command-inspect-hold/inspect",
                &p,
                true,
                Some(inspect_key),
                Some(&format!(
                    r#"{{"expectedVersion":1,"measurements":[{{"metricId":"{metric_id}","value":"5"}},{{"metricId":"{metric_id2}","value":"1"}}],"attachmentIds":[],"attachmentSetIds":[],"evidence":"Measured at chilled storage."}}"#
                )),
            ))
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::OK);
        assert_eq!(json(replay).await["data"]["version"], 2);

        let db = p.system.holdings[0].raw();
        let (hold_status, hold_version, hold_result, inspections): (String, i64, String, i64) =
            sqlx::query_as(
                "SELECT h.status,h.version,h.last_result,(SELECT count(*) FROM inventory_quality_inspections i WHERE i.hold_id=h.id) FROM inventory_quality_holds h WHERE h.id='command-inspect-hold'",
            )
            .fetch_one(db)
            .await
            .unwrap();
        assert_eq!(
            (hold_status.as_str(), hold_version, hold_result.as_str(), inspections),
            ("awaiting_release", 2, "pass", 1)
        );
        let measurements: Vec<(String, String, i64)> = sqlx::query_as(
            "SELECT metric_id,measured_value::text,passed FROM inventory_quality_measurements WHERE inspection_id=$1 ORDER BY ordinal",
        )
        .bind(format!("inspection-{inspect_key}"))
        .fetch_all(db)
        .await
        .unwrap();
        assert_eq!(
            measurements,
            vec![(metric_id, "5".into(), 1), (metric_id2, "1".into(), 1)],
            "measurement rows"
        );
        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id IN ($1,$2,$3,$4)",
        )
        .bind(create_key)
        .bind("quality-spec-command-update")
        .bind("quality-spec-command-activate")
        .bind(inspect_key)
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(receipts, 4);
        let audited: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM audit_events WHERE command_id IN ($1,$2,$3,$4)",
        )
        .bind(create_key)
        .bind("quality-spec-command-update")
        .bind("quality-spec-command-activate")
        .bind(inspect_key)
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(audited, 4);
        let outbox: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM outbox_events WHERE command_id IN ($1,$2,$3,$4)",
        )
        .bind(create_key)
        .bind("quality-spec-command-update")
        .bind("quality-spec-command-activate")
        .bind(inspect_key)
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(outbox, 4);
    })
    .await;
}

#[tokio::test]
async fn quality_cancellation_commands_use_the_workflow_ledger_and_after_image_notifications() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        grant_quality(&p).await;
        seed_quality(&p).await;
        let db = p.system.holdings[0].raw();
        let now = p.system.scenario.clock.now().get();
        // fixture-c is a current Quality recipient without an interactive
        // session. It proves the terminal creator notice uses the saved
        // after-image rather than the acting reviewer.
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "UPDATE assignments SET valid_from=0,valid_until=9007199254740991 WHERE id IN ('finance-fixture-a','finance-fixture-b');\
             INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) \
             VALUES('quality-creator','fixture-c','custom_finance_review','[\"quality\"]','company',0,9007199254740991,1,'holding');\
             INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,created_at,updated_at,quantity_scope,version) \
             VALUES('cancel-approve-hold','company','branch','warehouse','product','Review the wrong quality scope','2026-11-10','held','fixture-c',{now},{now},'whole',1),\
                   ('cancel-reject-hold','company','branch','warehouse','product','Review a disputed quality scope','2026-11-10','held','fixture-c',{now},{now},'whole',1),\
                   ('cancel-authority-hold','company','branch','warehouse','product','Review a denied authority','2026-11-10','held','fixture-c',{now},{now},'whole',1),\
                   ('cancel-purchase-hold','company','branch','warehouse','product','Review the current purchase','2026-11-10','held','fixture-c',{now},{now},'whole',1),\
                   ('cancel-purchase-denied-hold','company','branch','warehouse','product','Review a denied purchase','2026-11-10','held','fixture-c',{now},{now},'whole',1)"
        )))
        .execute(db)
        .await
        .unwrap();
        let app = prepared_router(&p);

        let request_key = "quality-cancel-request-http";
        let request_body = r#"{"expectedVersion":1,"action":"request","reason":"The hold was opened against the wrong quality scope."}"#;
        let requested = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/cancel-approve-hold/cancel",
                &p,
                false,
                Some(request_key),
                Some(request_body),
            ))
            .await
            .unwrap();
        let status = requested.status();
        let requested = json(requested).await;
        assert_eq!(status, StatusCode::OK, "{requested}");
        assert_eq!(requested["data"]["status"], "cancel_pending");
        assert_eq!(requested["data"]["version"], 2);
        assert_eq!(
            requested["data"]["cancellationId"],
            format!("quality-cancel-{request_key}")
        );

        let replay = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/cancel-approve-hold/cancel",
                &p,
                false,
                Some(request_key),
                Some(request_body),
            ))
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::OK);
        assert_eq!(json(replay).await["data"]["version"], 2);

        let (receipts, audits, outbox, events): (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),\
                    (SELECT count(*) FROM audit_events WHERE command_id=$1),\
                    (SELECT count(*) FROM outbox_events WHERE command_id=$1),\
                    (SELECT count(*) FROM inventory_quality_cancellations WHERE id=$2)",
        )
        .bind(request_key)
        .bind(format!("quality-cancel-{request_key}"))
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(
            (receipts, audits, outbox, events),
            (1, 1, 1, 1),
            "one replay-stable receipt, audit, outbox and owner event"
        );

        let pending_notice: (String, i64, String) = sqlx::query_as(
            "SELECT user_id,record_version,record_type FROM staff_notifications WHERE id=$1",
        )
        .bind(format!("{request_key}-fixture-b"))
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(pending_notice, ("fixture-b".into(), 2, "quality_hold".into()));
        let excluded: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM staff_notifications WHERE id IN ($1,$2)",
        )
        .bind(format!("{request_key}-fixture-a"))
        .bind(format!("{request_key}-fixture-c"))
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(excluded, 0, "pending review excludes creator and requester");
        let pending_bridge: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM inbox_events WHERE id=$1),\
                    (SELECT count(*) FROM inbox_deliveries WHERE id=$2)",
        )
        .bind(format!("legacy:{request_key}-fixture-b"))
        .bind(format!("{request_key}-fixture-b"))
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(pending_bridge, (1, 1));

        let self_review = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/cancel-approve-hold/cancel",
                &p,
                false,
                Some("quality-cancel-self-review"),
                Some(
                    r#"{"expectedVersion":2,"action":"approve","reason":"Approving my own cancellation request."}"#,
                ),
            ))
            .await
            .unwrap();
        assert_eq!(self_review.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            json(self_review).await["error"]["code"],
            "INDEPENDENT_CANCELLATION_REQUIRED"
        );

        let approve_key = "quality-cancel-approve-http";
        let approved = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/cancel-approve-hold/cancel",
                &p,
                true,
                Some(approve_key),
                Some(
                    r#"{"expectedVersion":2,"action":"approve","reason":"Independent review confirms the incorrect quality scope."}"#,
                ),
            ))
            .await
            .unwrap();
        let status = approved.status();
        let approved = json(approved).await;
        assert_eq!(status, StatusCode::OK, "{approved}");
        assert_eq!(approved["data"]["status"], "cancelled");
        assert_eq!(approved["data"]["version"], 3);

        let replay = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/cancel-approve-hold/cancel",
                &p,
                true,
                Some(approve_key),
                Some(
                    r#"{"expectedVersion":2,"action":"approve","reason":"Independent review confirms the incorrect quality scope."}"#,
                ),
            ))
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::OK);
        assert_eq!(json(replay).await["data"]["version"], 3);
        let stable: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),\
                    (SELECT count(*) FROM audit_events WHERE command_id=$1),\
                    (SELECT count(*) FROM outbox_events WHERE command_id=$1),\
                    (SELECT count(*) FROM inventory_quality_cancellations WHERE id=$2)",
        )
        .bind(approve_key)
        .bind(format!("quality-cancel-{approve_key}"))
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(stable, (1, 1, 1, 1));

        let terminal_notice: (String, i64) = sqlx::query_as(
            "SELECT user_id,record_version FROM staff_notifications WHERE id=$1",
        )
        .bind(format!("{approve_key}-fixture-c"))
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(terminal_notice, ("fixture-c".into(), 3));
        let approver_notice: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM staff_notifications WHERE id=$1",
        )
        .bind(format!("{approve_key}-fixture-b"))
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(approver_notice, 0, "terminal notice goes to the creator");
        let terminal_bridge: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM inbox_events WHERE id=$1),\
                    (SELECT count(*) FROM inbox_deliveries WHERE id=$2)",
        )
        .bind(format!("legacy:{approve_key}-fixture-c"))
        .bind(format!("{approve_key}-fixture-c"))
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(terminal_bridge, (1, 1));

        let stale = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/cancel-approve-hold/cancel",
                &p,
                true,
                Some("quality-cancel-stale-approve"),
                Some(
                    r#"{"expectedVersion":2,"action":"approve","reason":"A stale version must not apply."}"#,
                ),
            ))
            .await
            .unwrap();
        assert_eq!(stale.status(), StatusCode::CONFLICT);
        assert_eq!(json(stale).await["error"]["code"], "CONFLICT");
        let stale_rows: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id='quality-cancel-stale-approve'),\
                    (SELECT count(*) FROM inventory_quality_cancellations WHERE hold_id='cancel-approve-hold')",
        )
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(stale_rows, (0, 2));

        let reject_request_key = "quality-cancel-reject-request";
        let requested = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/cancel-reject-hold/cancel",
                &p,
                false,
                Some(reject_request_key),
                Some(
                    r#"{"expectedVersion":1,"action":"request","reason":"The disputed hold needs independent review."}"#,
                ),
            ))
            .await
            .unwrap();
        assert_eq!(requested.status(), StatusCode::OK);
        assert_eq!(json(requested).await["data"]["version"], 2);
        let reject_key = "quality-cancel-reject-http";
        let rejected = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/cancel-reject-hold/cancel",
                &p,
                true,
                Some(reject_key),
                Some(
                    r#"{"expectedVersion":2,"action":"reject","reason":"Independent review keeps the quality hold in place."}"#,
                ),
            ))
            .await
            .unwrap();
        let status = rejected.status();
        let rejected = json(rejected).await;
        assert_eq!(status, StatusCode::OK, "{rejected}");
        assert_eq!(rejected["data"]["status"], "held");
        assert_eq!(rejected["data"]["version"], 3);
        let rejected_event: (String, i64) = sqlx::query_as(
            "SELECT action,version FROM inventory_quality_cancellations WHERE id=$1",
        )
        .bind(format!("quality-cancel-{reject_key}"))
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(rejected_event, ("reject".into(), 3));
        let reject_notices: Vec<(String, i64)> = sqlx::query_as(
            "SELECT user_id,record_version FROM staff_notifications WHERE id IN ($1,$2) ORDER BY user_id",
        )
        .bind(format!("{reject_key}-fixture-a"))
        .bind(format!("{reject_key}-fixture-b"))
        .fetch_all(db)
        .await
        .unwrap();
        assert_eq!(
            reject_notices,
            vec![("fixture-a".into(), 3), ("fixture-b".into(), 3)],
            "reject returns to held and notifies current inspectors"
        );

        sqlx::query(
            "INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) \
             VALUES('*','custom_finance_review',3,'[\"finance.read\",\"quality.read\",\"quality.inspect\",\"quality.release\",\"module.quality.read\"]','[]','[\"holding\",\"branch\"]','fixture',0)",
        )
        .execute(db)
        .await
        .unwrap();
        sqlx::query("UPDATE iam_roles SET published_revision=3 WHERE company_id='*' AND id='custom_finance_review'")
            .execute(db)
            .await
            .unwrap();
        let denied_authority = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/cancel-authority-hold/cancel",
                &p,
                false,
                Some("quality-cancel-denied-authority"),
                Some(
                    r#"{"expectedVersion":1,"action":"request","reason":"A missing hold permission must not write."}"#,
                ),
            ))
            .await
            .unwrap();
        assert_eq!(denied_authority.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            json(denied_authority).await["error"]["code"],
            "QUALITY_ACCESS_CHANGED"
        );
        let authority_rows: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM inventory_quality_cancellations WHERE hold_id='cancel-authority-hold'",
        )
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(authority_rows, 0);
        sqlx::query("UPDATE iam_roles SET published_revision=2 WHERE company_id='*' AND id='custom_finance_review'")
            .execute(db)
            .await
            .unwrap();

        let allowed = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/cancel-purchase-hold/cancel",
                &p,
                false,
                Some("quality-cancel-allowed-purchase"),
                Some(
                    r#"{"expectedVersion":1,"action":"request","reason":"The current purchase permits the cancellation."}"#,
                ),
            ))
            .await
            .unwrap();
        assert_eq!(allowed.status(), StatusCode::OK);

        let mut package = db.begin().await.unwrap();
        sqlx::query(
            "UPDATE company_package_grants SET state='inactive',version=version+1,updated_at=$1 WHERE company_id='company' AND package_id='supply-quality'",
        )
        .bind(now + 1)
        .execute(&mut *package)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) \
             SELECT 'quality-cancel-purchase-inactive','company',package_id,'fixture-a','quality-cancel-purchase-inactive',NULL,\
                    jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$1 \
             FROM company_package_grants WHERE company_id='company' AND package_id='supply-quality'",
        )
        .bind(now + 1)
        .execute(&mut *package)
        .await
        .unwrap();
        package.commit().await.unwrap();

        let denied_purchase = app
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/cancel-purchase-denied-hold/cancel",
                &p,
                false,
                Some("quality-cancel-denied-purchase"),
                Some(
                    r#"{"expectedVersion":1,"action":"request","reason":"A missing purchase must not write."}"#,
                ),
            ))
            .await
            .unwrap();
        assert_eq!(denied_purchase.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            json(denied_purchase).await["error"]["code"],
            "PACKAGE_INACTIVE"
        );
        let purchase_rows: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM inventory_quality_cancellations WHERE hold_id='cancel-purchase-denied-hold'",
        )
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(purchase_rows, 0);
    })
    .await;
}

#[tokio::test]
async fn quality_release_and_scrap_dispositions_are_native_and_replayable() {
    use sha2::{Digest, Sha256};
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        grant_quality(&p).await;
        seed_quality(&p).await;
        let db = p.system.holdings[0].raw();
        let now = p.system.scenario.clock.now().get();
        let expires = now + 3_600_000;
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "UPDATE assignments SET valid_from=0,valid_until=9007199254740991 WHERE id IN ('finance-fixture-a','finance-fixture-b');\
             INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode) \
             VALUES('tracked-product','company','P2','Tracked product',1000,'lot'),\
                   ('serial-product','company','P3','Serial product',100,'serial');\
             INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor) \
             VALUES('warehouse','tracked-product',5000,1000),('warehouse','serial-product',1000,100);\
             INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,serial_number,manufactured_date,expiry_date,created_by,created_at) \
             VALUES('lot-a','company','tracked-product','lot','LOT-A',NULL,'2026-01-01','2027-01-01','fixture-a',{now}),\
                   ('lot-b','company','tracked-product','lot','LOT-B',NULL,'2026-01-01','2027-01-01','fixture-a',{now}),\
                   ('serial-lot','company','serial-product','serial',NULL,'SN-1','2026-01-01','2027-01-01','fixture-a',{now});\
             INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor) \
             VALUES('lot-a','warehouse',3000,600),('lot-b','warehouse',2000,400),('serial-lot','warehouse',1000,100);\
             INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,last_inspector,last_result,created_at,updated_at,quantity_scope,version) \
             VALUES('release-hold','company','branch','warehouse','product','Release the passing batch','2026-11-10','awaiting_release','fixture-a','fixture-c','pass',{now},{now},'whole',1),\
                   ('release-pass-required','company','branch','warehouse','product','A passing inspection is missing','2026-11-10','held','fixture-a','fixture-c','fail',{now},{now},'whole',1),\
                   ('release-denied-hold','company','branch','warehouse','product','Release under an inactive purchase','2026-11-10','awaiting_release','fixture-a','fixture-c','pass',{now},{now},'whole',1),\
                   ('scrap-hold','company','branch','warehouse','tracked-product','Scrap the failed tracked batch','2026-11-10','held','fixture-a','fixture-c','fail',{now},{now},'whole',1),\
                   ('scrap-pass-hold','company','branch','warehouse','tracked-product','A failed inspection is missing','2026-11-10','held','fixture-a','fixture-c','pass',{now},{now},'whole',1),\
                   ('scrap-exceed-hold','company','branch','warehouse','tracked-product','Scrap more than held','2026-11-10','held','fixture-a','fixture-c','fail',{now},{now},'whole',1),\
                   ('scrap-serial-hold','company','branch','warehouse','serial-product','Scrap part of a serial','2026-11-10','held','fixture-a','fixture-c','fail',{now},{now},'whole',1),\
                   ('scrap-no-lot-hold','company','branch','warehouse','tracked-product','Scrap without the affected lot','2026-11-10','held','fixture-a','fixture-c','fail',{now},{now},'whole',1),\
                   ('scrap-stale-evidence-hold','company','branch','warehouse','tracked-product','Scrap with stale evidence','2026-11-10','held','fixture-a','fixture-c','fail',{now},{now},'whole',1),\
                   ('scrap-purchase-hold','company','branch','warehouse','tracked-product','Scrap under an inactive purchase','2026-11-10','held','fixture-a','fixture-c','fail',{now},{now},'whole',1);\
             UPDATE inventory_quality_holds SET lot_id='lot-a',held_quantity_milli=3000 WHERE id='scrap-hold';\
             UPDATE inventory_quality_holds SET lot_id='lot-a',held_quantity_milli=1000 WHERE id='scrap-exceed-hold';\
             UPDATE inventory_quality_holds SET lot_id='serial-lot',held_quantity_milli=1000 WHERE id='scrap-serial-hold';\
             UPDATE inventory_quality_holds SET held_quantity_milli=3000 WHERE id='scrap-no-lot-hold';\
             UPDATE inventory_quality_holds SET lot_id='lot-a',held_quantity_milli=3000 WHERE id='scrap-stale-evidence-hold';\
             UPDATE inventory_quality_holds SET lot_id='lot-a',held_quantity_milli=3000 WHERE id='scrap-purchase-hold';\
             INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at) \
             VALUES('scrap-set','company','branch','quality_hold','scrap-hold','action','disposition','disposition_evidence',1,1,'internal','{{}}',0,10,'draft','fixture-b',{now},{now}),\
                   ('scrap-stale-set','company','branch','quality_hold','scrap-stale-evidence-hold','action','disposition','disposition_evidence',2,1,'internal','{{}}',0,10,'draft','fixture-b',{now},{now});\
             INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at,owner_version,company_id,detected_media_type,attachment_set_id) \
             VALUES('scrap-upload','quality_hold','scrap-hold','disposition_evidence','scrap.pdf','application/pdf',12,12,repeat('e',64),'scrap/staging','scrap/object','ready','fixture-b',{expires},{now},{now},1,'company','application/pdf','scrap-set');\
             INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at) \
             VALUES('scrap-file','scrap-upload','quality_hold','scrap-hold','disposition_evidence','scrap/object',1,'scrap.pdf','application/pdf',12,repeat('e',64),'fixture-b',{now},'company','application/pdf','available',{now},{now});\
             INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) \
             VALUES('scrap-link','scrap-set','scrap-file','draft','fixture-b',{now},{expires})"
        )))
        .execute(db)
        .await
        .unwrap();
        let app = prepared_router(&p);

        let unknown = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/release-hold/release",
                &p,
                true,
                Some("quality-release-unknown"),
                Some(r#"{"expectedVersion":1,"reason":"Independent release review.","extra":true}"#),
            ))
            .await
            .unwrap();
        assert_eq!(unknown.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(json(unknown).await["error"]["code"], "QUALITY_RELEASE_REQUIRED");

        let release_key = "quality-release-http";
        let release_body = r#"{"expectedVersion":1,"reason":"Independent release review confirms the passing inspection."}"#;
        let released = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/release-hold/release",
                &p,
                true,
                Some(release_key),
                Some(release_body),
            ))
            .await
            .unwrap();
        let status = released.status();
        let released = json(released).await;
        assert_eq!(status, StatusCode::OK, "{released}");
        assert_eq!(released["data"]["id"], "release-hold");
        assert_eq!(released["data"]["status"], "released");
        assert_eq!(released["data"]["version"], 2);
        let release_row: (String, String, Option<String>, Option<String>, Option<i64>, i64) =
            sqlx::query_as(
                "SELECT status,last_result,released_by,release_reason,released_at,version FROM inventory_quality_holds WHERE id='release-hold'",
            )
            .fetch_one(db)
            .await
            .unwrap();
        assert_eq!(
            release_row,
            (
                "released".into(),
                "pass".into(),
                Some("fixture-b".into()),
                Some("Independent release review confirms the passing inspection.".into()),
                Some(now),
                2
            )
        );
        let release_ledger: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),\
                    (SELECT count(*) FROM audit_events WHERE command_id=$1),\
                    (SELECT count(*) FROM outbox_events WHERE command_id=$1)",
        )
        .bind(release_key)
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(release_ledger, (1, 1, 1));
        let release_notice: (String, i64, String) = sqlx::query_as(
            "SELECT user_id,record_version,record_type FROM staff_notifications WHERE id=$1",
        )
        .bind(format!("{release_key}-fixture-a"))
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(
            release_notice,
            ("fixture-a".into(), 2, "quality_hold".into()),
            "terminal release notice goes to the creator"
        );
        let release_bridge: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM inbox_events WHERE id=$1),\
                    (SELECT count(*) FROM inbox_deliveries WHERE id=$2)",
        )
        .bind(format!("legacy:{release_key}-fixture-a"))
        .bind(format!("{release_key}-fixture-a"))
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(release_bridge, (1, 1));

        let release_replay = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/release-hold/release",
                &p,
                true,
                Some(release_key),
                Some(release_body),
            ))
            .await
            .unwrap();
        assert_eq!(release_replay.status(), StatusCode::OK);
        assert_eq!(json(release_replay).await["data"]["version"], 2);
        let release_stable: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),\
                    (SELECT count(*) FROM audit_events WHERE command_id=$1),\
                    (SELECT count(*) FROM outbox_events WHERE command_id=$1),\
                    (SELECT count(*) FROM staff_notifications WHERE id=$2)",
        )
        .bind(release_key)
        .bind(format!("{release_key}-fixture-a"))
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(release_stable, (1, 1, 1, 1));

        let self_release = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/release-denied-hold/release",
                &p,
                false,
                Some("quality-release-self"),
                Some(r#"{"expectedVersion":1,"reason":"The creator must not release this hold."}"#),
            ))
            .await
            .unwrap();
        assert_eq!(self_release.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            json(self_release).await["error"]["code"],
            "INDEPENDENT_RELEASE_REQUIRED"
        );

        let pass_required = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/release-pass-required/release",
                &p,
                true,
                Some("quality-release-pass-required"),
                Some(r#"{"expectedVersion":1,"reason":"A failing inspection must not release."}"#),
            ))
            .await
            .unwrap();
        assert_eq!(pass_required.status(), StatusCode::CONFLICT);
        assert_eq!(
            json(pass_required).await["error"]["code"],
            "QUALITY_PASS_REQUIRED"
        );

        let stale_release = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/release-denied-hold/release",
                &p,
                true,
                Some("quality-release-stale"),
                Some(r#"{"expectedVersion":9,"reason":"A stale release version must not apply."}"#),
            ))
            .await
            .unwrap();
        assert_eq!(stale_release.status(), StatusCode::CONFLICT);
        assert_eq!(json(stale_release).await["error"]["code"], "CONFLICT");

        let unknown_scrap = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/scrap-hold/disposition",
                &p,
                true,
                Some("quality-scrap-unknown"),
                Some(r#"{"expectedVersion":1,"dispositionDate":"2026-11-10","quantity":"1.5","reason":"Unknown key must be refused.","extra":1}"#),
            ))
            .await
            .unwrap();
        assert_eq!(unknown_scrap.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            json(unknown_scrap).await["error"]["code"],
            "QUALITY_DISPOSITION_REQUIRED"
        );

        let scrap_key = "quality-scrap-http";
        let scrap_body = r#"{"expectedVersion":1,"attachmentSetIds":["scrap-set"],"dispositionDate":"2026-11-10","quantity":"1.5","reason":"Controlled scrap after the failed inspection."}"#;
        let scrapped = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/scrap-hold/disposition",
                &p,
                true,
                Some(scrap_key),
                Some(scrap_body),
            ))
            .await
            .unwrap();
        let status = scrapped.status();
        let scrapped = json(scrapped).await;
        assert_eq!(status, StatusCode::OK, "{scrapped}");
        let count_id = format!("quality-scrap-{scrap_key}");
        assert_eq!(scrapped["data"]["id"], "scrap-hold");
        assert_eq!(scrapped["data"]["status"], "disposition_pending");
        assert_eq!(scrapped["data"]["version"], 2);
        assert_eq!(scrapped["data"]["countId"], count_id);

        let hold_row: (String, Option<String>, Option<String>, Option<String>, Option<String>, Option<i64>, i64) =
            sqlx::query_as(
                "SELECT status,disposition_kind,disposition_reason,disposition_requested_by,disposition_count_id,disposition_quantity_milli,version FROM inventory_quality_holds WHERE id='scrap-hold'",
            )
            .fetch_one(db)
            .await
            .unwrap();
        assert_eq!(
            hold_row,
            (
                "disposition_pending".into(),
                Some("scrap".into()),
                Some("Controlled scrap after the failed inspection.".into()),
                Some("fixture-b".into()),
                Some(count_id.clone()),
                Some(1500),
                2
            )
        );
        let count_exact: (String, String, i64, String, String) = sqlx::query_as(
            "SELECT status,number,net_value_minor,quality_hold_id,branch_id FROM inventory_counts WHERE id=$1",
        )
        .bind(&count_id)
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(
            count_exact,
            (
                "submitted".into(),
                format!("QSCRAP-{}", hex::encode_upper(&Sha256::digest(count_id.as_bytes())[..7])),
                -300,
                "scrap-hold".into(),
                "branch".into()
            )
        );
        let stock_version: i64 = sqlx::query_scalar(
            "SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='tracked-product'",
        )
        .fetch_one(db)
        .await
        .unwrap();
        let line: (String, i64, i64, i64, i64, i64) = sqlx::query_as(
            "SELECT product_id,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor FROM inventory_count_lines WHERE id=$1",
        )
        .bind(format!("{count_id}-0"))
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(
            line,
            (
                "tracked-product".into(),
                5000,
                1000,
                stock_version,
                3500,
                700
            )
        );
        let lot_lines: Vec<(String, String, i64, i64, i64, i64)> = sqlx::query_as(
            "SELECT id,result_lot_id,snapshot_quantity_milli,snapshot_value_minor,counted_quantity_milli,counted_value_minor FROM inventory_count_lot_lines WHERE count_line_id=$1 ORDER BY id",
        )
        .bind(format!("{count_id}-0"))
        .fetch_all(db)
        .await
        .unwrap();
        assert_eq!(
            lot_lines,
            vec![
                (
                    format!("{count_id}-0-lot-0"),
                    "lot-a".into(),
                    3000,
                    600,
                    1500,
                    300
                ),
                (
                    format!("{count_id}-0-lot-1"),
                    "lot-b".into(),
                    2000,
                    400,
                    2000,
                    400
                ),
            ]
        );
        let disposition_event: (String, String, i64, String) = sqlx::query_as(
            "SELECT action,count_id,reviewed_version,actor_id FROM inventory_quality_disposition_events WHERE id=$1",
        )
        .bind(format!("quality-disposition-{scrap_key}"))
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(
            disposition_event,
            ("request".into(), count_id.clone(), 1, "fixture-b".into())
        );
        let set_state: (String, Option<String>, Option<i64>) = sqlx::query_as(
            "SELECT state,bound_command_id,bound_at FROM attachment_sets WHERE id='scrap-set'",
        )
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(
            set_state,
            ("bound".into(), Some(scrap_key.into()), Some(now))
        );
        let link_state: (String, Option<String>, Option<String>, Option<i64>) = sqlx::query_as(
            "SELECT state,binding_type,binding_id,bound_at FROM attachment_links WHERE id='scrap-link'",
        )
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(
            link_state,
            (
                "bound".into(),
                Some("quality_disposition".into()),
                Some(count_id.clone()),
                Some(now)
            )
        );
        let event_details: (String, String, String, i32) = sqlx::query_as(
            "SELECT details_json::jsonb->>'bindingType',details_json::jsonb->>'bindingId',details_json::jsonb->'manifest'->0->>'linkId',jsonb_array_length(details_json::jsonb->'manifest') FROM attachment_events WHERE id=$1",
        )
        .bind(format!(
            "attachment-bind-{}",
            &hex::encode(Sha256::digest(format!("{scrap_key}\0scrap-set").as_bytes()))[..32]
        ))
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(
            event_details,
            (
                "quality_disposition".into(),
                count_id.clone(),
                "scrap-link".into(),
                1
            )
        );
        let scrap_ledger: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),\
                    (SELECT count(*) FROM audit_events WHERE command_id=$1),\
                    (SELECT count(*) FROM outbox_events WHERE command_id=$1)",
        )
        .bind(scrap_key)
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(scrap_ledger, (1, 1, 1));
        let finance_notice: (String, String, i64, String) = sqlx::query_as(
            "SELECT user_id,record_type,record_version,message FROM staff_notifications WHERE id=$1",
        )
        .bind(format!("{scrap_key}-fixture-a"))
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(
            finance_notice,
            (
                "fixture-a".into(),
                "stock_count".into(),
                1,
                "Quality scrap requires independent Finance review".into()
            )
        );
        let actor_notice: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM staff_notifications WHERE id=$1",
        )
        .bind(format!("{scrap_key}-fixture-b"))
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(actor_notice, 0, "the requesting manager is excluded");
        let finance_bridge: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM inbox_events WHERE id=$1),\
                    (SELECT count(*) FROM inbox_deliveries WHERE id=$2)",
        )
        .bind(format!("legacy:{scrap_key}-fixture-a"))
        .bind(format!("{scrap_key}-fixture-a"))
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(finance_bridge, (1, 1));

        let replay = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/scrap-hold/disposition",
                &p,
                true,
                Some(scrap_key),
                Some(scrap_body),
            ))
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::OK);
        assert_eq!(json(replay).await["data"]["version"], 2);
        let stable: (i64, i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM inventory_counts WHERE id=$1),\
                    (SELECT count(*) FROM inventory_count_lines WHERE id=$2),\
                    (SELECT count(*) FROM inventory_count_lot_lines WHERE count_line_id=$2),\
                    (SELECT count(*) FROM inventory_quality_disposition_events WHERE id=$3),\
                    (SELECT count(*) FROM attachment_events WHERE set_id='scrap-set' AND action='set.bound')",
        )
        .bind(&count_id)
        .bind(format!("{count_id}-0"))
        .bind(format!("quality-disposition-{scrap_key}"))
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(stable, (1, 1, 2, 1, 1), "replay does not duplicate rows");

        let scrap_self = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/scrap-serial-hold/disposition",
                &p,
                false,
                Some("quality-scrap-self"),
                Some(r#"{"expectedVersion":1,"dispositionDate":"2026-11-10","quantity":"1","reason":"The creator must not request disposal."}"#),
            ))
            .await
            .unwrap();
        assert_eq!(scrap_self.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            json(scrap_self).await["error"]["code"],
            "INDEPENDENT_DISPOSITION_REQUIRED"
        );

        let failed_required = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/scrap-pass-hold/disposition",
                &p,
                true,
                Some("quality-scrap-pass-required"),
                Some(r#"{"expectedVersion":1,"dispositionDate":"2026-11-10","quantity":"1","reason":"A passing inspection must not scrap."}"#),
            ))
            .await
            .unwrap();
        assert_eq!(failed_required.status(), StatusCode::CONFLICT);
        assert_eq!(
            json(failed_required).await["error"]["code"],
            "FAILED_INSPECTION_REQUIRED"
        );

        let over_held = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/scrap-exceed-hold/disposition",
                &p,
                true,
                Some("quality-scrap-over-held"),
                Some(r#"{"expectedVersion":1,"dispositionDate":"2026-11-10","quantity":"2","reason":"Scrap more than the held quantity."}"#),
            ))
            .await
            .unwrap();
        assert_eq!(over_held.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            json(over_held).await["error"]["code"],
            "DISPOSITION_QUANTITY_MISMATCH"
        );

        let partial_serial = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/scrap-serial-hold/disposition",
                &p,
                true,
                Some("quality-scrap-partial-serial"),
                Some(r#"{"expectedVersion":1,"dispositionDate":"2026-11-10","quantity":"0.5","reason":"A serial number must be one whole unit."}"#),
            ))
            .await
            .unwrap();
        assert_eq!(partial_serial.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            json(partial_serial).await["error"]["code"],
            "SERIAL_QUANTITY_INVALID"
        );

        let missing_lot = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/scrap-no-lot-hold/disposition",
                &p,
                true,
                Some("quality-scrap-missing-lot"),
                Some(r#"{"expectedVersion":1,"dispositionDate":"2026-11-10","quantity":"1","reason":"Tracked stock requires the affected lot."}"#),
            ))
            .await
            .unwrap();
        assert_eq!(missing_lot.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            json(missing_lot).await["error"]["code"],
            "QUALITY_LOT_REQUIRED"
        );

        let stale_evidence = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/scrap-stale-evidence-hold/disposition",
                &p,
                true,
                Some("quality-scrap-stale-evidence"),
                Some(r#"{"expectedVersion":1,"attachmentSetIds":["scrap-stale-set"],"dispositionDate":"2026-11-10","quantity":"1","reason":"Stale evidence must not bind."}"#),
            ))
            .await
            .unwrap();
        assert_eq!(stale_evidence.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            json(stale_evidence).await["error"]["code"],
            "QUALITY_ATTACHMENT_INVALID"
        );
        let stale_rows: (i64, String) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM inventory_counts WHERE quality_hold_id='scrap-stale-evidence-hold'),(SELECT state FROM attachment_sets WHERE id='scrap-stale-set')",
        )
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(stale_rows, (0, "draft".into()));

        let mut package = db.begin().await.unwrap();
        sqlx::query(
            "UPDATE company_package_grants SET state='inactive',version=version+1,updated_at=$1 WHERE company_id='company' AND package_id IN ('supply-quality','supply')",
        )
        .bind(now + 1)
        .execute(&mut *package)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) \
             SELECT 'quality-release-scrap-inactive-'||package_id,'company',package_id,'fixture-a','quality-release-scrap-inactive',NULL,\
                    jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$1 \
             FROM company_package_grants WHERE company_id='company' AND package_id IN ('supply-quality','supply')",
        )
        .bind(now + 1)
        .execute(&mut *package)
        .await
        .unwrap();
        package.commit().await.unwrap();

        let denied_release = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/release-denied-hold/release",
                &p,
                true,
                Some("quality-release-denied-purchase"),
                Some(r#"{"expectedVersion":1,"reason":"An inactive purchase must not release."}"#),
            ))
            .await
            .unwrap();
        assert_eq!(denied_release.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            json(denied_release).await["error"]["code"],
            "PACKAGE_INACTIVE"
        );

        let denied_scrap = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/scrap-purchase-hold/disposition",
                &p,
                true,
                Some("quality-scrap-denied-purchase"),
                Some(r#"{"expectedVersion":1,"dispositionDate":"2026-11-10","quantity":"1","reason":"An inactive counts purchase must not scrap."}"#),
            ))
            .await
            .unwrap();
        assert_eq!(denied_scrap.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            json(denied_scrap).await["error"]["code"],
            "PACKAGE_INACTIVE"
        );
    })
    .await;
}
