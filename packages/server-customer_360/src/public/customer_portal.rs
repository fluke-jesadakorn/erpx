//! Native customer-portal projection and acknowledgement ports.
//!
//! Composition reconstructs [`PortalCustomerActor`] from the committed
//! `erpx_server_customer_authority::public::portal_session` proof
//! (`PortalCustomerProof`) plus the registered Holding, supplies the Commercial
//! purchase scope and the Files shared-file read, and runs each use case inside
//! the customer-portal owned unit. Purchase controls, support threads and
//! private-file previews are merged by composition; they are not re-implemented
//! here.
pub use crate::adapters::portal::PgCustomerPortal;
pub use crate::application::portal::{
    CustomerPortalEffects, CustomerPortalRepository, CustomerPortalSharedFiles,
    PortalCommandEvidence, PortalFuture, customer_portal_command, customer_portal_projection,
};
pub use crate::domain::portal::{
    CustomerPortalItem, CustomerPortalProjection, CustomerSharedFile, MAX_ATTACHMENT_SET,
    MAX_ITEMS, MAX_NOTE, PROJECTION_VERSION, PortalAction, PortalActionFact, PortalActionRow,
    PortalBinding, PortalCommandFacts, PortalCommandInput, PortalCommandOutcome,
    PortalCommandPurpose, PortalCustomerActor, PortalFulfillment, PortalHolding,
    PortalPurchaseEnforcement, PortalPurchaseScope,
};
