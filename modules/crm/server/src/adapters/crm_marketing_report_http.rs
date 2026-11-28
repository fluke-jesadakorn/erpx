//! Native HTTP ingress for `GET /reports/crm-marketing`.
//!
//! This is the transport successor of the retired shared Next route: it keeps
//! the exact registered-host/origin admission, bounded query and
//! `{Cache-Control: private, no-store}` envelope, but delegates the read to the
//! owner port. No SQL or TypeScript fallback lives in this module; the owner
//! report adapter (`sales_report.rs`) executes the statement on composition's
//! serializable unit.
use crate::application::crm_marketing_report::{
    CrmMarketingReportFuture, CrmMarketingReportPayload,
};
use crate::domain::crm_marketing_report::{CrmMarketingReportQuery, request_invalid};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Query, Request, Response, Router, State,
    StatusCode, get, header, to_bytes,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Native route mounted by composition for the reports/crm-marketing family.
pub const CRM_MARKETING_REPORT_PATH: &str =
    "/api/v1/holdings/{holding_slug}/reports/crm-marketing";

const QUERY_LIMIT: usize = 4096;

pub struct CrmMarketingReportRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub query: CrmMarketingReportQuery,
}

/// Composition-owned session, Company scope and owner-unit adapter.
pub trait CrmMarketingReportBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: CrmMarketingReportRequest<'a>,
    ) -> CrmMarketingReportFuture<'a, CrmMarketingReportPayload>;
}

pub struct CrmMarketingReportTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn CrmMarketingReportBackend>,
}

type Targets = BTreeMap<(String, String), CrmMarketingReportTarget>;

struct CrmMarketingReportState {
    targets: Targets,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "CRM_MARKETING_REPORT_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b':'))
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_'))
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

fn find_target<'a>(
    targets: &'a Targets,
    headers: &'a HeaderMap,
    slug: &str,
) -> Result<&'a CrmMarketingReportTarget, BusinessError> {
    let host = unique_header(headers, "host")?.filter(|value| !value.is_empty()).ok_or_else(missing)?;
    let target = targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)?;
    let origin = unique_header(headers, "origin")?;
    if origin.is_some_and(|value| value != target.origin) {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "REPORT_ORIGIN",
            "The report is available from its Holding origin.",
        ));
    }
    Ok(target)
}

/// Register one exact `host` + `holding_slug` target per registered Holding.
pub fn crm_marketing_report_router(
    targets: Vec<CrmMarketingReportTarget>,
) -> Result<Router, BusinessError> {
    let mut registered = BTreeMap::new();
    for target in targets {
        if !valid_host(&target.host)
            || target.origin.strip_prefix("https://") != Some(target.host.as_str())
            || !valid_slug(&target.holding_slug)
            || registered
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    Ok(Router::new()
        .route(CRM_MARKETING_REPORT_PATH, get(read))
        .with_state(Arc::new(CrmMarketingReportState {
            targets: registered,
        })))
}

fn bounded(value: Value) -> Response {
    let mut response = Json(value).into_response();
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

fn error_response(error: BusinessError) -> Response {
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

async fn read(
    State(state): State<Arc<CrmMarketingReportState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let bytes = request.uri().query().unwrap_or("").as_bytes();
        if bytes.len() > QUERY_LIMIT
            || bytes.iter().enumerate().any(|(index, byte)| {
                *byte == b'%'
                    && !bytes
                        .get(index + 1..index + 3)
                        .is_some_and(|hex| hex.iter().all(u8::is_ascii_hexdigit))
            })
        {
            return Err(request_invalid());
        }
        let Query(query) = Query::<CrmMarketingReportQuery>::try_from_uri(request.uri())
            .map_err(|_| request_invalid())?;
        query.validate()?;
        let (parts, body) = request.into_parts();
        to_bytes(body, 0).await.map_err(|_| request_invalid())?;
        let target = find_target(&state.targets, &parts.headers, &slug)?;
        target
            .backend
            .read(CrmMarketingReportRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &parts.headers,
                query,
            })
            .await
    }
    .await;
    result.map_or_else(error_response, bounded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_is_the_retired_shared_route() {
        assert_eq!(
            CRM_MARKETING_REPORT_PATH,
            "/api/v1/holdings/{holding_slug}/reports/crm-marketing"
        );
    }

    #[test]
    fn duplicate_registered_targets_are_rejected() {
        struct Backend;
        impl CrmMarketingReportBackend for Backend {
            fn read<'a>(
                &'a self,
                _: CrmMarketingReportRequest<'a>,
            ) -> CrmMarketingReportFuture<'a, CrmMarketingReportPayload> {
                Box::pin(async { Ok(json!({})) })
            }
        }
        let target = || CrmMarketingReportTarget {
            host: "acme.jesadakorn.com".to_owned(),
            holding_slug: "acme".to_owned(),
            origin: "https://acme.jesadakorn.com".to_owned(),
            backend: Arc::new(Backend),
        };
        assert!(crm_marketing_report_router(vec![target()]).is_ok());
        assert_eq!(
            crm_marketing_report_router(vec![target(), target()])
                .unwrap_err()
                .code,
            "CRM_MARKETING_REPORT_CONFIGURATION_INVALID"
        );
    }

    #[test]
    fn origin_must_match_the_registered_host() {
        struct Backend;
        impl CrmMarketingReportBackend for Backend {
            fn read<'a>(
                &'a self,
                _: CrmMarketingReportRequest<'a>,
            ) -> CrmMarketingReportFuture<'a, CrmMarketingReportPayload> {
                Box::pin(async { Ok(json!({})) })
            }
        }
        let mut targets = BTreeMap::new();
        targets.insert(
            ("acme.jesadakorn.com".to_owned(), "acme".to_owned()),
            CrmMarketingReportTarget {
                host: "acme.jesadakorn.com".to_owned(),
                holding_slug: "acme".to_owned(),
                origin: "https://acme.jesadakorn.com".to_owned(),
                backend: Arc::new(Backend),
            },
        );
        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("acme.jesadakorn.com"));
        headers.insert("origin", HeaderValue::from_static("https://other.example.com"));
        let error = find_target(&targets, &headers, "acme")
            .err()
            .expect("origin mismatch is rejected");
        assert_eq!(error.code, "REPORT_ORIGIN");
        assert_eq!(error.kind, FailureKind::Forbidden);
    }
}
