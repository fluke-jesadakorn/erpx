//! Native CRM lead create / qualify command HTTP ingress.
pub use crate::adapters::leads_intake_http::{
    CrmLeadIntakeHttpBackend, CrmLeadIntakeHttpRequest, CrmLeadIntakeHttpTarget,
    crm_lead_intake_http_router,
};
