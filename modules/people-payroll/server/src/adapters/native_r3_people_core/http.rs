//! Owner-local Axum ingress for the native R3 People core reads. Host/Holding
//! targets, bounded context headers and the exact `{data: ...}` envelope are
//! owned here; every statement is executed by the Postgres adapter.

use super::application::{
    HrInput, PeopleCoreRead, hire_pay_group_workspace, hr_settings_data,
};
use super::domain::{
    HR_SETTINGS_PATH, PAY_GROUPS_PATH, command_key_required, configuration_invalid,
    invalid, missing, origin_forbidden, valid_id, valid_impact, valid_slug, valid_topic,
};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, get, header, serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

pub const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
pub const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");
pub const CONTEXT_PERMISSIONS: HeaderName =
    HeaderName::from_static("x-erpx-context-permissions");
pub const WORKSPACE_BRANCH: HeaderName =
    HeaderName::from_static("x-erpx-workspace-branch");
pub const WORKSPACE_DEPARTMENT: HeaderName =
    HeaderName::from_static("x-erpx-workspace-department");

const MAX_QUERY: usize = 4096;
const MAX_IMPACT_PARAM: usize = 18_099;

/// One exact registered browser origin/Holding pair and its owner store.
pub struct PeopleCoreTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub store: Arc<dyn PeopleCoreRead>,
}

type Targets = BTreeMap<(String, String), PeopleCoreTarget>;

struct CoreState {
    targets: Targets,
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

fn bounded(body: serde_json::Value) -> Response {
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

fn unique_header<'a>(
    headers: &'a HeaderMap,
    name: HeaderName,
) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
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

/// `URLSearchParams.get` decodes `+` and `%XX` before the zod schema parses.
fn decode(value: &str) -> Result<String, BusinessError> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' => {
                let high = bytes
                    .get(index + 1)
                    .copied()
                    .and_then(hex)
                    .ok_or_else(invalid)?;
                let low = bytes
                    .get(index + 2)
                    .copied()
                    .and_then(hex)
                    .ok_or_else(invalid)?;
                out.push(high * 16 + low);
                index += 3;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| invalid())
}

/// Last value wins, matching `Object.fromEntries(new URLSearchParams(...))`.
fn query_pairs(query: Option<&str>) -> Result<BTreeMap<String, String>, BusinessError> {
    let raw = query.unwrap_or("");
    if raw.len() > MAX_QUERY {
        return Err(invalid());
    }
    let mut values = BTreeMap::new();
    for pair in raw.split('&').filter(|part| !part.is_empty()) {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        values.insert(decode(name)?, decode(value)?);
    }
    Ok(values)
}

/// The HR settings zod schema: at most one selector, with bounded values.
fn hr_input(
    query: Option<&str>,
    company: &str,
    branch: Option<String>,
    department: Option<String>,
    permissions: Vec<String>,
) -> Result<HrInput, BusinessError> {
    let pairs = query_pairs(query)?;
    let topic = pairs.get("topic").cloned();
    if topic.as_deref().is_some_and(|value| !valid_topic(value)) {
        return Err(invalid());
    }
    if pairs
        .get("editor")
        .is_some_and(|value| value != "person")
    {
        return Err(invalid());
    }
    let engagement = pairs.get("engagement").cloned();
    let position = pairs.get("position").cloned();
    if engagement.as_deref().is_some_and(|value| !valid_id(value))
        || position.as_deref().is_some_and(|value| !valid_id(value))
    {
        return Err(invalid());
    }
    let impact = match pairs.get("impact") {
        Some(value) => {
            if value.len() > MAX_IMPACT_PARAM || !valid_impact(value) {
                return Err(invalid());
            }
            value.split(',').map(str::to_owned).collect()
        }
        None => Vec::new(),
    };
    let selectors = [
        topic.is_some(),
        pairs.contains_key("editor"),
        engagement.is_some(),
        position.is_some(),
        !impact.is_empty(),
    ]
    .iter()
    .filter(|selected| **selected)
    .count();
    if selectors > 1 {
        return Err(invalid());
    }
    Ok(HrInput {
        company: company.to_owned(),
        branch,
        department,
        permissions,
        topic,
        editor: pairs.contains_key("editor"),
        engagement,
        position,
        impact,
    })
}

