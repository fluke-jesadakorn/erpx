//! Workplace Inbox HTTP read ingress. Write commands remain with the retained
//! TypeScript consumer until the Workflow ledger ownership moves natively.
pub const CONTRACT_VERSION: &str = "collaboration.inbox.http.v1";

pub use crate::adapters::inbox_http::{
    InboxCategoryCounts, InboxHttpBackend, InboxHttpFuture, InboxHttpReadReply,
    InboxHttpReadRequest, InboxHttpState, InboxHttpTarget, InboxItemData, InboxPageData,
    InboxQuery, InboxReadKind, InboxSummaryData, inbox_router, read_notice, read_notices,
    read_notifications, read_people, read_summary,
};
