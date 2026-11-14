//! Owner-local HTTP contract for native Legal trust material commands.
//!
//! Only transport is here: exact registered Holding target, Origin, idempotency
//! key, bounded JSON, wire parsing and error mapping. Composition resolves the
//! current Account session to a `LegalTrustActor` and supplies the serializable
//! Holding transaction plus the Workflow/Identity bridges that
//! `execute_legal_trust_command` already requires. The offline PDF verifier,
//! revocation/timestamp/modification and three-state behaviour are unchanged.
use crate::domain::{
    pdf_verification::TrustMaterialKind,
    trust_management::{LegalTrustActor, LegalTrustCommand, LegalTrustReceipt},
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, post,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

pub const TRUST_BODY_LIMIT: usize = 2 * 1024 * 1024 + 64 * 1024;
pub const TRUST_NAME_MIN: usize = 3;
pub const TRUST_NAME_MAX: usize = 200;
pub const TRUST_SOURCE_MAX: usize = 2000;
pub const TRUST_CONTENT_MIN: usize = 20;
pub const TRUST_CONTENT_MAX: usize = 2_000_000;

pub type LegalTrustHttpFuture<'a> =
    Pin<Box<dyn Future<Output = Result<LegalTrustReceipt, BusinessError>> + Send + 'a>>;

pub struct LegalTrustHttpCommandRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub key: &'a CommandKey,
    pub request_hash: &'a str,
    pub command: &'a LegalTrustCommand,
}

/// Composition implements this with the current Account session, Company scope
/// and the existing owner transaction. It must call
/// `execute_legal_trust_command`; it may not persist trust material itself.
pub trait LegalTrustHttpBackend: Send + Sync {
    fn command<'a>(
        &'a self,
        request: LegalTrustHttpCommandRequest<'a>,
    ) -> LegalTrustHttpFuture<'a>;
}

pub struct LegalTrustHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn LegalTrustHttpBackend>,
}

pub struct LegalTrustHttpService {
    targets: BTreeMap<(String, String), LegalTrustHttpTarget>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegalTrustCommandWire {
    pub action: String,
    pub material_id: Option<String>,
    pub material_version: Option<i64>,
    pub kind: Option<String>,
    pub name: Option<String>,
    pub source: Option<String>,
    pub content: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegalTrustCommandReply {
    pub id: String,
    pub version: u32,
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
        .map(|value| value.to_str().map_err(|_| malformed("LEGAL_REQUEST_INVALID", "Use a valid request header.")))
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

fn bounded_text(value: &Option<String>, min: usize, max: usize, code: &'static str, message: &'static str) -> Result<String, BusinessError> {
    let value = value.as_deref().map(str::trim).unwrap_or_default();
    let length = value.chars().count();
    if length < min || length > max {
        return Err(malformed(code, message));
    }
    Ok(value.to_owned())
}

/// Parse the retained `trust.*` body into the owner command. Additions are
/// bounded; a URL must be HTTPS; the exact material version is required for a
/// decision so a stale manager cannot approve a changed fingerprint.
pub fn parse_trust_command(raw: &Value) -> Result<LegalTrustCommand, BusinessError> {
    let wire: LegalTrustCommandWire = serde_json::from_value(raw.clone())
        .map_err(|_| malformed("LEGAL_REQUEST_INVALID", "Use a current Legal trust action."))?;
    match wire.action.as_str() {
        "trust.add" => {
            let kind = match wire.kind.as_deref() {
                Some("root") => TrustMaterialKind::Root,
                Some("intermediate") => TrustMaterialKind::Intermediate,
                Some("crl") => TrustMaterialKind::Crl,
                Some("ocsp") => TrustMaterialKind::Ocsp,
                _ => {
                    return Err(malformed(
                        "LEGAL_REQUEST_INVALID",
                        "Choose root, intermediate, crl or ocsp trust material.",
                    ));
                }
            };
            let name = bounded_text(
                &wire.name,
                TRUST_NAME_MIN,
                TRUST_NAME_MAX,
                "LEGAL_REQUEST_INVALID",
                "Name the trust material in 3 to 200 characters.",
            )?;
            let source = bounded_text(
                &wire.source,
                1,
                TRUST_SOURCE_MAX,
                "LEGAL_REQUEST_INVALID",
                "Use an HTTPS source for the trust material.",
            )?;
            if !source.starts_with("https://") {
                return Err(malformed(
                    "LEGAL_REQUEST_INVALID",
                    "Use an HTTPS source for the trust material.",
                ));
            }
            let content = bounded_text(
                &wire.content,
                TRUST_CONTENT_MIN,
                TRUST_CONTENT_MAX,
                "LEGAL_REQUEST_INVALID",
                "Supply the specified certificate or revocation material.",
            )?;
            Ok(LegalTrustCommand::Add {
                kind,
                name,
                source,
                content,
            })
        }
        "trust.approve" | "trust.retire" => {
            let id = wire
                .material_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty() && value.len() <= 200)
                .ok_or_else(|| {
                    malformed(
                        "LEGAL_REQUEST_INVALID",
                        "Choose the current trust material.",
                    )
                })?;
            let version = wire
                .material_version
                .filter(|value| *value > 0)
                .ok_or_else(|| {
                    BusinessError::new(
                        FailureKind::InvalidInput,
                        "LEGAL_VERSION_CONFLICT",
                        "Trust material changed. Refresh before saving.",
                    )
                })?;
            let version = u64::try_from(version).map_err(|_| {
                malformed(
                    "LEGAL_VERSION_CONFLICT",
                    "Trust material changed. Refresh before saving.",
                )
            })?;
            let id = erpx_server_shared::public::RecordId::new(id)?;
            if wire.action == "trust.approve" {
                Ok(LegalTrustCommand::Approve { id, version })
            } else {
                Ok(LegalTrustCommand::Retire { id, version })
            }
        }
        _ => Err(malformed(
            "LEGAL_REQUEST_INVALID",
            "Choose a current Legal trust action.",
        )),
    }
}

/// Company-scope rule for trust management, kept with the owner command so a
/// composition mount cannot grant branch-only trust authority.
pub fn authorize_trust_actor(actor: &LegalTrustActor) -> Result<(), BusinessError> {
    if actor.holding_id.as_str().is_empty() || actor.company_id.as_str().is_empty() {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "LEGAL_SCOPE_REQUIRED",
            "Select a current Company before managing Legal trust.",
        ));
    }
    if actor.branch_id.is_some() || actor.department_id.is_some() {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "LEGAL_COMPANY_SCOPE",
            "Manage trust at Company scope.",
        ));
    }
    Ok(())
}

