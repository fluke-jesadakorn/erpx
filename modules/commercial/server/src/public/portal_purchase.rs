//! Portal grant-fence identity and pre-purchase decision, not actor authority.
pub use crate::adapters::portal_purchase::{
    PortalPurchaseFence, PortalPurchasePreDecision, PortalPurchaseShadow, begin_portal_purchase,
    finish_portal_purchase, inspect_portal_purchase_pre, write_portal_purchase_shadow,
};
