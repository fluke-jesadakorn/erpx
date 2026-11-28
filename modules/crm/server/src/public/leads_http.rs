//! Native CRM lead / opportunity command HTTP ingress.
pub use crate::adapters::leads_http::{
    CrmLeadCommandHttpBackend, CrmLeadCommandHttpFuture, CrmLeadCommandHttpRequest,
    CrmLeadCommandHttpTarget, CrmLeadCommandKind, crm_lead_command_http_router,
};
