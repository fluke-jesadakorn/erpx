//! Native Manufacturing read-plan axum ingress.
//!
//! Transport facts only: an exact registered Host/Holding target, browser
//! Origin, a closed `RD_*` read kind and a bounded caller scope. The scope is
//! planned by the owner against the frozen `ReadPlan` registry; no SQL is
//! accepted from the wire and no TypeScript fallback exists. The returned
//! statement is executed by the caller on its own unit, so caller filters and
//! the caller's single SQL snapshot stay inside the caller's transaction.
use crate::adapters::native_command_sql::{ReadScope, bind};
use crate::public::native_command::{is_owner_read, invalid};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 8 * 1024;
const ORIGIN_HEADER: HeaderName = HeaderName::from_static("origin");

/// The owner-planned statement for one closed read kind.
pub type NativeReadPlanFuture<'a> =
    Pin<Box<dyn Future<Output = Result<serde_json::Value, BusinessError>> + Send + 'a>>;

/// Scope facts the live caller already resolved from its WorkContext.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeReadPlanScope {
    pub id: Option<String>,
    pub company_id: Option<String>,
    pub branch_id: Option<String>,
    pub specification_id: Option<String>,
    pub order_id: Option<String>,
    pub version: Option<i64>,
}

impl NativeReadPlanScope {
    fn as_scope(&self) -> ReadScope<'_> {
        ReadScope {
            id: self.id.as_deref(),
            company_id: self.company_id.as_deref(),
            branch_id: self.branch_id.as_deref(),
            specification_id: self.specification_id.as_deref(),
            order_id: self.order_id.as_deref(),
            version: self.version,
        }
    }
}

pub struct NativeReadPlanRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub kind: &'a str,
    pub scope: &'a NativeReadPlanScope,
}

/// Pure owner backend. Composition injects it; it plans one exact static
/// statement and never opens a connection, commits or locks.
pub trait NativeReadPlanBackend: Send + Sync {
    fn plan<'a>(&'a self, request: NativeReadPlanRequest<'a>) -> NativeReadPlanFuture<'a>;
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PlanEnvelope {
    kind: String,
    #[serde(default)]
    scope: NativeReadPlanScope,
}

pub struct NativeReadPlanTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn NativeReadPlanBackend>,
}

struct AppState {
    targets: BTreeMap<(String, String), NativeReadPlanTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the registered application origin.",
    )
}
fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "MANUFACTURING_READ_ROUTE_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
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
                .any(|b| matches!(b, b'/' | b'?' | b'#' | b'@' | b':'))
            && port.is_none_or(|p| {
                (1..=5).contains(&p.len())
                    && p.bytes().all(|b| b.is_ascii_digit())
                    && p.parse::<u16>().is_ok_and(|v| v > 0)
            })
    })
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}

fn bounded_scope(scope: &NativeReadPlanScope) -> Result<(), BusinessError> {
    for value in [
        scope.id.as_deref(),
        scope.company_id.as_deref(),
        scope.branch_id.as_deref(),
        scope.specification_id.as_deref(),
        scope.order_id.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if value.is_empty() || value.len() > 200 {
            return Err(invalid());
        }
    }
    Ok(())
}

/// Plan a closed read kind on the owner registry. The caller receives the exact
/// static SQL and its ordered binds; the owner never sees a live row.
pub fn plan(kind: &str, scope: &NativeReadPlanScope) -> Result<serde_json::Value, BusinessError> {
    if !is_owner_read(kind) {
        return Err(invalid());
    }
    bounded_scope(scope)?;
    let bound = bind(kind, &scope.as_scope())?;
    Ok(serde_json::json!({
        "kind": bound.kind,
        "caller": bound.caller,
        "statement": {"sql": bound.sql, "values": bound.binds},
    }))
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

fn bounded_response(body: serde_json::Value) -> Response {
    let mut response = (StatusCode::OK, Json(body)).into_response();
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

/// Build the exact Holding-target read-plan router. Every target must be a
/// distinct registered host/slug pair with a valid browser Origin.
pub fn native_manufacturing_read_router(
    targets: Vec<NativeReadPlanTarget>,
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
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/manufacturing/read-plan",
            post(read_plan),
        )
        .with_state(Arc::new(AppState {
            targets: configured,
        })))
}

