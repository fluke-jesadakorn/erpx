//! CRM Console vertical slice.
pub mod gateway;
pub mod models;
pub mod presenter;
pub mod view;
pub use gateway::{CrmGateway, CrmProtoTransport};
pub use models::{CrmLeadItem, CrmOpportunityItem, CrmOpportunityItems};
pub use presenter::{CrmContent, CrmPresenter};
pub use view::{CrmRenderedState, render_crm_state};
