//! Owner-local axum ingress for the native Platform usage read.
//!
//! The router transports only a registered Platform Host, a bounded Company,
//! a UTC month and the session-derived Platform role/permission headers. It
//! never mints authority and contains no SQL.

use crate::adapters::native_platform_usage::application::{
    ExecutiveUsageQuery, SharedExecutiveUsageRead, executive_usage,
};
use crate::adapters::native_platform_usage::domain::{MAX_COMPANY, valid_company};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Query, Response, Router, State, StatusCode, get,
    header, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

const ROLE: erpx_server_runtime::public::http_support::HeaderName =
    erpx_server_runtime::public::http_support::HeaderName::from_static("x-erpx-platform-role");
const PERMISSIONS: erpx_server_runtime::public::http_support::HeaderName =
    erpx_server_runtime::public::http_support::HeaderName::from_static("x-erpx-platform-permissions");
const COMPANY: erpx_server_runtime::public::http_support::HeaderName =
    erpx_server_runtime::public::http_support::HeaderName::from_static("x-erpx-context-company");

#[derive(Clone)]
pub struct PlatformUsageTarget {
    pub host: String,
    pub reader: SharedExecutiveUsageRead,
}

pub struct PlatformUsageService {
    targets: BTreeMap<String, SharedExecutiveUsageRead>,
}

struct PlatformUsageState {
    service: Arc<PlatformUsageService>,
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PLATFORM_USAGE_REQUEST_INVALID",
        "Use one bounded Platform usage read.",
    )
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
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

fn unique_header<'a>(headers: &'a HeaderMap, name: erpx_server_runtime::public::http_support::HeaderName) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
        .transpose()
}

impl PlatformUsageService {
    pub fn new(targets: Vec<PlatformUsageTarget>) -> Result<Self, BusinessError> {
        let mut configured = BTreeMap::new();
        for target in targets {
            if target.host.is_empty()
                || target
                    .host
                    .bytes()
                    .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@'))
                || configured.insert(target.host.clone(), target.reader).is_some()
            {
                return Err(BusinessError::invalid(
                    "PLATFORM_USAGE_CONFIG_INVALID",
                    "Use exact distinct registered Platform Host targets.",
                ));
            }
        }
        if configured.is_empty() {
            return Err(BusinessError::invalid(
                "PLATFORM_USAGE_CONFIG_INVALID",
                "Use exact distinct registered Platform Host targets.",
            ));
        }
        Ok(Self {
            targets: configured,
        })
    }

    pub async fn read(
        &self,
        headers: &HeaderMap,
        values: &BTreeMap<String, String>,
    ) -> Response {
        let result = async {
            let host = unique_header(headers, header::HOST)?
                .filter(|host| !host.is_empty())
                .ok_or_else(missing)?;
            let reader = self.targets.get(host).ok_or_else(missing)?;
            let company = unique_header(headers, COMPANY)?
                .filter(|value| valid_company(value) && value.len() <= MAX_COMPANY)
                .ok_or_else(invalid)?;
            let role = unique_header(headers, ROLE)?.ok_or_else(invalid)?;
            let raw_permissions = unique_header(headers, PERMISSIONS)?.unwrap_or("");
            let mut permissions = Vec::new();
            for item in raw_permissions.split(',') {
                let permission = item.trim();
                if !permission.is_empty() {
                    if permission.len() > 120
                        || permission
                            .bytes()
                            .any(|b| matches!(b, b'\n' | b'\r' | b'\0'))
                    {
                        return Err(invalid());
                    }
                    permissions.push(permission.to_owned());
                }
            }
            let month = values.get("month").map(String::as_str).ok_or_else(invalid)?;
            if values.len() != 1 {
                return Err(invalid());
            }
            let query = ExecutiveUsageQuery::parse(company, month, role, &permissions)?;
            let usage = executive_usage(&query, reader.as_ref()).await?;
            Ok::<_, BusinessError>(serde_json::json!({
                "companyId": usage.company_id,
                "month": usage.month,
                "generationJobs": usage.generation_jobs,
            }))
        }
        .await;
        match result {
            Ok(body) => bounded(body),
            Err(error) => error_response(error),
        }
    }
}

