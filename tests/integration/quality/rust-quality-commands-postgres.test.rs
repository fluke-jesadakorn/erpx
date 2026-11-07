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
         VALUES('*','custom_finance_review',2,'[\"finance.read\",\"quality.read\",\"quality.hold\",\"quality.inspect\",\"quality.release\",\"module.quality.read\"]','[]','[\"holding\",\"branch\"]','fixture',0);\
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

        let unsupported = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/holdings/acme/quality/evidence-hold/inspect",
                &p,
                true,
                Some("quality-command-evidence-set"),
                Some(
                    r#"{"expectedVersion":2,"result":"fail","evidence":"Set evidence remains retained.","attachmentSetIds":["evidence-set"]}"#,
                ),
            ))
            .await
            .unwrap();
        let status = unsupported.status();
        let unsupported = json(unsupported).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{unsupported}");
        assert_eq!(
            unsupported["error"]["code"],
            "QUALITY_ATTACHMENT_SET_UNSUPPORTED"
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
