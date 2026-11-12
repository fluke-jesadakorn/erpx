//! Native People workforce commands: directory person create/update/status,
//! engagement create/transition, compensation create/transition and the
//! private person detail read.
//!
//! The caller owns authorization composition and the serializable Holding
//! transaction. Organization placement, Identity account linking and Finance
//! money snapshots are supplied through the public ports; their defaults fail
//! closed.
pub use crate::adapters::workforce::{PgWorkforceCommand, PgWorkforceReader};
pub use crate::application::workforce::{
    CompensationInsert, CompensationMoneyPort, CompensationMoneyRequest,
    CompensationTransitionWrite, EngagementInsert, EngagementTransitionWrite,
    NoCompensationMoneyPort, NoPersonAccountLinkPort, NoStaffPlacementPort, PersonAccountLinkPort,
    PersonAccountLinkRequest, PersonInsert, PersonRecordInsert, PersonRecordResult,
    PersonStatusWrite, PersonUpdate, ResolvedPlacement, StaffPlacementPort, StaffPlacementRequest,
    StoredWorkforceReceipt, WorkforceCommandFuture, WorkforceCommandRepository,
    WorkforceLedgerEntry, create_compensation, create_engagement, create_person_record,
    person_detail, save_person, set_person_status, transition_compensation, transition_engagement,
};
pub use crate::domain::workforce::{
    COMPENSATION_CREATE_OPERATION, COMPENSATION_RECORD_TYPE, COMPENSATION_TRANSITION_OPERATION,
    CompensationComponent, CompensationCreateInput, CompensationRow, CompensationTransitionInput,
    CompensationWriteResult, DIRECTORY_CREATE_OPERATION, DIRECTORY_STATUS_OPERATION,
    DIRECTORY_UPDATE_OPERATION, ENGAGEMENT_CREATE_OPERATION, ENGAGEMENT_RECORD_TYPE,
    ENGAGEMENT_TRANSITION_OPERATION, EngagementCreateInput, EngagementRow,
    EngagementTransitionInput, EngagementWriteResult, PERSON_CREATE_OPERATION, PERSON_RECORD_TYPE,
    PersonCreateInput, PersonProfile, PersonRecordInput, PersonStatusFacts, PersonStatusInput,
    PersonStatusResult, PersonUpdateInput, PersonWriteResult, WorkforceCommandActor,
    WorkforceCompensationSummary, WorkforcePersonDetail, WorkforcePersonRow, compensation_minor,
    plan_compensation_transition, plan_engagement_transition, plan_person_status, valid_date,
};
