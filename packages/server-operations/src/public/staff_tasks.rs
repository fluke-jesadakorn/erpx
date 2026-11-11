//! Native Staff Work task/alert commands and bounded Work reads.
pub const CONTRACT_VERSION: &str = "operations.staff-tasks.v1";
pub use crate::adapters::staff_tasks::{PgStaffTasks, lock_staff_task_sources, read_staff_task};
pub use crate::application::staff_tasks::{
    AlertPlan, DelegateTaskPlan, DelegationFact, PostFact, ProjectFact, RevokeDelegationPlan,
    RouteFact, SaveTaskPlan, StaffFact, StaffTaskFuture, StaffTaskRepository, StaffTaskSources,
    TransitionTaskPlan, can_attach_task, delegate_task, escalate_staff_tasks, list_tasks,
    revoke_task_delegation, save_task, task_detail, task_is_assigned, transition_task,
};
pub use crate::domain::staff_tasks::{
    ActiveDelegation, AlertTask, ApprovalView, AttachmentRow, DecisionFact, DelegateTaskCommand,
    DelegationHistoryRow, MAX_ALERTS, MAX_APPROVERS, MAX_DESCRIPTION, MAX_LIST, MAX_TITLE,
    RevokeTaskDelegationCommand, SaveTaskCommand, StaffTaskActor, StaffTaskAlertResult,
    StoredTaskReceipt, TaskAccessFacts, TaskAction, TaskDetail, TaskReceipt, TaskRow, TaskScope,
    TransitionPlan, TransitionTaskCommand, can_attach, can_configure, delegation_record_id,
    fingerprint_json, is_assigned, permitted_actions, plan_transition, require_scope,
    task_record_id,
};
