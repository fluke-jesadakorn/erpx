//! Native CRM identity ports and PostgreSQL owner adapters.
//! All five owner-complete identity actions (`create-account`, `link-finance`,
//! `create-contact`, `link-guest` and `guest-sharing`) are implemented here and
//! dispatched at the owner HTTP ingress in `public::accounts_http`.
pub use crate::adapters::accounts::{PgCrmAccountCreate, PgCrmIdentityFamily};
pub use crate::application::accounts::{
    AccountCreateRepository, AccountLinkFinanceRepository, AccountsFuture,
    IdentityFamilyRepository, change_guest_sharing, create_account, create_contact, link_account_finance,
    link_guest,
};
pub use crate::domain::accounts::{
    AccountCreateInput, AccountCreatePlan, AccountLinkFinanceActor, AccountLinkFinanceInput,
    AccountLinkFinancePlan, ContactCreateInput, ContactCreatePlan, GuestLinkInput, GuestLinkPlan,
    GuestSharingInput, GuestSharingPlan, CREATE_ACCOUNT_OPERATION, CREATE_CONTACT_OPERATION,
    GUEST_SHARING_OPERATION, LINK_FINANCE_OPERATION, LINK_GUEST_OPERATION,
    contact_record_id_for_key, guest_link_record_id_for_key, record_id_for_key,
    sharing_event_id_for_key, validate_account_create, validate_account_link_finance,
    validate_contact_create, validate_guest_link, validate_guest_sharing,
};
