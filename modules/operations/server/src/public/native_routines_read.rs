//! Stable owner exports for the native routine-template list read.
//!
//! Composition mounts `router_for_holdings` on the private API service and
//! injects its purchased-capability scope through `router_with_scope`. The
//! TypeScript route authority for this read is retired by the lead.

pub use crate::adapters::native_routines_read::{
    PgTemplateRead, TemplateReadTarget, router, router_for_holdings, router_with_scope,
};
pub use crate::application::routine_template_read::{
    TemplateListFuture, TemplateRead, TemplateReadHandler, TemplateScope, TemplateScopeFragment,
    TrustedTemplateScope,
};
pub use crate::domain::routine_template_read::{
    MAX_ID_BYTES, TEMPLATE_LIST_SQL, TemplateActor, TemplateRow, render_list_sql, sql_oracle,
    validate_identity,
};
