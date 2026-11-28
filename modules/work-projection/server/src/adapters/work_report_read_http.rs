//! Owner-local HTTP ingress for the native Work report projection read.
//!
//! Transport facts only. The caller supplies the already-authorized,
//! policy-scoped report rows as JSON; this route performs the matching,
//! aggregation and pagination in the owner's native PostgreSQL read. It opens
//! no transaction, reads no session and grants no authority. The Account
//! session and the serializable Holding unit stay composition responsibilities
//! supplied through the backend trait.
use crate::adapters::work_report_read::{WorkReportMode, WorkReportProjection, WorkReportRead};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body, header, post, serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

pub type WorkReportHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Bounded, validated projection request body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkReportHttpProjection {
    pub mode: WorkReportMode,
    pub scoped: String,
    pub start: i64,
    pub end: i64,
    pub page: i64,
    pub export_all: bool,
}

pub struct WorkReportHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub projection: &'a WorkReportHttpProjection,
}

#[derive(Clone, Debug)]
pub struct WorkReportHttpResponse {
    pub data: Value,
    /// Validated current context rendered by the composition backend.
    pub context: Value,
}

/// Composition owns the Account session and the current work role.
pub trait WorkReportHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: WorkReportHttpRequest<'a>,
    ) -> WorkReportHttpFuture<'a, WorkReportHttpResponse>;
}

pub struct WorkReportHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn WorkReportHttpBackend>,
}

pub struct WorkReportHttpState {
    targets: BTreeMap<(String, String), WorkReportHttpTarget>,
}

const MAX_SCOPED_BYTES: usize = 4 * 1024 * 1024;
const MAX_PAGE: i64 = 100_000;

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn input() -> BusinessError {
    BusinessError::invalid("VALIDATION_FAILED", "Use the current Work report route.")
}

fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
    )
}

fn unique_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(input());
    }
    first
        .map(|value| value.to_str().map_err(|_| input()))
        .transpose()
}

fn valid_origin(origin: &str) -> bool {
    let Some((scheme, authority)) = origin.split_once("://") else {
        return false;
    };
    if !matches!(scheme, "https" | "http") || authority.is_empty() || authority.len() > 255 {
        return false;
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    if host.is_empty()
        || host.len() > 253
        || host.starts_with('.')
        || host.ends_with('.')
        || !host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                && !label.starts_with('-')
                && !label.ends_with('-')
        })
    {
        return false;
    }
    match port {
        None => true,
        Some(port) => {
            !port.is_empty()
                && port.len() <= 5
                && port.bytes().all(|byte| byte.is_ascii_digit())
                && port.parse::<u16>().is_ok_and(|value| value > 0)
        }
    }
}

fn secured(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

fn json_response(status: StatusCode, value: Value) -> Response {
    let mut response = (status, Json(value)).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    secured(response)
}

fn error_response(error: BusinessError) -> Response {
    let status = match error.kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    };
    json_response(
        status,
        json!({"error": {"code": error.code, "message": error.message}}),
    )
}

/// Validates the caller-supplied projection without interpreting its rows.
pub fn parse_work_report_projection(value: &Value) -> Result<WorkReportHttpProjection, BusinessError> {
    let object = value.as_object().ok_or_else(input)?;
    if object.len() != 6 {
        return Err(input());
    }
    let text = |key: &str| -> Result<String, BusinessError> {
        object
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(input)
    };
    let integer = |key: &str| -> Result<i64, BusinessError> {
        object
            .get(key)
            .and_then(Value::as_i64)
            .ok_or_else(input)
    };
    let mode = match text("mode")?.as_str() {
        "period" => WorkReportMode::Period,
        "open" => WorkReportMode::Open,
        "drafts" => WorkReportMode::Drafts,
        _ => return Err(input()),
    };
    let scoped = text("scoped")?;
    if scoped.is_empty() || scoped.len() > MAX_SCOPED_BYTES || !scoped.starts_with('[') {
        return Err(input());
    }
    let start = integer("start")?;
    let end = integer("end")?;
    let page = integer("page")?;
    if !(1..=MAX_PAGE).contains(&page) || start < 0 || end < start {
        return Err(input());
    }
    let export_all = object.get("exportAll").and_then(Value::as_bool).ok_or_else(input)?;
    Ok(WorkReportHttpProjection {
        mode,
        scoped,
        start,
        end,
        page,
        export_all,
    })
}

impl<'a> From<&'a WorkReportHttpProjection> for WorkReportProjection<'a> {
    fn from(value: &'a WorkReportHttpProjection) -> Self {
        WorkReportProjection {
            mode: value.mode,
            scoped: &value.scoped,
            start: value.start,
            end: value.end,
            page: value.page,
        }
    }
}

/// Serializes the exact native report projection with the retained field names.
pub fn native_work_report_json(report: &WorkReportRead) -> Value {
    json!({
        "counts": {
            "total": report.counts.total,
            "open": report.counts.open,
            "drafts": report.counts.drafts,
            "unknown": report.counts.unknown,
            "blocked": report.counts.blocked,
            "units": report.counts.units,
        },
        "money": report.money.iter().map(|row| json!({"currency": row.currency, "amount": row.amount})).collect::<Vec<_>>(),
        "estimates": report.estimates.iter().map(|row| json!({"currency": row.currency, "amount": row.amount})).collect::<Vec<_>>(),
        "stageCounts": report.stage_counts.iter().map(|row| json!({"stage": row.stage, "count": row.count, "oldest": row.oldest})).collect::<Vec<_>>(),
        "rows": report.rows,
        "allRows": report.all_rows,
    })
}

