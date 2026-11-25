//! Owner-local Control read-plan router for the five pinned provider-context
//! reads. A closed discriminator resolves to a byte-pinned statement; the TS
//! caller executes the reply on its own already-open Control transaction, so
//! the caller's lease/snapshot semantics are preserved and no SQL literal
//! remains in the TypeScript owner. No SQL is accepted from the wire and no
//! business authority is minted here.
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, routing::post,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};

pub const PC_REVOKED_SESSION: &str = "pc_revoked_session";
pub const PC_PLATFORM_ASSIGNMENTS: &str = "pc_platform_assignments";
pub const PC_PROVIDER_CONTEXT: &str = "pc_provider_context";
pub const PC_WORKSPACE_PREFERENCE: &str = "pc_workspace_preference";
pub const PC_CONTEXT_RECEIPT: &str = "pc_context_receipt";

/// Byte-identical successors of the five TypeScript statements pinned in
/// `tests/unit/identity_access/read-tail-r36-dedupe.test.ts`. `?` markers are
/// preserved for the caller's `SqlDatabase.prepare`.
pub const SQL_REVOKED_SESSION: &str =
    "SELECT 1 FROM iam_revoked_sessions revocation WHERE revocation.company_id='platform' AND revocation.user_id=? AND revocation.session_id IN(?,(SELECT parent_session_id FROM sso_session_bindings WHERE session_id=?))";
pub const SQL_PLATFORM_ASSIGNMENTS: &str =
    "SELECT assignment.id,assignment.role,assignment.version FROM platform_assignments assignment WHERE assignment.user_id=? AND iam_assignment_active(assignment.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(assignment.user_id,'platform',floor(extract(epoch FROM clock_timestamp())*1000)::bigint) ORDER BY assignment.valid_until,assignment.id";
pub const SQL_PROVIDER_CONTEXT: &str =
    "SELECT user_id,assignment_id,revision,updated_at FROM admin_provider_contexts WHERE session_id=?";
pub const SQL_WORKSPACE_PREFERENCE: &str =
    "SELECT platform_assignment_id FROM account_workspace_preferences WHERE user_id=?";
pub const SQL_CONTEXT_RECEIPT: &str =
    "SELECT company_id,actor_id,request_hash,result FROM iam_receipts WHERE id=?";

const BODY_LIMIT: usize = 8 * 1024;
const ORIGIN_HEADER: HeaderName = HeaderName::from_static("origin");

/// A planned Control read. `sql` is a byte-pinned owner constant; `values` is
/// the positional payload in the statement's bind order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlReadPlan {
    pub sql: &'static str,
    pub values: Vec<Value>,
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "IDENTITY_CONTROL_READ_PLAN_INVALID",
        "Use one bounded native Control read plan.",
    )
}

/// Resolve one registered kind. Callers never supply SQL, a relation, a
/// predicate or an unbound value type.
pub fn plan(kind: &str, values: &[Value]) -> Result<ControlReadPlan, BusinessError> {
    let (sql, expected) = match kind {
        PC_REVOKED_SESSION => (SQL_REVOKED_SESSION, 3),
        PC_PLATFORM_ASSIGNMENTS => (SQL_PLATFORM_ASSIGNMENTS, 1),
        PC_PROVIDER_CONTEXT => (SQL_PROVIDER_CONTEXT, 1),
        PC_WORKSPACE_PREFERENCE => (SQL_WORKSPACE_PREFERENCE, 1),
        PC_CONTEXT_RECEIPT => (SQL_CONTEXT_RECEIPT, 1),
        _ => return Err(invalid()),
    };
    if values.len() != expected || !values.iter().all(Value::is_string) {
        return Err(invalid());
    }
    if values.iter().any(|value| {
        value
            .as_str()
            .is_none_or(|text| text.is_empty() || text.len() > 512)
    }) {
        return Err(invalid());
    }
    Ok(ControlReadPlan {
        sql,
        values: values.to_vec(),
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlReadTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
}

struct StateData {
    targets: BTreeMap<(String, String), ControlReadTarget>,
}

fn unique(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first.map(|value| value.to_str().map_err(|_| invalid())).transpose()
}

fn valid_origin(value: &str) -> bool {
    value.strip_prefix("https://").is_some_and(|rest| {
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

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_'))
}

fn decode(body: &[u8]) -> Result<(String, Vec<Value>), BusinessError> {
    #[derive(Deserialize)]
    struct Wire {
        kind: String,
        #[serde(default)]
        values: Vec<Value>,
    }
    let wire: Wire = serde_json::from_slice(body).map_err(|_| invalid())?;
    if wire.kind.is_empty() || wire.kind.len() > 160 || wire.values.len() > 3 {
        return Err(invalid());
    }
    Ok((wire.kind, wire.values))
}

async fn read_plan(
    State(state): State<Arc<StateData>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let headers = request.headers().clone();
        let host = unique(&headers, header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(invalid)?;
        let target = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(invalid)?;
        if unique(&headers, ORIGIN_HEADER)?.is_some_and(|value| value != target.origin) {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            ));
        }
        if request.uri().query().is_some() {
            return Err(invalid());
        }
        let body = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| invalid())?;
        let (kind, values) = decode(&body)?;
        let planned = plan(&kind, &values)?;
        Ok(json!({"data":{"sql":planned.sql,"values":planned.values}}))
    }
    .await;
    match result {
        Ok(value) => {
            let mut response = (StatusCode::OK, Json(value)).into_response();
            response.headers_mut().insert(
                header::CACHE_CONTROL,
                header::HeaderValue::from_static("private, no-store"),
            );
            response
        }
        Err(error) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error":{"code":error.code,"message":error.message}})),
        )
            .into_response(),
    }
}

