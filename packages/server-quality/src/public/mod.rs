pub mod inventory_consumption;

pub mod field_service_reservation;

pub mod field_service_detail;
pub mod initial_count_history;

pub mod quality_http;

pub mod quality_commands;
pub mod quality_read;
pub mod quality_alerts;
pub mod purchase_lineage;
pub mod purchase_lineage_join;
pub mod quality_inspection_attachment_refs;
pub mod goods_receipt_hold;

pub mod attachment_hold;
pub mod attachment_descriptor;

pub mod portal_commerce_locks;

/// Native-owned customer-return Quality hold statement port.
pub mod customer_return_hold {
    pub use crate::adapters::customer_return_hold_statement::{
        customer_return_hold_statements, CustomerReturnHoldInput, CustomerReturnHoldStatements,
        HoldStatement,
    };
}

/// Native customer-return Quality hold statement HTTP ingress.
pub mod customer_return_hold_http {
    pub use crate::adapters::customer_return_hold_http::{
        hold_statement_router, HoldStatementBackend, HoldStatementFuture, HoldStatementRequest,
        HoldStatementTarget,
    };
}
