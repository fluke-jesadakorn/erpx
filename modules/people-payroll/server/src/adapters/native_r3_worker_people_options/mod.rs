//! Native R3 owner slice for the `GET .../people/project-worker-startup/people`
//! read. The axum owner executes the Company-scoped statement with sqlx,
//! validates and shapes the response in Rust, and a strict no-SQL TypeScript
//! transport can replace the retired plan.
//!
//! Command authority, commercial package admission and source witnesses stay
//! with composition and the retained worker-startup owner unit; this module
//! owns only the bounded options read and its public constructors.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{PeopleFuture, WorkerPeopleOptionsRead};
pub use domain::{
    MAX_PEOPLE, MAX_QUERY, WorkerPersonOption, invalid, unavailable, valid_id, valid_query,
};
pub use http::{WorkerPeopleOptionsTarget, router, router_with_reader};
pub use postgres::{PgWorkerPeopleOptionsRead, SQL_ORACLE_SHA256, SQL_WORKER_PEOPLE_OPTIONS};

#[cfg(test)]
mod tests {
    use super::domain::{WorkerPersonOption, valid_id, valid_query};

    #[test]
    fn owner_dto_and_bounds_are_pure() {
        let option = WorkerPersonOption {
            id: "p-1".into(),
            legal_name: "Ada".into(),
            version: 3,
            engagement_id: "e-1".into(),
            engagement_version: 2,
        };
        assert_eq!(option.id, "p-1");
        assert!(valid_id("acme-co"));
        assert!(valid_query("Ada"));
    }
}
