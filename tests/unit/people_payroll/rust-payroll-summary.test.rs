use erpx_server_people_payroll::public::payroll_summary::{
    PayrollReadActor, PayrollReadFuture, PayrollReadRepository, PayrollRunRow, PayrollSummaryRow,
    payroll_runs, payroll_summary,
};
use erpx_server_people_payroll::public::payroll_summary_http::{
    PayrollReadHttpBackend, PayrollReadHttpFuture, PayrollReadHttpRequest, PayrollReadHttpResponse,
    PayrollReadHttpTarget, people_payroll_router,
};
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use std::sync::{Arc, Mutex};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);
const EXPIRES: i64 = 1_800_000_100_000;

fn company(value: &str) -> CompanyId {
    CompanyId::new(value).unwrap()
}

fn actor(permissions: &[&str]) -> PayrollReadActor {
    PayrollReadActor {
        company_id: company("company"),
        branch_id: None,
        department_id: None,
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        expires_at: EXPIRES,
    }
}

fn summary_row(month: &str, currency: &str) -> PayrollSummaryRow {
    PayrollSummaryRow {
        company_id: "company".into(),
        pay_month: month.into(),
        currency: currency.into(),
        run_count: 1,
        headcount: 2,
        gross_minor: 10_000,
        deductions_minor: 1_000,
        employer_cost_minor: 500,
        net_minor: 9_000,
    }
}

fn run_row(id: &str) -> PayrollRunRow {
    PayrollRunRow {
        company_id: "company".into(),
        id: id.into(),
        period_start: "2026-10-01".into(),
        period_end: "2026-10-31".into(),
        pay_date: "2026-10-31".into(),
        currency: "THB".into(),
        status: "posted".into(),
        mode: "production".into(),
        revision: 1,
        version: 1,
        totals_json: Some("{}".into()),
        payment_batch_id: None,
        payment_status: None,
        net_payable_minor: None,
    }
}

struct FakeRepo {
    summary: Vec<PayrollSummaryRow>,
    runs: Vec<PayrollRunRow>,
    summary_calls: usize,
    run_calls: usize,
}

impl PayrollReadRepository for FakeRepo {
    type Error = BusinessError;

    fn summary<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: Option<&'a erpx_server_people_payroll::public::payroll_summary::PayrollMonth>,
    ) -> PayrollReadFuture<'a, Vec<PayrollSummaryRow>, Self::Error> {
        self.summary_calls += 1;
        Box::pin(async move { Ok(self.summary.clone()) })
    }

    fn runs<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: bool,
    ) -> PayrollReadFuture<'a, Vec<PayrollRunRow>, Self::Error> {
        self.run_calls += 1;
        Box::pin(async move { Ok(self.runs.clone()) })
    }
}

fn repo() -> FakeRepo {
    FakeRepo {
        summary: vec![summary_row("2026-10", "THB")],
        runs: vec![run_row("run")],
        summary_calls: 0,
        run_calls: 0,
    }
}

