//! Owner-local HTTP ingress for native Billing target evidence commands.
//!
//! Covers `record-operating-cost`, `review-target-capacity` and
//! `approve-target-extra`. Transport facts only; the Account/Platform session,
//! current Holding authority, trusted Beta storage limit and serializable
//! Control unit remain composition responsibilities supplied through the
//! backend trait. Rollout approval retains its dedicated route.
use crate::domain::target_evidence_command::{
    ApproveTargetExtra, BillingTargetEvidenceCommand, RecordOperatingCost, ReviewTargetCapacity,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 8 * 1024;

pub type BillingTargetEvidenceHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct BillingTargetEvidenceHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub command: BillingTargetEvidenceCommand,
    pub headers: &'a HeaderMap,
}

#[derive(Clone, Debug)]
pub struct BillingTargetEvidenceHttpResponse {
    /// Exact `{ok:true}` command receipt.
    pub data: Value,
    /// Validated current context rendered by the composition backend.
    pub context: Value,
}

/// Composition owns the session, authority, trusted storage cap and unit.
pub trait BillingTargetEvidenceHttpBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: BillingTargetEvidenceHttpRequest<'a>,
    ) -> BillingTargetEvidenceHttpFuture<'a, BillingTargetEvidenceHttpResponse>;
}

pub struct BillingTargetEvidenceHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn BillingTargetEvidenceHttpBackend>,
}

pub struct BillingTargetEvidenceHttpState {
    targets: BTreeMap<(String, String), BillingTargetEvidenceHttpTarget>,
}

struct ParsedTarget<'a> {
    target: &'a BillingTargetEvidenceHttpTarget,
    host: &'a str,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
    )
}

fn request_invalid() -> BusinessError {
    BusinessError::invalid("VALIDATION_FAILED", "Use a valid target evidence request.")
}

fn command_key_required() -> BusinessError {
    BusinessError::invalid(
        "COMMAND_KEY_REQUIRED",
        "Use a valid Idempotency-Key for this action.",
    )
}

fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BILLING_TARGET_EVIDENCE_SOURCE_INVALID",
        "The native target evidence response is unavailable.",
    )
}

fn unique_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(request_invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| request_invalid()))
        .transpose()
}

fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        !rest.is_empty()
            && !rest
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
    }) || origin
        .strip_prefix("http://127.0.0.1:")
        .is_some_and(|port| {
            !port.is_empty() && port.len() <= 5 && port.bytes().all(|byte| byte.is_ascii_digit())
        })
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OperatingCostBody {
    version: i64,
    reason: String,
    target_month: String,
    category: String,
    amount_satang: i64,
    evidence_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CapacityBody {
    version: i64,
    reason: String,
    target_month: String,
    projected_original_bytes: i64,
    primary_available_bytes: i64,
    backup_available_bytes: i64,
    reserved_bytes: i64,
    company_limit_bytes: i64,
    evidence_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExtraBody {
    version: i64,
    reason: String,
    target_month: String,
    extra_kind: String,
    amount_satang: i64,
    service_start: i64,
    service_end: i64,
    note: String,
    evidence_id: String,
}

impl BillingTargetEvidenceHttpState {
    pub fn new(targets: Vec<BillingTargetEvidenceHttpTarget>) -> Result<Self, BusinessError> {
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
                    "BILLING_TARGET_EVIDENCE_HTTP_CONFIGURATION_INVALID",
                    "Use exact distinct registered Holding targets.",
                ));
            }
        }
        Ok(Self {
            targets: configured,
        })
    }

    fn target<'a>(
        &'a self,
        headers: &'a HeaderMap,
        holding_slug: &str,
    ) -> Result<ParsedTarget<'a>, BusinessError> {
        let host = unique_header(headers, "host")?.ok_or_else(missing)?;
        let target = self
            .targets
            .get(&(host.to_owned(), holding_slug.to_owned()))
            .ok_or_else(missing)?;
        if unique_header(headers, "origin")?.is_some_and(|value| value != target.origin) {
            return Err(origin_forbidden());
        }
        Ok(ParsedTarget { target, host })
    }

    async fn execute(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
        build: impl FnOnce(&[u8], CommandKey) -> Result<BillingTargetEvidenceCommand, BusinessError>,
    ) -> Response {
        let result: Result<Response, BusinessError> = async {
            if query.is_some() {
                return Err(request_invalid());
            }
            let target = self.target(headers, holding_slug)?;
            let content_type = unique_header(headers, "content-type")?
                .unwrap_or_default()
                .split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase();
            if content_type != "application/json" {
                return Err(request_invalid());
            }
            let key = unique_header(headers, "idempotency-key")?
                .filter(|value| !value.is_empty())
                .ok_or_else(command_key_required)?;
            if !(8..=180).contains(&key.len())
                || !key.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'.' | b'-')
                })
            {
                return Err(command_key_required());
            }
            if body.len() > BODY_LIMIT {
                return Err(request_invalid());
            }
            let command = build(body, CommandKey::new(key)?)?;
            command.validate()?;
            let response = target
                .target
                .backend
                .execute(BillingTargetEvidenceHttpRequest {
                    host: target.host,
                    holding_slug: &target.target.holding_slug,
                    command,
                    headers,
                })
                .await?;
            if response.data != json!({"ok": true}) {
                return Err(source_invalid());
            }
            Ok(json_response(
                StatusCode::OK,
                json!({"data": {"ok": true}, "context": response.context}),
            ))
        }
        .await;
        result.unwrap_or_else(error_response)
    }
}

