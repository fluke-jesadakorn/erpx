use super::account_contracts::AccountRevision;
use crate::domain::account_security::{RevokeSecurity, SecuritySeed};
use erpx_server_shared::public::BusinessError;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct AccountMfaEnrollInput {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct AccountMfaVerifyInput {
    pub code: String,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct AccountMfaEnrollment {
    pub secret: String,
    pub uri: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct AccountMfaVerified {
    pub verified_until: AccountRevision,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct AccountMfaEnrollmentResponse {
    pub data: AccountMfaEnrollment,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct AccountMfaVerifiedResponse {
    pub data: AccountMfaVerified,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct AccountSecurityFamily {
    pub handle: String,
    pub current: bool,
    pub created_at: AccountRevision,
    pub expires_at: AccountRevision,
    pub user_agent: Option<String>,
    pub audiences: Vec<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct AccountSecuritySeed {
    pub account_key: String,
    pub revision: String,
    pub mfa_enabled: bool,
    pub mfa_fresh_until: Option<AccountRevision>,
    pub families: Vec<AccountSecurityFamily>,
    pub legacy_sessions: AccountRevision,
}
impl TryFrom<SecuritySeed> for AccountSecuritySeed {
    type Error = BusinessError;
    fn try_from(seed: SecuritySeed) -> Result<Self, Self::Error> {
        Ok(Self {
            account_key: seed.account_key,
            revision: seed.revision,
            mfa_enabled: seed.mfa_enabled,
            mfa_fresh_until: seed.mfa_fresh_until.map(AccountRevision::new).transpose()?,
            legacy_sessions: AccountRevision::new(i64::try_from(seed.legacy_sessions).map_err(
                |_| super::super::adapters::account_http::invalid("INVALID_SESSION_COUNT"),
            )?)?,
            families: seed
                .families
                .into_iter()
                .map(|f| {
                    Ok(AccountSecurityFamily {
                        handle: f.handle,
                        current: f.current,
                        created_at: AccountRevision::new(f.created_at)?,
                        expires_at: AccountRevision::new(f.expires_at)?,
                        user_agent: f.user_agent,
                        audiences: f.audiences,
                    })
                })
                .collect::<Result<_, BusinessError>>()?,
        })
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct AccountSecurityInput {
    pub account_key: String,
    pub expected_revision: String,
    pub scope: AccountSecurityScope,
    #[cfg_attr(feature = "contracts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    pub reason: String,
}
impl<'de> Deserialize<'de> for AccountSecurityInput {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Wire {
            account_key: String,
            expected_revision: String,
            scope: AccountSecurityScope,
            #[serde(default, deserialize_with = "optional_family")]
            family: Option<String>,
            reason: String,
        }
        let input = Wire::deserialize(deserializer)?;
        Ok(Self {
            account_key: input.account_key,
            expected_revision: input.expected_revision,
            scope: input.scope,
            family: input.family,
            reason: input.reason,
        })
    }
}
fn optional_family<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub enum AccountSecurityScope {
    Family,
    All,
}
impl From<AccountSecurityInput> for RevokeSecurity {
    fn from(input: AccountSecurityInput) -> Self {
        Self {
            account_key: input.account_key,
            expected_revision: input.expected_revision,
            scope: match input.scope {
                AccountSecurityScope::Family => "family",
                AccountSecurityScope::All => "all",
            }
            .into(),
            family: input.family,
            reason: input.reason,
        }
    }
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct AccountSecurityResponse {
    pub data: AccountSecuritySeed,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct AccountSecurityRevoked {
    pub revoked: bool,
    pub count: AccountRevision,
    pub reauthenticate: bool,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct AccountSecurityRevokeResponse {
    pub data: AccountSecurityRevoked,
}
pub use crate::adapters::account_security_http::account_security_router;
#[cfg(feature = "contracts")]
pub use crate::adapters::account_security_http::security_openapi;