#[tokio::test]
async fn summary_requires_company_payroll_policy_and_a_calendar_month_before_the_source_read() {
    let mut source = repo();
    let rows = payroll_summary(&actor(&["payroll.read"]), &mut source, None, NOW)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(source.summary_calls, 1);

    let error = payroll_summary(&actor(&[]), &mut source, None, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PERMISSION_REQUIRED");
    assert_eq!(error.kind, FailureKind::Forbidden);

    let mut branch = actor(&["payroll.read"]);
    branch.branch_id = Some(RecordId::new("branch").unwrap());
    let error = payroll_summary(&branch, &mut source, None, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "COMPANY_PAYROLL_SCOPE_REQUIRED");

    let mut expired = actor(&["payroll.read"]);
    expired.expires_at = NOW.get();
    let error = payroll_summary(&expired, &mut source, None, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");
    assert_eq!(error.kind, FailureKind::Conflict);

    for month in ["2026-13", "2026-00", "2026-1", "x", ""] {
        let error = payroll_summary(&actor(&["payroll.read"]), &mut source, Some(month), NOW)
            .await
            .unwrap_err();
        assert_eq!(error.code, "INVALID_PAY_MONTH");
    }
    assert_eq!(
        source.summary_calls, 1,
        "invalid policy/month must not read"
    );
}

#[tokio::test]
async fn runs_allow_payment_only_read_but_never_branch_scope_or_a_foreign_company() {
    let mut source = repo();
    let rows = payroll_runs(&actor(&["payroll.payment.read"]), &mut source, NOW)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(source.run_calls, 1);

    let error = payroll_runs(&actor(&[]), &mut source, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PERMISSION_REQUIRED");

    let mut department = actor(&["payroll.read"]);
    department.department_id = Some(RecordId::new("department").unwrap());
    let error = payroll_runs(&department, &mut source, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "COMPANY_PAYROLL_SCOPE_REQUIRED");

    source.runs[0].company_id = "foreign".into();
    let error = payroll_runs(&actor(&["payroll.read"]), &mut source, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PAYROLL_READ_SOURCE_INVALID");
}

#[tokio::test]
async fn invalid_source_rows_duplicate_keys_and_cross_company_rows_fail_closed() {
    let mut source = repo();
    source.summary = vec![summary_row("2026-10", "THB"), summary_row("2026-10", "THB")];
    let error = payroll_summary(&actor(&["payroll.read"]), &mut source, None, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PAYROLL_READ_SOURCE_INVALID");
    assert_eq!(error.kind, FailureKind::Unavailable);

    source.summary = vec![summary_row("2026-10", "THB")];
    source.summary[0].company_id = "foreign".into();
    let error = payroll_summary(&actor(&["payroll.read"]), &mut source, None, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PAYROLL_READ_SOURCE_INVALID");

    source.runs = vec![run_row("run"), run_row("run")];
    let error = payroll_runs(&actor(&["payroll.read"]), &mut source, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PAYROLL_READ_SOURCE_INVALID");

    source.runs = vec![run_row("run")];
    source.runs[0].totals_json = Some("[]".into());
    let error = payroll_runs(&actor(&["payroll.read"]), &mut source, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PAYROLL_READ_SOURCE_INVALID");
}

struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
    fail: bool,
}
impl PayrollReadHttpBackend for Probe {
    fn read<'a>(
        &'a self,
        request: PayrollReadHttpRequest<'a>,
    ) -> PayrollReadHttpFuture<'a, PayrollReadHttpResponse> {
        Box::pin(async move {
            self.seen
                .lock()
                .unwrap()
                .push(format!("read:{:?}", request.kind));
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "PAYROLL_READ_CHANGED",
                    "Synthetic changed payroll source.",
                ));
            }
            Ok(PayrollReadHttpResponse {
                data: erpx_server_runtime::public::http_support::serde_json::json!([]),
                context: erpx_server_runtime::public::http_support::serde_json::json!({"userId":"user"}),
            })
        })
    }
}

async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, serde_json::Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream.write_all(raw.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let text = String::from_utf8_lossy(&bytes);
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
    )
}

#[tokio::test]
async fn owner_router_forwards_exact_targets_queries_and_bounded_errors() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = people_payroll_router(vec![
        PayrollReadHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(Probe {
                seen: seen.clone(),
                fail: false,
            }),
        },
        PayrollReadHttpTarget {
            host: "fail.example.test".into(),
            holding_slug: "fail".into(),
            origin: "https://fail.example.test".into(),
            backend: Arc::new(Probe {
                seen: seen.clone(),
                fail: true,
            }),
        },
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
    });
    let get = |path: &str| {
        format!("GET {path} HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n\r\n")
    };
    let (status, body) = http(addr, get("/api/v1/holdings/acme/people/payroll/summary")).await;
    assert_eq!(status, 200);
    assert_eq!(body["data"], serde_json::json!([]));
    let (status, _) = http(
        addr,
        get("/api/v1/holdings/acme/people/payroll/summary?payMonth=2026-10"),
    )
    .await;
    assert_eq!(status, 200);
    let (status, body) = http(
        addr,
        get("/api/v1/holdings/acme/people/payroll/summary?payMonth=2026-10&payMonth=2026-11"),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "PEOPLE_PAYROLL_REQUEST_INVALID");
    let (status, _) = http(addr, get("/api/v1/holdings/acme/people/payroll/runs")).await;
    assert_eq!(status, 200);
    let (status, _) = http(addr, get("/api/v1/holdings/missing/people/payroll/summary")).await;
    assert_eq!(status, 404);
    let (status, body) = http(
        addr,
        "GET /api/v1/holdings/fail/people/payroll/runs HTTP/1.1\r\nHost: fail.example.test\r\nConnection: close\r\n\r\n".into(),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "PAYROLL_READ_CHANGED");
    let seen = seen.lock().unwrap().clone();
    assert!(
        seen.iter()
            .any(|value| value == "read:Summary { pay_month: None }")
    );
    assert!(
        seen.iter()
            .any(|value| value == "read:Summary { pay_month: Some(\"2026-10\") }")
    );
    assert!(seen.iter().any(|value| value == "read:Runs"));
    server.abort();
}
