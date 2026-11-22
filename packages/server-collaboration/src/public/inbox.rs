//! Native Inbox read port. Delivery/notice rows are owner-local; every other
//! source destination is a composition port and fails closed when absent.
pub const CONTRACT_VERSION: &str = "collaboration.inbox.v1";

pub use crate::adapters::inbox_http::{
    InboxCategoryCounts, InboxHttpBackend, InboxHttpFuture, InboxHttpReadReply,
    InboxHttpReadRequest, InboxHttpState, InboxHttpTarget, InboxItemData, InboxPageData,
    InboxQuery, InboxReadKind, InboxSummaryData, inbox_router,
};
pub use crate::adapters::inbox_pg::{
    InboxCursor, InboxNotificationQuery, InboxScope, InboxTarget, InboxTargetFuture,
    InboxTargetPort, PgInboxUnit, UnavailableInboxTargets, unavailable_targets,
};
pub use crate::adapters::inbox_targets::{
    ChainedInboxTargets, InboxTargetVisibility, PgInboxTargets, RetainedInboxDestination,
    encode_uri_component, is_owned_inbox_target, retained_inbox_destination, source_record_label,
};
pub use crate::adapters::inbox_calendar::{
    CalendarInboxTargets, calendar_appointment_href, is_calendar_inbox_target,
};
pub use crate::adapters::inbox_crm::{
    CrmInboxTargets, crm_activity_href, crm_lead_href, is_crm_inbox_target, require_purchase_history,
};