fn operating_cost(
    body: &[u8],
    key: CommandKey,
) -> Result<BillingTargetEvidenceCommand, BusinessError> {
    let parsed =
        serde_json::from_slice::<OperatingCostBody>(body).map_err(|_| request_invalid())?;
    Ok(BillingTargetEvidenceCommand::OperatingCost(
        RecordOperatingCost {
            key,
            expected_version: parsed.version,
            reason: parsed.reason,
            target_month: parsed.target_month,
            category: parsed.category,
            amount_satang: parsed.amount_satang,
            evidence_id: RecordId::new(parsed.evidence_id)?,
        },
    ))
}

fn capacity(body: &[u8], key: CommandKey) -> Result<BillingTargetEvidenceCommand, BusinessError> {
    let parsed = serde_json::from_slice::<CapacityBody>(body).map_err(|_| request_invalid())?;
    Ok(BillingTargetEvidenceCommand::Capacity(
        ReviewTargetCapacity {
            key,
            expected_version: parsed.version,
            reason: parsed.reason,
            target_month: parsed.target_month,
            projected_original_bytes: parsed.projected_original_bytes,
            primary_available_bytes: parsed.primary_available_bytes,
            backup_available_bytes: parsed.backup_available_bytes,
            reserved_bytes: parsed.reserved_bytes,
            company_limit_bytes: parsed.company_limit_bytes,
            evidence_id: RecordId::new(parsed.evidence_id)?,
        },
    ))
}

fn extra(body: &[u8], key: CommandKey) -> Result<BillingTargetEvidenceCommand, BusinessError> {
    let parsed = serde_json::from_slice::<ExtraBody>(body).map_err(|_| request_invalid())?;
    Ok(BillingTargetEvidenceCommand::Extra(ApproveTargetExtra {
        key,
        expected_version: parsed.version,
        reason: parsed.reason,
        target_month: parsed.target_month,
        extra_kind: parsed.extra_kind,
        amount_satang: parsed.amount_satang,
        service_start: parsed.service_start,
        service_end: parsed.service_end,
        note: parsed.note,
        evidence_id: RecordId::new(parsed.evidence_id)?,
    }))
}

async fn operating_handler(
    State(state): State<Arc<BillingTargetEvidenceHttpState>>,
    Path(holding_slug): Path<String>,
    request: Request,
) -> Response {
    let query = request.uri().query().map(str::to_owned);
    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return error_response(request_invalid()),
    };
    state
        .execute(
            &headers,
            &holding_slug,
            query.as_deref(),
            &body,
            operating_cost,
        )
        .await
}

async fn capacity_handler(
    State(state): State<Arc<BillingTargetEvidenceHttpState>>,
    Path(holding_slug): Path<String>,
    request: Request,
) -> Response {
    let query = request.uri().query().map(str::to_owned);
    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return error_response(request_invalid()),
    };
    state
        .execute(&headers, &holding_slug, query.as_deref(), &body, capacity)
        .await
}

async fn extra_handler(
    State(state): State<Arc<BillingTargetEvidenceHttpState>>,
    Path(holding_slug): Path<String>,
    request: Request,
) -> Response {
    let query = request.uri().query().map(str::to_owned);
    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return error_response(request_invalid()),
    };
    state
        .execute(&headers, &holding_slug, query.as_deref(), &body, extra)
        .await
}

