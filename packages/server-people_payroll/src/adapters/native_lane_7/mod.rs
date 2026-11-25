//! Native migration lane 7 for people_payroll.
//!
//! One live read family is owned here end to end: the legacy people access
//! transition clock that was a TypeScript SELECT in
//! `adapters/legacy-license-access.ts`. Rust owns the statement, its Company
//! scope, the projection and the response; the retained TypeScript is a strict
//! transport that fails closed with a 503 when the native service is absent.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{LegacyPeopleClock, TransitionFuture, TransitionQuery, read_next_transition};
pub use domain::{
    LegacyPeopleTransition, READ_KIND, ROUTE, invalid_company, invalid_observed_at, unavailable,
    validate_company,
};
pub use http::{LaneTarget, router, router_with_reader};
pub use postgres::{PgLegacyPeopleClock, SQL_LEGACY_PEOPLE_TRANSITION};

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Digest;

    #[test]
    fn response_payload_matches_the_client_contract() {
        let facts = LegacyPeopleTransition {
            deadline: Some(1_764_010_200_000),
        };
        let body = serde_json::to_string(&serde_json::json!({
            "kind": READ_KIND,
            "transition": {"deadline": facts.deadline}
        }))
        .expect("serialize");
        assert_eq!(
            body,
            r#"{"kind":"people_payroll_lane_7_legacy_people_transition","transition":{"deadline":1764010200000}}"#
        );
        let hash = sha2::Sha256::digest(body.as_bytes());
        let hex = hash
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            hex,
            "b9d5a6988cbe97e37f2c13ff946ff430253e78d54d4ded15907db887ac65b98f"
        );
    }

    #[test]
    fn null_deadline_oracle_is_pinned() {
        let body = serde_json::to_string(&serde_json::json!({
            "kind": READ_KIND,
            "transition": {"deadline": Option::<i64>::None}
        }))
        .expect("serialize");
        assert_eq!(
            body,
            r#"{"kind":"people_payroll_lane_7_legacy_people_transition","transition":{"deadline":null}}"#
        );
        let hex = sha2::Sha256::digest(body.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            hex,
            "bd064dfed7af1df5efe7abc0ae841cc03c6bd998ea8845416b8d3189c30431b3"
        );
    }
}
