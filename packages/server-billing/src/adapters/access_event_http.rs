//! Owner-local HTTP ingress for the native Billing evidence-access audit
//! command.
//!
//! Transport facts only: registered Host/Holding target, exact Origin,
//! Idempotency-Key, bounded exact JSON input and business-error mapping. The
//! Account/Platform session, current Holding authority and serializable Control
//! unit remain composition responsibilities supplied through the backend trait.
//! This module never mints a session, actor or Holding scope.

use crate::adapters::billing_owner_write::BillingOwnerWriteInput;
use crate::domain::access_event::AccessEvent;
use crate::domain::billing_owner_write::OwnerWriteKind;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, post, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind, HoldingId};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 8 * 1024;

pub type BillingAccessEventHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct BillingAccessEventHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub command: AccessEvent,
    pub headers: &'a HeaderMap,
}

#[derive(Clone, Debug)]
pub struct BillingAccessEventHttpResponse {
    /// Exact `{ok:true}` command receipt.
    pub data: Value,
    /// Validated current context rendered by the composition backend.
    pub context: Value,
}

/// One owner-wide write command: an allow-listed `kind` plus positional values.
#[derive(Clone, Debug)]
pub struct BillingOwnerWriteHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub command: &'a BillingOwnerWriteInput,
    pub headers: &'a HeaderMap,
    pub key: &'a str,
}

#[derive(Clone, Debug)]
pub struct BillingOwnerWriteHttpResponse {
    /// Owner-authored statement `{sql, values}` for the caller's own unit.
    pub data: Value,
    /// Validated current context rendered by the composition backend.
    pub context: Value,
}

/// Composition owns the session, authority and serializable Control unit.
pub trait BillingAccessEventHttpBackend: Send + Sync {
    fn append<'a>(
        &'a self,
        request: BillingAccessEventHttpRequest<'a>,
    ) -> BillingAccessEventHttpFuture<'a, BillingAccessEventHttpResponse>;

    fn owner_write<'a>(
        &'a self,
        request: BillingOwnerWriteHttpRequest<'a>,
    ) -> BillingAccessEventHttpFuture<'a, BillingOwnerWriteHttpResponse>;
}

pub struct BillingAccessEventHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn BillingAccessEventHttpBackend>,
}

pub struct BillingAccessEventHttpState {
    targets: BTreeMap<(String, String), BillingAccessEventHttpTarget>,
}

struct ParsedTarget<'a> {
    target: &'a BillingAccessEventHttpTarget,
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
    BusinessError::invalid(
        "BILLING_ACCESS_EVENT_REQUEST_INVALID",
        "Use one bounded Billing access event request.",
    )
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
        "BILLING_ACCESS_EVENT_SOURCE_INVALID",
        "The native Billing access audit response is unavailable.",
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
struct AccessEventBody {
    event_id: String,
    evidence_id: String,
    outcome: String,
    reason: String,
}

