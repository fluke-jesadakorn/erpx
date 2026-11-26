//! Native Support SLA workspace read contract.
//!
//! Composition supplies the already-authorized serializable Holding unit,
//! selected Support scope, `support.sla` purchase gates and clock. This owner
//! surface executes the SQL and returns the complete retained payload; it opens
//! no transaction and keeps no TypeScript fallback.
pub const CONTRACT_VERSION: &str = "support.sla-workspace.v1";

pub use crate::adapters::sla_workspace::{load_sla_workspace, PgSlaWorkspace};
pub use crate::adapters::support_ticket_read::SupportReadScope;
pub use crate::application::sla_workspace::{
    read_sla_workspace, SlaWorkspaceFuture, SlaWorkspacePorts,
};
pub use crate::domain::sla_workspace::{
    can_approve, project, SlaPolicyRow, SlaPolicyView, SlaTicketRow, SlaTicketView,
    SlaWorkspaceScope, SlaWorkspaceView,
};
