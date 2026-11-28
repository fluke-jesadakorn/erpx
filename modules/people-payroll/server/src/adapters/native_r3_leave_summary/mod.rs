//! Native owner slice for the `GET .../leave/summary` read served by the
//! `platform/workplace/src/routes/api/v1/holdings/[holdingSlug]/leave/summary/route.ts`
//! handler. The axum owner executes the Company-scoped statements with sqlx,
//! validates and shapes the exact `{ data: { asOf, setupRequired, setupMessage,
//! policy, balances, upcoming, pending, history, holidays } }` envelope in
//! Rust, and no TypeScript prepare/batch fallback is part of this slice.
//!
//! Composition and the Next route deletion are lead-owned; this module owns
//! only the slice and its public constructors.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{
    LeaveSummaryQuery, LeaveSummaryRead, SummaryFuture, assert_unambiguous, leave_summary,
};
pub use domain::{
    Balance, BalanceView, HolidayRow, LeaveRequestRow, LeaveSummary, LeaveSummaryFacts, LeaveType,
    NO_POLICY_MESSAGE, NO_STAFF_MESSAGE, PolicyFact, SummaryPolicy, invalid, missing,
    policy_ambiguous, unavailable, utc_date, utc_year, valid_id,
};
pub use http::{
    LeaveSummaryTarget, router_for_holdings, router_with_reader,
};
pub use postgres::{
    PgLeaveSummaryRead, SQL_ACTIVE_STAFF, SQL_POLICY_MATCH, SQL_SUMMARY_BALANCE,
    SQL_SUMMARY_HOLIDAYS, SQL_SUMMARY_REQUESTS, SQL_SUMMARY_TYPES,
};
