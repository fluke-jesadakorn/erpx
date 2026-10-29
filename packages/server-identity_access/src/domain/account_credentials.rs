use super::account_auth::{EmailAddress, SecretPurpose};
use erpx_server_shared::public::{BusinessError, FailureKind};

pub const CREDENTIAL_PROOF_LIFETIME_MS: i64 = 3_600_000;
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CredentialProofKind {
    EmailVerification,
    PasswordReset,
}
impl CredentialProofKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EmailVerification => "email_verification",
            Self::PasswordReset => "password_reset",
        }
    }
    pub fn purpose(self) -> SecretPurpose {
        match self {
            Self::EmailVerification => SecretPurpose::EmailVerification,
            Self::PasswordReset => SecretPurpose::PasswordReset,
        }
    }
}
pub fn invalid_proof() -> BusinessError {
    BusinessError::new(
        FailureKind::Unauthenticated,
        "ACCOUNT_PROOF_UNAVAILABLE",
        "This link expired, was replaced or was already used. Request a new link.",
    )
}
pub fn signup_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "INVITATION_REQUIRED",
        "Use a current approved invitation to create an Account.",
    )
}

/// A return stays within the exact Account origin, including after percent decoding.
pub fn account_return_path(value: Option<&str>) -> Result<String, BusinessError> {
    let value = value.unwrap_or("/");
    if value.len() > 4096
        || !value.starts_with('/')
        || value.starts_with("//")
        || value
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '\\')
    {
        return Err(BusinessError::invalid(
            "INVALID_RETURN_PATH",
            "Choose a relative Account return path.",
        ));
    }
    let mut decoded = Vec::new();
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(BusinessError::invalid(
                    "INVALID_RETURN_PATH",
                    "Choose a valid Account return path.",
                ));
            }
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3])
                .ok()
                .and_then(|part| u8::from_str_radix(part, 16).ok())
                .ok_or_else(|| {
                    BusinessError::invalid(
                        "INVALID_RETURN_PATH",
                        "Choose a valid Account return path.",
                    )
                })?;
            decoded.push(hex);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    let decoded = String::from_utf8(decoded).map_err(|_| {
        BusinessError::invalid("INVALID_RETURN_PATH", "Choose a valid Account return path.")
    })?;
    if decoded.starts_with("//") || decoded.chars().any(|c| c.is_control() || c == '\\') {
        return Err(BusinessError::invalid(
            "INVALID_RETURN_PATH",
            "Choose a relative Account return path.",
        ));
    }
    Ok(value.into())
}

#[derive(Clone)]
pub struct InvitationCredentialFacts {
    pub invitation_id: String,
    pub holding_id: String,
    pub company_id: String,
    pub person_id: String,
    pub email: EmailAddress,
    pub expected_email: EmailAddress,
    pub now: i64,
    pub expires_at: i64,
    pub pending: bool,
    pub independently_reviewed: bool,
    pub published_role_current: bool,
    pub person_active: bool,
    pub person_unlinked: bool,
    pub authorizer_current: bool,
    pub authorizer_can_manage: bool,
    pub scope_within_parent: bool,
    pub grant_expires_at: i64,
    pub directory_only: bool,
    pub existing_user: bool,
}
/// Pure policy; facts must come from retained owner sources, never a browser DTO.
pub fn validate_invitation_credential(
    facts: &InvitationCredentialFacts,
) -> Result<(), BusinessError> {
    if facts.invitation_id.is_empty()
        || facts.holding_id.is_empty()
        || facts.company_id.is_empty()
        || facts.person_id.is_empty()
        || !facts.pending
        || facts.expires_at <= facts.now
        || facts.grant_expires_at <= facts.now
        || facts.email != facts.expected_email
        || !facts.independently_reviewed
        || !facts.published_role_current
        || !facts.person_active
        || !facts.person_unlinked
        || !facts.authorizer_current
        || !facts.authorizer_can_manage
        || !facts.scope_within_parent
        || facts.directory_only
        || facts.existing_user
    {
        return Err(signup_unavailable());
    }
    Ok(())
}
