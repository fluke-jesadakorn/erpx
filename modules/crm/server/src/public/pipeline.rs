//! Native CRM lead/opportunity pipeline read ports and the PostgreSQL owner
//! adapter.
pub use crate::adapters::pipeline::PgCrmPipelineRead;
pub use crate::application::pipeline::{
    CrmPipelineCustomerDirectory, CrmPipelineEntitlementPort, CrmPipelineFuture,
    CrmPipelinePeopleDirectory, CrmPipelineRepository, crm_pipeline,
};
pub use crate::domain::pipeline::{
    CRM_PIPELINE_ENTITLEMENT_UNPROVEN, CrmForecastSourceRow, CrmForecastTotal, CrmPipelineActor,
    CrmPipelineCursor, CrmPipelineEntitlement, CrmPipelinePage, CrmPipelinePageData,
    CrmPipelineQuery, CrmPipelineRow, CrmPurchaseControls, PAGE_SIZE, validate_forecast,
};