impl LegalTrustHttpService {
    pub fn new(targets: Vec<LegalTrustHttpTarget>) -> Result<Self, BusinessError> {
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
        let origin = match unique_header(headers, header::ORIGIN) {
            Ok(origin) => origin,
            Err(error) => return error_response(error),
        };
        if origin != Some(target.origin.as_str()) {
            return error_response(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            ));
        }
        let key = match unique_header(headers, HeaderName::from_static("idempotency-key")) {
            Ok(Some(key)) => match CommandKey::new(key) {
                Ok(key) => key,
                Err(error) => return error_response(error),
            },
            Ok(None) => {
                return error_response(BusinessError::new(
                    FailureKind::InvalidInput,
                    "COMMAND_KEY_REQUIRED",
                    "Idempotency-Key is required.",
                ));
            }
            Err(error) => return error_response(error),
        };
        if body.len() > TRUST_BODY_LIMIT {
            return error_response(BusinessError::new(
                FailureKind::InvalidInput,
                "BODY_TOO_LARGE",
                "Use smaller trust material.",
            ));
        }
        let request_hash = crate::domain::legal_records::sha256_hex(body);
        let raw: Value = match serde_json::from_slice(body) {
            Ok(value @ Value::Object(_)) => value,
            _ => {
                return error_response(BusinessError::new(
                    FailureKind::InvalidInput,
                    "INVALID_JSON",
                    "Use a JSON object body.",
                ));
            }
        };
        let command = match parse_trust_command(&raw) {
            Ok(command) => command,
            Err(error) => return error_response(error),
        };
        match target
            .backend
            .command(LegalTrustHttpCommandRequest {
                host,
                holding_slug,
                headers,
                key: &key,
                request_hash: &request_hash,
                command: &command,
            })
            .await
        {
            Ok(receipt) => private_json(&serde_json::json!({
                "data": LegalTrustCommandReply {
                    id: receipt.id.as_str().to_owned(),
                    version: receipt.version,
                }
            })),
            Err(error) => error_response(error),
        }
    }

    fn target<'a>(
        &'a self,
        headers: &HeaderMap,
        slug: &str,
    ) -> Result<&'a LegalTrustHttpTarget, BusinessError> {
        let host = unique_header(headers, header::HOST)?.ok_or_else(|| {
            BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
        })?;
        self.targets
            .get(&(host.to_owned(), slug.to_owned()))
            .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."))
    }
}

