//! Native owner slice for the `GET .../leave/calendar` read served by the
//! `packages/workplace/src/routes/api/v1/holdings/[holdingSlug]/leave/calendar/route.ts`
//! handler. The axum owner executes the Company-scoped statements with sqlx,
//! validates and shapes the exact `{ data: { month, requests, holidays } }`
//! envelope in Rust, and no TypeScript prepare/batch fallback is part of this
//! slice.
//!
//! Composition and the Next route deletion are lead-owned; this module owns
//! only the slice and its public constructors.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{
    CalendarFuture, LeaveCalendarQuery, LeaveCalendarRead, leave_calendar_month,
};
pub use domain::{
    HolidayRow, LeaveCalendarFacts, LeaveCalendarMonth, LeaveRequestRow, invalid, missing,
    month_bounds, month_invalid, month_required, unavailable, valid_id, valid_month,
};
pub use http::{
    LeaveCalendarTarget, router_for_holdings, router_with_reader,
};
pub use postgres::{
    PgLeaveCalendarRead, SQL_ACTIVE_STAFF, SQL_CALENDAR_HOLIDAYS, SQL_CALENDAR_REQUESTS,
};
