//! Native migration lane 32 for Benefits.
//!
//! The owner executes the released payroll-input read with sqlx behind an
//! owner-local axum router. The retained TypeScript adapters are thin clients:
//! they issue no SQL and fail closed when the native owner is unavailable.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{PayrollInputRead, PayrollInputReadFuture, ReadPayrollInputs};
pub use domain::{
    LaneTarget, PayrollInputFact, PayrollInputsRequest, PayrollInputsResponse,
};
pub use http::{router, router_for_holdings, router_with_reader};
pub use postgres::{PgPayrollInputRead, EXECUTED_SQL, SOURCE_SQL};
