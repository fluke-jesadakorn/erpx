//! Support auxiliary read contract for Sales and Collaboration consumers.
//!
//! The caller keeps the current purchase/release gate and Holding transaction.
//! These reads return only what the selected Support scope already reveals.
pub const CONTRACT_VERSION: &str = "support.auxiliary-reads.v1";

pub use crate::adapters::support_auxiliary::{
    SupportAlertSourceSnapshot, SupportAuxiliaryScope, SupportIncidentSourceSnapshot,
    SupportPolicySourceSnapshot, SupportSalesWorkspace, SupportSessionSourceSnapshot,
    SupportTicketSourceSnapshot, read_alert_source, read_incident_source, read_policy_source,
    read_sales_workspace, read_session_source, read_ticket_source, require_source,
};
