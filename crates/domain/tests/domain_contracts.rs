//! Domain-grouped integration test harness. Individual cases remain in their original files.

#[path = "asset_maintenance.rs"]
mod asset_maintenance;
#[path = "finance_chart.rs"]
mod finance_chart;
#[path = "hr.rs"]
mod hr;
#[path = "marketing_content.rs"]
mod marketing_content;
#[path = "marketing_delivery.rs"]
mod marketing_delivery;
#[path = "pos_refund.rs"]
mod pos_refund;
#[path = "procurement_inventory.rs"]
mod procurement_inventory;
#[path = "quality_inspection.rs"]
mod quality_inspection;
