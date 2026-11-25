//! Native migration lane 6 for people_payroll.
//!
//! Owner-local calendar-availability leave-day read: the axum router transports
//! bounded facts, the Postgres reader executes the retired company-scoped SQL
//! and the domain owns validation and the response projection.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{CalendarAvailabilityRead, LeaveDayFuture};
pub use domain::{LeaveDay, LeaveDayPage, LeaveDayQuery};
pub use http::{LaneTarget, READ_PATH, router, router_with_reader};
pub use postgres::PgCalendarAvailabilityRead;
