//! Domain-grouped integration test harness. Individual cases remain in their original files.

#[path = "cli.rs"]
mod cli;
#[path = "g11_asset_maintenance_acceptance.rs"]
mod g11_asset_maintenance_acceptance;
#[path = "g11_product_context_acceptance.rs"]
mod g11_product_context_acceptance;
#[path = "g11_sales_transport_acceptance.rs"]
mod g11_sales_transport_acceptance;
#[path = "g9_product_context_acceptance.rs"]
mod g9_product_context_acceptance;
#[path = "g9_product_transport_acceptance.rs"]
mod g9_product_transport_acceptance;
