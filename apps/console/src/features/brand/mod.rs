//! Brand Console vertical slice.

pub mod gateway;
pub mod presenter;
pub mod view;

pub use gateway::{BrandGateway, BrandProtoTransport};
pub use presenter::{BrandContent, BrandPresenter};
pub use view::{BrandRenderedState, render_brand_state};
