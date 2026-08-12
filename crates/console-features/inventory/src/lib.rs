pub mod gateway;
pub mod models;
pub mod presenter;
pub mod view;

pub use gateway::{
    InventoryGateway, InventoryProtoTransport, present_inventory_item, present_inventory_items,
};
pub use models::{InventoryReservationItem, InventoryStockItem, InventoryStockItems};
pub use presenter::{InventoryContent, InventoryPresenter};
pub use view::{InventoryRenderedState, render_inventory_state};
erpx_ui_core::declare_feature_registration!(
    InventoryFeatureSurface,
    "inventory",
    "inventory.workspace",
    "/workspaces/inventory",
    "operations",
    "erpx-feature-inventory-v1",
    dioxus
);
