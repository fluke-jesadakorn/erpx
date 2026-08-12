//! Target-neutral Operations UI models.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsExceptionItem {
    pub exception_id: String,
    pub version: u64,
    pub reference: String,
    pub scope: String,
    pub source_context: String,
    pub source_event_id: String,
    pub source_event_type: String,
    pub policy_ref: String,
    pub category: String,
    pub severity: String,
    pub assignee_id: String,
    pub manager_id: String,
    pub due_at: String,
    pub status: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsExceptionItems {
    pub exceptions: Vec<OperationsExceptionItem>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkQueuePolicyItem {
    pub policy_id: String,
    pub version: u64,
    pub scope: String,
    pub name: String,
    pub allowed_sources: Vec<String>,
    pub escalation_after_minutes: u32,
    pub status: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkQueuePolicyItems {
    pub policies: Vec<WorkQueuePolicyItem>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsSourceCountItem {
    pub source_context: String,
    pub reference_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsSummaryItem {
    pub read_row_count: u64,
    pub exception_count: u64,
    pub work_queue_policy_count: u64,
    pub event_count: u64,
    pub source_reference_count: u64,
    pub source_counts: Vec<OperationsSourceCountItem>,
    pub latest_event_at: Option<String>,
}
