//! Finance Console vertical slice.
pub mod gateway;
pub mod models;
pub mod presenter;
pub mod view;
pub use gateway::{FinanceGateway, FinanceProtoTransport};
pub use models::{FinanceItem, FinanceItems};
pub use presenter::{FinanceContent, FinancePresenter};
pub use view::{FinanceRenderedState, render_finance_state};
