//! Native Work navigation HTTP contract for composition.
pub use crate::adapters::work_navigation_http::{
    WorkNavigationHttpBackend, WorkNavigationHttpFuture, WorkNavigationHttpTarget,
    WorkNavigationMarkHttpResponse, WorkNavigationMarkHttpRequest, WorkNavigationReadHttpResponse,
    WorkNavigationReadHttpRequest, work_navigation_router,
};
pub use crate::adapters::work_navigation_execution::WorkNavigationBackend;
pub use crate::application::work_navigation::{
    MarkHomeReadCommand, NavigationFacts, NavigationQuery, WorkNavigationPort,
    mark_home_read, navigation_workspace_base, read_navigation,
};
pub use crate::domain::navigation_wire::{canonical_destination_link, canonical_destination_links};
pub use crate::domain::work_navigation_wire::{attention_items_json, navigation_snapshot_json};
