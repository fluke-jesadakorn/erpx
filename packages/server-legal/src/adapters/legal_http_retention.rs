//! Owner-local HTTP ingress for the native Legal retention commands.
//!
//! Transport facts only: exact registered Holding target, Origin, idempotency
//! key, bounded JSON-object body and owner error mapping. Composition resolves
//! the normal Account session to `LegalAuxiliaryActor` and supplies the exact
//! `DatabasePool`/unit it will hand to `execute_retention`. The router never
//! opens a transaction and never calls `execute_retention` itself.
use crate::{
    application::retention::RetentionCommand, domain::auxiliary::LegalAuxiliaryActor,
    domain::legal_records::sha256_hex,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, post,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind};
use serde::Serialize;
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

pub const RETENTION_BODY_LIMIT: usize = 512 * 1024;

/// Actor resolution belongs to composition: it must consume the normal Account
/// session proof, never a header-derived shortcut.
pub struct LegalRetentionHttpSessionRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
}

/// The backend receives the proof-derived actor and the caller-supplied pool
/// (or transaction handle) it needs to call `execute_retention`.
pub struct LegalRetentionHttpCommandRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub actor: &'a LegalAuxiliaryActor,
    pub pool: &'a DatabasePool,
    pub now: i64,
    pub key: &'a CommandKey,
    pub request_hash: &'a str,
    pub command: &'a RetentionCommand,
    pub raw: &'a Value,
}

pub type LegalRetentionHttpFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Value, BusinessError>> + Send + 'a>>;
pub type LegalRetentionActorFuture<'a> =
    Pin<Box<dyn Future<Output = Result<LegalAuxiliaryActor, BusinessError>> + Send + 'a>>;

/// Composition implements both methods. `resolve_actor` derives the current
/// actor from the Account proof; `command` calls `execute_retention` (or
/// `execute_retention_command` with `PgLegalRetention`) on its own unit.
pub trait LegalRetentionHttpBackend: Send + Sync {
    fn resolve_actor<'a>(
        &'a self,
        request: LegalRetentionHttpSessionRequest<'a>,
    ) -> LegalRetentionActorFuture<'a>;
    fn command<'a>(
        &'a self,
        request: LegalRetentionHttpCommandRequest<'a>,
    ) -> LegalRetentionHttpFuture<'a>;
}

pub struct LegalRetentionHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    /// The pool composition will pass to `execute_retention`. Retained here so
    /// the backend receives one caller-owned handle instead of inventing one.
    pub pool: DatabasePool,
    pub clock: Arc<dyn Clock + Send + Sync>,
    pub backend: Arc<dyn LegalRetentionHttpBackend>,
}

#[derive(Debug, Serialize)]
pub struct LegalRetentionCommandReply {
    pub data: Value,
}

pub struct LegalRetentionHttpService {
    targets: BTreeMap<(String, String), LegalRetentionHttpTarget>,
}

#[derive(Debug)]
struct ParsedRetentionRequest {
    key: CommandKey,
    request_hash: String,
    raw: Value,
    command: RetentionCommand,
}

fn malformed(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn status(kind: FailureKind) -> StatusCode {
    match kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn error_response(error: BusinessError) -> Response {
    (
        status(error.kind),
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

fn private_json(value: &impl Serialize) -> Response {
    let mut response = (StatusCode::OK, Json(value)).into_response();
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    response
}

fn unique_header(
    headers: &HeaderMap,
    name: HeaderName,
) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(malformed(
            "LEGAL_REQUEST_INVALID",
            "Use one unambiguous request header value.",
        ));
    }
    first
        .map(|value| {
            value
                .to_str()
                .map_err(|_| malformed("LEGAL_REQUEST_INVALID", "Use a valid request header."))
        })
        .transpose()
}

fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        let (host, port) = match rest.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (rest, None),
        };
        !host.is_empty()
            && !host
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
            && port.is_none_or(|port| {
                (1..=5).contains(&port.len())
                    && port.bytes().all(|byte| byte.is_ascii_digit())
                    && port.parse::<u16>().is_ok_and(|value| value > 0)
            })
    })
}

