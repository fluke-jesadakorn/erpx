//! Native CRM account ports and the PostgreSQL owner adapter.
//! `create-account` and `link-finance` are implemented and mounted at the
//! owner HTTP ingress in `public::accounts_http`. `create-contact`,
//! `link-guest` and `guest-sharing` remain TypeScript-owned for now.
pub use crate::adapters::accounts::PgCrmAccountCreate;
pub use crate::application::accounts::{
    AccountCreateRepository, AccountLinkFinanceRepository, AccountsFuture, create_account,
    link_account_finance,
};
pub use crate::domain::accounts::{
    AccountCreateInput, AccountCreatePlan, AccountLinkFinanceActor, AccountLinkFinanceInput,
    AccountLinkFinancePlan, CREATE_ACCOUNT_OPERATION, LINK_FINANCE_OPERATION, record_id_for_key,
    validate_account_create, validate_account_link_finance,
};
