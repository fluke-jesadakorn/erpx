use erpx_server_shared::public::{BusinessError, FailureKind};

pub struct SecurityFamily {
    pub handle: String,
    pub current: bool,
    pub created_at: i64,
    pub expires_at: i64,
    pub user_agent: Option<String>,
    pub audiences: Vec<String>,
}
pub struct SecuritySeed {
    pub account_key: String,
    pub revision: String,
    pub mfa_enabled: bool,
    pub mfa_fresh_until: Option<i64>,
    pub families: Vec<SecurityFamily>,
    pub legacy_sessions: usize,
}
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevokeSecurity {
    pub account_key: String,
    pub expected_revision: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    pub reason: String,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevokedSecurity {
    pub revoked: bool,
    pub count: usize,
    pub reauthenticate: bool,
}
pub fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ACCOUNT_SECURITY_CHANGED",
        "Your sign-ins or authenticator changed. Reload and review the current sessions.",
    )
}
pub fn opaque_handle(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl RevokeSecurity {
    pub fn validate(mut self) -> Result<Self, BusinessError> {
        self.reason=self.reason.trim_matches(|c:char|matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')).into();
        if !opaque_handle(&self.account_key)
            || !opaque_handle(&self.expected_revision)
            || !(3..=1000).contains(&self.reason.encode_utf16().count())
            || match self.scope.as_str() {
                "family" => !self.family.as_deref().is_some_and(opaque_handle),
                "all" => self.family.is_some(),
                _ => true,
            }
        {
            return Err(BusinessError::invalid(
                "INVALID_ACCOUNT_SECURITY",
                "Review your captured security request.",
            ));
        }
        Ok(self)
    }
}
