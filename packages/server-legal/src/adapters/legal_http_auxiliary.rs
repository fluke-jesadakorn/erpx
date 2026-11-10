//! Owner-local HTTP ingress for the native auxiliary Legal reads.
//!
//! Transport facts only: exact registered Holding target, bounded path/query,
//! private response headers and owner error mapping. The current Account
//! session, selected Company/branch/department and the serializable owner unit
//! stay with the composition backend.
use crate::{
    application::auxiliary::parse_auxiliary_path, domain::auxiliary::LegalAuxiliaryRead,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Query, Request, Response, Router,
    State, StatusCode, get, header,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const QUERY_LIMIT: usize = 1024;

pub type LegalAuxiliaryHttpFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Value, BusinessError>> + Send + 'a>>;

pub struct LegalAuxiliaryHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub read: LegalAuxiliaryRead,
}

/// Composition implements this over the current Account session and one
/// serializable Holding transaction. It never returns a TypeScript fallback.
pub trait LegalAuxiliaryHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: LegalAuxiliaryHttpRequest<'a>,
    ) -> LegalAuxiliaryHttpFuture<'a>;
}

pub struct LegalAuxiliaryHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn LegalAuxiliaryHttpBackend>,
}

struct LegalAuxiliaryHttpState {
    targets: BTreeMap<(String, String), LegalAuxiliaryHttpTarget>,
}

#[derive(Debug, Default, Deserialize)]
pub struct LegalAuxiliaryQueryWire {
    #[serde(rename = "ruleId")]
    pub rule_id: Option<String>,
    #[serde(rename = "asOf")]
    pub as_of: Option<String>,
    pub q: Option<String>,
    #[serde(rename = "page")]
    pub page: Option<String>,
    pub kind: Option<String>,
    pub state: Option<String>,
    pub mine: Option<String>,
    #[serde(rename = "format")]
    pub format: Option<String>,
    pub category: Option<String>,
    pub search: Option<String>,
}

impl LegalAuxiliaryQueryWire {
    fn pairs(&self) -> Vec<(String, String)> {
        let mut pairs = Vec::new();
        for (key, value) in [
            ("ruleId", &self.rule_id),
            ("asOf", &self.as_of),
            ("q", &self.q),
            ("page", &self.page),
            ("kind", &self.kind),
            ("state", &self.state),
            ("mine", &self.mine),
            ("format", &self.format),
            ("category", &self.category),
            ("search", &self.search),
        ] {
            if let Some(value) = value {
                pairs.push((key.to_owned(), value.clone()));
            }
        }
        pairs
    }
}

fn malformed(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, code, message)
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

fn private_json(value: &Value) -> Response {
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
    origin
        .strip_prefix("https://")
        .is_some_and(|rest| !rest.is_empty() && !rest.bytes().any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@')))
}

fn split_path(raw: &str) -> Result<Vec<String>, BusinessError> {
    if raw.len() > QUERY_LIMIT {
        return Err(malformed(
            "LEGAL_REQUEST_INVALID",
            "Use a bounded Legal resource path.",
        ));
    }
    let mut path = Vec::new();
    for segment in raw.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ));
        }
        path.push(segment.to_owned());
    }
    Ok(path)
}

impl LegalAuxiliaryHttpState {
    fn target<'a>(
        &'a self,
        headers: &HeaderMap,
        slug: &str,
    ) -> Result<&'a LegalAuxiliaryHttpTarget, BusinessError> {
        let host = unique_header(headers, header::HOST)?.ok_or_else(|| {
            BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
        })?;
        self.targets
            .get(&(host.to_owned(), slug.to_owned()))
            .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."))
    }
}

pub struct LegalAuxiliaryHttpService {
    state: Arc<LegalAuxiliaryHttpState>,
}

impl LegalAuxiliaryHttpService {
    pub fn new(targets: Vec<LegalAuxiliaryHttpTarget>) -> Result<Self, BusinessError> {
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
            state: Arc::new(LegalAuxiliaryHttpState { targets: configured }),
        })
    }

    pub async fn read(
        &self,
        host: &str,
        holding_slug: &str,
        headers: &HeaderMap,
        raw_path: &str,
        wire: LegalAuxiliaryQueryWire,
    ) -> Response {
        let target = match self.state.target(headers, holding_slug) {
            Ok(target) => target,
            Err(error) => return error_response(error),
        };
        let registered = match unique_header(headers, header::HOST) {
            Ok(Some(registered)) => registered,
            _ => return error_response(malformed("LEGAL_REQUEST_INVALID", "Use the registered Host.")),
        };
        if !host.is_empty() && host != registered {
            return error_response(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ));
        }
        let path = match split_path(raw_path) {
            Ok(path) => path,
            Err(error) => return error_response(error),
        };
        let Some(read) = parse_auxiliary_path(&path, &wire.pairs()) else {
            return error_response(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ));
        };
        let host = target.host.clone();
        match target
            .backend
            .read(LegalAuxiliaryHttpRequest {
                host: &host,
                holding_slug,
                headers,
                read,
            })
            .await
        {
            Ok(data) => private_json(&data),
            Err(error) => error_response(error),
        }
    }

    pub fn router(self: Arc<Self>) -> Router {
        Router::new()
            .route("/api/v1/holdings/{holding_slug}/legal/{*path}", get(read))
            .with_state(self)
    }
}

async fn read(
    State(service): State<Arc<LegalAuxiliaryHttpService>>,
    Path((holding_slug, path)): Path<(String, String)>,
    Query(wire): Query<LegalAuxiliaryQueryWire>,
    headers: HeaderMap,
    request: Request,
) -> Response {
    let host = unique_header(&headers, header::HOST)
        .ok()
        .flatten()
        .unwrap_or_default();
    drop(request);
    service
        .read(host, &holding_slug, &headers, &path, wire)
        .await
}

