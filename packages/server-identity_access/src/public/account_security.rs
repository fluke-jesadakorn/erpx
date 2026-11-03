pub use crate::adapters::account_mfa_crypto::{AccountMfaKeyring, MfaSecret};
pub use crate::adapters::account_mfa_postgres::{
    FreshMfaProof, PostgresAccountMfa, require_fresh_mfa_in_transaction,
};
pub use crate::adapters::account_security_postgres::PostgresAccountSecurity;
pub use crate::application::account_security::{
    AccountMfaRepository, AccountSecurity, AccountSecurityRepository,
};
pub use crate::domain::account_mfa::{
    MFA_FRESH_MS, MFA_STEP_MS, base32, candidate_counters, enrollment_uri, fresh_until,
    recent_authentication, valid_code,
};
pub use crate::domain::account_security::{
    RevokeSecurity, RevokedSecurity, SecurityFamily, SecuritySeed,
};
pub use crate::domain::account_security_snapshot::{
    Actor as AccountSecurityActor, Image as AccountSecuritySource,
};

/// Retained development projection used by existing callers.
pub fn project_account_security_snapshot(
    image: &AccountSecuritySource,
    actor: &AccountSecurityActor,
    now: i64,
) -> Result<SecuritySeed, erpx_server_shared::public::BusinessError> {
    crate::domain::account_security_snapshot::seed(image, actor, now, "development")
}

/// Configured-environment projection for a production runtime.
pub fn project_account_security_snapshot_for_environment(
    image: &AccountSecuritySource,
    actor: &AccountSecurityActor,
    now: i64,
    environment: &str,
) -> Result<SecuritySeed, erpx_server_shared::public::BusinessError> {
    crate::domain::account_security_snapshot::seed(image, actor, now, environment)
}
