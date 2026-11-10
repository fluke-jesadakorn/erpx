//! Native Platform operations projection read replacing the retained TS summary fallback.
pub use crate::adapters::platform_operations_projection_http::{
    PlatformOperationsProjectionHttpBackend, PlatformOperationsProjectionHttpFuture,
    PlatformOperationsProjectionHttpReadRequest, PlatformOperationsProjectionHttpResponse,
    PlatformOperationsProjectionHttpService, PlatformOperationsProjectionHttpTarget,
    platform_operations_projection_router,
};
pub use crate::application::platform_operations::platform_operations_projection;
pub use crate::domain::platform_operations::PlatformOperationsProjection;
