//! Native Customer 360 read-plan HTTP ingress.
//!
//! Transport facts only: exact registered Host/Holding targets, exact browser
//! Origin and Company scope, bounded JSON and a closed statement reply. The
//! kind is resolved by the owner's [`plan_for`] registry; no SQL, relation name
//! or bind is accepted from the wire and no database is opened here. The
//! caller executes the returned statement on its own already-open unit.
use crate::adapters::native_360_read_sql::plan_for;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, Value, body::to_bytes, header, json, post, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

const BODY_LIMIT: usize = 8 * 1024;
const MAX_VALUES: usize = 32;

/// One exact registered Holding target. `origin` is the browser origin and
/// `company_id` is the Company the native client carries on every plan call.
pub struct Customer360ReadPlanTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub company_id: String,
}

struct Customer360ReadPlanState {
    targets: BTreeMap<(String, String), Customer360ReadPlanTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_360_READ_PLAN_INVALID",
        "Use one registered Customer 360 native read plan.",
    )
}

fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
    )
}

fn company_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "CUSTOMER_360_READ_PLAN_COMPANY_INVALID",
        "Use the registered Holding Company.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_360_READ_PLAN_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
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
        Json(json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

fn bounded(body: Value) -> Response {
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

fn target<'a>(
    state: &'a Customer360ReadPlanState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a Customer360ReadPlanTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn require_target_facts(
    target: &Customer360ReadPlanTarget,
    headers: &HeaderMap,
) -> Result<(), BusinessError> {
    if unique_header(headers, header::ORIGIN)?.is_some_and(|value| value != target.origin) {
        return Err(origin_forbidden());
    }
    if unique_header(headers, HeaderName::from_static("x-erpx-context-company"))?
        != Some(target.company_id.as_str())
    {
        return Err(company_invalid());
    }
    Ok(())
}

fn primitive(value: &Value) -> bool {
    match value {
        Value::Null | Value::Bool(_) | Value::String(_) => true,
        Value::Number(number) => number.as_f64().is_some_and(f64::is_finite),
        _ => false,
    }
}

/// Pure ingress decision shared by the router and the Rust tests. It validates
/// the target facts and the closed plan request and returns the reply body.
fn plan_request(
    state: &Customer360ReadPlanState,
    headers: &HeaderMap,
    slug: &str,
    query: Option<&str>,
    bytes: &[u8],
) -> Result<Value, BusinessError> {
    let target = target(state, headers, slug)?;
    require_target_facts(target, headers)?;
    if query.is_some_and(|value| !value.is_empty()) {
        return Err(invalid());
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    let kind = value.get("kind").and_then(Value::as_str).ok_or_else(invalid)?;
    let values = value.get("values").and_then(Value::as_array).ok_or_else(invalid)?;
    if values.len() > MAX_VALUES || !values.iter().all(primitive) {
        return Err(invalid());
    }
    let plan = plan_for(kind, values.len()).map_err(|_| invalid())?;
    Ok(json!({"data":{"statement":{"sql":plan.sql,"values":values}}}))
}

async fn read_plan(
    State(state): State<Arc<Customer360ReadPlanState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let bytes = match to_bytes(body, BODY_LIMIT).await {
        Ok(bytes) => bytes,
        Err(_) => return error_response(invalid()),
    };
    match plan_request(
        &state,
        &parts.headers,
        &slug,
        parts.uri.query(),
        &bytes,
    ) {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

/// Build the owner router for exact registered Holding targets. Duplicates,
/// empty hosts, bad slugs/origins and empty Companies fail closed.
pub fn router_for_holdings(
    targets: Vec<Customer360ReadPlanTarget>,
) -> Result<Router, BusinessError> {
    let mut map = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !valid_origin(&target.origin)
            || target.company_id.is_empty()
            || map
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    target,
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/customer-360/read-plan",
            post(read_plan),
        )
        .with_state(Arc::new(Customer360ReadPlanState { targets: map })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_360_read_sql::{RD_PORTAL_ACTION_BY_BINDING, RD_PORTAL_ACTION_BY_KEY};

    const HOST: &str = "acme.example.test";
    const SLUG: &str = "acme";
    const ORIGIN: &str = "https://acme.example.test";
    const COMPANY: &str = "company";
    const KEY_SQL: &str = "SELECT id,binding_id,order_id,action,status,created_at,request_hash,user_id FROM customer_portal_actions WHERE request_key=?";

    fn target() -> Customer360ReadPlanTarget {
        Customer360ReadPlanTarget {
            host: HOST.into(),
            holding_slug: SLUG.into(),
            origin: ORIGIN.into(),
            company_id: COMPANY.into(),
        }
    }

    fn state() -> Customer360ReadPlanState {
        Customer360ReadPlanState {
            targets: BTreeMap::from([((HOST.to_owned(), SLUG.to_owned()), target())]),
        }
    }

    fn headers(host: &str, origin: Option<&str>, company: Option<&str>) -> HeaderMap {
        let mut map = HeaderMap::new();
        map.insert(header::HOST, HeaderValue::from_str(host).expect("host"));
        if let Some(origin) = origin {
            map.insert(header::ORIGIN, HeaderValue::from_str(origin).expect("origin"));
        }
        if let Some(company) = company {
            map.insert(
                HeaderName::from_static("x-erpx-context-company"),
                HeaderValue::from_str(company).expect("company"),
            );
        }
        map
    }

    fn body(kind: &str, values: &str) -> Vec<u8> {
        format!("{{\"kind\":\"{kind}\",\"values\":{values},\"now\":1800000000000}}").into_bytes()
    }

    #[test]
    fn plans_are_returned_in_the_exact_client_reply_shape() {
        let state = state();
        let reply = plan_request(
            &state,
            &headers(HOST, Some(ORIGIN), Some(COMPANY)),
            SLUG,
            None,
            &body(RD_PORTAL_ACTION_BY_KEY, "[\"request-key\"]"),
        )
        .expect("planned");
        assert_eq!(reply["data"]["statement"]["sql"], KEY_SQL);
        assert_eq!(reply["data"]["statement"]["values"][0], "request-key");
        let binding = plan_request(
            &state,
            &headers(HOST, Some(ORIGIN), Some(COMPANY)),
            SLUG,
            None,
            &body(RD_PORTAL_ACTION_BY_BINDING, "[\"binding\",\"acknowledge_delivery\"]"),
        )
        .expect("planned");
        assert_eq!(binding["data"]["statement"]["values"].as_array().map(Vec::len), Some(2));
        assert!(binding["data"]["statement"]["sql"].as_str().is_some_and(|sql| sql.ends_with("binding_id=? AND action=?")));
    }

    #[test]
    fn exact_target_facts_and_closed_plan_kinds_are_enforced() {
        let state = state();
        assert!(plan_request(&state, &headers("other.example.test", Some(ORIGIN), Some(COMPANY)), SLUG, None, &body(RD_PORTAL_ACTION_BY_KEY, "[\"k\"]")).is_err());
        assert!(plan_request(&state, &headers(HOST, Some(ORIGIN), Some(COMPANY)), "other", None, &body(RD_PORTAL_ACTION_BY_KEY, "[\"k\"]")).is_err());
        assert!(plan_request(&state, &headers(HOST, Some("https://evil.example.test"), Some(COMPANY)), SLUG, None, &body(RD_PORTAL_ACTION_BY_KEY, "[\"k\"]")).is_err());
        assert!(plan_request(&state, &headers(HOST, Some(ORIGIN), Some("other")), SLUG, None, &body(RD_PORTAL_ACTION_BY_KEY, "[\"k\"]")).is_err());
        assert!(plan_request(&state, &headers(HOST, None, None), SLUG, None, &body(RD_PORTAL_ACTION_BY_KEY, "[\"k\"]")).is_err());
        assert!(plan_request(&state, &headers(HOST, Some(ORIGIN), Some(COMPANY)), SLUG, Some("x=1"), &body(RD_PORTAL_ACTION_BY_KEY, "[\"k\"]")).is_err());
        assert!(plan_request(&state, &headers(HOST, Some(ORIGIN), Some(COMPANY)), SLUG, None, &body("c360_rd_unknown", "[]")).is_err());
        assert!(plan_request(&state, &headers(HOST, Some(ORIGIN), Some(COMPANY)), SLUG, None, &body(RD_PORTAL_ACTION_BY_KEY, "[]")).is_err());
        assert!(plan_request(&state, &headers(HOST, Some(ORIGIN), Some(COMPANY)), SLUG, None, &body(RD_PORTAL_ACTION_BY_KEY, "[\"k\",\"extra\"]")).is_err());
        assert!(plan_request(&state, &headers(HOST, Some(ORIGIN), Some(COMPANY)), SLUG, None, &body(RD_PORTAL_ACTION_BY_KEY, "[{\"sql\":\"x\"}]")).is_err());
        assert!(plan_request(&state, &headers(HOST, Some(ORIGIN), Some(COMPANY)), SLUG, None, b"not json").is_err());
    }

    #[test]
    fn router_configuration_fails_closed_on_duplicates_and_bad_targets() {
        assert!(router_for_holdings(vec![target()]).is_ok());
        assert!(router_for_holdings(vec![target(), target()]).is_err());
        assert!(router_for_holdings(vec![Customer360ReadPlanTarget { host: String::new(), ..target() }]).is_err());
        assert!(router_for_holdings(vec![Customer360ReadPlanTarget { holding_slug: "Bad".into(), ..target() }]).is_err());
        assert!(router_for_holdings(vec![Customer360ReadPlanTarget { origin: "http://acme.example.test".into(), ..target() }]).is_err());
        assert!(router_for_holdings(vec![Customer360ReadPlanTarget { company_id: String::new(), ..target() }]).is_err());
    }

    #[tokio::test]
    async fn actual_router_serves_the_client_plan_over_http() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let router = router_for_holdings(vec![target()]).expect("router");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let addr = listener.local_addr().expect("addr");
        let _server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });
        let payload = format!("{{\"kind\":\"{}\",\"values\":[\"request-key\"],\"now\":1800000000000}}", RD_PORTAL_ACTION_BY_KEY);
        let raw = format!(
            "POST /api/v1/holdings/{SLUG}/customer-360/read-plan HTTP/1.1\r\nHost: {HOST}\r\nOrigin: {ORIGIN}\r\nx-erpx-context-company: {COMPANY}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nConnection: close\r\n\r\n{payload}",
            payload.len()
        );
        let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
        stream.write_all(raw.as_bytes()).await.expect("write");
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.expect("read");
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.starts_with("HTTP/1.1 200"), "{text}");
        assert!(text.contains("cache-control: private, no-store"));
        let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
        let reply: Value = serde_json::from_str(body).expect("json reply");
        assert_eq!(reply["data"]["statement"]["sql"], KEY_SQL);
        assert_eq!(reply["data"]["statement"]["values"][0], "request-key");
    }
}
