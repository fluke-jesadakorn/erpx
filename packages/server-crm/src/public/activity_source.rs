//! Bounded CRM-owned source identity, not a full purchase or actor authority.
pub use crate::adapters::activity_source::{
    CrmActivitySourceKind, CrmActivitySourceRequest, CurrentCrmActivitySource,
    CrmActivityRecipientFacts, CurrentCrmActivityRecipientSource, inspect_crm_activity_recipient_source,
    inspect_crm_activity_source, lock_crm_activity_sources,
    CrmReviewedAccountLinkRequest, CurrentCrmReviewedAccountLink,
    inspect_crm_reviewed_account_link, lock_crm_reviewed_account_link_sources,
};
