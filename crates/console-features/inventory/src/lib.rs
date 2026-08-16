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
erpx_ui_kit::declare_current_workspace_feature!(
    InventoryFeatureSurface,
    erpx_ui_kit::CurrentFeatureKind::Inventory,
    "inventory",
    [("inventory.workspace", "/inventory", "operations")],
    "erpx-feature-inventory-v2"
);
