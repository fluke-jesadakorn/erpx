//! Owner-local axum ingress for the native supplier-bill detail read.
//!
//! Transport facts only: registered Holding target, match id, bounded JSON and
//! business-error mapping. Account session, current Company scope, purchased
//! package, flow primary and the serializable Holding transaction are supplied
//! through the backend by composition; this router never mints them and never
//! executes SQL itself.

use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

pub type SupplierBillDetailHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct SupplierBillDetailHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub bill_id: &'a str,
    pub headers: &'a HeaderMap,
}

#[derive(Clone, Debug)]
pub struct SupplierBillDetailHttpResponse {
    /// Complete retained `supplierBillDetail` data: `bill`, `lines`, `receipts`,
    /// `events`, `accounts`, `canReview`, `canPrepare` and the optional
    /// composition-supplied `purchase`. The router never reshapes it.
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}

impl SupplierBillDetailHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

pub trait SupplierBillDetailHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: SupplierBillDetailHttpRequest<'a>,
    ) -> SupplierBillDetailHttpFuture<'a, SupplierBillDetailHttpResponse>;
}

pub struct SupplierBillDetailHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact registered browser origin, for example `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn SupplierBillDetailHttpBackend>,
}

struct StateData {
    targets: BTreeMap<(String, String), SupplierBillDetailHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "PROCUREMENT_REQUEST_INVALID",
        "Use one unambiguous Procurement supplier-bill request.",
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

pub fn supplier_bill_detail_router(
    targets: Vec<SupplierBillDetailHttpTarget>,
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
                "PROCUREMENT_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/supplier-bills/{bill_id}",
            get(read_detail),
        )
        .with_state(Arc::new(StateData {
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
    state: &'a StateData,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a SupplierBillDetailHttpTarget, BusinessError> {
    let host = unique_header(headers, "host")?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

async fn read_detail(
    State(state): State<Arc<StateData>>,
    Path((slug, bill_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        if request.uri().query().is_some() {
            return Err(request_invalid());
        }
        let target = target(&state, request.headers(), &slug)?;
        if let Some(origin) = unique_header(request.headers(), "origin")? {
            if origin != target.origin {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "ORIGIN_FORBIDDEN",
                    "Use the application origin.",
                ));
            }
        }
        let bill_id = RecordId::new(bill_id).map_err(|_| request_invalid())?;
        target
            .backend
            .read(SupplierBillDetailHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                bill_id: bill_id.as_str(),
                headers: request.headers(),
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Backend;

    impl SupplierBillDetailHttpBackend for Backend {
        fn read<'a>(
            &'a self,
            _: SupplierBillDetailHttpRequest<'a>,
        ) -> SupplierBillDetailHttpFuture<'a, SupplierBillDetailHttpResponse> {
            Box::pin(async { Err(BusinessError::invalid("TEST", "test")) })
        }
    }

    fn target(host: &str, slug: &str) -> SupplierBillDetailHttpTarget {
        SupplierBillDetailHttpTarget {
            host: host.into(),
            holding_slug: slug.into(),
            origin: "https://acme.jesadakorn.com".into(),
            backend: Arc::new(Backend),
        }
    }

    #[test]
    fn rejects_duplicate_and_invalid_targets() {
        assert!(supplier_bill_detail_router(vec![target("acme.jesadakorn.com", "acme")]).is_ok());
        assert!(supplier_bill_detail_router(vec![
            target("acme.jesadakorn.com", "acme"),
            target("acme.jesadakorn.com", "acme"),
        ])
        .is_err());
        let mut bad = target("acme.jesadakorn.com", "acme");
        bad.origin = "http://acme.jesadakorn.com".into();
        assert!(supplier_bill_detail_router(vec![bad]).is_err());
    }

    #[test]
    fn response_keeps_data_and_context() {
        let value = SupplierBillDetailHttpResponse {
            data: serde_json::json!({"bill": {}, "canReview": true}),
            context: serde_json::json!({"companyId": "c"}),
        }
        .value();
        assert_eq!(value["data"]["canReview"], serde_json::json!(true));
        assert_eq!(value["context"]["companyId"], serde_json::json!("c"));
    }
}