fn context(
    headers: &HeaderMap,
) -> Result<(String, String, Vec<String>, Option<String>, Option<String>), BusinessError> {
    let company = unique_header(headers, CONTEXT_COMPANY)?
        .filter(|value| valid_id(value))
        .ok_or_else(invalid)?
        .to_owned();
    let user = unique_header(headers, CONTEXT_USER)?
        .filter(|value| valid_id(value))
        .ok_or_else(invalid)?
        .to_owned();
    let permissions = unique_header(headers, CONTEXT_PERMISSIONS)?
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect();
    let branch = unique_header(headers, WORKSPACE_BRANCH)?
        .filter(|value| valid_id(value))
        .map(str::to_owned);
    let department = unique_header(headers, WORKSPACE_DEPARTMENT)?
        .filter(|value| valid_id(value))
        .map(str::to_owned);
    Ok((company, user, permissions, branch, department))
}

/// Register one exact target per Holding. Composition owns this constructor.
pub fn router(targets: Vec<PeopleCoreTarget>) -> Result<Router, BusinessError> {
    let mut registered = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || target.origin.strip_prefix("https://") != Some(target.host.as_str())
            || !valid_slug(&target.holding_slug)
            || registered
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    target,
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if registered.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(HR_SETTINGS_PATH, get(hr_settings))
        .route(PAY_GROUPS_PATH, get(pay_groups))
        .with_state(Arc::new(CoreState {
            targets: registered,
        })))
}

fn resolve<'a>(
    state: &'a CoreState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a PeopleCoreTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    let target = state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)?;
    if unique_header(headers, header::ORIGIN)?
        .is_some_and(|origin| origin != target.origin)
    {
        return Err(origin_forbidden());
    }
    Ok(target)
}

async fn hr_settings(
    State(state): State<Arc<CoreState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let headers = request.headers().clone();
        let target = resolve(&state, &headers, &slug)?;
        let (company, _user, permissions, branch, department) = context(&headers)?;
        let input = hr_input(
            request.uri().query(),
            &company,
            branch,
            department,
            permissions,
        )?;
        let data = hr_settings_data(target.store.as_ref(), &input).await?;
        Ok::<_, BusinessError>(serde_json::json!({ "data": data }))
    }
    .await;
    result.map_or_else(error_response, bounded)
}

