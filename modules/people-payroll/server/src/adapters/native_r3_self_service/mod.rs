//! Native R3 owner slice for `GET .../self-service/summary`. The People-owned
//! next-shift statement executes with sqlx and the retired response headers and
//! envelope are reproduced in axum. Remaining aggregate sources stay behind
//! composition ports (Operations, Leave, Overtime, Work, Records).

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{NextShiftRead, OwnerFuture, SummaryAggregatePort};
pub use domain::{
    NextShift, SummaryActor, SummaryResult, SummaryTarget, invalid, not_found, unavailable,
    valid_id, valid_slug,
};
pub use http::{SummaryPorts, router, router_with_reader};
pub use postgres::{PgNextShiftRead, SQL_NEXT_SHIFT};

#[cfg(test)]
mod tests {
    use super::domain::{SummaryActor, valid_id};

    #[test]
    fn actor_contract_is_pure() {
        let actor = SummaryActor {
            company_id: "acme-co".into(),
            user_id: "u-1".into(),
            assignment_id: "a-1".into(),
            engagement_id: None,
        };
        assert_eq!(actor.company_id, "acme-co");
        assert!(valid_id("acme-co"));
    }
}
