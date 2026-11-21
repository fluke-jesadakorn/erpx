//! Bounded CRM-owned source identity, not a full purchase or actor authority.
pub use crate::adapters::activity_source::{
    CrmActivityRecipientFacts, CrmActivitySourceKind, CrmActivitySourceRequest,
    CrmReviewedAccountLinkRequest, CurrentCrmActivityRecipientSource, CurrentCrmActivitySource,
    CurrentCrmReviewedAccountLink, inspect_crm_activity_recipient_source,
    inspect_crm_activity_source, inspect_crm_reviewed_account_link, lock_crm_activity_sources,
    lock_crm_reviewed_account_link_sources,
};
