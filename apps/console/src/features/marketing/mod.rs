//! Marketing Console vertical slice.
pub mod gateway;
pub mod models;
pub mod presenter;
pub mod view;
pub use gateway::{MarketingGateway, MarketingProtoTransport};
pub use models::{MarketingLeadItem, MarketingLeadItems};
pub use presenter::{MarketingContent, MarketingPresenter};
pub use view::{MarketingRenderedState, render_marketing_state};
