//! Commerce order notification scope only; caller retains Portal authority.
pub use crate::adapters::portal_notification_source::{
    PortalOrderScope, PortalOrderScopeWitness, lock_portal_order_sources, read_portal_order_scope,
};
