pub use crate::adapters::field_service_workspace::{
    CurrentServiceWorkspaceTickets, ServiceTicketDisplay, ServiceTicketReference,
    ServiceWorkspaceTicketScope, ServiceWorkspaceTickets, lock_service_workspace_tickets,
    read_service_workspace_tickets,
};

pub use crate::adapters::field_service_workspace::{
    CurrentServiceSupportSettlement, ServiceSupportSettlementRows,
    ServiceSupportSettlementTicket, ServiceSupportSettlementAlert,
    lock_service_support_settlement, read_service_support_settlement,
};

/// Native raw source observation; its opaque revision binds owning-row digests.
pub const SERVICE_PACKAGE_SETTLEMENT_CONTRACT:&str="support.service-settlement.v1";
