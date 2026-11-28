//! Public surface for the native CRM contact guest-history read.
//!
//! Composition assembles the actor, scope, purchase and Hotel-history facts
//! plus the serializable unit behind `GuestHistoryHttpBackend`; the router and
//! PostgreSQL adapter remain owner-owned and have no TypeScript fallback.
pub use crate::adapters::guest_history_http::{
    GuestHistoryHttpBackend, GuestHistoryHttpFuture, GuestHistoryHttpResponse,
    GuestHistoryHttpTarget, GuestHistoryReadRequest, guest_history_http_router,
};
pub use crate::adapters::guest_history_postgres::{
    PMS_PROPERTIES_SQL, PMS_RECORDS_SQL, STAYS_SQL, CONTACT_SQL, NATIVE_READ_FILTER,
    PgGuestHistory,
};
pub use crate::application::guest_history::{
    GuestHistoryFuture, GuestHistoryRepository, crm_guest_history,
};
pub use crate::domain::guest_history::{
    GuestContactRow, GuestHistoryActor, GuestHistorySource, GuestHistoryView, GuestStayRow,
    validate_guest_history,
};
