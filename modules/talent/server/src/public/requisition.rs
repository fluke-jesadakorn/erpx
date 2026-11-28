//! Portable Talent requisition contract.
pub use crate::adapters::requisition::PgTalentRequisitions;
pub use crate::application::requisition::{
    RequisitionFuture, TalentRequisitionCommandRepository, TalentRequisitionReadRepository,
    change_requisition, create_requisition, requisition_detail, requisition_workspace,
};
pub use crate::domain::requisition::{
    CREATE_OPERATION, ChangePlan, CreatePlan, MAX_DESCRIPTION, MAX_EVENT_ROWS, MAX_OPENINGS,
    MAX_REASON, MAX_TITLE, MAX_WORKSPACE_ROWS, ReceiptSnapshot, RequisitionAction,
    RequisitionChange, RequisitionEventRow, RequisitionReceipt, RequisitionRequest, RequisitionRow,
    TalentActor, ValidatedChange, ValidatedRequisition, access_changed, branch_scope_required,
    change_identity, change_plan, create_identity, create_plan, idempotency_conflict,
    independent_review_required, normalize_text, readback_invalid, request_invalid,
    requisition_changed, requisition_id, requisition_not_found, source_invalid,
};
