pub mod account_contracts;
pub mod account_mail;
pub mod account_sessions;
pub mod account_sso;
pub use crate::adapters::account_crypto::{
    Argon2Passwords, SystemAccountClock, SystemAuthenticationSecrets,
};
pub use crate::adapters::account_http::account_auth_router;
#[cfg(feature = "contracts")]
pub use crate::adapters::account_http::account_openapi;
pub use crate::adapters::account_postgres::PostgresAccountRepository;
pub use crate::application::account_auth::{
    AccountAuthentication, InitializedDevice, PresentedSecrets, SelectedAccount, SignedIn,
};
pub use crate::application::account_ports::{
    AccountFuture, AccountRepository, AuthenticationSecrets, CompleteSignIn, CompletedSignIn,
    DeviceProof, DeviceSelection, PasswordHashing, ReserveSignIn, SelectedDevice,
    SignInReservation,
};
pub use crate::domain::account_auth::{
    AccountPolicy, AccountSession, AccountUser, CredentialSnapshot, DeviceAccount, DeviceAccounts,
    EmailAddress, HoldingOrigin, MAXIMUM_SAVED_ACCOUNTS, OpaqueSecret, Password, PasswordHash,
    RequestAudience, SESSION_LIFETIME_MS, SecretDigest, SecretPurpose, valid_session_window,
};

pub mod work_context;
pub use crate::adapters::account_credentials_http::account_credentials_router;
#[cfg(feature = "contracts")]
pub use crate::adapters::account_credentials_http::credentials_openapi;
pub use crate::adapters::account_credentials_postgres::PostgresAccountCredentials;
pub use crate::adapters::account_invitations_http::account_invitations_router;
#[cfg(feature = "contracts")]
pub use crate::adapters::account_invitations_http::invitation_openapi;
pub use crate::adapters::account_invitations_postgres::PostgresAccountInvitations;
pub use crate::adapters::account_mail_crypto::{AccountMailKeyring, OpenedAccountMail};
pub use crate::adapters::postgres_invitation_authority::PostgresInvitationCredentialAuthority;
pub use crate::application::account_credentials::{
    AccountCredentialRepository, AccountCredentials, AccountMailCryptography, ChangeCredential,
    InvitationCredentialAuthority, InvitationCredentialLease, InvitationCredentialQuery,
    PreparedCredentialMail, ResetCredential, SignUpCredential,
    UnavailableInvitationCredentialAuthority, VerifiedCredential, VerifyCredential,
};
pub use crate::application::account_invitations::{
    AccountInvitationRepository, AccountInvitations, InvitationAccountState,
};
pub use crate::domain::account_credentials::{
    CREDENTIAL_PROOF_LIFETIME_MS, CredentialProofKind, InvitationCredentialFacts,
    account_return_path, validate_invitation_credential,
};
pub use crate::domain::invitation_authority::{StaffInvitationGrant, invitation_scope_covers};

pub mod permission_catalog;

pub mod work_admission;

pub mod effective_authority;
pub mod release_reviewers;

pub mod account_revocation;

pub mod command_metadata;

pub mod account_device_http;
pub mod account_security;
pub mod account_security_http;
