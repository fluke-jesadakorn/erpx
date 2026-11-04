//! Owner-local HTTP ingress for the Company package overview and purchase
//! admission reads.
//!
//! Transport facts only: registered Host/Holding target, Company identity,
//! optional capability query and business-error mapping. The normal Account
//! session, current Platform/Company authority, serializable owner unit,
//! catalogue source and rendered context remain composition responsibilities
//! supplied through the backend trait. This router never mints a session,
//! grant, actor or entitlement.
use crate::application::purchases::{PurchaseRequest, validate_purchase_request};
use crate::domain::packages::{CapabilityPurpose, PackageEnforcementMode};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

pub type CompanyPackageReadHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompanyPackageReadHttpKind {
    Overview,
    Admission {
        capabilities: Vec<String>,
        purpose: CapabilityPurpose,
        mode: PackageEnforcementMode,
    },
}

pub struct CompanyPackageReadHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub company: &'a CompanyId,
    pub kind: CompanyPackageReadHttpKind,
}

#[derive(Clone, Debug)]
pub struct CompanyPackageReadHttpResponse {
    /// Owner read data. The backend adds the validated current context.
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}

impl CompanyPackageReadHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, scope, authority and owner-unit adapter.
/// The router never mints a session, actor, Company or entitlement.
pub trait CompanyPackageReadHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: CompanyPackageReadHttpRequest<'a>,
    ) -> CompanyPackageReadHttpFuture<'a, CompanyPackageReadHttpResponse>;
}

pub struct CompanyPackageReadHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn CompanyPackageReadHttpBackend>,
}

struct CompanyPackageHttpState {
    targets: BTreeMap<(String, String), CompanyPackageReadHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCIAL_REQUEST_INVALID",
        "Use one unambiguous Commercial package request.",
    )
}

fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        !rest.is_empty()
            && !rest
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
    })
}

fn target<'a>(
    state: &'a CompanyPackageHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a CompanyPackageReadHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn unique_header(
    headers: &HeaderMap,
    name: erpx_server_runtime::public::http_support::HeaderName,
) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(request_invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| request_invalid()))
        .transpose()
}

fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn percent_decode(value: &str) -> Result<String, BusinessError> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes.get(index + 1).copied().and_then(hex);
            let low = bytes.get(index + 2).copied().and_then(hex);
            let (Some(high), Some(low)) = (high, low) else {
                return Err(request_invalid());
            };
            out.push(high * 16 + low);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).map_err(|_| request_invalid())
}

fn admission_kind(query: Option<&str>) -> Result<CompanyPackageReadHttpKind, BusinessError> {
    let Some(query) = query else {
        return Err(BusinessError::invalid(
            "COMMERCIAL_CAPABILITIES_REQUIRED",
            "Choose a bounded set of owner capabilities.",
        ));
    };
    if query.len() > 8192 {
        return Err(request_invalid());
    }
    let mut capabilities = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut purpose = None;
    let mut mode = None;
    for pair in query.split('&') {
        if pair.is_empty() {
            return Err(request_invalid());
        }
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        let name = percent_decode(name)?;
        let value = percent_decode(value)?;
        if name.chars().any(char::is_control) || value.chars().any(char::is_control) {
            return Err(request_invalid());
        }
        match name.as_str() {
            "capability" => {
                if value.is_empty() || !seen.insert(value.clone()) {
                    return Err(request_invalid());
                }
                capabilities.push(value);
            }
            "purpose" => {
                if purpose.is_some() {
                    return Err(request_invalid());
                }
                purpose = Some(match value.as_str() {
                    "new-work" => CapabilityPurpose::NewWork,
                    "settle-existing" => CapabilityPurpose::SettleExisting,
                    "history" => CapabilityPurpose::History,
                    _ => return Err(request_invalid()),
                });
            }
            "mode" => {
                if mode.is_some() {
                    return Err(request_invalid());
                }
                mode = Some(match value.as_str() {
                    "disabled" => PackageEnforcementMode::Disabled,
                    "shadow" => PackageEnforcementMode::Shadow,
                    "enforced" => PackageEnforcementMode::Enforced,
                    _ => return Err(request_invalid()),
                });
            }
            _ => return Err(request_invalid()),
        }
    }
    let request = PurchaseRequest {
        capabilities,
        purpose: purpose.unwrap_or(CapabilityPurpose::NewWork),
        mode: mode.unwrap_or(PackageEnforcementMode::Enforced),
    };
    validate_purchase_request(&request)?;
    Ok(CompanyPackageReadHttpKind::Admission {
        capabilities: request.capabilities,
        purpose: request.purpose,
        mode: request.mode,
    })
}

pub fn company_package_router(
    targets: Vec<CompanyPackageReadHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || target.holding_slug.is_empty()
            || !valid_origin(&target.origin)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "COMMERCIAL_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/commercial/companies/{company_id}/packages",
            get(packages),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/commercial/companies/{company_id}/purchase-admission",
            get(admission),
        )
        .with_state(Arc::new(CompanyPackageHttpState {
            targets: configured,
        })))
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

fn bounded(value: serde_json::Value) -> Response {
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

async fn packages(
    State(state): State<Arc<CompanyPackageHttpState>>,
    Path((slug, company_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    read(state, slug, company_id, request, false).await
}

async fn admission(
    State(state): State<Arc<CompanyPackageHttpState>>,
    Path((slug, company_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    read(state, slug, company_id, request, true).await
}

async fn read(
    state: Arc<CompanyPackageHttpState>,
    slug: String,
    company_id: String,
    request: Request,
    admission: bool,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        let company = CompanyId::new(company_id)?;
        let kind = if admission {
            admission_kind(request.uri().query())?
        } else {
            if request.uri().query().is_some_and(|query| !query.is_empty()) {
                return Err(request_invalid());
            }
            CompanyPackageReadHttpKind::Overview
        };
        target
            .backend
            .read(CompanyPackageReadHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: request.headers(),
                company: &company,
                kind,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}
