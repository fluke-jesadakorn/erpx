//! No-TEMP Files routing facts; not authority, purchase or complete ancestry.
pub use crate::adapters::field_attachment_routing::{
    CurrentAttachmentReadRoute, CurrentFieldAttachmentRouting,
    CurrentFieldAttachmentRoutingObservation, CurrentGlobalAttachmentAncestry,
    CurrentGlobalFieldVisitLink, CurrentUnscopedAttachmentLink,
    CurrentUnscopedGlobalAttachmentAncestry, FIELD_ATTACHMENT_ANCESTRY_SQL,
    GlobalFieldVisitLinkFacts, inspect_field_attachment_routing,
    lock_field_attachment_routing_sources, observe_field_attachment_routing,
    observe_global_attachment_ancestry, observe_global_field_visit_link,
    observe_unscoped_attachment_link, observe_unscoped_global_attachment_ancestry,
};
