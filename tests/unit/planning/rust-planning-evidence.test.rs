//! Owner-local unit tests for the native Planning evidence SEARCH port.
//! These prove bounded input/cursor validation, metric-exact owner SQL,
//! application admission and the fail-closed HTTP ingress. No PostgreSQL,
//! composition mount or browser journey is implied.
use base64::Engine;
use erpx_planning_server::public::evidence::{
    CAPACITY_FEASIBLE_RECOMMENDATIONS, EvidenceActor, EvidenceEligibility, EvidenceFuture,
    EvidenceHttpBackend, EvidenceHttpFuture, EvidenceHttpRequest, EvidenceHttpResponse,
    EvidenceHttpTarget, EvidenceQuery, EvidenceQueryInput, EvidenceRepository, EvidenceRow,
    PROPOSED_RECOMMENDATIONS, SHORTAGE_SCENARIOS, build_search_sql, cursor_id, planning_evidence_router,
    search_evidence, valid_rows, validate_rows,
};
use erpx_platform_runtime_server::public::http_support::{serde_json, serve};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

fn query(metric: &str) -> EvidenceQuery {
    EvidenceQuery {
        metric: metric.into(),
        from: "2026-01-01".into(),
        to: "2026-12-31".into(),
        query: "cell".into(),
        cursor_id: None,
        limit: 25,
    }
}

fn actor() -> EvidenceActor {
    EvidenceActor {
        user_id: "planner".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        assignment_id: "assignment".into(),
        department_id: None,
        role: "planner".into(),
        modules: vec!["planning".into()],
        permissions: vec!["records.read".into()],
    }
}

fn row(id: &str) -> EvidenceRow {
    serde_json::from_value(serde_json::json!({
        "id": id, "scenario_id": "scenario", "recommendation_id": null,
        "branch_id": "branch", "branch_name": "Branch", "warehouse_id": "warehouse",
        "warehouse_name": "Warehouse", "specification_id": "specification",
        "specification_name": "Spec", "output_product_id": "output",
        "output_product_name": "Product", "specification_version": 2,
        "demand_quantity_milli": "2000", "due_date": "2026-10-04",
        "capacity_minutes": "120", "scenario_status": "calculated",
        "scenario_version": 1, "scenario_created_at": 100, "scenario_updated_at": 300,
        "recommendation_status": null, "recommended_quantity_milli": null,
        "required_minutes": null, "feasible_date": null,
        "recommendation_version": null, "scenario_version_ref": null,
        "recommendation_created_at": null, "recommendation_updated_at": null,
        "shortage_count": "1", "shortage_items_json": "[]"
    }))
    .unwrap()
}

#[test]
fn owner_sql_is_metric_exact_and_bounded() {
    let scenario = build_search_sql(SHORTAGE_SCENARIOS);
    assert!(scenario.contains("s.id AS id"));
    assert!(scenario.contains("s.due_date::text BETWEEN $4 AND $5"));
    assert!(scenario.contains("shortageMilli"));
    assert!(scenario.contains("s.id>$14"));
    assert!(scenario.contains("LIMIT $15"));
    for metric in [PROPOSED_RECOMMENDATIONS, CAPACITY_FEASIBLE_RECOMMENDATIONS] {
        let sql = build_search_sql(metric);
        assert!(sql.contains("r.id AS id"));
        assert!(sql.contains("r.feasible_date::text BETWEEN $4 AND $5"));
        assert!(sql.contains("r.id>$14"));
    }
    assert!(build_search_sql(PROPOSED_RECOMMENDATIONS).contains("r.status='proposed'"));
    assert!(build_search_sql(CAPACITY_FEASIBLE_RECOMMENDATIONS)
        .contains("r.required_minutes<=s.capacity_minutes"));
    let false_sql = build_search_sql("bogus");
    assert!(false_sql.contains("AND FALSE"));
    let with_eligibility = build_search_sql(SHORTAGE_SCENARIOS);
    assert!(with_eligibility.contains("s.id=ANY($3)"));
}