async fn read_plan(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let headers = request.headers().clone();
        let host = unique_header(&headers, header::HOST)?
            .filter(|v| !v.is_empty())
            .ok_or_else(missing)?
            .to_owned();
        let origin = unique_header(&headers, ORIGIN_HEADER)?.ok_or_else(forbidden)?;
        // fetch() forbids overriding Host, so native loopback POSTs arrive with a
        // loopback Host. Fall back to the validated Origin host; the exact Origin
        // check below still applies to the selected target.
        let target = state
            .targets
            .get(&(host, slug.clone()))
            .or_else(|| {
                origin
                    .strip_prefix("https://")
                    .and_then(|rest| match rest.rsplit_once(':') {
                        Some((h, _)) => Some(h),
                        None => Some(rest),
                    })
                    .and_then(|h| state.targets.get(&(h.to_owned(), slug.clone())))
            })
            .ok_or_else(missing)?;
        if origin != target.origin {
            return Err(forbidden());
        }
        if request.uri().query().is_some() {
            return Err(invalid());
        }
        let bytes = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| invalid())?;
        let envelope: PlanEnvelope =
            serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        if envelope.kind.is_empty() || envelope.kind.len() > 120 {
            return Err(invalid());
        }
        target
            .backend
            .plan(NativeReadPlanRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                kind: &envelope.kind,
                scope: &envelope.scope,
            })
            .await
    }
    .await;
    match result {
        Ok(data) => bounded_response(serde_json::json!({"data": data})),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::public::native_command as kinds;

    fn scope() -> NativeReadPlanScope {
        NativeReadPlanScope {
            id: Some("record-1".into()),
            company_id: Some("company-1".into()),
            branch_id: Some("branch-1".into()),
            specification_id: Some("spec-1".into()),
            order_id: Some("order-1".into()),
            version: Some(3),
        }
    }

    #[test]
    fn every_registered_kind_plans_an_exact_static_statement() {
        for kind in kinds::kinds() {
            let value = plan(kind, &scope()).unwrap_or_else(|e| panic!("{kind}: {e:?}"));
            let statement = &value["statement"];
            let sql = statement["sql"].as_str().unwrap();
            assert_eq!(sql, kinds::plan(kind).unwrap().sql, "{kind}");
            let values = statement["values"].as_array().unwrap();
            assert_eq!(values.len(), sql.matches('?').count(), "{kind}");
        }
    }

    #[test]
    fn branch_scope_binding_keeps_the_unscoped_predicate() {
        let value = plan(kinds::RD_ORDERS, &NativeReadPlanScope {
            company_id: Some("company-1".into()),
            ..NativeReadPlanScope::default()
        })
        .unwrap();
        assert_eq!(
            value["statement"]["values"],
            serde_json::json!([ "company-1", null, null ])
        );
        assert!(value["statement"]["sql"]
            .as_str()
            .unwrap()
            .contains("(?::text IS NULL OR o.branch_id=?)"));
    }

    #[test]
    fn unknown_kind_and_invalid_scope_fail_closed() {
        assert_eq!(
            plan("bogus.read", &scope()).unwrap_err().code,
            "MANUFACTURING_READ_PLAN_INVALID"
        );
        assert_eq!(
            plan(kinds::RD_SPECIFICATION, &NativeReadPlanScope::default())
                .unwrap_err()
                .code,
            "MANUFACTURING_READ_SCOPE_REQUIRED"
        );
        let long = NativeReadPlanScope {
            id: Some("x".repeat(201)),
            ..scope()
        };
        assert_eq!(
            plan(kinds::RD_SPECIFICATION, &long).unwrap_err().code,
            "MANUFACTURING_READ_PLAN_INVALID"
        );
    }

    #[test]
    fn route_configuration_requires_distinct_registered_targets() {
        struct Noop;
        impl NativeReadPlanBackend for Noop {
            fn plan<'a>(
                &'a self,
                request: NativeReadPlanRequest<'a>,
            ) -> NativeReadPlanFuture<'a> {
                let value = plan(request.kind, request.scope);
                Box::pin(async move { value })
            }
        }
        let target = || NativeReadPlanTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(Noop),
        };
        assert!(native_manufacturing_read_router(vec![target()]).is_ok());
        assert!(native_manufacturing_read_router(vec![]).is_err());
        assert!(native_manufacturing_read_router(vec![target(), target()]).is_err());
        assert!(valid_origin("https://acme.example.test:3443"));
        assert!(!valid_origin("http://acme.example.test"));
        assert!(valid_slug("acme-dev_1"));
        assert!(!valid_slug("Acme"));
    }
}
