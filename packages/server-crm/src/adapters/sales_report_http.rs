//! Native report ingress. Composition resolves the Account session and opens
//! the authorized CRM owner unit before invoking the report adapter.
use crate::domain::sales_report::{SalesReportQuery, request_invalid};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Query, Request, Response, Router, State,
    StatusCode, header, routing::get, to_bytes,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

pub type CrmSalesReportHttpFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Value, BusinessError>> + Send + 'a>>;

pub struct CrmSalesReportHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub query: SalesReportQuery,
}

pub trait CrmSalesReportHttpBackend: Send + Sync {
    fn read<'a>(&'a self, request: CrmSalesReportHttpRequest<'a>) -> CrmSalesReportHttpFuture<'a>;
}

pub struct CrmSalesReportHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn CrmSalesReportHttpBackend>,
}

type Targets = BTreeMap<(String, String), CrmSalesReportHttpTarget>;

pub fn crm_sales_report_router(
    targets: Vec<CrmSalesReportHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut registered = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !target
                .host
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b':'))
            || target.origin.strip_prefix("https://") != Some(target.host.as_str())
            || target.holding_slug.is_empty()
            || target.holding_slug.len() > 128
            || !target.holding_slug.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
            })
            || registered
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "CRM_SALES_REPORT_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/reports/sales-crm",
            get(read),
        )
        .with_state(Arc::new(registered)))
}

fn find_target<'a>(
    targets: &'a Targets,
    headers: &HeaderMap,
    slug: String,
) -> Result<&'a CrmSalesReportHttpTarget, BusinessError> {
    let mut hosts = headers.get_all(header::HOST).iter();
    let host = hosts.next().and_then(|host| host.to_str().ok());
    if hosts.next().is_some() {
        return Err(request_invalid());
    }
    let target = host
        .and_then(|host| targets.get(&(host.to_owned(), slug)))
        .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."))?;
    let mut origins = headers.get_all(header::ORIGIN).iter();
    if origins
        .next()
        .is_some_and(|origin| origin.as_bytes() != target.origin.as_bytes())
        || origins.next().is_some()
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "REPORT_ORIGIN",
            "The report is available from its Holding origin.",
        ));
    }
    Ok(target)
}

async fn read(
    State(targets): State<Arc<Targets>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = find_target(&targets, request.headers(), slug)?;
        let bytes = request.uri().query().unwrap_or("").as_bytes();
        if bytes.len() > 4096
            || bytes.iter().enumerate().any(|(i, byte)| {
                *byte == b'%'
                    && !bytes
                        .get(i + 1..i + 3)
                        .is_some_and(|hex| hex.iter().all(u8::is_ascii_hexdigit))
            })
        {
            return Err(request_invalid());
        }
        let Query(query) = Query::<SalesReportQuery>::try_from_uri(request.uri())
            .map_err(|_| request_invalid())?;
        query.validate()?;
        let (parts, body) = request.into_parts();
        to_bytes(body, 0).await.map_err(|_| request_invalid())?;
        target
            .backend
            .read(CrmSalesReportHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &parts.headers,
                query,
            })
            .await
    }
    .await;
    let mut response = match result {
        Ok(value) => Json(value).into_response(),
        Err(error) => {
            let status = match error.kind {
                FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
                FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
                FailureKind::Forbidden => StatusCode::FORBIDDEN,
                FailureKind::NotFound => StatusCode::NOT_FOUND,
                FailureKind::Conflict => StatusCode::CONFLICT,
                FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            };
            (
                status,
                Json(json!({"error":{"code":error.code,"message":error.message}})),
            )
                .into_response()
        }
    };
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
