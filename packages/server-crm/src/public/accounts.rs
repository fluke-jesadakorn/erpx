//! Native CRM account create ports and the PostgreSQL owner adapter.
//! Only `create-account` is implemented. `link-finance` / `link-guest`
//! remain IdentityAccess-owned in TypeScript per the CRM plan.
pub use crate::adapters::accounts::PgCrmAccountCreate;
pub use crate::application::accounts::{AccountsFuture, AccountCreateRepository, create_account};
pub use crate::domain::accounts::{
    AccountCreateInput, AccountCreatePlan, CREATE_ACCOUNT_OPERATION, record_id_for_key,
    validate_account_create,
};
