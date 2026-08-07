//! Target-neutral Maintenance Console vertical slice.

pub mod gateway;
pub mod models;
pub mod presenter;
pub mod view;

pub use gateway::{
    MaintenanceGateway, MaintenanceProtoTransport, present_work_order, present_work_orders,
};
pub use models::{
    AssetFactItem, DowntimeItem, MaintenanceRequestItem, WorkOrderItem, WorkOrderItems,
};
pub use presenter::{MaintenanceContent, MaintenancePresenter};
pub use view::{MaintenanceRenderedState, render_maintenance_state};
