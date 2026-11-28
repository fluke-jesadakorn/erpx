//! Native Work calendar read HTTP ingress and owner-local router.
pub use crate::adapters::calendar_read_http::{
    CalendarReadCommandInput, CalendarReadFuture, CalendarReadHttpBackend, CalendarReadHttpRequest,
    CalendarReadHttpTarget, PgCalendarReadBackend, calendar_read_router,
};
