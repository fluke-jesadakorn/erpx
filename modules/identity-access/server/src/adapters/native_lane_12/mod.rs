//! Native migration lane 12 for identity_access.
//!
//! The owner-local People-template attribute read set (`attributes`,
//! `activeRole`, `groupReferences`) and the three workflow approval assignment
//! facts (`canOverrideStaffStepRole`, `approvalViewActiveScope`,
//! `expenseDecisionEvidence`) are executed by this axum/sqlx module. The
//! TypeScript adapters transport bounded company/assignment inputs and pin the
//! response; there is no TypeScript SQL fallback. Command guards stay in TS.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{AttributeReadFuture, PeopleTemplateRead};
pub use domain::{ApprovalRead, AttributePage, AttributeRead, LaneTarget, ReadPage, valid_id};
pub use http::{router, router_for_holdings, router_with_reader};
pub use postgres::{
    ACTIVE_ROLE_SQL, APPROVAL_ACTIVE_SCOPE_SQL_TS, APPROVAL_EXPENSE_EVIDENCE_SQL_TS,
    APPROVAL_OVERRIDE_SQL_TS, ATTRIBUTES_SQL, GROUP_REFERENCES_SQL, PgPeopleTemplateRead, render,
};