/// Validate Origin, Idempotency-Key, body bound and the retention wire before
/// any session or transaction work. Split out so the transport rules are unit
/// testable without a database.
fn parse_request(
    expected_origin: &str,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<ParsedRetentionRequest, BusinessError> {
    let origin = unique_header(headers, header::ORIGIN)?;
    if origin != Some(expected_origin) {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ORIGIN_FORBIDDEN",
            "Use the application origin.",
        ));
    }
    let key = match unique_header(headers, HeaderName::from_static("idempotency-key"))? {
        Some(key) => CommandKey::new(key)?,
        None => {
            return Err(BusinessError::new(
                FailureKind::InvalidInput,
                "COMMAND_KEY_REQUIRED",
                "Idempotency-Key is required.",
            ));
        }
    };
    if body.len() > RETENTION_BODY_LIMIT {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "BODY_TOO_LARGE",
            "Use a smaller Legal request.",
        ));
    }
    let request_hash = sha256_hex(body);
    let raw: Value = match serde_json::from_slice(body) {
        Ok(value @ Value::Object(_)) => value,
        _ => {
            return Err(BusinessError::new(
                FailureKind::InvalidInput,
                "INVALID_JSON",
                "Use a JSON object body.",
            ));
        }
    };
    let command = crate::application::retention::parse_retention_command(&raw)?;
    Ok(ParsedRetentionRequest {
        key,
        request_hash,
        raw,
        command,
    })
}

impl LegalRetentionHttpService {
    pub fn new(targets: Vec<LegalRetentionHttpTarget>) -> Result<Self, BusinessError> {
        let mut configured = BTreeMap::new();
        for target in targets {
            let key = (target.host.clone(), target.holding_slug.clone());
            if target.host.is_empty()
                || target.holding_slug.is_empty()
                || !valid_origin(&target.origin)
                || configured.contains_key(&key)
            {
                return Err(BusinessError::invalid(
                    "LEGAL_HTTP_CONFIGURATION_INVALID",
                    "Use exact distinct registered Holding targets.",
                ));
            }
            configured.insert(key, target);
        }
        Ok(Self {
            targets: configured,
        })
    }

    pub async fn command(
        &self,
        host: &str,
        holding_slug: &str,
        headers: &HeaderMap,
        body: &[u8],
    ) -> Response {
        let Some(target) = self
            .targets
            .get(&(host.to_owned(), holding_slug.to_owned()))
            .or_else(|| {
                unique_header(headers, header::HOST)
                    .ok()
                    .flatten()
                    .and_then(|registered| {
                        self.targets
                            .get(&(registered.to_owned(), holding_slug.to_owned()))
                    })
            })
        else {
            return error_response(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ));
        };
        let parsed = match parse_request(&target.origin, headers, body) {
            Ok(parsed) => parsed,
            Err(error) => return error_response(error),
        };
        let actor = match target
            .backend
            .resolve_actor(LegalRetentionHttpSessionRequest {
                host,
                holding_slug,
                headers,
            })
            .await
        {
            Ok(actor) => actor,
            Err(error) => return error_response(error),
        };
        let now = target.clock.now().get();
        match target
            .backend
            .command(LegalRetentionHttpCommandRequest {
                host,
                holding_slug,
                headers,
                actor: &actor,
                pool: &target.pool,
                now,
                key: &parsed.key,
                request_hash: &parsed.request_hash,
                command: &parsed.command,
                raw: &parsed.raw,
            })
            .await
        {
            Ok(data) => private_json(&LegalRetentionCommandReply { data }),
            Err(error) => error_response(error),
        }
    }
}

struct LegalRetentionHttpState {
    service: Arc<LegalRetentionHttpService>,
}

async fn command(
    State(state): State<Arc<LegalRetentionHttpState>>,
    Path(holding_slug): Path<String>,
    headers: HeaderMap,
    request: Request,
) -> Response {
    let host = unique_header(&headers, header::HOST)
        .ok()
        .flatten()
        .unwrap_or_default();
    let target_ok = state
        .service
        .targets
        .get(&(host.to_owned(), holding_slug.clone()))
        .is_some();
    if !target_ok {
        return error_response(BusinessError::new(
            FailureKind::NotFound,
            "NOT_FOUND",
            "Not found.",
        ));
    }
    let body = match to_bytes(request.into_body(), RETENTION_BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => {
            return error_response(BusinessError::new(
                FailureKind::InvalidInput,
                "BODY_TOO_LARGE",
                "Use a smaller Legal request.",
            ));
        }
    };
    state
        .service
        .command(&host, &holding_slug, &headers, &body)
        .await
}