impl BillingAccessEventHttpState {
    pub fn new(targets: Vec<BillingAccessEventHttpTarget>) -> Result<Self, BusinessError> {
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
                    "BILLING_ACCESS_EVENT_HTTP_CONFIGURATION_INVALID",
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

    pub async fn append(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self.append_inner(headers, holding_slug, query, body).await {
            Ok(response) => response,
            Err(error) => error_response(error),
        }
    }

    async fn append_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Result<Response, BusinessError> {
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
        let parsed =
            serde_json::from_slice::<AccessEventBody>(body).map_err(|_| request_invalid())?;
        let command = crate::adapters::access_event_postgres::build_access_event(
            crate::adapters::access_event_postgres::access_event_command_key(key)?,
            crate::adapters::access_event_postgres::access_event_id(&parsed.event_id)?,
            crate::adapters::access_event_postgres::access_event_id(&parsed.evidence_id)?,
            crate::adapters::access_event_postgres::access_event_outcome(&parsed.outcome)?,
            parsed.reason,
        );
        command.validate()?;
        let response = target
            .target
            .backend
            .append(BillingAccessEventHttpRequest {
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

    /// Owner-wide typed write: exact Host/Origin/Idempotency-Key, allow-listed
    /// kind, bounded exact JSON. The backend returns the owner-authored
    /// statement, which the caller executes in its own serializable unit.
    pub async fn owner_write(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self
            .owner_write_inner(headers, holding_slug, query, body)
            .await
        {
            Ok(response) => response,
            Err(error) => error_response(error),
        }
    }

    async fn owner_write_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Result<Response, BusinessError> {
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
        let command =
            serde_json::from_slice::<BillingOwnerWriteInput>(body).map_err(|_| request_invalid())?;
        OwnerWriteKind::parse(&command.kind).map_err(|_| request_invalid())?;
        let _ = HoldingId::new(target.target.holding_slug.as_str()).map_err(|_| request_invalid())?;
        let response = target
            .target
            .backend
            .owner_write(BillingOwnerWriteHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                command: &command,
                headers,
                key,
            })
            .await?;
        let sql = response
            .data
            .get("sql")
            .and_then(Value::as_str)
            .ok_or_else(source_invalid)?;
        if sql.is_empty()
            || !response
                .data
                .get("values")
                .and_then(Value::as_array)
                .is_some_and(|values| !values.is_empty() && values.len() <= 64)
        {
            return Err(source_invalid());
        }
        Ok(json_response(
            StatusCode::OK,
            json!({"data": response.data, "context": response.context}),
        ))
    }
}

async fn append_handler(
    State(state): State<Arc<BillingAccessEventHttpState>>,
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
        .append(&headers, &holding_slug, query.as_deref(), &body)
        .await
}

async fn owner_write_handler(
    State(state): State<Arc<BillingAccessEventHttpState>>,
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
        .owner_write(&headers, &holding_slug, query.as_deref(), &body)
        .await
}

pub fn billing_access_event_router(
    targets: Vec<BillingAccessEventHttpTarget>,
) -> Result<Router, BusinessError> {
    let state = Arc::new(BillingAccessEventHttpState::new(targets)?);
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/billing/evidence/access",
            post(append_handler),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/billing/owner-write",
            post(owner_write_handler),
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

    impl BillingAccessEventHttpBackend for Fake {
        fn append<'a>(
            &'a self,
            request: BillingAccessEventHttpRequest<'a>,
        ) -> BillingAccessEventHttpFuture<'a, BillingAccessEventHttpResponse> {
            Box::pin(async move {
                self.seen.lock().unwrap().push(format!(
                    "{}:{}:{}:{}",
                    request.host,
                    request.holding_slug,
                    request.command.event_id.as_str(),
                    request.command.outcome.as_str()
                ));
                Ok(BillingAccessEventHttpResponse {
                    data: json!({"ok": true}),
                    context: json!({"companyId": "acme-co"}),
                })
            })
        }

        fn owner_write<'a>(
            &'a self,
            request: BillingOwnerWriteHttpRequest<'a>,
        ) -> BillingAccessEventHttpFuture<'a, BillingOwnerWriteHttpResponse> {
            Box::pin(async move {
                self.seen.lock().unwrap().push(format!(
                    "{}:{}:{}:{}",
                    request.host,
                    request.holding_slug,
                    request.command.kind,
                    request.key
                ));
                Ok(BillingOwnerWriteHttpResponse {
                    data: json!({
                        "sql": "UPDATE billing_accounts SET version=version+1 WHERE holding_id=?",
                        "values": ["h001"]
                    }),
                    context: json!({"companyId": "acme-co"}),
                })
            })
        }
    }

    fn state() -> (BillingAccessEventHttpState, Arc<Fake>) {
        let fake = Arc::new(Fake {
            seen: Mutex::new(Vec::new()),
        });
        let state = BillingAccessEventHttpState::new(vec![BillingAccessEventHttpTarget {
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

    const BODY: &[u8] = br#"{"eventId":"billing-access-1","evidenceId":"evidence-1","outcome":"denied","reason":"EVIDENCE_NOT_AVAILABLE"}"#;

    #[test]
    fn appends_exact_target_and_event() {
        let (state, fake) = state();
        let response = block_on(state.append(
            &headers(Some("http://127.0.0.1:3102"), Some("billing:access:1")),
            "acme",
            None,
            BODY,
        ));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            fake.seen.lock().unwrap().as_slice(),
            ["holding.erpxbrowser.test:3102:acme:billing-access-1:denied"]
        );
    }

    #[test]
    fn rejects_wrong_origin_holding_key_and_body() {
        let (state, _) = state();
        for (origin, slug, key, body) in [
            (
                Some("https://evil.test"),
                "acme",
                Some("billing:access:1"),
                BODY,
            ),
            (
                Some("http://127.0.0.1:3102"),
                "other",
                Some("billing:access:1"),
                BODY,
            ),
            (Some("http://127.0.0.1:3102"), "acme", None, BODY),
            (
                Some("http://127.0.0.1:3102"),
                "acme",
                Some("billing:access:1"),
                br#"{"eventId":"billing-access-1","evidenceId":"evidence-1","outcome":"denied","reason":"EVIDENCE_NOT_AVAILABLE","extra":true}"#,
            ),
        ] {
            let response = block_on(state.append(&headers(origin, key), slug, None, body));
            assert!(
                response.status().is_client_error(),
                "{origin:?} {slug} {key:?}"
            );
        }
    }

    const OWNER_BODY: &[u8] =
        br#"{"kind":"increment_account_version","values":["h001"],"now":1}"#;

    #[test]
    fn owner_write_forwards_exact_kind_and_key() {
        let (state, fake) = state();
        let response = block_on(state.owner_write(
            &headers(Some("http://127.0.0.1:3102"), Some("billing:owner:1")),
            "acme",
            None,
            OWNER_BODY,
        ));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            fake.seen.lock().unwrap().as_slice(),
            ["holding.erpxbrowser.test:3102:acme:increment_account_version:billing:owner:1"]
        );
    }

    #[test]
    fn owner_write_rejects_unknown_kind_before_backend() {
        let (state, fake) = state();
        let response = block_on(state.owner_write(
            &headers(Some("http://127.0.0.1:3102"), Some("billing:owner:1")),
            "acme",
            None,
            br#"{"kind":"not_a_billing_write","values":["h001"],"now":1}"#,
        ));
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(fake.seen.lock().unwrap().is_empty());
    }
}
