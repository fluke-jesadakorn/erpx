use serde::{Deserialize, Serialize};

/// A JSON revision is an exact JavaScript integer, never a lossy i64 conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 0, maximum = 9_007_199_254_740_991_i64))]
pub struct AccountRevision(#[cfg_attr(feature = "contracts", ts(type = "number"))] i64);
impl AccountRevision {
    pub fn new(value: i64) -> Result<Self, erpx_server_shared::public::BusinessError> {
        if !(0..=crate::domain::account_auth::MAX_DEVICE_REVISION).contains(&value) {
            return Err(erpx_server_shared::public::BusinessError::invalid(
                "INVALID_DEVICE_REVISION",
                "Review the current saved account revision.",
            ));
        }
        Ok(Self(value))
    }
    pub fn get(self) -> i64 {
        self.0
    }
}
impl TryFrom<i64> for AccountRevision {
    type Error = erpx_server_shared::public::BusinessError;
    fn try_from(value: i64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<AccountRevision> for i64 {
    fn from(value: AccountRevision) -> Self {
        value.0
    }
}
impl Serialize for AccountRevision {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.0)
    }
}
impl<'de> Deserialize<'de> for AccountRevision {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(erpx_server_shared::public::json_numbers::deserialize_safe_i64(deserializer)?)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Deserialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmailSignInInput {
    pub email: String,
    pub password: String,
    #[cfg_attr(feature = "contracts", ts(optional))]
    pub revision: Option<AccountRevision>,
    #[cfg_attr(feature = "contracts", ts(optional))]
    #[serde(rename = "callbackURL")]
    pub callback_url: Option<String>,
    #[cfg_attr(feature = "contracts", ts(optional))]
    pub remember_me: Option<bool>,
}

#[derive(Deserialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmailSignUpInput {
    pub name: String,
    pub email: String,
    pub password: String,
    #[cfg_attr(feature = "contracts", ts(optional))]
    #[serde(rename = "callbackURL")]
    pub callback_url: Option<String>,
    #[cfg_attr(feature = "contracts", ts(optional))]
    pub invitation_token: Option<String>,
}

#[derive(Deserialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmailVerificationInput {
    pub token: String,
    #[cfg_attr(feature = "contracts", ts(optional))]
    #[serde(rename = "callbackURL")]
    pub callback_url: Option<String>,
    #[cfg_attr(feature = "contracts", ts(optional))]
    pub revision: Option<AccountRevision>,
}

#[derive(Deserialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PasswordChangeInput {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Deserialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PasswordResetRequestInput {
    pub email: String,
    #[cfg_attr(feature = "contracts", ts(optional))]
    pub redirect_to: Option<String>,
}

#[derive(Deserialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PasswordResetInput {
    pub new_password: String,
    pub token: String,
}

#[derive(Deserialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmailVerificationRequestInput {
    pub email: String,
    #[cfg_attr(feature = "contracts", ts(optional))]
    #[serde(rename = "callbackURL")]
    pub callback_url: Option<String>,
}

#[derive(Deserialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct DeviceSelectionInput {
    pub handle: String,
    pub revision: AccountRevision,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountUserDto {
    pub id: String,
    pub name: String,
    pub email: String,
    pub email_verified: bool,
    pub image: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountSessionDto {
    pub id: String,
    pub user_id: String,
    pub expires_at: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct AccountSessionResponse {
    pub session: AccountSessionDto,
    pub user: AccountUserDto,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct DeviceAccountDto {
    pub handle: String,
    pub name: String,
    pub email: String,
    pub image: Option<String>,
    pub current: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct DeviceAccountsDto {
    pub initialized: bool,
    pub revision: AccountRevision,
    pub maximum: usize,
    pub accounts: Vec<DeviceAccountDto>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct AccountAuthError {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct AccountAuthErrorResponse {
    pub error: AccountAuthError,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct EmailSignInResponse {
    pub user: AccountUserDto,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct AccountOperationResponse {
    pub status: bool,
}

impl From<crate::domain::account_auth::AccountUser> for AccountUserDto {
    fn from(value: crate::domain::account_auth::AccountUser) -> Self {
        Self {
            id: value.id,
            name: value.name,
            email: value.email,
            email_verified: value.email_verified,
            image: value.image,
        }
    }
}
impl TryFrom<crate::domain::account_auth::DeviceAccounts> for DeviceAccountsDto {
    type Error = erpx_server_shared::public::BusinessError;
    fn try_from(value: crate::domain::account_auth::DeviceAccounts) -> Result<Self, Self::Error> {
        Ok(Self {
            initialized: value.initialized,
            revision: AccountRevision::new(value.revision)?,
            maximum: value.maximum,
            accounts: value
                .accounts
                .into_iter()
                .map(|account| DeviceAccountDto {
                    handle: account.handle,
                    name: account.name,
                    email: account.email,
                    image: account.image,
                    current: account.current,
                })
                .collect(),
        })
    }
}
