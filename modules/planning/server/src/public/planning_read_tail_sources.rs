//! Native Planning read-tail contract. Callers render only fixed owner-controlled
//! source-row, ordered-aggregate and single-row heads.
pub use crate::adapters::planning_read_tail_sources::{
    PLANNING_READ_TAIL_SOURCES_JSON, PlanningReadTailSources, planning_read_tail_sources,
    render_jsonb_agg_ordered, render_jsonb_row_by_key, render_source_rows,
};
