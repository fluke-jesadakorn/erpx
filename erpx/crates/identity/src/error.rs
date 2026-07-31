use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum IdentityError {
    #[error("identity reference is empty or invalid")]
    InvalidIdentityReference,
    #[error("password hashing failed")]
    PasswordHashingFailed,
    #[error("stored password digest is invalid")]
    InvalidPasswordDigest,
    #[error("password verification failed")]
    PasswordVerificationFailed,
    #[error("password hashing parameters are too weak or invalid")]
    InvalidPasswordParameters,
    #[error("random secret generation failed")]
    RandomnessUnavailable,
    #[error("access-token lifetime must be positive")]
    InvalidAccessTokenLifetime,
    #[error("access-token lifetime exceeds the five-minute maximum")]
    AccessTokenLifetimeExceeded,
    #[error("access token is malformed")]
    MalformedAccessToken,
    #[error("access token signature is invalid")]
    InvalidAccessTokenSignature,
    #[error("access token is expired")]
    AccessTokenExpired,
    #[error("access-token signing key must be at least 256 bits")]
    InvalidSigningKey,
    #[error("refresh token is invalid")]
    InvalidRefreshToken,
    #[error("refresh-token reuse was detected")]
    RefreshTokenReuseDetected,
    #[error("session is revoked or expired")]
    SessionInactive,
    #[error("recovery code count must be between 1 and 32")]
    InvalidRecoveryCodeCount,
    #[error("recovery code is invalid")]
    InvalidRecoveryCode,
    #[error("recovery code has already been consumed")]
    RecoveryCodeAlreadyConsumed,
    #[error("key reference is empty or invalid")]
    InvalidKeyReference,
    #[error("encryption key reference was not found: {0}")]
    KeyReferenceNotFound(String),
    #[error("TOTP secret encryption failed")]
    TotpEncryptionFailed,
    #[error("TOTP secret decryption failed")]
    TotpDecryptionFailed,
    #[error("TOTP code is invalid")]
    InvalidTotpCode,
    #[error("TOTP code has already been used")]
    TotpReplay,
    #[error("TOTP enrollment is invalid or expired")]
    TotpEnrollmentInvalid,
    #[error("TOTP enrollment lifetime is invalid")]
    InvalidTotpEnrollmentLifetime,
    #[error("recovery session is invalid or expired")]
    RecoverySessionInvalid,
    #[error("recovery session lifetime is invalid")]
    InvalidRecoverySessionLifetime,
    #[error("challenge lifetime must be positive")]
    InvalidChallengeLifetime,
    #[error("challenge lifetime exceeds the configured maximum")]
    ChallengeLifetimeExceeded,
    #[error("WebAuthn challenge is malformed")]
    MalformedChallenge,
    #[error("WebAuthn challenge has expired")]
    ChallengeExpired,
    #[error("WebAuthn challenge is not valid yet")]
    ChallengeNotYetValid,
    #[error("WebAuthn challenge has already been consumed")]
    ChallengeReplay,
    #[error("WebAuthn challenge does not match")]
    ChallengeMismatch,
    #[error("WebAuthn ceremony kind does not match the challenge")]
    CeremonyMismatch,
    #[error("WebAuthn provider is not configured")]
    WebAuthnProviderUnavailable,
    #[error("WebAuthn provider configuration is invalid")]
    WebAuthnConfigurationInvalid,
    #[error("WebAuthn RP ID is invalid")]
    WebAuthnRpIdInvalid,
    #[error("WebAuthn origin is invalid")]
    WebAuthnOriginInvalid,
    #[error("WebAuthn RP ID does not match the configured RP")]
    WebAuthnRpIdMismatch,
    #[error("WebAuthn origin is not allowed")]
    WebAuthnOriginNotAllowed,
    #[error("WebAuthn credential ID does not match")]
    WebAuthnCredentialIdMismatch,
    #[error("WebAuthn credential counter moved backwards")]
    CredentialCounterRollback,
    #[error("WebAuthn credential is invalid")]
    InvalidCredential,
    #[error("identity migration execution failed: {0}")]
    Migration(String),
}
