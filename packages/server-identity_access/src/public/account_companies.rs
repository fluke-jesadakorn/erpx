//! Company choices are presentation data, never a Company permission or grant.
pub const CONTRACT_VERSION: &str = "identity.account-companies.v1";
pub use crate::adapters::account_companies::{
    AccountCompanyChoice, lock_account_company_sources, read_account_companies,
};
