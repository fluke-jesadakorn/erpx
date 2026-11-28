//! Native Legal work-purchases filter contract. Callers supply only trusted
//! feature tables and their existing visibility/purchase predicates.
pub use crate::adapters::work_purchases::{
    WORK_PURCHASES_SOURCES_JSON, WorkPurchasesSources, render_work_feature_source,
    render_work_rows, work_approval_source, work_purchases_sources, work_record_source,
};
