pub(crate) mod order;
pub(crate) mod order_http;
pub(crate) mod quality_masters;
pub(crate) mod quality_receipt_origin;
pub(crate) mod quality_return_join;
pub(crate) mod quality_return_origin;
pub(crate) mod quality_scrap_stock;
pub(crate) mod receipt_http;
pub(crate) mod record_source;

pub(crate) mod postgres_inventory_consumption;

pub(crate) mod field_service_reservation;

pub(crate) mod field_service_completion;

pub(crate) mod field_service_detail;

pub(crate) mod service_parts_posting;

pub mod pos_catalog;
pub(crate) mod receipt;
pub(crate) mod service_initial_count_masters;
pub(crate) mod service_stock_masters;

pub(crate) mod finance_payable_source;
pub mod legacy_fulfillment;
pub mod receipt_use_cases;
pub mod sales_delivery;

pub(crate) mod wave2_http;
pub(crate) mod owner_write_postgres;
pub(crate) mod owner_write_http;
pub(crate) mod wave2_postgres;

pub(crate) mod portal_commerce_locks;
pub(crate) mod portal_notification_source;
pub(crate) mod sales_reservation_source;

pub(crate) mod supplier_promise_version;

pub(crate) mod supplier_promise_source;