impl WorkReportHttpState {
    pub fn new(targets: Vec<WorkReportHttpTarget>) -> Result<Self, BusinessError> {
        let mut configured = BTreeMap::new();
        for target in targets {
            if target.host.is_empty()
                || target.holding_slug.is_empty()
                || target.holding_slug.len() > 128
                || !target.holding_slug.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'-')
                })
                || !valid_origin(&target.origin)
                || configured
                    .insert((target.host.clone(), target.holding_slug.clone()), target)
                    .is_some()
            {
                return Err(BusinessError::invalid(
                    "WORK_REPORT_HTTP_CONFIGURATION_INVALID",
                    "Use exact distinct registered Holding targets.",
                ));
            }
        }
        Ok(Self { targets: configured })
    }

    fn target<'a>(
        &'a self,
        headers: &'a HeaderMap,
        holding_slug: &str,
    ) -> Result<&'a WorkReportHttpTarget, BusinessError> {
        let host = unique_header(headers, "host")?.ok_or_else(missing)?;
        let target = self
            .targets
            .get(&(host.to_owned(), holding_slug.to_owned()))
            .ok_or_else(missing)?;
        let origin = unique_header(headers, "origin")?;
        if origin.is_some_and(|value| value != target.origin) {
            return Err(origin_forbidden());
        }
        Ok(target)
    }

    pub async fn read(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        projection: &WorkReportHttpProjection,
    ) -> Response {
        match self.read_inner(headers, holding_slug, projection).await {
            Ok(response) => response,
            Err(error) => error_response(error),
        }
    }

    async fn read_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        projection: &WorkReportHttpProjection,
    ) -> Result<Response, BusinessError> {
        let target = self.target(headers, holding_slug)?;
        let response = target
            .backend
            .read(WorkReportHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers,
                projection,
            })
            .await?;
        Ok(json_response(
            StatusCode::OK,
            json!({"data": response.data, "context": response.context}),
        ))
    }
}

async fn read_handler(
    State(state): State<Arc<WorkReportHttpState>>,
    Path(holding_slug): Path<String>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let bytes = match body::to_bytes(body, MAX_SCOPED_BYTES + 4096).await {
        Ok(bytes) => bytes,
        Err(_) => return error_response(BusinessError::invalid("VALIDATION_FAILED", "Report request too large.")),
    };
    let value = match serde_json::from_slice::<Value>(&bytes) {
        Ok(value) => value,
        Err(_) => return error_response(input()),
    };
    let projection = match parse_work_report_projection(&value) {
        Ok(projection) => projection,
        Err(error) => return error_response(error),
    };
    state
        .read(&parts.headers, &holding_slug, &projection)
        .await
}

pub fn work_report_router(targets: Vec<WorkReportHttpTarget>) -> Result<Router, BusinessError> {
    let state = Arc::new(WorkReportHttpState::new(targets)?);
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/work/native-report",
            post(read_handler),
        )
        .with_state(state))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn projection(export_all: bool) -> Value {
        json!({"mode": "open", "scoped": "[{}]", "start": 0, "end": 10, "page": 1, "exportAll": export_all})
    }

    #[test]
    fn parses_the_bounded_projection() {
        let parsed = parse_work_report_projection(&projection(true)).unwrap();
        assert_eq!(parsed.mode, WorkReportMode::Open);
        assert_eq!(parsed.scoped, "[{}]");
        assert!(parsed.export_all);
        let native = WorkReportProjection::from(&parsed);
        assert_eq!(native.page, 1);
    }

    #[test]
    fn rejects_invalid_projections() {
        assert!(parse_work_report_projection(&json!({})).is_err());
        let mut bad = projection(false);
        bad.as_object_mut().unwrap().insert("mode".into(), json!("other"));
        assert!(parse_work_report_projection(&bad).is_err());
        let mut bad = projection(false);
        bad.as_object_mut().unwrap().insert("page".into(), json!(0));
        assert!(parse_work_report_projection(&bad).is_err());
        let mut bad = projection(false);
        bad.as_object_mut().unwrap().insert("scoped".into(), json!("not-json"));
        assert!(parse_work_report_projection(&bad).is_err());
    }

    #[test]
    fn serializes_the_exact_native_projection() {
        let report = WorkReportRead {
            counts: crate::adapters::work_report_read::WorkReportCounts {
                total: 4, open: 2, drafts: 1, unknown: 1, blocked: 1, units: 3.5,
            },
            money: vec![],
            estimates: vec![],
            stage_counts: vec![],
            rows: vec![json!({"id": "one"})],
            all_rows: vec![json!({"id": "one"}), json!({"id": "two"})],
        };
        let value = native_work_report_json(&report);
        assert_eq!(value["counts"]["total"], 4);
        assert_eq!(value["rows"][0]["id"], "one");
        assert_eq!(value["allRows"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn router_rejects_duplicate_targets() {
        let backend: Arc<dyn WorkReportHttpBackend> = Arc::new(FakeBackend);
        let target = || WorkReportHttpTarget {
            host: "holding.erpxbrowser.test:3102".into(),
            holding_slug: "acme".into(),
            origin: "http://127.0.0.1:3102".into(),
            backend: backend.clone(),
        };
        assert!(work_report_router(vec![target()]).is_ok());
        assert!(work_report_router(vec![target(), target()]).is_err());
    }

    struct FakeBackend;
    impl WorkReportHttpBackend for FakeBackend {
        fn read<'a>(
            &'a self,
            _: WorkReportHttpRequest<'a>,
        ) -> WorkReportHttpFuture<'a, WorkReportHttpResponse> {
            Box::pin(async {
                Ok(WorkReportHttpResponse {
                    data: json!({}),
                    context: json!({}),
                })
            })
        }
    }
}