/// Mount point for composition. Reads stay on `legal_auxiliary_router`; this
/// router serves only the native retention command family.
pub fn legal_retention_router(
    targets: Vec<LegalRetentionHttpTarget>,
) -> Result<Router, BusinessError> {
    let service = Arc::new(LegalRetentionHttpService::new(targets)?);
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/legal/retention/commands",
            post(command),
        )
        .with_state(Arc::new(LegalRetentionHttpState { service })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn headers(origin: Option<&str>, key: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("acme.example"));
        if let Some(origin) = origin {
            headers.insert(header::ORIGIN, HeaderValue::from_str(origin).unwrap());
        }
        if let Some(key) = key {
            headers.insert(
                HeaderName::from_static("idempotency-key"),
                HeaderValue::from_str(key).unwrap(),
            );
        }
        headers
    }

    fn body(action: &str) -> Vec<u8> {
        json!({"action":action,"id":"item-one","expectedVersion":2,"reason":"Retention elapsed"})
            .to_string()
            .into_bytes()
    }

    #[test]
    fn origins_must_be_https_without_path_or_wildcard() {
        assert!(valid_origin("https://acme.example"));
        assert!(valid_origin("https://acme.example:8443"));
        assert!(!valid_origin("http://acme.example"));
        assert!(!valid_origin("https://acme.example/path"));
        assert!(!valid_origin("https://"));
        assert!(!valid_origin("https://acme.example:0"));
    }

    #[test]
    fn request_requires_origin_exact_key_and_a_json_object() {
        let error = parse_request(
            "https://acme.example",
            &headers(Some("https://other.example"), Some("command-key-one")),
            &body("retention.assign"),
        )
        .unwrap_err();
        assert_eq!(error.code, "ORIGIN_FORBIDDEN");
        let error = parse_request(
            "https://acme.example",
            &headers(None, Some("command-key-one")),
            &body("retention.assign"),
        )
        .unwrap_err();
        assert_eq!(error.code, "ORIGIN_FORBIDDEN");
        let error = parse_request(
            "https://acme.example",
            &headers(Some("https://acme.example"), None),
            &body("retention.assign"),
        )
        .unwrap_err();
        assert_eq!(error.code, "COMMAND_KEY_REQUIRED");
        let error = parse_request(
            "https://acme.example",
            &headers(Some("https://acme.example"), Some("command-key-one")),
            b"[]",
        )
        .unwrap_err();
        assert_eq!(error.code, "INVALID_JSON");
        let error = parse_request(
            "https://acme.example",
            &headers(Some("https://acme.example"), Some("command-key-one")),
            &vec![b'x'; RETENTION_BODY_LIMIT + 1],
        )
        .unwrap_err();
        assert_eq!(error.code, "BODY_TOO_LARGE");
        // Duplicate origin headers are ambiguous and rejected.
        let mut duplicate = headers(
            Some("https://acme.example"),
            Some("command-key-one"),
        );
        duplicate.append(
            header::ORIGIN,
            HeaderValue::from_static("https://acme.example"),
        );
        assert_eq!(
            parse_request(
                "https://acme.example",
                &duplicate,
                &body("retention.assign")
            )
            .unwrap_err()
            .code,
            "LEGAL_REQUEST_INVALID"
        );
    }

    #[test]
    fn request_accepts_only_native_retention_actions() {
        for action in [
            "retention.assign",
            "retention.request",
            "retention.approve",
            "retention.cancel",
            "retention.retry",
        ] {
            assert!(
                parse_request(
                    "https://acme.example",
                    &headers(Some("https://acme.example"), Some("command-key-one")),
                    &body(action),
                )
                .is_ok(),
                "{action}"
            );
        }
        let error = parse_request(
            "https://acme.example",
            &headers(Some("https://acme.example"), Some("command-key-one")),
            &body("trust.add"),
        )
        .unwrap_err();
        assert_eq!(error.code, "LEGAL_REQUEST_INVALID");
        let parsed = parse_request(
            "https://acme.example",
            &headers(Some("https://acme.example"), Some("command-key-one")),
            &body("retention.assign"),
        )
        .unwrap();
        assert_eq!(parsed.request_hash.len(), 64);
        assert_eq!(parsed.command.action.operation(), "retention.assign");
        assert_eq!(parsed.key.as_str(), "command-key-one");
    }
}
