//! Native migration lane 9, family 3: the work-review person-route read.
//!
//! The axum owner executes the Company-scoped `routePeople` statement with
//! sqlx, validates and shapes the response in Rust, and a strict multi-holding
//! TS transport replaces the deleted TypeScript SQL.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{PersonRouteRead, RouteFuture};
pub use domain::{RoutePerson, valid_id, valid_slug, valid_users};
pub use http::{
    router, router_for_holdings, router_with_reader, router_with_readers,
};
pub use postgres::{PgPersonRouteRead, SQL_ROUTE_PEOPLE};

#[cfg(test)]
mod tests {
    use super::domain::{RoutePerson, valid_id, valid_users};
    use erpx_server_runtime::public::http_support::serde_json::{self, json};
    use sha2::Digest;

    #[test]
    fn response_shape_matches_the_client_contract_oracle() {
        let rows = vec![RoutePerson {
            user_id: "user-1".into(),
            legal_name: "Ada Lovelace".into(),
        }]
        .into_iter()
        .map(|row| json!({"user_id": row.user_id, "legal_name": row.legal_name}))
        .collect::<Vec<_>>();
        let body = serde_json::to_string(&json!({"results": rows})).unwrap();
        assert_eq!(body, r#"{"results":[{"legal_name":"Ada Lovelace","user_id":"user-1"}]}"#);
        let hex = sha2::Sha256::digest(body.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            hex,
            "07600369a17612953f9810e031254acd20df6597dc6246ccdfe405767fed1dc2",
            "update this oracle in Rust and in tests/unit/people_payroll/native-lane-9.test.ts"
        );
    }

    #[test]
    fn owners_never_accept_callers_sql_or_unbounded_values() {
        assert!(valid_id("user-1"));
        assert!(!valid_id("a b"));
        assert!(!valid_id("x'; DROP TABLE hr_people --"));
        assert!(valid_users(&["user-1".into()]));
        assert!(!valid_users(&[]));
    }
}
