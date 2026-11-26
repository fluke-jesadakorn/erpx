//! Native Work page-layout HTTP contract for composition.
pub use crate::adapters::page_layouts_http::{
    PageLayoutsCommandHttpRequest, PageLayoutsHttpBackend, PageLayoutsHttpFuture,
    PageLayoutsHttpResponse, PageLayoutsHttpTarget, PageLayoutsReadHttpRequest, page_layouts_router,
};
pub use crate::adapters::page_layouts_read::{
    PAGE_LAYOUTS_READ_SQL, PageLayoutRow, read_page_layout_rows, select_layouts,
};
pub use crate::domain::page_layouts::{
    CONFIGURABLE_PAGE_KEYS, PageLayout, PageLayoutAction, PageLayoutInput, is_configurable_page,
    layout_snapshot_json, parse_page_layout_input,
};
