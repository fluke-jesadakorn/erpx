//! Native R3 owner slice for the `GET .../people/country-packs` read served by
//! the retained Next people catch-all. The axum owner executes the Company-
//! scoped country-pack statement with sqlx and returns the exact `{data}` body;
//! no TypeScript SQL remains for this bounded subpath.
//!
//! Authority (holding target), purchased-package admission and the retained
//! route's transport stay with composition and the original handler; this
//! module owns only the bounded options read and its public constructors.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{CountryPackFuture, CountryPacksRead};
pub use domain::{CountryPackRow, invalid, unavailable, valid_id, valid_slug};
pub use http::{CountryPacksTarget, router, router_with_reader};
pub use postgres::{PgCountryPacksRead, SQL_COUNTRY_PACKS, SQL_ORACLE_SHA256};

#[cfg(test)]
mod tests {
    use super::domain::valid_id;

    #[test]
    fn owner_dto_and_bounds_are_pure() {
        assert!(valid_id("acme-co"));
        assert!(!valid_id("bad id"));
    }
}
