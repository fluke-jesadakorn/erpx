//! Sales Console vertical slice.

pub mod gateway;
pub mod models;
pub mod presenter;
pub mod view;

pub use gateway::{SalesGateway, SalesProtoTransport};
pub use models::{SalesOrderItem, SalesOrderItems, SalesQuoteItem};
pub use presenter::{SalesContent, SalesPresenter};
pub use view::{SalesRenderedState, render_sales_state};