#[test]
fn input_and_cursor_are_fail_closed() {
    let input = |limit: u32, query: &str| EvidenceQueryInput {
        metric: SHORTAGE_SCENARIOS.into(),
        from: "2026-01-01".into(),
        to: "2026-12-31".into(),
        query: query.into(),
        cursor: None,
        limit,
    };
    assert!(input(25, "cell").into_query().is_ok());
    assert!(input(0, "").into_query().is_err());
    assert!(input(51, "").into_query().is_err());
    assert!(input(25, &"x".repeat(101)).into_query().is_err());
    let opaque = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(r#"{"metric":"planning-shortage-scenarios","filters":{},"scope":["c","a","",""],"id":"rec-1"}"#);
    assert_eq!(cursor_id(&opaque).unwrap(), "rec-1");
    let mut with_cursor = input(25, "");
    with_cursor.cursor = Some(opaque);
    assert_eq!(with_cursor.into_query().unwrap().cursor_id.as_deref(), Some("rec-1"));
}

#[test]
fn row_validation_rejects_broken_pages() {
    assert!(validate_rows(SHORTAGE_SCENARIOS, &[row("a"), row("b")]).is_ok());
    assert!(validate_rows(SHORTAGE_SCENARIOS, &[row("a"), row("a")]).is_err());
    let mut many = Vec::new();
    for index in 0..52 {
        many.push(row(&format!("r-{index}")));
    }
    assert!(validate_rows(SHORTAGE_SCENARIOS, &many).is_err());
}

struct Fake {
    seen: Arc<Mutex<Vec<String>>>,
    reject: bool,
}

impl EvidenceRepository for Fake {
    fn search<'a>(
        &'a mut self,
        actor: &'a EvidenceActor,
        eligibility: &'a EvidenceEligibility,
        query: &'a EvidenceQuery,
    ) -> EvidenceFuture<'a, Vec<EvidenceRow>> {
        let seen = self.seen.clone();
        let reject = self.reject;
        Box::pin(async move {
            seen.lock().unwrap().push(format!(
                "{}|{}|{}|{}|{}",
                actor.company_id,
                actor.branch_id.as_deref().unwrap_or(""),
                query.metric,
                query.limit,
                match eligibility {
                    EvidenceEligibility::All => "all",
                    EvidenceEligibility::Scenarios(_) => "scoped",
                }
            ));
            if reject {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "EVIDENCE_SOURCE_UNAVAILABLE",
                    "native down",
                ));
            }
            let mut rows = vec![row("a"), row("b")];
            rows.truncate(query.limit as usize + 1);
            Ok(rows)
        })
    }
}

#[tokio::test]
async fn application_checks_admission_and_returns_full_rows() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut repository = Fake {
        seen: seen.clone(),
        reject: false,
    };
    let rows = search_evidence(
        &actor(),
        &EvidenceEligibility::Scenarios(vec!["scenario".into()]),
        &query(SHORTAGE_SCENARIOS),
        &mut repository,
    )
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].specification_name.as_deref(), Some("Spec"));
    assert_eq!(rows[0].shortage_items_json, "[]");
    assert_eq!(
        seen.lock().unwrap()[0],
        "company|branch|planning-shortage-scenarios|25|scoped"
    );

    let mut denied = actor();
    denied.modules = vec!["finance".into()];
    assert!(search_evidence(
        &denied,
        &EvidenceEligibility::All,
        &query(SHORTAGE_SCENARIOS),
        &mut repository,
    )
    .await
    .is_err());
}

struct Backend {
    seen: Arc<Mutex<Vec<String>>>,
    valid: bool,
}

