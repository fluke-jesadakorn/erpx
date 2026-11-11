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