async fn pay_groups(
    State(state): State<Arc<CoreState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        if !valid_id(&id) {
            return Err(invalid());
        }
        let headers = request.headers().clone();
        let target = resolve(&state, &headers, &slug)?;
        let (company, _user, permissions, branch, department) = context(&headers)?;
        // The retired POST requires this header; the GET does not. Keeping the
        // check here would change GET parity, so it is deliberately not read.
        let _ = command_key_required("COMMAND_KEY_REQUIRED");
        let data = hire_pay_group_workspace(
            target.store.as_ref(),
            &company,
            &id,
            branch.as_deref(),
            department.as_deref(),
            &permissions,
        )
        .await?;
        Ok::<_, BusinessError>(serde_json::json!({ "data": data }))
    }
    .await;
    result.map_or_else(error_response, bounded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_r3_people_core::application::{CoreFuture, PeopleCoreRead};
    use crate::adapters::native_r3_people_core::domain::{PayGroupOption, unavailable};
    use serde_json::json;

    struct Probe;

    impl PeopleCoreRead for Probe {
        fn today(&self) -> CoreFuture<'_, String> {
            Box::pin(async { Ok("2026-11-28".into()) })
        }
        fn pay_group_options(&self, _: &str) -> CoreFuture<'_, Vec<PayGroupOption>> {
            Box::pin(async {
                Ok(vec![PayGroupOption {
                    id: "pg-1".into(),
                    name: "Standard".into(),
                    currency: "THB".into(),
                    frequency: "monthly".into(),
                }])
            })
        }
        fn engagement(
            &self,
            _: &str,
            _: &str,
            _: Option<&str>,
            _: Option<&str>,
        ) -> CoreFuture<'_, Option<super::super::domain::HrEngagement>> {
            Box::pin(async { Err(unavailable()) })
        }
        fn assignments(
            &self,
            _: &str,
            _: &str,
        ) -> CoreFuture<'_, Vec<super::super::domain::HrAssignment>> {
            Box::pin(async { Err(unavailable()) })
        }
        fn policies(&self, _: &str) -> CoreFuture<'_, Vec<super::super::domain::HrPolicy>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn calendars(&self, _: &str) -> CoreFuture<'_, Vec<super::super::domain::HrCalendar>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn eligibility(&self, _: &str, _: &str) -> CoreFuture<'_, Option<super::super::domain::HrEligibility>> {
            Box::pin(async { Ok(None) })
        }
        fn affected_people(
            &self,
            _: &str,
            _: &str,
            _: Option<&str>,
            _: Option<&str>,
        ) -> CoreFuture<'_, Vec<super::super::domain::HrAffectedPerson>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn future_shifts(&self, _: &str, _: &str, _: i64) -> CoreFuture<'_, Vec<super::super::domain::HrShiftTotal>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn future_leaves(&self, _: &str, _: &str, _: &str) -> CoreFuture<'_, Vec<super::super::domain::HrPersonTotal>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn current_policies(
            &self,
            _: &str,
            _: &str,
            _: &str,
        ) -> CoreFuture<'_, Vec<super::super::domain::HrCurrentPolicy>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn engagement_groups(
            &self,
            _: &str,
            _: &str,
        ) -> CoreFuture<'_, Vec<super::super::domain::HrEngagementGroup>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn hire_source(&self, _: &str, _: &str) -> CoreFuture<'_, Option<super::super::domain::HireSource>> {
            Box::pin(async { Err(unavailable()) })
        }
        fn hire_receipt(&self, _: &str, _: &str) -> CoreFuture<'_, Option<super::super::domain::HireReceipt>> {
            Box::pin(async { Err(unavailable()) })
        }
        fn same_currency_groups(&self, _: &str, _: &str) -> CoreFuture<'_, Vec<PayGroupOption>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn matching_people(&self, _: &str, _: &str) -> CoreFuture<'_, Vec<super::super::domain::HirePerson>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn offer_application(&self, _: &str, _: &str) -> CoreFuture<'_, Option<String>> {
            Box::pin(async { Ok(None) })
        }
    }

    fn target(host: &str, slug: &str) -> PeopleCoreTarget {
        PeopleCoreTarget {
            host: host.into(),
            holding_slug: slug.into(),
            origin: format!("https://{host}"),
            store: Arc::new(Probe),
        }
    }

    #[test]
    fn path_constants_match_the_shared_handlers() {
        assert_eq!(HR_SETTINGS_PATH, "/api/v1/holdings/{holding_slug}/people/hr-settings");
        assert_eq!(
            PAY_GROUPS_PATH,
            "/api/v1/holdings/{holding_slug}/people/hire-handoffs/{id}/pay-groups"
        );
    }

    #[test]
    fn targets_are_exact_and_distinct() {
        assert!(router(vec![target("acme.test", "acme")]).is_ok());
        assert!(router(vec![target("acme.test", "acme"), target("acme.test", "acme")]).is_err());
        assert!(router(vec![]).is_err());
        assert!(router(vec![target("acme.test", "Acme")]).is_err());
    }

    #[test]
    fn hr_query_rejects_multiple_selectors_and_bad_values() {
        let mut input = hr_input(
            Some("topic=pay-groups&engagement=e-1"),
            "acme-co",
            None,
            None,
            vec![],
        );
        assert!(input.is_err());
        input = hr_input(Some("topic=unknown"), "acme-co", None, None, vec![]);
        assert!(input.is_err());
        input = hr_input(Some("impact=e-1"), "acme-co", None, None, vec![]);
        assert!(input.is_ok());
        assert_eq!(input.unwrap().impact, vec!["e-1".to_owned()]);
    }

    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, String, serde_json::Value) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(raw.as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.unwrap();
        let text = String::from_utf8_lossy(&bytes).to_string();
        let status = text
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse().ok())
            .unwrap();
        let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
        (
            status,
            head.to_owned(),
            serde_json::from_str(body).unwrap_or(json!(null)),
        )
    }

    #[tokio::test]
    async fn router_owns_envelope_headers_and_scope() {
        let router = router(vec![target("acme.test", "acme")]).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        let get = |host: &str, slug: &str, query: &str| {
            format!(
                "GET /api/v1/holdings/{slug}/people/hr-settings{query} HTTP/1.1\r\nHost: {host}\r\nOrigin: https://{host}\r\nConnection: close\r\n\
                 x-erpx-context-company: acme-co\r\nx-erpx-context-user: usr-1\r\n\
                 x-erpx-context-permissions: people.manage,organization.manage,people.payroll.details,payroll.prepare\r\n\r\n"
            )
        };
        let ok = http(addr, get("acme.test", "acme", "?topic=pay-groups")).await;
        assert_eq!(ok.0, 200);
        assert!(ok.1.to_ascii_lowercase().contains("cache-control: private, no-store"));
        assert_eq!(ok.2["data"]["topic"], json!("pay-groups"));
        assert_eq!(ok.2["data"]["groups"][0]["id"], json!("pg-1"));
        assert_eq!(ok.2["data"]["canManage"], json!(true));
        assert_eq!(http(addr, get("other.test", "acme", "?topic=pay-groups")).await.0, 404);
        assert_eq!(http(addr, get("acme.test", "acme", "?topic=x")).await.0, 422);
        server.abort();
    }
}
