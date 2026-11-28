//! Native Commerce wave2 warehouse-master axum command ingress.
pub use crate::adapters::wave2_http::{
    WarehouseSaveHttpBackend, WarehouseSaveHttpFuture, WarehouseSaveHttpRequest,
    WarehouseSaveHttpResponse, WarehouseSaveHttpTarget, commerce_warehouse_save_router,
};
