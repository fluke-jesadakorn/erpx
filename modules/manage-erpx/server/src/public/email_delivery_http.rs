//! Native sandbox email delivery command HTTP ingress and owner router.
pub use crate::adapters::email_delivery_http::{
    email_delivery_http_router, EmailDeliveryHttpBackend, EmailDeliveryHttpFuture,
    EmailDeliveryHttpKind, EmailDeliveryHttpRequest, EmailDeliveryHttpResponse,
    EmailDeliveryHttpTarget,
};
