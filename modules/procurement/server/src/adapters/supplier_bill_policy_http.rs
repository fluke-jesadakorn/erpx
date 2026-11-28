//! Owner-local HTTP ingress for the native Procurement supplier-invoice policy
//! read on the retained Supply route.
//!
//! Transport facts only: registered Host/Holding target, exact browser origin,
//! bounded private JSON and business-error mapping. Account session resolution,
//! Company scope, Company Finance authority, the commercial purchased-package
//! overlay and the owner SQL read remain composition responsibilities behind
//! [`SupplierBillPolicyHttpBackend`]. Nothing in this router grants authority.
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    header, routing::get, serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

pub type SupplierBillPolicyHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct SupplierBillPolicyHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
}

#[derive(Clone, Debug)]
pub struct SupplierBillPolicyHttpResponse {
    /// Exact `supplierInvoicePolicy` data: `matchingMode`, `version`,
    /// `updatedByName`, `updatedAt`, `canManage`, `events`, `companyId` and the
    /// optional composition-supplied `purchase` overlay.
    pub data: serde_json::Value,
    /// Composition-owned current work context.
    pub context: serde_json::Value,
}

impl SupplierBillPolicyHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account/Holding proof, Company authority and owner-unit
/// adapter. The router never mints a session, actor, Company, authority or
/// purchase overlay and never executes SQL.
pub trait SupplierBillPolicyHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: SupplierBillPolicyHttpRequest<'a>,
    ) -> SupplierBillPolicyHttpFuture<'a, SupplierBillPolicyHttpResponse>;
}

pub struct SupplierBillPolicyHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn SupplierBillPolicyHttpBackend>,
}

struct SupplierBillPolicyHttpState {
    targets: BTreeMap<(String, String), SupplierBillPolicyHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "PROCUREMENT_REQUEST_INVALID",
        "Use one unambiguous Procurement invoice policy request.",
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

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

/// Native ingress for the retained supplier-bill policy Supply route. The
/// write half of the same path is already served by the native supplier-bill
/// command mount; composition owns the final route cutover.
pub fn supplier_bill_policy_http_router(
    targets: Vec<SupplierBillPolicyHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !valid_origin(&target.origin)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "PROCUREMENT_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/supplier-bills/policy",
            get(read_policy),
        )
        .with_state(Arc::new(SupplierBillPolicyHttpState {
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

fn target<'a>(
    state: &'a SupplierBillPolicyHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a SupplierBillPolicyHttpTarget, BusinessError> {
    let host = unique_header(headers, "host")?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

async fn read_policy(
    State(state): State<Arc<SupplierBillPolicyHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        if request.uri().query().is_some() {
            return Err(request_invalid());
        }
        let target = target(&state, request.headers(), &slug)?;
        if unique_header(request.headers(), "origin")?.is_some_and(|value| value != target.origin) {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            ));
        }
        let response = target
            .backend
            .read(SupplierBillPolicyHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: request.headers(),
            })
            .await?;
        if !response.data.is_object() || !response.context.is_object() {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "SUPPLIER_BILL_POLICY_INVALID",
                "The Procurement invoice policy could not be read back.",
            ));
        }
        Ok(response.value())
    }
    .await;
    match result {
        Ok(value) => bounded(value),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_platform_runtime_server::public::http_support::serde_json::json;

    #[test]
    fn router_requires_exact_distinct_registered_targets() {
        let target = || SupplierBillPolicyHttpTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.jesadakorn.com".into(),
            backend: Arc::new(Noop),
        };
        assert!(supplier_bill_policy_http_router(vec![target()]).is_ok());
        assert!(supplier_bill_policy_http_router(vec![target(), target()]).is_err());
        let bad_origin = SupplierBillPolicyHttpTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "acme".into(),
            origin: "http://acme.jesadakorn.com".into(),
            backend: Arc::new(Noop),
        };
        assert!(supplier_bill_policy_http_router(vec![bad_origin]).is_err());
    }

    #[test]
    fn unknown_target_and_mixed_case_slug_are_closed() {
        let state = SupplierBillPolicyHttpState {
            targets: BTreeMap::new(),
        };
        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("acme.jesadakorn.com"));
        assert_eq!(target(&state, &headers, "acme").err().unwrap().code, "NOT_FOUND");
        assert!(!valid_slug("Acme"));
        assert!(valid_slug("acme-dev_1"));
    }

    struct Noop;

    impl SupplierBillPolicyHttpBackend for Noop {
        fn read<'a>(
            &'a self,
            _: SupplierBillPolicyHttpRequest<'a>,
        ) -> SupplierBillPolicyHttpFuture<'a, SupplierBillPolicyHttpResponse> {
            Box::pin(async {
                Ok(SupplierBillPolicyHttpResponse {
                    data: json!({}),
                    context: json!({}),
                })
            })
        }
    }
}
