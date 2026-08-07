//! Customer Authority Console vertical slice.

pub mod gateway;
pub mod presenter;
pub mod view;

pub use gateway::{CustomerAuthorityGateway, CustomerAuthorityProtoTransport};
pub use presenter::{CustomerAuthorityContent, CustomerAuthorityPresenter};
pub use view::{CustomerAuthorityRenderedState, render_customer_authority_state};
