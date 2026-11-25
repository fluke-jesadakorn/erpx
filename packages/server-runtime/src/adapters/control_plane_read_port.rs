//! Owner-local native read-port ingress for the runtime control plane.
//!
//! The axum ingress resolves one closed discriminator to an exact statement
//! owned by `adapters::postgres::control_plane::sql`. The TypeScript caller
//! executes the returned plan on its own already-open unit, so no owner SQL
//! literal remains in TypeScript and no SQL is accepted from the wire.
//! Session-scoped lock/unlock statements stay with the live session authority.

use crate::adapters::postgres::control_plane::sql;
use crate::public::http_support::{
    HeaderMap, HeaderName, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};

/// Discriminators shared with the TypeScript native read client.
pub const RD_MIGRATION_RECEIPTS: &str = "runtime_rd_migration_receipts";
pub const RD_MIGRATION_CHECKSUM: &str = "runtime_rd_migration_checksum";
pub const RD_RECEIPT_TABLE_PRESENT: &str = "runtime_rd_receipt_table_present";
pub const RD_COVERAGE_PROCEDURE_PRESENT: &str = "runtime_rd_coverage_procedure_present";
pub const RD_COVERAGE_REFRESH: &str = "runtime_rd_coverage_refresh";
pub const RD_RUNTIME_HEALTH: &str = "runtime_rd_runtime_health";
pub const RD_UTC_DATE: &str = "runtime_rd_utc_date";
pub const RD_OBJECT_LOCK: &str = "runtime_rd_object_lock";
pub const RD_OBJECT_UNLOCK: &str = "runtime_rd_object_unlock";
pub const RD_OBJECT_XACT_LOCK: &str = "runtime_rd_object_xact_lock";
pub const RD_PAIR_LOCK: &str = "runtime_rd_pair_lock";
pub const RD_PAIR_UNLOCK: &str = "runtime_rd_pair_unlock";

const BODY_LIMIT: usize = 64 * 1024;
const ORIGIN_HEADER: HeaderName = HeaderName::from_static("origin");

/// A planned control-plane read. `sql` is byte-identical to the retained native
/// port constant; `values` is the positional payload in bind order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlPlanePlan {
    pub sql: &'static str,
    pub values: Vec<Value>,
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "RUNTIME_CONTROL_PLANE_READ_INVALID",
        "Use one bounded native control-plane read plan.",
    )
}

/// Plan one registered control-plane read. Only the closed discriminator is
/// accepted; the caller never supplies SQL, a relation or a predicate.
pub fn plan(kind: &str, values: &[Value]) -> Result<ControlPlanePlan, BusinessError> {
    let (sql, expected) = match kind {
        RD_MIGRATION_RECEIPTS => (sql::MIGRATION_RECEIPTS, 0),
        RD_RECEIPT_TABLE_PRESENT => (sql::RECEIPT_TABLE_PRESENT, 0),
        RD_COVERAGE_PROCEDURE_PRESENT => (sql::COVERAGE_PROCEDURE_PRESENT, 0),
        RD_COVERAGE_REFRESH => ("SELECT ensure_work_cache_coverage()", 0),
        RD_RUNTIME_HEALTH => (sql::RUNTIME_HEALTH, 0),
        RD_UTC_DATE => (
            "SELECT to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') utc_date",
            0,
        ),
        RD_MIGRATION_CHECKSUM => (sql::RECEIPT_CHECKSUM, 1),
        RD_OBJECT_LOCK => (sql::OBJECT_LOCK, 1),
        RD_OBJECT_UNLOCK => (sql::OBJECT_UNLOCK, 1),
        RD_OBJECT_XACT_LOCK => (sql::OBJECT_XACT_LOCK, 1),
        RD_PAIR_LOCK => (sql::PAIR_LOCK, 2),
        RD_PAIR_UNLOCK => (sql::PAIR_UNLOCK, 2),
        _ => return Err(invalid()),
    };
    if values.len() != expected || (expected > 0 && !values.iter().all(Value::is_string)) {
        return Err(invalid());
    }
    Ok(ControlPlanePlan {
        sql,
        values: values.to_vec(),
    })
}

#[derive(Clone)]
struct Target {
    origin: String,
}

#[derive(Clone)]
struct StateData {
    targets: BTreeMap<String, Target>,
}

struct Input {
    kind: String,
    values: Vec<Value>,
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
        let host = rest.split_once(':').map_or(rest, |(h, _)| h);
        !host.is_empty()
            && !host.bytes().any(|b| matches!(b, b'/' | b'?' | b'#' | b'@' | b':'))
    })
}

fn decode(body: &[u8]) -> Result<Input, BusinessError> {
    #[derive(Deserialize)]
    struct Wire {
        kind: String,
        #[serde(default)]
        values: Vec<Value>,
    }
    let wire: Wire = serde_json::from_slice(body).map_err(|_| invalid())?;
    if wire.kind.is_empty() || wire.kind.len() > 160 || wire.values.len() > 8 {
        return Err(invalid());
    }
    Ok(Input {
        kind: wire.kind,
        values: wire.values,
    })
}

async fn read_port(
    State(state): State<Arc<StateData>>,
    Path(_slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let headers = request.headers().clone();
        let host = unique(&headers, header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(invalid)?;
        let target = state.targets.get(host).ok_or_else(invalid)?;
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
        let input = decode(&body)?;
        let plan = plan(&input.kind, &input.values)?;
        Ok(serde_json::json!({"data":{"sql":plan.sql,"values":plan.values}}))
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
            Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
        )
            .into_response(),
    }
}