impl EvidenceHttpBackend for Backend {
    fn search<'a>(
        &'a self,
        request: EvidenceHttpRequest<'a>,
    ) -> EvidenceHttpFuture<'a, EvidenceHttpResponse> {
        let seen = self.seen.clone();
        let valid = self.valid;
        Box::pin(async move {
            seen.lock().unwrap().push(format!(
                "{}|{}|{}|{}",
                request.host,
                request.holding_slug,
                request.query.metric,
                request.query.limit
            ));
            let rows = if valid {
                vec![serde_json::to_value(row("a")).unwrap()]
            } else {
                vec![serde_json::json!({"id":"a"})]
            };
            Ok(EvidenceHttpResponse {
                data: serde_json::json!({"rows": rows}),
            })
        })
    }
}

fn target(
    backend: Arc<dyn EvidenceHttpBackend>,
    host: &str,
    slug: &str,
) -> EvidenceHttpTarget {
    EvidenceHttpTarget {
        host: host.into(),
        holding_slug: slug.into(),
        origin: format!("https://{host}"),
        backend,
    }
}

async fn post(addr: std::net::SocketAddr, raw: String) -> (u16, serde_json::Value, String) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream.write_all(raw.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let text = String::from_utf8_lossy(&bytes).to_string();
    let status = text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse().ok())
        .unwrap();
    let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
    (
        status,
        serde_json::from_str(body).unwrap_or(serde_json::Value::Null),
        text.to_ascii_lowercase(),
    )
}

fn body() -> String {
    serde_json::json!({
        "metric": SHORTAGE_SCENARIOS,
        "from": "2026-01-01",
        "to": "2026-12-31",
        "query": "cell",
        "cursor": null,
        "limit": 25
    })
    .to_string()
}

fn request(path: &str, host: &str, payload: &str) -> String {
    format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nOrigin: https://{host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    )
}

#[tokio::test]
async fn http_ingress_is_transport_and_body_fail_closed() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let valid = Arc::new(Backend {
        seen: seen.clone(),
        valid: true,
    });
    let invalid = Arc::new(Backend {
        seen: seen.clone(),
        valid: false,
    });
    let router = planning_evidence_router(vec![
        target(valid, "acme.example.test", "acme"),
        target(invalid, "bad.example.test", "bad"),
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });
    let path = "/api/v1/holdings/acme/planning/evidence/search";

    let payload = body();
    let (status, body, headers) = post(addr, request(path, "acme.example.test", &payload)).await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["rows"][0]["id"], "a");
    assert!(headers.contains("cache-control: private, no-store"));

    let (status, response, _) = post(
        addr,
        request(&format!("{path}?view=all"), "acme.example.test", &payload),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(response["error"]["code"], "EVIDENCE_REQUEST_INVALID");

    let (status, response, _) =
        post(addr, request(path, "acme.example.test", &serde_json::json!({"metric":"bogus"}).to_string())).await;
    assert_eq!(status, 422);
    assert_eq!(response["error"]["code"], "EVIDENCE_REQUEST_INVALID");

    let (status, response, _) = post(
        addr,
        request(
            "/api/v1/holdings/bad/planning/evidence/search",
            "bad.example.test",
            &payload,
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(response["error"]["code"], "EVIDENCE_VALUE_INVALID");

    let (status, _, _) = post(addr, request("/api/v1/holdings/missing/planning/evidence/search", "acme.example.test", &payload)).await;
    assert_eq!(status, 404);

    let seen = seen.lock().unwrap().clone();
    assert_eq!(
        seen,
        vec![
            "acme.example.test|acme|planning-shortage-scenarios|25",
            "bad.example.test|bad|planning-shortage-scenarios|25"
        ]
    );
    server.abort();
}

#[test]
fn response_row_guard_rejects_partial_rows() {
    let full = serde_json::to_value(row("a")).unwrap();
    assert!(valid_rows(&serde_json::json!({"rows":[full]})));
    assert!(!valid_rows(&serde_json::json!({"rows":[{"id":"a"}]})));
}