/// Mount point for composition. The exact paths served are the auxiliary Legal
/// GET surface; any other path fails closed with `NOT_FOUND`.
pub fn legal_auxiliary_router(
    targets: Vec<LegalAuxiliaryHttpTarget>,
) -> Result<Router, BusinessError> {
    let service = Arc::new(LegalAuxiliaryHttpService::new(targets)?);
    Ok(service.router())
}

/// The exact action set this router does not serve. Kept next to the router so
/// a mount cannot accidentally select an unported read.
pub const UNPORTED_READS: [&str; 9] = [
    "people",
    "runtime-health",
    "release-acceptance",
    "features",
    "rule-impact",
    "processing",
    "export",
    "evidence",
    "hold-preview",
];

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_runtime::public::http_support::body::to_bytes;

    struct Backend(std::sync::Mutex<Vec<String>>);

    impl LegalAuxiliaryHttpBackend for Backend {
        fn read<'a>(
            &'a self,
            request: LegalAuxiliaryHttpRequest<'a>,
        ) -> LegalAuxiliaryHttpFuture<'a> {
            self.0
                .lock()
                .unwrap()
                .push(request.read.operation().to_owned());
            Box::pin(async move { Ok(serde_json::json!({"read": request.read.operation()})) })
        }
    }

    fn service() -> (Arc<LegalAuxiliaryHttpService>, Arc<Backend>) {
        let backend = Arc::new(Backend(std::sync::Mutex::new(Vec::new())));
        let service = LegalAuxiliaryHttpService::new(vec![LegalAuxiliaryHttpTarget {
            host: "acme.example".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example".into(),
            backend: backend.clone(),
        }])
        .unwrap();
        (Arc::new(service), backend)
    }

    fn headers(host: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_str(host).unwrap());
        headers
    }

    #[test]
    fn configuration_rejects_duplicate_or_insecure_targets() {
        let backend: Arc<dyn LegalAuxiliaryHttpBackend> =
            Arc::new(Backend(std::sync::Mutex::new(Vec::new())));
        assert!(
            LegalAuxiliaryHttpService::new(vec![LegalAuxiliaryHttpTarget {
                host: "acme.example".into(),
                holding_slug: "acme".into(),
                origin: "http://acme.example".into(),
                backend: backend.clone(),
            }])
            .is_err()
        );
        let target = || LegalAuxiliaryHttpTarget {
            host: "acme.example".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example".into(),
            backend: backend.clone(),
        };
        assert!(LegalAuxiliaryHttpService::new(vec![target(), target()]).is_err());
    }

    #[test]
    fn paths_split_and_reject_traversal() {
        assert_eq!(split_path("retention/item-one").unwrap(), vec!["retention", "item-one"]);
        assert!(split_path("retention//item").is_err());
        assert!(split_path("../item").is_err());
        assert!(split_path("retention/.").is_err());
    }

    #[tokio::test]
    async fn unported_paths_fail_closed_before_the_backend() {
        let (service, backend) = service();
        let response = service
            .read(
                "",
                "acme",
                &headers("acme.example"),
                "features/item-one",
                LegalAuxiliaryQueryWire::default(),
            )
            .await;
        assert_eq!(response.into_parts().0.status, StatusCode::NOT_FOUND);
        assert!(backend.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn registered_reads_reach_the_backend_with_private_headers() {
        let (service, backend) = service();
        let response = service
            .read(
                "",
                "acme",
                &headers("acme.example"),
                "workspace",
                LegalAuxiliaryQueryWire::default(),
            )
            .await;
        let (parts, body) = response.into_parts();
        assert_eq!(parts.status, StatusCode::OK);
        assert_eq!(
            parts.headers.get(header::CACHE_CONTROL).unwrap(),
            "private, no-store"
        );
        let value: Value = serde_json::from_slice(&to_bytes(body, 4096).await.unwrap()).unwrap();
        assert_eq!(value["read"], "legal.workspace.read");
        assert_eq!(backend.0.lock().unwrap().as_slice(), ["legal.workspace.read"]);
    }

    #[tokio::test]
    async fn wrong_host_and_wrong_holding_are_not_found() {
        let (service, backend) = service();
        let wrong_host = service
            .read(
                "",
                "acme",
                &headers("other.example"),
                "workspace",
                LegalAuxiliaryQueryWire::default(),
            )
            .await;
        assert_eq!(wrong_host.into_parts().0.status, StatusCode::NOT_FOUND);
        let wrong_slug = service
            .read(
                "",
                "other",
                &headers("acme.example"),
                "workspace",
                LegalAuxiliaryQueryWire::default(),
            )
            .await;
        assert_eq!(wrong_slug.into_parts().0.status, StatusCode::NOT_FOUND);
        assert!(backend.0.lock().unwrap().is_empty());
    }

    #[test]
    fn query_wire_preserves_only_bounded_read_parameters() {
        let wire = LegalAuxiliaryQueryWire {
            rule_id: Some("rule-one".into()),
            as_of: Some("2024-02-29".into()),
            q: Some("contract".into()),
            ..LegalAuxiliaryQueryWire::default()
        };
        assert_eq!(
            wire.pairs(),
            vec![
                ("ruleId".to_owned(), "rule-one".to_owned()),
                ("asOf".to_owned(), "2024-02-29".to_owned()),
                ("q".to_owned(), "contract".to_owned()),
            ]
        );
        assert!(LegalAuxiliaryQueryWire::default().pairs().is_empty());
    }
}
