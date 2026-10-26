use super::account_auth::{AccountPolicy, AccountSession, HoldingOrigin, SESSION_LIFETIME_MS};
use erpx_server_shared::public::{BusinessError, FailureKind};
use url::Url;

pub const SSO_BROWSER_LIFETIME_MS: i64 = 300_000;
pub const SSO_GRANT_LIFETIME_MS: i64 = 60_000;
pub fn rejected_sso() -> BusinessError {
    BusinessError::new(
        FailureKind::Unauthenticated,
        "INVALID_SSO_TRANSACTION",
        "This sign-in link expired or could not be verified. Start again from your Holding.",
    )
}
fn wire_secret(value: &str) -> bool {
    value.len() == 43
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}
#[derive(Clone)]
pub struct SsoSecret(String);
impl SsoSecret {
    pub fn new(value: String) -> Result<Self, BusinessError> {
        if !wire_secret(&value) {
            return Err(rejected_sso());
        }
        Ok(Self(value))
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SsoDigest(String);
impl SsoDigest {
    pub fn new(value: String) -> Result<Self, BusinessError> {
        if !wire_secret(&value) {
            return Err(rejected_sso());
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SsoTransactionId(String);
impl SsoTransactionId {
    pub fn new(value: String) -> Result<Self, BusinessError> {
        if value.len() != 36
            || !value.bytes().enumerate().all(|(index, byte)| {
                if [8, 13, 18, 23].contains(&index) {
                    byte == b'-'
                } else {
                    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
                }
            })
        {
            return Err(rejected_sso());
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[derive(Clone)]
pub struct SsoBrowserCookie {
    pub transaction_id: SsoTransactionId,
    pub browser: SsoSecret,
    pub verifier: SsoSecret,
}
impl SsoBrowserCookie {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        if value.len() > 128 {
            return Err(rejected_sso());
        }
        let parts = value.split('.').collect::<Vec<_>>();
        if parts.len() != 3 {
            return Err(rejected_sso());
        }
        Ok(Self {
            transaction_id: SsoTransactionId::new(parts[0].into())?,
            browser: SsoSecret::new(parts[1].into())?,
            verifier: SsoSecret::new(parts[2].into())?,
        })
    }
    pub fn cookie_value(&self) -> String {
        format!(
            "{}.{}.{}",
            self.transaction_id.as_str(),
            self.browser.expose(),
            self.verifier.expose()
        )
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SsoReturnPath(String);
impl SsoReturnPath {
    /// The current JS URL policy: normalize path segments, drop fragments and strip callback credentials.
    pub fn sanitize(value: Option<&str>) -> Self {
        Self(sanitize_return(value).unwrap_or_else(|| "/".into()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn stored(value: String) -> Result<Self, BusinessError> {
        let clean = Self::sanitize(Some(&value));
        if clean.0 != value {
            return Err(rejected_sso());
        }
        Ok(clean)
    }
}
fn decoded(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut result = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let part = std::str::from_utf8(bytes.get(index + 1..index + 3)?).ok()?;
            result.push(u8::from_str_radix(part, 16).ok()?);
            index += 3;
        } else {
            result.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(result).ok()
}
fn forbidden_character(value: &str) -> bool {
    value
        .chars()
        .any(|character| character == '\\' || character.is_control())
}
fn sanitize_return(value: Option<&str>) -> Option<String> {
    let value = value?;
    if value.is_empty()
        || value.len() > 4096
        || !value.starts_with('/')
        || value.starts_with("//")
        || value
            .chars()
            .any(|character| character.is_whitespace() || character == '\u{feff}')
        || forbidden_character(value)
    {
        return None;
    }
    let plain = decoded(value)?;
    if plain.starts_with("//") || forbidden_character(&plain) {
        return None;
    }
    let mut url = Url::parse("https://sso-return.invalid")
        .ok()?
        .join(value)
        .ok()?;
    if url.origin().ascii_serialization() != "https://sso-return.invalid" {
        return None;
    }
    let path = decoded(url.path())?;
    if path == "/sso"
        || path.starts_with("/sso/")
        || path == "/api/auth"
        || path.starts_with("/api/auth/")
        || path == "/login"
        || path.starts_with("/login/")
        || path == "/product-entry"
        || path.starts_with("/product-entry/")
    {
        return None;
    }
    let pairs = url
        .query_pairs()
        .filter(|(key, _)| {
            !["code", "state", "token", "session_token", "_rsc"].contains(&key.as_ref())
        })
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    url.set_query(None);
    if !pairs.is_empty() {
        url.query_pairs_mut().extend_pairs(pairs);
    }
    Some(format!(
        "{}{}",
        url.path(),
        url.query()
            .filter(|query| !query.is_empty())
            .map(|query| format!("?{query}"))
            .unwrap_or_default()
    ))
}

#[derive(Clone)]
pub struct SsoPolicy {
    account: AccountPolicy,
}
impl SsoPolicy {
    pub fn new(account: AccountPolicy) -> Result<Self, BusinessError> {
        account.validate()?;
        Ok(Self { account })
    }
    pub fn account(&self) -> &AccountPolicy {
        &self.account
    }
    pub fn holding(&self, audience: &str) -> Result<&HoldingOrigin, BusinessError> {
        self.account
            .holdings
            .iter()
            .find(|holding| holding.audience == audience)
            .ok_or_else(rejected_sso)
    }
    pub fn callback(&self, audience: &str) -> Result<String, BusinessError> {
        Ok(format!("{}/sso/callback", self.holding(audience)?.origin))
    }
}
#[derive(Clone)]
pub struct BrowserSsoTransaction {
    pub id: SsoTransactionId,
    pub browser_hash: SsoDigest,
    pub state_hash: SsoDigest,
    pub challenge: SsoDigest,
    pub audience: String,
    pub environment: String,
    pub callback: String,
    pub return_path: SsoReturnPath,
    pub created_at: i64,
    pub expires_at: i64,
    pub consumed_at: Option<i64>,
}
impl BrowserSsoTransaction {
    pub fn require_current(&self, policy: &SsoPolicy, now: i64) -> Result<(), BusinessError> {
        if self.environment != policy.account.environment
            || self.callback != policy.callback(&self.audience)?
            || self.consumed_at.is_some()
            || self.created_at > now
            || self.expires_at <= now
            || !self
                .expires_at
                .checked_sub(self.created_at)
                .is_some_and(|duration| duration > 0 && duration <= SSO_BROWSER_LIFETIME_MS)
        {
            return Err(rejected_sso());
        }
        Ok(())
    }
}
#[derive(Clone)]
pub struct SsoAuthorizationGrant {
    pub code_hash: SsoDigest,
    pub transaction_id: SsoTransactionId,
    pub parent_session_id: String,
    pub audience: String,
    pub environment: String,
    pub callback: String,
    pub state_hash: SsoDigest,
    pub challenge: SsoDigest,
    pub created_at: i64,
    pub expires_at: i64,
    pub consumed_at: Option<i64>,
}
impl SsoAuthorizationGrant {
    pub fn require_current(
        &self,
        browser: &BrowserSsoTransaction,
        policy: &SsoPolicy,
        now: i64,
    ) -> Result<(), BusinessError> {
        browser.require_current(policy, now)?;
        if self.consumed_at.is_some()
            || self.created_at > now
            || self.expires_at <= now
            || self.expires_at > browser.expires_at
            || !self
                .expires_at
                .checked_sub(self.created_at)
                .is_some_and(|duration| duration > 0 && duration <= SSO_GRANT_LIFETIME_MS)
            || self.transaction_id != browser.id
            || self.audience != browser.audience
            || self.environment != browser.environment
            || self.callback != browser.callback
        {
            return Err(rejected_sso());
        }
        Ok(())
    }
}
pub fn grant_expiry(
    now: i64,
    browser: &BrowserSsoTransaction,
    parent: &AccountSession,
) -> Result<i64, BusinessError> {
    let expires = now
        .checked_add(SSO_GRANT_LIFETIME_MS)
        .ok_or_else(rejected_sso)?
        .min(browser.expires_at)
        .min(parent.expires_at)
        .min(
            parent
                .authenticated_at
                .checked_add(SESSION_LIFETIME_MS)
                .ok_or_else(rejected_sso)?,
        );
    if expires <= now {
        return Err(rejected_sso());
    }
    Ok(expires)
}
