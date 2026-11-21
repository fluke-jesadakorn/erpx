//! Native sandbox email delivery command HTTP ingress and owner router.
pub use crate::adapters::email_delivery_http::{
    EmailDeliveryHttpBackend, EmailDeliveryHttpFuture, EmailDeliveryHttpKind,
    EmailDeliveryHttpRequest, EmailDeliveryHttpResponse, EmailDeliveryHttpTarget,
    email_delivery_http_router,
};