pub fn billing_target_evidence_router(
    targets: Vec<BillingTargetEvidenceHttpTarget>,
) -> Result<Router, BusinessError> {
    let state = Arc::new(BillingTargetEvidenceHttpState::new(targets)?);
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/billing/target/evidence/operating-cost",
            post(operating_handler),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/billing/target/evidence/capacity",
            post(capacity_handler),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/billing/target/extra",
            post(extra_handler),
        )
        .with_state(state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Fake {
        seen: Mutex<Vec<String>>,
    }

    impl BillingTargetEvidenceHttpBackend for Fake {
        fn execute<'a>(
            &'a self,
            request: BillingTargetEvidenceHttpRequest<'a>,
        ) -> BillingTargetEvidenceHttpFuture<'a, BillingTargetEvidenceHttpResponse> {
            Box::pin(async move {
                self.seen.lock().unwrap().push(format!(
                    "{}:{}:{}:{}",
                    request.host,
                    request.holding_slug,
                    request.command.action(),
                    request.command.expected_version()
                ));
                Ok(BillingTargetEvidenceHttpResponse {
                    data: json!({"ok": true}),
                    context: json!({"companyId": "acme-co"}),
                })
            })
        }
    }

    fn state() -> (BillingTargetEvidenceHttpState, Arc<Fake>) {
        let fake = Arc::new(Fake {
            seen: Mutex::new(Vec::new()),
        });
        let state = BillingTargetEvidenceHttpState::new(vec![BillingTargetEvidenceHttpTarget {
            host: "holding.erpxbrowser.test:3102".into(),
            holding_slug: "acme".into(),
            origin: "http://127.0.0.1:3102".into(),
            backend: fake.clone(),
        }])
        .unwrap();
        (state, fake)
    }

    fn headers(origin: Option<&str>, key: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("host", "holding.erpxbrowser.test:3102".parse().unwrap());
        if let Some(origin) = origin {
            headers.insert("origin", origin.parse().unwrap());
        }
        if let Some(key) = key {
            headers.insert("idempotency-key", key.parse().unwrap());
        }
        headers.insert("content-type", "application/json".parse().unwrap());
        headers
    }

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Runtime::new().unwrap().block_on(future)
    }

    const COST: &[u8] = br#"{"version":2,"reason":"Reviewed actual cost","targetMonth":"2026-09","category":"infrastructure","amountSatang":25000,"evidenceId":"evidence-1"}"#;
    const CAPACITY: &[u8] = br#"{"version":2,"reason":"Reviewed capacity","targetMonth":"2026-12","projectedOriginalBytes":10000000000,"primaryAvailableBytes":30000000000,"backupAvailableBytes":30000000000,"reservedBytes":1000000000,"companyLimitBytes":25000000000,"evidenceId":"evidence-1"}"#;
    const EXTRA: &[u8] = br#"{"version":2,"reason":"Reviewed extra service","targetMonth":"2026-12","extraKind":"setup","amountSatang":10000,"serviceStart":1796112000000,"serviceEnd":1796198400000,"note":"Setup","evidenceId":"evidence-1"}"#;

    #[test]
    fn forwards_exact_evidence_commands() {
        let (state, fake) = state();
        type Build = fn(&[u8], CommandKey) -> Result<BillingTargetEvidenceCommand, BusinessError>;
        for (build, body) in [
            (operating_cost as Build, COST),
            (capacity as Build, CAPACITY),
            (extra as Build, EXTRA),
        ] {
            let response = block_on(state.execute(
                &headers(Some("http://127.0.0.1:3102"), Some("billing:evidence:1")),
                "acme",
                None,
                body,
                build,
            ));
            assert_eq!(response.status(), StatusCode::OK);
        }
        assert_eq!(
            fake.seen.lock().unwrap().as_slice(),
            [
                "holding.erpxbrowser.test:3102:acme:record-operating-cost:2",
                "holding.erpxbrowser.test:3102:acme:review-target-capacity:2",
                "holding.erpxbrowser.test:3102:acme:approve-target-extra:2"
            ]
        );
    }

    #[test]
    fn rejects_wrong_origin_holding_key_and_body() {
        let (state, _) = state();
        for (origin, slug, key, body) in [
            (Some("https://evil.test"), "acme", Some("billing:evidence:1"), COST),
            (Some("http://127.0.0.1:3102"), "other", Some("billing:evidence:1"), COST),
            (Some("http://127.0.0.1:3102"), "acme", None, COST),
            (
                Some("http://127.0.0.1:3102"),
                "acme",
                Some("billing:evidence:1"),
                br#"{"version":2,"reason":"Reviewed actual cost","targetMonth":"2026-09","category":"infrastructure","amountSatang":25000,"evidenceId":"evidence-1","extra":true}"#,
            ),
        ] {
            let response = block_on(state.execute(&headers(origin, key), slug, None, body, operating_cost));
            assert!(response.status().is_client_error(), "{origin:?} {slug} {key:?}");
        }
    }
}
