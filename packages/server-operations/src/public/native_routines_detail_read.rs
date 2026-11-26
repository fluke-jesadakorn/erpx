//! Public surface for the native routine-template detail read. Composition
//! mounts the router; TypeScript consumers use a strict transport, never the
//! retired `getTemplate` SQL.

pub use crate::adapters::native_routines_detail_read::{
    router, router_for_holdings, router_with_scope, PgTemplateDetailRead, TemplateDetailTarget,
};
pub use crate::application::routine_template_detail_read::{
    TemplateDetailFuture, TemplateDetailHandler, TemplateDetailRead,
};
pub use crate::application::routine_template_read::{
    TemplateScope, TemplateScopeFragment, TrustedTemplateScope,
};
pub use crate::domain::routine_template_detail_read::{
    render_detail_sql, validate_template_id, DEFAULT_SCOPE, TEMPLATE_DETAIL_SQL,
};
pub use crate::domain::routine_template_read::{MAX_ID_BYTES, TemplateActor, TemplateRow};