struct LegalTrustHttpState {
    service: Arc<LegalTrustHttpService>,
}

async fn command(
    State(state): State<Arc<LegalTrustHttpState>>,
    Path(holding_slug): Path<String>,
    headers: HeaderMap,
    request: Request,
) -> Response {
    let host = unique_header(&headers, header::HOST)
        .ok()
        .flatten()
        .unwrap_or_default();
    let target_ok = state.service.target(&headers, &holding_slug).is_ok();
    if !target_ok {
        return error_response(BusinessError::new(
            FailureKind::NotFound,
            "NOT_FOUND",
            "Not found.",
        ));
    }
    let body = match to_bytes(request.into_body(), TRUST_BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => {
            return error_response(BusinessError::new(
                FailureKind::InvalidInput,
                "BODY_TOO_LARGE",
                "Use smaller trust material.",
            ));
        }
    };
    state
        .service
        .command(&host, &holding_slug, &headers, &body)
        .await
}

/// Mount point for composition. The existing offline PDF trust/revocation/
/// timestamp/modification three-state verifier is untouched.
pub fn legal_trust_router(
    targets: Vec<LegalTrustHttpTarget>,
) -> Result<Router, BusinessError> {
    let service = Arc::new(LegalTrustHttpService::new(targets)?);
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/legal/trust/commands",
            post(command),
        )
        .with_state(Arc::new(LegalTrustHttpState { service })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_shared::public::{CompanyId, HoldingId, RecordId, UserId};
    use serde_json::json;
    use std::sync::Mutex;

    struct Backend {
        calls: Mutex<Vec<(String, String)>>,
        fail: Option<BusinessError>,
    }

    impl LegalTrustHttpBackend for Backend {
        fn command<'a>(
            &'a self,
            request: LegalTrustHttpCommandRequest<'a>,
        ) -> LegalTrustHttpFuture<'a> {
            self.calls.lock().unwrap().push((
                request.command.operation().to_owned(),
                request.key.as_str().to_owned(),
            ));
            let fail = self.fail.clone();
            Box::pin(async move {
                if let Some(error) = fail {
                    return Err(error);
                }
                Ok(LegalTrustReceipt {
                    id: CompanyId::new("company-one").unwrap(),
                    version: 1,
                })
            })
        }
    }

    fn service(fail: Option<BusinessError>) -> (LegalTrustHttpService, Arc<Backend>) {
        let backend = Arc::new(Backend {
            calls: Mutex::new(Vec::new()),
            fail,
        });
        let service = LegalTrustHttpService::new(vec![LegalTrustHttpTarget {
            host: "acme.example".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example".into(),
            backend: backend.clone(),
        }])
        .unwrap();
        (service, backend)
    }

    fn headers(origin: &str, key: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("acme.example"));
        headers.insert(header::ORIGIN, HeaderValue::from_str(origin).unwrap());
        if let Some(key) = key {
            headers.insert(
                HeaderName::from_static("idempotency-key"),
                HeaderValue::from_str(key).unwrap(),
            );
        }
        headers
    }

    fn body(action: &str) -> Vec<u8> {
        json!({
            "action": action,
            "kind": "root",
            "name": "Issuing root",
            "source": "https://ca.example/root.cer",
            "content": "MIIBszCCAVmgAwIBAgIUexamplecontent",
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn add_requires_bounded_https_material() {
        let base = json!({
            "action": "trust.add",
            "kind": "intermediate",
            "name": "Issuing intermediate",
            "source": "https://ca.example/intermediate.cer",
            "content": "MIIBszCCAVmgAwIBAgIUexamplecontent",
        });
        assert_eq!(parse_trust_command(&base).unwrap().operation(), "trust.add");
        let mut bad_kind = base.clone();
        bad_kind["kind"] = json!("other");
        let mut short_name = base.clone();
        short_name["name"] = json!("Na");
        let mut plain_source = base.clone();
        plain_source["source"] = json!("http://ca.example/a");
        let mut short_content = base.clone();
        short_content["content"] = json!("short");
        let mut missing_kind = base.clone();
        missing_kind.as_object_mut().unwrap().remove("kind");
        for value in [bad_kind, short_name, plain_source, short_content, missing_kind] {
            assert!(parse_trust_command(&value).is_err(), "{value}");
        }
    }

    #[test]
    fn decisions_require_a_current_positive_version() {
        let approve = json!({"action":"trust.approve","materialId":"material-one","materialVersion":2});
        assert_eq!(parse_trust_command(&approve).unwrap().operation(), "trust.approve");
        assert!(parse_trust_command(&json!({"action":"trust.retire","materialId":"material-one"})).is_err());
        assert!(parse_trust_command(&json!({"action":"trust.approve","materialId":"","materialVersion":1})).is_err());
        assert!(parse_trust_command(&json!({"action":"trust.approve","materialId":"material-one","materialVersion":0})).is_err());
        assert!(parse_trust_command(&json!({"action":"trust.delete"})).is_err());
    }

    #[test]
    fn trust_management_requires_company_scope() {
        let actor = |branch: Option<&str>| LegalTrustActor {
            holding_id: HoldingId::new("holding-one").unwrap(),
            company_id: CompanyId::new("company-one").unwrap(),
            user_id: UserId::new("user-one").unwrap(),
            branch_id: branch.map(|value| RecordId::new(value).unwrap()),
            department_id: None,
        };
        assert!(authorize_trust_actor(&actor(None)).is_ok());
        let error = authorize_trust_actor(&actor(Some("branch-one"))).unwrap_err();
        assert_eq!(error.code, "LEGAL_COMPANY_SCOPE");
        assert_eq!(error.kind, FailureKind::Forbidden);
    }

    #[tokio::test]
    async fn command_requires_exact_origin_and_idempotency() {
        let (service, backend) = service(None);
        let body = body("trust.add");
        let wrong_origin = service
            .command(
                "acme.example",
                "acme",
                &headers("https://other.example", Some("command-key-one")),
                &body,
            )
            .await;
        assert_eq!(wrong_origin.into_parts().0.status, StatusCode::FORBIDDEN);
        let no_key = service
            .command(
                "acme.example",
                "acme",
                &headers("https://acme.example", None),
                &body,
            )
            .await;
        assert_eq!(no_key.into_parts().0.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(backend.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn add_command_reaches_the_owner_backend_with_the_exact_key() {
        let (service, backend) = service(None);
        let response = service
            .command(
                "acme.example",
                "acme",
                &headers("https://acme.example", Some("command-key-one")),
                &body("trust.add"),
            )
            .await;
        let (parts, body) = response.into_parts();
        assert_eq!(parts.status, StatusCode::OK);
        let value: Value = serde_json::from_slice(&to_bytes(body, 4096).await.unwrap()).unwrap();
        assert_eq!(value["data"]["id"], "company-one");
        assert_eq!(value["data"]["version"], 1);
        assert_eq!(
            backend.calls.lock().unwrap().as_slice(),
            [("trust.add".to_owned(), "command-key-one".to_owned())]
        );
    }

    #[tokio::test]
    async fn owner_failures_map_to_the_exact_status() {
        let (service, _) = service(Some(BusinessError::new(
            FailureKind::Conflict,
            "LEGAL_TRUST_VERSION_CONFLICT",
            "Trust material changed.",
        )));
        let response = service
            .command(
                "acme.example",
                "acme",
                &headers("https://acme.example", Some("command-key-one")),
                &body("trust.add"),
            )
            .await;
        assert_eq!(response.into_parts().0.status, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn unknown_holding_or_oversized_body_fails_closed() {
        let (service, backend) = service(None);
        let wrong = service
            .command(
                "acme.example",
                "other",
                &headers("https://acme.example", Some("command-key-one")),
                &body("trust.add"),
            )
            .await;
        assert_eq!(wrong.into_parts().0.status, StatusCode::NOT_FOUND);
        let oversized = vec![b'x'; TRUST_BODY_LIMIT + 1];
        let response = service
            .command(
                "acme.example",
                "acme",
                &headers("https://acme.example", Some("command-key-one")),
                &oversized,
            )
            .await;
        assert_eq!(
            response.into_parts().0.status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert!(backend.calls.lock().unwrap().is_empty());
    }
}
