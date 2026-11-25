//! Native migration lane 6 for people_payroll.
//!
//! Owner-local calendar-availability leave-day read: the axum router transports
//! bounded facts, the Postgres reader executes the retired company-scoped SQL
//! and the domain owns validation and the response projection.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{
    CalendarAvailabilityRead, CalendarPeopleFuture, CalendarPeopleRead, LeaveDayFuture,
    UnavailableCalendarPeopleRead,
};
pub use domain::{
    CalendarPeoplePage, CalendarPeopleQuery, CalendarPerson, LeaveDay, LeaveDayPage, LeaveDayQuery,
    MAX_PAGE_SIZE, MAX_SCOPE_IDS, MAX_SEARCH_LEN, validate_calendar_people,
};
pub use http::{
    CALENDAR_PEOPLE_PATH, PERSON_SCOPE_PATH, READ_PATH, LaneTarget, router, router_for_holdings,
    router_with_reader,
};
pub use postgres::{PgCalendarAvailabilityRead, PgCalendarPeopleRead};
