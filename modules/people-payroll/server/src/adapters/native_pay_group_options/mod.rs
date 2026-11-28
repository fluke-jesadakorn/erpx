//! Native owner slice for the active pay-group options read served by the
//! `GET .../people/pay-groups` catch-all branch. The axum owner executes the
//! Company-scoped statement with sqlx, validates and shapes the response in
//! Rust, and a strict no-SQL TypeScript transport replaces the retired plan.
//!
//! Composition and the catch-all deletion are lead-owned; this module only
//! owns the slice and its public constructors.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{OptionsFuture, PayGroupOptionsRead};
pub use domain::{MAX_GROUPS, PayGroupOption, invalid, unavailable, valid_id};
pub use http::{
    PayGroupOptionsTarget, router, router_with_reader,
};
pub use postgres::{PgPayGroupOptionsRead, SQL_ORACLE_SHA256, SQL_PAY_GROUP_OPTIONS};

#[cfg(test)]
mod tests {
    use super::domain::{PayGroupOption, valid_id};

    #[test]
    fn owner_dto_and_bounds_are_pure() {
        let option = PayGroupOption {
            id: "pg-1".into(),
            name: "Monthly".into(),
            currency: "THB".into(),
            frequency: "monthly".into(),
        };
        assert_eq!(option.id, "pg-1");
        assert!(valid_id("acme-co"));
        assert!(!valid_id("x'; DROP TABLE hr_pay_groups --"));
    }
}