/// Private Holding-keyed router. Mounting stays with composition; the router
/// itself is owner-owned and independently testable.
pub fn control_plane_read_router(origins: Vec<String>) -> Result<Router, BusinessError> {
    let mut targets = BTreeMap::new();
    for origin in origins {
        if !valid_origin(&origin) {
            return Err(invalid());
        }
        let host = origin
            .strip_prefix("https://")
            .and_then(|rest| rest.split_once(':').map_or(Some(rest), |(h, _)| Some(h)))
            .ok_or_else(invalid)?
            .to_owned();
        if targets.insert(host, Target { origin }).is_some() {
            return Err(invalid());
        }
    }
    if targets.is_empty() {
        return Err(invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/runtime/control-plane-read-port",
            post(read_port),
        )
        .with_state(Arc::new(StateData { targets })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_are_byte_identical_to_the_native_port() {
        assert_eq!(plan(RD_MIGRATION_RECEIPTS, &[]).unwrap().sql, sql::MIGRATION_RECEIPTS);
        assert_eq!(plan(RD_MIGRATION_CHECKSUM, &[Value::from("a.sql")]).unwrap().sql, sql::RECEIPT_CHECKSUM);
        assert_eq!(plan(RD_RECEIPT_TABLE_PRESENT, &[]).unwrap().sql, sql::RECEIPT_TABLE_PRESENT);
        assert_eq!(plan(RD_COVERAGE_PROCEDURE_PRESENT, &[]).unwrap().sql, sql::COVERAGE_PROCEDURE_PRESENT);
        assert_eq!(plan(RD_RUNTIME_HEALTH, &[]).unwrap().sql, sql::RUNTIME_HEALTH);
        assert_eq!(plan(RD_COVERAGE_REFRESH, &[]).unwrap().sql, "SELECT ensure_work_cache_coverage()");
        assert_eq!(
            plan(RD_UTC_DATE, &[]).unwrap().sql,
            "SELECT to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') utc_date"
        );
    }

    #[test]
    fn object_lock_plans_are_byte_identical_to_the_native_port() {
        for (kind, expected) in [
            (RD_OBJECT_LOCK, sql::OBJECT_LOCK),
            (RD_OBJECT_UNLOCK, sql::OBJECT_UNLOCK),
            (RD_OBJECT_XACT_LOCK, sql::OBJECT_XACT_LOCK),
        ] {
            let planned = plan(kind, &[Value::from("attachment-object:c:k")]).unwrap();
            assert_eq!(planned.sql, expected, "{kind}");
            assert_eq!(planned.values.len(), 1, "{kind}");
            assert!(planned.sql.contains("hashtextextended($1,0)"), "{kind}");
            assert!(plan(kind, &[]).is_err(), "{kind} must require one key");
            assert!(plan(kind, &[Value::from(7)]).is_err(), "{kind} must require a string key");
        }
    }

    #[test]
    fn pair_lock_plans_are_byte_identical_to_the_native_port() {
        for (kind, expected) in [
            (RD_PAIR_LOCK, sql::PAIR_LOCK),
            (RD_PAIR_UNLOCK, sql::PAIR_UNLOCK),
        ] {
            let planned = plan(kind, &[Value::from("transaction"), Value::from("aggregate-1")]).unwrap();
            assert_eq!(planned.sql, expected, "{kind}");
            assert_eq!(planned.values.len(), 2, "{kind}");
            assert!(planned.sql.contains("hashtext($1),hashtext($2)"), "{kind}");
            assert!(plan(kind, &[Value::from("only-one")]).is_err(), "{kind} needs two keys");
            assert!(plan(kind, &[Value::from(1), Value::from(2)]).is_err(), "{kind} needs string keys");
        }
    }

    #[test]
    fn runtime_health_plan_is_bind_free_and_read_only() {
        let planned = plan(RD_RUNTIME_HEALTH, &[]).unwrap();
        assert_eq!(planned.sql, sql::RUNTIME_HEALTH);
        assert!(planned.values.is_empty());
        assert!(planned.sql.starts_with("SELECT current_setting("));
        assert!(!planned.sql.contains('$'));
    }

    #[test]
    fn utc_date_plan_is_bind_free_and_read_only() {
        let planned = plan(RD_UTC_DATE, &[]).unwrap();
        assert!(planned.values.is_empty());
        assert!(planned.sql.starts_with("SELECT "));
        assert!(!planned.sql.contains('$'));
        assert!(!planned.sql.to_ascii_uppercase().contains("UPDATE"));
    }

    #[test]
    fn rejects_unknown_kinds_and_wrong_arity() {
        assert!(plan("SELECT 1", &[]).is_err());
        assert!(plan(RD_MIGRATION_CHECKSUM, &[]).is_err());
        assert!(plan(RD_MIGRATION_RECEIPTS, &[Value::from("x")]).is_err());
        assert!(plan(RD_MIGRATION_CHECKSUM, &[Value::from(1)]).is_err());
    }

    #[test]
    fn router_requires_a_registered_origin() {
        assert!(control_plane_read_router(vec![]).is_err());
        assert!(control_plane_read_router(vec!["http://x".to_owned()]).is_err());
        assert!(control_plane_read_router(vec![
            "https://acme.jesadakorn.com".to_owned()
        ])
        .is_ok());
    }
}
