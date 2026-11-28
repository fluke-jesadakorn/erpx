pub const CONTRACT_VERSION: &str = "collaboration.service-invoice-flow-notification.v1";
pub use crate::adapters::service_invoice_flow_notification::{
    RetainedServiceInvoiceFlowNotice, ServiceInvoiceFlowNotice, lock_service_invoice_flow_notices,
    stage_service_invoice_flow_notice,
};
