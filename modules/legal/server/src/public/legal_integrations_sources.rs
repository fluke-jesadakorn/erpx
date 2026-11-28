//! Native Legal integrations read contract. Callers supply only trusted feature
//! tables, Workflow projection fragments and the owner-built visibility and
//! purchase predicates.
pub use crate::adapters::legal_integrations_sources::{
    LEGAL_INTEGRATIONS_SOURCES_JSON, LegalIntegrationsSources, legal_calendar_base_sql,
    legal_integrations_sources, render_legal_approvals, render_legal_calendar_feature,
    render_legal_calendar_rows, render_legal_tasks,
};
