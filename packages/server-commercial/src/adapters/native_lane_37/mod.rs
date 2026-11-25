//! Native migration lane 37 for commercial.
//!
//! The owner axum router executes the four retired TypeScript seat-history
//! SELECTs with sqlx, projects the rows and derives the Holding seat meter. The
//! TypeScript consumer is a strict transport that fails closed with 503.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{EmptySeatHistoryRead, ReadRequest, SeatHistoryFuture, SeatHistoryRead};
pub use domain::{
    derive_holding_seat_intervals, result_hash, HoldingSeatMeter, SeatCoverage,
    SeatHistoryResponse, SeatHistoryRows, SeatInterval, SeatMeterUser, SQL_ASSIGNMENTS,
    SQL_COVERAGE, SQL_MEMBERS, SQL_ORACLE, SQL_ROLES,
};
pub use http::{
    router, router_for_holdings, router_with_reader, router_with_readers, Lane37Target, LaneTarget,
};
pub use postgres::PgSeatHistoryRead;

#[cfg(test)]
mod lane_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn oracle_hashes_are_stable_over_sql_and_result() {
        let sql = serde_json::to_value(SQL_ORACLE).expect("sql");
        assert_eq!(
            result_hash(&sql),
            "09a6e67ece0a798f73826c179f846944675d3acd868682485afac2470385f661"
        );
        let empty = SeatHistoryResponse {
            rows: SeatHistoryRows::default().to_json(),
            meter: domain::unavailable_meter(),
        };
        assert_eq!(
            result_hash(&serde_json::to_value(&empty).expect("json")),
            "b59b557f07a11b0046a76c1f78835595cc075f6140fa474bba12c9d1d5440afc"
        );
    }

    #[test]
    fn response_shape_is_pinned() {
        let value = serde_json::to_value(SeatHistoryResponse {
            rows: SeatHistoryRows::default().to_json(),
            meter: domain::unavailable_meter(),
        })
        .expect("json");
        assert_eq!(
            value,
            json!({
                "rows": {
                    "historyStartedAt": null,
                    "assignments": [],
                    "members": [],
                    "roles": []
                },
                "meter": { "coverage": "unavailable", "users": [] }
            })
        );
    }
}
