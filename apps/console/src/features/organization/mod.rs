//! Organization Console vertical slice.

pub mod gateway;
pub mod presenter;
pub mod view;

pub use gateway::{OrganizationGateway, OrganizationProtoTransport};
pub use presenter::{OrganizationContent, OrganizationPresenter};
pub use view::{OrganizationRenderedState, render_organization_state};