/// Composition-owned mount point. Targets are exact `(host, holding_slug)`
/// pairs with a matching application origin; duplicates and malformed targets
/// fail closed before any route is built.
pub fn router_for_holdings(
    targets: Vec<ControlReadTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !valid_origin(&target.origin)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    target,
                )
                .is_some()
        {
            return Err(invalid());
        }
    }
    if configured.is_empty() {
        return Err(invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/identity-access/control-read-plan",
            post(read_plan),
        )
        .with_state(Arc::new(StateData { targets: configured })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_runtime::public::http_support::serve;
    use std::net::SocketAddr;

    fn target(host: &str) -> ControlReadTarget {
        ControlReadTarget {
            host: host.into(),
            holding_slug: "acme".into(),
            origin: format!("https://{host}"),
        }
    }

    #[test]
    fn plans_are_byte_identical_to_the_pinned_control_statements() {
        for (kind, sql, count) in [
            (PC_REVOKED_SESSION, SQL_REVOKED_SESSION, 3),
            (PC_PLATFORM_ASSIGNMENTS, SQL_PLATFORM_ASSIGNMENTS, 1),
            (PC_PROVIDER_CONTEXT, SQL_PROVIDER_CONTEXT, 1),
            (PC_WORKSPACE_PREFERENCE, SQL_WORKSPACE_PREFERENCE, 1),
            (PC_CONTEXT_RECEIPT, SQL_CONTEXT_RECEIPT, 1),
        ] {
            let values = vec![Value::from("v"); count];
            let planned = plan(kind, &values).unwrap();
            assert_eq!(planned.sql, sql, "{kind}");
            assert_eq!(planned.values.len(), count, "{kind}");
            assert!(planned.sql.starts_with("SELECT "), "{kind}");
            assert!(!planned.sql.contains("INSERT INTO"), "{kind}");
            assert!(!planned.sql.contains("DELETE FROM"), "{kind}");
            assert!(!planned.sql.contains("UPDATE "), "{kind}");
        }
        assert_eq!(plan("unknown", &[]).unwrap_err().code, "IDENTITY_CONTROL_READ_PLAN_INVALID");
    }

    #[test]
    fn plan_arity_and_value_types_are_closed() {
        assert!(plan(PC_REVOKED_SESSION, &[Value::from("a"), Value::from("b")]).is_err());
        assert!(plan(PC_REVOKED_SESSION, &vec![Value::from("a"); 4]).is_err());
        assert!(plan(PC_CONTEXT_RECEIPT, &[Value::from(7)]).is_err());
        assert!(plan(PC_CONTEXT_RECEIPT, &[Value::from("")]).is_err());
        assert!(plan(PC_CONTEXT_RECEIPT, &[]).is_err());
    }

    #[test]
    fn router_targets_are_exact_and_distinct() {
        assert!(router_for_holdings(vec![target("acme.example.test")]).is_ok());
        assert!(router_for_holdings(vec![
            target("acme.example.test"),
            target("acme.example.test"),
        ])
        .is_err());
        assert!(router_for_holdings(vec![]).is_err());
        assert!(router_for_holdings(vec![ControlReadTarget {
            origin: "http://acme.example.test".into(),
            ..target("acme.example.test")
        }])
        .is_err());
        assert!(router_for_holdings(vec![ControlReadTarget {
            holding_slug: "Acme".into(),
            ..target("acme.example.test")
        }])
        .is_err());
    }

    fn post(path: &str, host: &str, origin: Option<&str>, body: &str) -> String {
        let origin = origin
            .map(|value| format!("Origin: {value}\r\n"))
            .unwrap_or_default();
        format!(
            "POST {path} HTTP/1.1\r\nHost: {host}\r\n{origin}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    async fn http(addr: SocketAddr, raw: String) -> (u16, Value) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(raw.as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.unwrap();
        let text = String::from_utf8_lossy(&bytes);
        let status = text
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse().ok())
            .unwrap();
        let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
        (status, serde_json::from_str(body).unwrap_or(Value::Null))
    }

    #[tokio::test]
    async fn http_router_returns_only_a_bounded_statement_reply() {
        let router = router_for_holdings(vec![target("acme.example.test")]).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = serve(listener, router).await;
        });
        let body = json!({"kind": PC_CONTEXT_RECEIPT, "values": ["key-1"]}).to_string();
        let path = "/api/v1/holdings/acme/identity-access/control-read-plan";
        let (status, reply) = http(
            addr,
            post(path, "acme.example.test", Some("https://acme.example.test"), &body),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(reply["data"]["sql"], SQL_CONTEXT_RECEIPT);
        assert_eq!(reply["data"]["values"], json!(["key-1"]));

        let (status, reply) = http(
            addr,
            post(
                path,
                "acme.example.test",
                Some("https://other.example.test"),
                &body,
            ),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(reply["error"]["code"], "ORIGIN_FORBIDDEN");

        let (status, reply) = http(
            addr,
            post(path, "other.example.test", None, &body),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(reply["error"]["code"], "IDENTITY_CONTROL_READ_PLAN_INVALID");

        let bad = json!({"kind": "arbitrary_sql", "values": ["DROP TABLE x"]}).to_string();
        let (status, reply) = http(
            addr,
            post(path, "acme.example.test", None, &bad),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(reply["error"]["code"], "IDENTITY_CONTROL_READ_PLAN_INVALID");
    }
}
