//! Existing parts reservation scope only, without posting or personal rights.
pub use crate::adapters::field_service_reservation::{
    CurrentFieldServiceStockScope, FieldServiceStockScopeRequest,
    lock_field_service_stock_scope_sources, read_current_field_service_stock_scope,
};
