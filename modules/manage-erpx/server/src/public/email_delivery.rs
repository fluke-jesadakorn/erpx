//! Native sandbox email delivery command port for composition.
pub use crate::adapters::email_delivery_postgres::{EmailDeliveryError, PgEmailDelivery};
pub use crate::application::email_delivery::{reserve_email_delivery, EmailDeliveryRepository};
pub use crate::domain::email_delivery::EmailDeliveryInput;
