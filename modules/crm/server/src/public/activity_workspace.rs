//! Native CRM activity workspace read port and its PostgreSQL owner adapters.
//!
//! Owner-side preparation only: no composition mount, no purchase gate and no
//! TypeScript consumer switch is part of this port. The caller binds the
//! repository/directory on its own serializable Holding unit.
pub use crate::adapters::activity_workspace::{
    ACTIVITIES_SQL, HISTORY_SQL, LEAD_SQL, NATIVE_READ_FILTER, RECIPIENTS_SQL, WORK_ROWS_SQL,
    PgActivityDirectory, PgActivityWorkspace, SiteMeasurementCard,
};
pub use crate::application::activity_workspace::{
    ActivityWorkspaceDirectory, ActivityWorkspaceFuture, ActivityWorkspaceRepository,
    activity_work, activity_workspace,
};
pub use crate::domain::activity_workspace::{
    ActivityItemView, ActivityRecipient, ActivityWorkspaceData, ActivityWorkspaceView,
    CrmActivityRow, CrmLeadRow, CrmWorkItem, WorkspaceLeadView, activity_can_change,
    due_date_millis, effective_manager, work_items, workspace_view,
};
pub use crate::domain::activities_native::ActivityActor;