pub fn platform_usage_router(
    targets: Vec<PlatformUsageTarget>,
) -> Result<Router, BusinessError> {
    let service = Arc::new(PlatformUsageService::new(targets)?);
    Ok(Router::new()
        .route("/api/v1/platform/usage/executive", get(read_usage))
        .with_state(Arc::new(PlatformUsageState { service })))
}

pub fn platform_usage_router_for_hosts(
    hosts: Vec<String>,
    reader: SharedExecutiveUsageRead,
) -> Result<Router, BusinessError> {
    platform_usage_router(
        hosts
            .into_iter()
            .map(|host| PlatformUsageTarget {
                host,
                reader: reader.clone(),
            })
            .collect(),
    )
}

async fn read_usage(
    State(state): State<Arc<PlatformUsageState>>,
    headers: HeaderMap,
    Query(values): Query<BTreeMap<String, String>>,
) -> Response {
    state.service.read(&headers, &values).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_platform_usage::application::UsageFuture;
    use crate::adapters::native_platform_usage::domain::{MAX_USAGE, source_invalid};

    struct Counter(i64);

    impl crate::adapters::native_platform_usage::application::ExecutiveUsageRead for Counter {
        fn generation_jobs<'a>(
            &'a self,
            _company: &'a str,
            _month: &'a str,
        ) -> UsageFuture<'a, i64> {
            Box::pin(async move {
                if self.0 < 0 {
                    Err(source_invalid())
                } else {
                    Ok(self.0)
                }
            })
        }
    }

    fn service(count: i64) -> PlatformUsageService {
        PlatformUsageService::new(vec![PlatformUsageTarget {
            host: "erpx-internal.example.test".into(),
            reader: Arc::new(Counter(count)),
        }])
        .unwrap()
    }

    fn headers(role: &str, permissions: &str, company: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "erpx-internal.example.test".parse().unwrap());
        headers.insert(ROLE, role.parse().unwrap());
        headers.insert(PERMISSIONS, permissions.parse().unwrap());
        headers.insert(COMPANY, company.parse().unwrap());
        headers
    }

    fn query(month: &str) -> BTreeMap<String, String> {
        BTreeMap::from([("month".to_owned(), month.to_owned())])
    }

    #[tokio::test]
    async fn read_returns_the_complete_usage_payload() {
        let response = service(9)
            .read(
                &headers("platform_auditor", "billing.usage.read", "company-001"),
                &query("2026-06"),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "private, no-store");
        let bytes = erpx_server_runtime::public::http_support::body::to_bytes(
            response.into_body(),
            64 * 1024,
        )
        .await
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["companyId"], "company-001");
        assert_eq!(value["month"], "2026-06");
        assert_eq!(value["generationJobs"], 9);
    }

    #[tokio::test]
    async fn read_fails_closed_on_scope_and_source_errors() {
        let response = service(1)
            .read(
                &headers("platform_owner", "billing.usage.read", "company-001"),
                &query("2026-06"),
            )
            .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let response = service(1)
            .read(
                &headers("platform_admin", "billing.usage.read", "x'. DROP"),
                &query("2026-06"),
            )
            .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let response = service(1)
            .read(
                &headers("platform_admin", "billing.usage.read", "company-001"),
                &query("2026-13"),
            )
            .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let response = service(-1)
            .read(
                &headers("platform_admin", "billing.usage.read", "company-001"),
                &query("2026-06"),
            )
            .await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(MAX_USAGE > 0);
    }

    #[test]
    fn configuration_is_closed() {
        let reader: SharedExecutiveUsageRead = Arc::new(Counter(1));
        assert!(platform_usage_router_for_hosts(
            vec!["erpx-internal.example.test".into()],
            reader.clone(),
        )
        .is_ok());
        assert!(platform_usage_router_for_hosts(vec![], reader.clone()).is_err());
        assert!(platform_usage_router_for_hosts(
            vec!["a".into(), "a".into()],
            reader,
        )
        .is_err());
    }
}
