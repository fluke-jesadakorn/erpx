//! Native R3 owner slice for `GET/POST .../expenses/{...path}` and
//! `POST .../expenses/preview`. The axum owner executes People-owned SQLx over
//! `finance_expenses`, validates every catch-all subpath and reproduces the
//! retired envelope, status and headers. Cross-owner command, options, staff,
//! route, approval and AI ports are supplied by composition.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{
    ExpenseAiPort, ExpenseCommandPort, ExpenseOptionsPort, ExpenseOwnerRead, ExpensePreviewPort,
    OwnerFuture,
};
pub use domain::{
    Decision, ExpenseActor, ExpensePath, ExpenseTarget, MAX_PATH_SEGMENTS, MAX_SEGMENT,
    command_key_required, invalid, not_found, origin_forbidden, unavailable, valid_id, valid_segment,
    valid_slug,
};
pub use http::{ExpensePorts, router, router_with_ports};
pub use postgres::{MAX_ROWS, PgExpenseOwnerRead, SQL_EXPENSE_DETAIL, SQL_EXPENSE_LIST};

#[cfg(test)]
mod tests {
    use super::domain::{ExpensePath, valid_id, valid_segment};

    #[test]
    fn owner_slice_exports_pure_contracts() {
        assert!(valid_id("e-1"));
        assert!(valid_segment("payments"));
        assert_eq!(
            ExpensePath::parse(&["e-1".to_owned(), "reverse".to_owned()]).unwrap(),
            ExpensePath::Reverse("e-1".into())
        );
    }
}
