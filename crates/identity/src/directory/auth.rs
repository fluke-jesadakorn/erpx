//! Fail-closed authentication policy for generic LDAP, on-premises AD DS,
//! and Univention Corporate Server (UCS) connectors.
//!
//! The concrete protocol adapters are supplied by the server/worker
//! integration layer.  This module owns the security boundary around those
//! adapters: only LDAPS/StartTLS policies are accepted, bind operations are
//! bounded, bind results must include certificate-validation evidence, and an
//! external identity link is derived from an immutable connector-scoped key.
//! There is no implicit local-authentication fallback.

use std::{
    fmt,
    future::Future,
    net::SocketAddr,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
    thread,
    time::{Duration, Instant},
};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use zeroize::Zeroizing;

use super::tls::{
    DirectoryTransportMode, TimeoutPolicy, TlsHandshakeEvidence, TlsPolicy, TlsPolicyError,
};

pub const MAX_DIRECTORY_USERNAME_LENGTH: usize = 512;
pub const MAX_BREAK_GLASS_REASON_LENGTH: usize = 256;
pub const MAX_MFA_REFERENCE_LENGTH: usize = 256;
pub const MAX_STEP_UP_LIFETIME: ChronoDuration = ChronoDuration::minutes(5);
pub const MAX_BREAK_GLASS_LIFETIME: ChronoDuration = ChronoDuration::minutes(15);

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryAuthError {
    #[error("directory connector configuration was rejected")]
    InvalidConfiguration,
    #[error("directory communication requires the configured TLS policy")]
    TlsPolicyRejected,
    #[error("directory certificate validation failed")]
    TlsCertificateValidationFailed,
    #[error("directory authentication timed out during {0:?}")]
    Timeout(DirectoryTimeoutPhase),
    #[error("directory service is unavailable")]
    TransportUnavailable,
    #[error("directory adapter has not passed its external readiness preflight")]
    ExternalCertificationRequired,
    #[error("directory bind was rejected")]
    BindRejected,
    #[error("directory account is disabled")]
    AccountDisabled,
    #[error("directory account is deleted")]
    AccountDeleted,
    #[error("directory account is locked")]
    AccountLocked,
    #[error("directory returned an invalid authentication response")]
    ProtocolViolation,
    #[error("high-risk access requires an MFA step-up")]
    HighRiskMfaRequired,
    #[error("MFA step-up was rejected")]
    MfaStepUpRejected,
    #[error("MFA step-up receipt is expired or not yet valid")]
    MfaStepUpExpired,
    #[error("the external identity link is invalid")]
    InvalidExternalIdentityLink,
    #[error("the external identity link cannot be changed")]
    ExternalIdentityLinkMutation,
    #[error("local break-glass authentication is disabled")]
    BreakGlassDisabled,
    #[error("local break-glass authentication was rejected")]
    BreakGlassRejected,
    #[error("the break-glass grant does not match the requested principal")]
    BreakGlassSubjectMismatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectoryTimeoutPhase {
    Connect,
    StartTls,
    Bind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectoryProvider {
    GenericLdap,
    ActiveDirectoryDomainServices,
    UniventionCorporateServer,
}

impl DirectoryProvider {
    pub const fn segment(self) -> &'static str {
        match self {
            Self::GenericLdap => "ldap",
            Self::ActiveDirectoryDomainServices => "ad-ds",
            Self::UniventionCorporateServer => "ucs",
        }
    }

    pub const fn native_change_feed(self) -> NativeChangeFeed {
        match self {
            Self::GenericLdap => NativeChangeFeed::Rfc4533,
            Self::ActiveDirectoryDomainServices => NativeChangeFeed::AdDirSync,
            Self::UniventionCorporateServer => NativeChangeFeed::UcsProvisioningService,
        }
    }

    pub const fn immutable_identity_attribute(self) -> ImmutableIdentityAttribute {
        match self {
            Self::GenericLdap => ImmutableIdentityAttribute::LdapEntryUuid,
            Self::ActiveDirectoryDomainServices => ImmutableIdentityAttribute::AdObjectGuid,
            Self::UniventionCorporateServer => ImmutableIdentityAttribute::UcsEntryUuid,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeChangeFeed {
    Rfc4533,
    AdDirSync,
    UcsProvisioningService,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorScope {
    Platform,
    Holding(Uuid),
    Branch {
        holding_id: Uuid,
        company_id: Uuid,
        branch_id: Uuid,
    },
}

impl ConnectorScope {
    const fn is_invalid(self) -> bool {
        match self {
            Self::Platform => false,
            Self::Holding(holding_id) => holding_id.is_nil(),
            Self::Branch {
                holding_id,
                company_id,
                branch_id,
            } => holding_id.is_nil() || company_id.is_nil() || branch_id.is_nil(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BreakGlassPolicy {
    Disabled,
    LocalOnly,
}

/// The only provider-specific value permitted to establish an external
/// identity link.  Login names, DNs, UPNs, email addresses, and display names
/// are mutable attributes and deliberately do not appear in this enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImmutableIdentityAttribute {
    LdapEntryUuid,
    AdObjectGuid,
    UcsEntryUuid,
}

/// The structural authentication gate applied before a connector can receive
/// a bind request.  This report is intentionally explicit about the security
/// properties that are otherwise easy for a future adapter to omit.  It does
/// not claim live vendor certification; credentialed LDAP, AD DS, and UCS
/// acceptance remains an external gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectoryAuthenticationPreflight {
    connector_ref: Uuid,
    scope: ConnectorScope,
    provider: DirectoryProvider,
    transport_mode: DirectoryTransportMode,
    native_change_feed: NativeChangeFeed,
    immutable_identity_attribute: ImmutableIdentityAttribute,
    high_risk_mfa_required: bool,
    break_glass: BreakGlassPolicy,
    automatic_local_fallback: bool,
    external_certification_required: bool,
}

impl DirectoryAuthenticationPreflight {
    pub const fn connector_ref(self) -> Uuid {
        self.connector_ref
    }

    pub const fn scope(self) -> ConnectorScope {
        self.scope
    }

    pub const fn provider(self) -> DirectoryProvider {
        self.provider
    }

    pub const fn transport_mode(self) -> DirectoryTransportMode {
        self.transport_mode
    }

    pub const fn native_change_feed(self) -> NativeChangeFeed {
        self.native_change_feed
    }

    pub const fn immutable_identity_attribute(self) -> ImmutableIdentityAttribute {
        self.immutable_identity_attribute
    }

    pub const fn high_risk_mfa_required(self) -> bool {
        self.high_risk_mfa_required
    }

    pub const fn break_glass(self) -> BreakGlassPolicy {
        self.break_glass
    }

    pub const fn automatic_local_fallback_disabled(self) -> bool {
        !self.automatic_local_fallback
    }

    pub const fn external_certification_required(self) -> bool {
        self.external_certification_required
    }
}

/// Connector configuration is validated before a transport can receive a
/// bind request.  A platform connector and a holding connector use the same
/// policy; only the persisted scope differs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectoryConnectorConfig {
    connector_ref: Uuid,
    scope: ConnectorScope,
    provider: DirectoryProvider,
    endpoint: String,
    tls: TlsPolicy,
    timeouts: TimeoutPolicy,
    native_change_feed: NativeChangeFeed,
    break_glass: BreakGlassPolicy,
}

impl DirectoryConnectorConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        connector_ref: Uuid,
        scope: ConnectorScope,
        provider: DirectoryProvider,
        endpoint: impl Into<String>,
        tls: TlsPolicy,
        timeouts: TimeoutPolicy,
        native_change_feed: NativeChangeFeed,
        break_glass: BreakGlassPolicy,
    ) -> Result<Self, DirectoryAuthError> {
        let endpoint = endpoint.into();
        if connector_ref.is_nil() || scope.is_invalid() {
            return Err(DirectoryAuthError::InvalidConfiguration);
        }
        tls.validate_endpoint(&endpoint)
            .map_err(|_| DirectoryAuthError::InvalidConfiguration)?;
        if !transport_matches_provider(provider, tls.mode()) {
            return Err(DirectoryAuthError::InvalidConfiguration);
        }
        if provider.native_change_feed() != native_change_feed {
            return Err(DirectoryAuthError::InvalidConfiguration);
        }
        Ok(Self {
            connector_ref,
            scope,
            provider,
            endpoint,
            tls,
            timeouts,
            native_change_feed,
            break_glass,
        })
    }

    /// Performs the fail-closed structural gate shared by all directory
    /// providers.  A connector must pass this gate before any transport is
    /// allowed to receive credentials.  The returned report is policy
    /// evidence only; it intentionally does not turn configuration presence
    /// into live LDAP, AD DS, or UCS certification.
    pub fn preflight(&self) -> Result<DirectoryAuthenticationPreflight, DirectoryAuthError> {
        if self.connector_ref.is_nil() || self.scope.is_invalid() {
            return Err(DirectoryAuthError::InvalidConfiguration);
        }
        self.tls
            .validate_endpoint(&self.endpoint)
            .map_err(|_| DirectoryAuthError::InvalidConfiguration)?;
        if self.provider.native_change_feed() != self.native_change_feed {
            return Err(DirectoryAuthError::InvalidConfiguration);
        }
        // These are compile-time closed enums, but keep the check at the
        // ownership boundary so deserialization or a future representation
        // change cannot make a plaintext or implicit local fallback valid.
        if !transport_matches_provider(self.provider, self.tls.mode())
            || self.automatic_local_fallback_enabled()
        {
            return Err(DirectoryAuthError::InvalidConfiguration);
        }
        Ok(DirectoryAuthenticationPreflight {
            connector_ref: self.connector_ref,
            scope: self.scope,
            provider: self.provider,
            transport_mode: self.tls.mode(),
            native_change_feed: self.native_change_feed,
            immutable_identity_attribute: self.provider.immutable_identity_attribute(),
            high_risk_mfa_required: true,
            break_glass: self.break_glass,
            automatic_local_fallback: false,
            external_certification_required: true,
        })
    }

    const fn automatic_local_fallback_enabled(&self) -> bool {
        false
    }

    pub const fn connector_ref(&self) -> Uuid {
        self.connector_ref
    }

    pub const fn scope(&self) -> ConnectorScope {
        self.scope
    }

    pub const fn provider(&self) -> DirectoryProvider {
        self.provider
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn tls(&self) -> &TlsPolicy {
        &self.tls
    }

    pub const fn timeouts(&self) -> TimeoutPolicy {
        self.timeouts
    }

    pub const fn native_change_feed(&self) -> NativeChangeFeed {
        self.native_change_feed
    }

    pub const fn break_glass(&self) -> BreakGlassPolicy {
        self.break_glass
    }
}

/// Usernames may be DN, UPN, or provider-specific login names.  Anonymous
/// binds are not representable because the credential is always non-empty.
#[derive(Clone, PartialEq, Eq)]
pub struct DirectoryUsername(String);

impl DirectoryUsername {
    pub fn try_new(value: impl Into<String>) -> Result<Self, DirectoryAuthError> {
        let value = value.into();
        if value.trim().is_empty()
            || value.len() > MAX_DIRECTORY_USERNAME_LENGTH
            || value.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err(DirectoryAuthError::BindRejected);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for DirectoryUsername {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DirectoryUsername(REDACTED)")
    }
}

/// Passwords and bind secrets are zeroized on drop and never participate in
/// `Debug` or `Display` output.  Adapters receive a short-lived borrow only.
#[derive(Clone, PartialEq, Eq)]
pub struct RedactedSecret(Zeroizing<String>);

impl RedactedSecret {
    pub fn try_new(value: impl Into<String>) -> Result<Self, DirectoryAuthError> {
        let value = value.into();
        if value.is_empty() || value.len() > 4096 || value.bytes().any(|byte| byte == 0) {
            return Err(DirectoryAuthError::BindRejected);
        }
        Ok(Self(Zeroizing::new(value)))
    }

    /// Concrete adapters use this only while constructing the underlying bind
    /// packet.  It must never be copied into logs, SQLx payloads, or outbox
    /// events.
    pub fn expose_to_transport(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for RedactedSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RedactedSecret(REDACTED)")
    }
}

impl fmt::Display for RedactedSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("REDACTED")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalIdentityKey {
    GenericLdapEntryUuid(Uuid),
    ActiveDirectoryObjectGuid(Uuid),
    UcsEntryUuid(Uuid),
}

impl ExternalIdentityKey {
    pub const fn provider(self) -> DirectoryProvider {
        match self {
            Self::GenericLdapEntryUuid(_) => DirectoryProvider::GenericLdap,
            Self::ActiveDirectoryObjectGuid(_) => DirectoryProvider::ActiveDirectoryDomainServices,
            Self::UcsEntryUuid(_) => DirectoryProvider::UniventionCorporateServer,
        }
    }

    pub const fn value(self) -> Uuid {
        match self {
            Self::GenericLdapEntryUuid(value)
            | Self::ActiveDirectoryObjectGuid(value)
            | Self::UcsEntryUuid(value) => value,
        }
    }

    pub fn try_for_provider(
        provider: DirectoryProvider,
        value: Uuid,
    ) -> Result<Self, DirectoryAuthError> {
        if value.is_nil() {
            return Err(DirectoryAuthError::InvalidExternalIdentityLink);
        }
        Ok(match provider {
            DirectoryProvider::GenericLdap => Self::GenericLdapEntryUuid(value),
            DirectoryProvider::ActiveDirectoryDomainServices => {
                Self::ActiveDirectoryObjectGuid(value)
            }
            DirectoryProvider::UniventionCorporateServer => Self::UcsEntryUuid(value),
        })
    }
}

/// The stable link is derived from the connector and the provider's immutable
/// directory object key.  It intentionally contains no username, display
/// name, email, or mutable DN.
#[derive(Clone, PartialEq, Eq, Hash, Serialize)]
pub struct ExternalIdentityLinkRef(String);

impl ExternalIdentityLinkRef {
    pub fn derive(
        connector_ref: Uuid,
        provider: DirectoryProvider,
        key: ExternalIdentityKey,
    ) -> Result<Self, DirectoryAuthError> {
        if connector_ref.is_nil() || key.value().is_nil() || key.provider() != provider {
            return Err(DirectoryAuthError::InvalidExternalIdentityLink);
        }
        Ok(Self(format!(
            "erpx:directory-link:{}:{}:{}",
            connector_ref.hyphenated(),
            provider.segment(),
            key.value().hyphenated()
        )))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Persistence must treat the link as an append-only identity key.  A
    /// changed link is a new identity, never an update to this one.
    pub fn ensure_unchanged(&self, candidate: &Self) -> Result<(), DirectoryAuthError> {
        if self == candidate {
            Ok(())
        } else {
            Err(DirectoryAuthError::ExternalIdentityLinkMutation)
        }
    }
}

impl fmt::Debug for ExternalIdentityLinkRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ExternalIdentityLinkRef")
            .field(&self.0)
            .finish()
    }
}

/// A bind result is intentionally narrow.  The concrete adapter must map
/// vendor-specific responses into this typed result before the identity layer
/// can issue a principal.  Mutable attributes are not a linking key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectoryIdentity {
    key: ExternalIdentityKey,
    account_status: DirectoryAccountStatus,
}

impl DirectoryIdentity {
    pub fn new(key: ExternalIdentityKey, account_status: DirectoryAccountStatus) -> Self {
        Self {
            key,
            account_status,
        }
    }

    pub const fn key(&self) -> ExternalIdentityKey {
        self.key
    }

    pub const fn account_status(&self) -> DirectoryAccountStatus {
        self.account_status
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectoryAccountStatus {
    Enabled,
    Disabled,
    Deleted,
    Locked,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectoryBindResult {
    /// The connector whose authenticated transport produced this result.
    /// The authenticator checks this before deriving an identity link so a
    /// valid response from another connector cannot be relinked locally.
    connector_ref: Uuid,
    tls_evidence: TlsHandshakeEvidence,
    identity: DirectoryIdentity,
}

impl DirectoryBindResult {
    pub fn new(
        connector_ref: Uuid,
        tls_evidence: TlsHandshakeEvidence,
        identity: DirectoryIdentity,
    ) -> Self {
        Self {
            connector_ref,
            tls_evidence,
            identity,
        }
    }

    pub const fn connector_ref(&self) -> Uuid {
        self.connector_ref
    }

    pub fn tls_evidence(&self) -> &TlsHandshakeEvidence {
        &self.tls_evidence
    }

    pub const fn identity(&self) -> &DirectoryIdentity {
        &self.identity
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryTransportError {
    #[error("directory TLS handshake was rejected")]
    TlsHandshakeRejected,
    #[error("directory certificate validation failed")]
    CertificateValidationFailed,
    #[error("directory service is unavailable")]
    Unavailable,
    #[error("directory bind was rejected")]
    Rejected,
    #[error("directory account is disabled")]
    Disabled,
    #[error("directory account is deleted")]
    Deleted,
    #[error("directory account is locked")]
    Locked,
    #[error("directory response violated the adapter contract")]
    ProtocolViolation,
}

pub type DirectoryBindFuture<'a> =
    Pin<Box<dyn Future<Output = Result<DirectoryBindResult, DirectoryTransportError>> + Send + 'a>>;

/// An adapter must pass its external readiness preflight before receiving a
/// bind request.  It must then perform LDAPS or StartTLS using certificate
/// validation and return a handshake evidence record with the typed directory
/// identity.  It must not perform anonymous binds or retry on plaintext.
pub trait DirectoryTransport: Send + Sync {
    /// The concrete adapter owns the credentialed environment check.  A
    /// missing endpoint, credential, CA, or vendor-specific certification
    /// must return `ExternalCertificationRequired`; the authenticator will not
    /// call `bind` after a failed preflight.
    fn preflight(&self, config: &DirectoryConnectorConfig) -> Result<(), DirectoryAuthError>;

    fn bind<'a>(&'a self, request: DirectoryBindRequest<'a>) -> DirectoryBindFuture<'a>;
}

pub struct DirectoryBindRequest<'a> {
    connector_ref: Uuid,
    endpoint: &'a str,
    tls: &'a TlsPolicy,
    timeouts: TimeoutPolicy,
    username: &'a DirectoryUsername,
    credential: &'a RedactedSecret,
}

impl<'a> DirectoryBindRequest<'a> {
    pub const fn connector_ref(&self) -> Uuid {
        self.connector_ref
    }

    pub fn endpoint(&self) -> &str {
        self.endpoint
    }

    pub const fn tls(&self) -> &'a TlsPolicy {
        self.tls
    }

    pub const fn timeouts(&self) -> TimeoutPolicy {
        self.timeouts
    }

    pub fn username(&self) -> &DirectoryUsername {
        self.username
    }

    pub fn credential(&self) -> &RedactedSecret {
        self.credential
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessRisk {
    Standard,
    HighRisk { operation_ref: Uuid },
}

impl AccessRisk {
    pub const fn operation_ref(self) -> Option<Uuid> {
        match self {
            Self::Standard => None,
            Self::HighRisk { operation_ref } => Some(operation_ref),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct MfaStepUpProof {
    challenge_ref: String,
    assertion_ref: String,
}

impl MfaStepUpProof {
    pub fn try_new(
        challenge_ref: impl Into<String>,
        assertion_ref: impl Into<String>,
    ) -> Result<Self, DirectoryAuthError> {
        let challenge_ref = challenge_ref.into();
        let assertion_ref = assertion_ref.into();
        if !valid_reference(&challenge_ref) || !valid_reference(&assertion_ref) {
            return Err(DirectoryAuthError::MfaStepUpRejected);
        }
        Ok(Self {
            challenge_ref,
            assertion_ref,
        })
    }

    pub fn challenge_ref(&self) -> &str {
        &self.challenge_ref
    }

    pub fn assertion_ref(&self) -> &str {
        &self.assertion_ref
    }
}

impl fmt::Debug for MfaStepUpProof {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Challenge and assertion references are bearer-adjacent MFA
        // material.  They must never be emitted through a request's derived
        // Debug implementation or an error context.
        formatter.write_str("MfaStepUpProof(REDACTED)")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MfaAssurance {
    Totp,
    WebAuthn,
    HardwareKey,
    FederatedMfa,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MfaSubject {
    Directory(ExternalIdentityLinkRef),
    LocalPrincipal(Uuid),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepUpReceipt {
    subject: MfaSubject,
    operation_ref: Uuid,
    assurance: MfaAssurance,
    verified_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

impl StepUpReceipt {
    pub fn try_new(
        subject: MfaSubject,
        operation_ref: Uuid,
        assurance: MfaAssurance,
        verified_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<Self, DirectoryAuthError> {
        if operation_ref.is_nil()
            || matches!(&subject, MfaSubject::LocalPrincipal(principal_ref) if principal_ref.is_nil())
            || expires_at <= verified_at
            || expires_at - verified_at > MAX_STEP_UP_LIFETIME
        {
            return Err(DirectoryAuthError::MfaStepUpRejected);
        }
        Ok(Self {
            subject,
            operation_ref,
            assurance,
            verified_at,
            expires_at,
        })
    }

    pub const fn subject(&self) -> &MfaSubject {
        &self.subject
    }

    pub const fn operation_ref(&self) -> Uuid {
        self.operation_ref
    }

    pub const fn assurance(&self) -> MfaAssurance {
        self.assurance
    }

    pub const fn verified_at(&self) -> DateTime<Utc> {
        self.verified_at
    }

    pub const fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }

    fn validate_at(&self, now: DateTime<Utc>) -> Result<(), DirectoryAuthError> {
        if self.operation_ref.is_nil()
            || matches!(&self.subject, MfaSubject::LocalPrincipal(principal_ref) if principal_ref.is_nil())
            || self.expires_at <= self.verified_at
            || self.expires_at - self.verified_at > MAX_STEP_UP_LIFETIME
        {
            return Err(DirectoryAuthError::MfaStepUpRejected);
        }
        if now < self.verified_at || now >= self.expires_at {
            return Err(DirectoryAuthError::MfaStepUpExpired);
        }
        Ok(())
    }
}

pub struct StepUpRequest<'a> {
    subject: &'a MfaSubject,
    operation_ref: Uuid,
    proof: &'a MfaStepUpProof,
    now: DateTime<Utc>,
}

impl<'a> StepUpRequest<'a> {
    pub const fn subject(&self) -> &'a MfaSubject {
        self.subject
    }

    pub const fn operation_ref(&self) -> Uuid {
        self.operation_ref
    }

    pub fn proof(&self) -> &'a MfaStepUpProof {
        self.proof
    }

    pub const fn now(&self) -> DateTime<Utc> {
        self.now
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum MfaStepUpError {
    #[error("MFA proof was rejected")]
    Rejected,
    #[error("MFA proof is expired")]
    Expired,
    #[error("MFA provider is unavailable")]
    Unavailable,
}

/// The MFA service owns the actual TOTP/WebAuthn/device verification.  It
/// receives an identity-bound, operation-bound proof request and must return a
/// short-lived receipt.  The caller still rechecks the receipt before issuing
/// an authenticated result.
pub trait MfaStepUpVerifier: Send + Sync {
    fn verify(&self, request: &StepUpRequest<'_>) -> Result<StepUpReceipt, MfaStepUpError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectoryAuthenticationRequest {
    username: DirectoryUsername,
    credential: RedactedSecret,
    risk: AccessRisk,
    step_up: Option<MfaStepUpProof>,
}

impl DirectoryAuthenticationRequest {
    pub fn new(
        username: DirectoryUsername,
        credential: RedactedSecret,
        risk: AccessRisk,
        step_up: Option<MfaStepUpProof>,
    ) -> Self {
        Self {
            username,
            credential,
            risk,
            step_up,
        }
    }

    pub fn username(&self) -> &DirectoryUsername {
        &self.username
    }

    pub const fn risk(&self) -> AccessRisk {
        self.risk
    }

    pub const fn step_up(&self) -> Option<&MfaStepUpProof> {
        self.step_up.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthenticatedDirectoryIdentity {
    connector_ref: Uuid,
    scope: ConnectorScope,
    provider: DirectoryProvider,
    link: ExternalIdentityLinkRef,
    immutable_value: Uuid,
    tls_evidence: TlsHandshakeEvidence,
    mfa: Option<StepUpReceipt>,
    authenticated_at: DateTime<Utc>,
}

impl AuthenticatedDirectoryIdentity {
    pub const fn connector_ref(&self) -> Uuid {
        self.connector_ref
    }

    pub const fn scope(&self) -> ConnectorScope {
        self.scope
    }

    pub const fn provider(&self) -> DirectoryProvider {
        self.provider
    }

    pub fn link(&self) -> &ExternalIdentityLinkRef {
        &self.link
    }

    /// The immutable provider object value used by the directory write model
    /// to resolve the imported identity.  Mutable login attributes are never
    /// exposed as the linking key.
    pub const fn immutable_value(&self) -> Uuid {
        self.immutable_value
    }

    pub fn tls_evidence(&self) -> &TlsHandshakeEvidence {
        &self.tls_evidence
    }

    pub const fn mfa(&self) -> Option<&StepUpReceipt> {
        self.mfa.as_ref()
    }

    pub const fn authenticated_at(&self) -> DateTime<Utc> {
        self.authenticated_at
    }

    /// Returns the credential-free record that a persistence/outbox layer may
    /// store.  The username, bind secret, MFA challenge, and MFA assertion are
    /// intentionally absent; the immutable link and validated TLS/MFA facts
    /// are the complete authentication evidence.
    pub fn persistence_evidence(&self) -> DirectoryAuthenticationEvidence {
        DirectoryAuthenticationEvidence::Directory {
            connector_ref: self.connector_ref,
            scope: self.scope,
            provider: self.provider,
            identity_link: self.link.clone(),
            tls: self.tls_evidence.clone(),
            mfa: self.mfa.as_ref().map(MfaAuthenticationEvidence::from),
            authenticated_at: self.authenticated_at,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MfaAuthenticationEvidence {
    operation_ref: Uuid,
    assurance: MfaAssurance,
    verified_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

impl From<&StepUpReceipt> for MfaAuthenticationEvidence {
    fn from(receipt: &StepUpReceipt) -> Self {
        Self {
            operation_ref: receipt.operation_ref,
            assurance: receipt.assurance,
            verified_at: receipt.verified_at,
            expires_at: receipt.expires_at,
        }
    }
}

impl MfaAuthenticationEvidence {
    pub const fn operation_ref(&self) -> Uuid {
        self.operation_ref
    }

    pub const fn assurance(&self) -> MfaAssurance {
        self.assurance
    }

    pub const fn verified_at(&self) -> DateTime<Utc> {
        self.verified_at
    }

    pub const fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "authentication_kind", rename_all = "snake_case")]
pub enum DirectoryAuthenticationEvidence {
    Directory {
        connector_ref: Uuid,
        scope: ConnectorScope,
        provider: DirectoryProvider,
        identity_link: ExternalIdentityLinkRef,
        tls: TlsHandshakeEvidence,
        mfa: Option<MfaAuthenticationEvidence>,
        authenticated_at: DateTime<Utc>,
    },
    LocalBreakGlass {
        principal_ref: Uuid,
        session_ref: Uuid,
        incident_ref: String,
        issued_at: DateTime<Utc>,
        authenticated_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
        mfa: Option<MfaAuthenticationEvidence>,
    },
}

/// The verifier is intentionally separate from directory authentication.  A
/// directory outage must never cause this path to be tried automatically.
pub trait LocalBreakGlassVerifier: Send + Sync {
    fn verify(
        &self,
        request: &LocalBreakGlassRequest<'_>,
    ) -> Result<LocalBreakGlassGrant, LocalBreakGlassError>;
}

pub struct LocalBreakGlassRequest<'a> {
    origin: LocalBreakGlassOrigin,
    principal_ref: Uuid,
    credential: &'a RedactedSecret,
    incident_ref: &'a str,
    risk: AccessRisk,
    step_up: Option<&'a MfaStepUpProof>,
    requested_at: DateTime<Utc>,
}

impl<'a> LocalBreakGlassRequest<'a> {
    /// Constructs the only request type accepted by the local break-glass
    /// path.  The caller is the trusted local server boundary and must invoke
    /// this constructor only after verifying the request arrived from the
    /// local host; there is deliberately no remote constructor or fallback.
    pub fn try_new_local(
        origin: LocalBreakGlassOrigin,
        principal_ref: Uuid,
        credential: &'a RedactedSecret,
        incident_ref: &'a str,
        risk: AccessRisk,
        step_up: Option<&'a MfaStepUpProof>,
        requested_at: DateTime<Utc>,
    ) -> Result<Self, DirectoryAuthError> {
        if principal_ref.is_nil()
            || incident_ref.trim().is_empty()
            || incident_ref.len() > MAX_BREAK_GLASS_REASON_LENGTH
            || incident_ref.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err(DirectoryAuthError::BreakGlassRejected);
        }
        Ok(Self {
            origin,
            principal_ref,
            credential,
            incident_ref,
            risk,
            step_up,
            requested_at,
        })
    }

    pub const fn origin(&self) -> LocalBreakGlassOrigin {
        self.origin
    }

    pub const fn principal_ref(&self) -> Uuid {
        self.principal_ref
    }

    pub fn credential(&self) -> &RedactedSecret {
        self.credential
    }

    pub fn incident_ref(&self) -> &str {
        self.incident_ref
    }

    pub const fn risk(&self) -> AccessRisk {
        self.risk
    }

    pub const fn step_up(&self) -> Option<&'a MfaStepUpProof> {
        self.step_up
    }

    pub const fn requested_at(&self) -> DateTime<Utc> {
        self.requested_at
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalBreakGlassGrant {
    principal_ref: Uuid,
    session_ref: Uuid,
    issued_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

impl LocalBreakGlassGrant {
    pub fn try_new(
        principal_ref: Uuid,
        session_ref: Uuid,
        expires_at: DateTime<Utc>,
        issued_at: DateTime<Utc>,
    ) -> Result<Self, DirectoryAuthError> {
        if principal_ref.is_nil()
            || session_ref.is_nil()
            || expires_at <= issued_at
            || expires_at - issued_at > MAX_BREAK_GLASS_LIFETIME
        {
            return Err(DirectoryAuthError::BreakGlassRejected);
        }
        Ok(Self {
            principal_ref,
            session_ref,
            issued_at,
            expires_at,
        })
    }

    pub const fn principal_ref(&self) -> Uuid {
        self.principal_ref
    }

    pub const fn session_ref(&self) -> Uuid {
        self.session_ref
    }

    pub const fn issued_at(&self) -> DateTime<Utc> {
        self.issued_at
    }

    pub const fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum LocalBreakGlassError {
    #[error("local break-glass credential was rejected")]
    Rejected,
    #[error("local break-glass verifier is unavailable")]
    Unavailable,
}

/// Peer-origin evidence required before a local break-glass request can be
/// constructed.  The server boundary must pass the actual transport peer
/// address; forwarded headers and caller-claimed locality are not valid
/// evidence.  Keeping the validated address in the request prevents the
/// authentication path from silently treating an arbitrary request as local.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalBreakGlassOrigin {
    peer_address: SocketAddr,
}

impl LocalBreakGlassOrigin {
    pub fn try_from_peer_address(peer_address: SocketAddr) -> Result<Self, DirectoryAuthError> {
        if !peer_address.ip().is_loopback() {
            return Err(DirectoryAuthError::BreakGlassRejected);
        }
        Ok(Self { peer_address })
    }

    pub const fn peer_address(self) -> SocketAddr {
        self.peer_address
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthenticatedBreakGlassIdentity {
    principal_ref: Uuid,
    session_ref: Uuid,
    incident_ref: String,
    issued_at: DateTime<Utc>,
    authenticated_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    mfa: Option<StepUpReceipt>,
}

impl AuthenticatedBreakGlassIdentity {
    pub const fn principal_ref(&self) -> Uuid {
        self.principal_ref
    }

    pub const fn session_ref(&self) -> Uuid {
        self.session_ref
    }

    pub const fn issued_at(&self) -> DateTime<Utc> {
        self.issued_at
    }

    pub fn incident_ref(&self) -> &str {
        &self.incident_ref
    }

    pub const fn authenticated_at(&self) -> DateTime<Utc> {
        self.authenticated_at
    }

    pub const fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }

    pub const fn mfa(&self) -> Option<&StepUpReceipt> {
        self.mfa.as_ref()
    }

    /// Returns the credential-free append-only record suitable for durable
    /// authentication evidence and audit/outbox persistence.
    pub fn persistence_evidence(&self) -> DirectoryAuthenticationEvidence {
        DirectoryAuthenticationEvidence::LocalBreakGlass {
            principal_ref: self.principal_ref,
            session_ref: self.session_ref,
            incident_ref: self.incident_ref.clone(),
            issued_at: self.issued_at,
            authenticated_at: self.authenticated_at,
            expires_at: self.expires_at,
            mfa: self.mfa.as_ref().map(MfaAuthenticationEvidence::from),
        }
    }
}

pub struct DirectoryAuthenticator {
    config: DirectoryConnectorConfig,
}

impl DirectoryAuthenticator {
    pub fn new(config: DirectoryConnectorConfig) -> Result<Self, DirectoryAuthError> {
        // Revalidate at the ownership boundary so a future deserialization
        // path cannot bypass the fail-closed structural gate.
        config.preflight()?;
        Ok(Self { config })
    }

    pub const fn config(&self) -> &DirectoryConnectorConfig {
        &self.config
    }

    pub async fn authenticate_directory<T: DirectoryTransport + ?Sized>(
        &self,
        request: DirectoryAuthenticationRequest,
        transport: &T,
        mfa_verifier: Option<&dyn MfaStepUpVerifier>,
        _caller_supplied_at: DateTime<Utc>,
    ) -> Result<AuthenticatedDirectoryIdentity, DirectoryAuthError> {
        // Keep the historical argument for source compatibility, but never
        // trust caller-provided time for MFA expiry or authentication evidence.
        self.authenticate_directory_at(request, transport, mfa_verifier)
            .await
    }

    async fn authenticate_directory_at<T: DirectoryTransport + ?Sized>(
        &self,
        request: DirectoryAuthenticationRequest,
        transport: &T,
        mfa_verifier: Option<&dyn MfaStepUpVerifier>,
    ) -> Result<AuthenticatedDirectoryIdentity, DirectoryAuthError> {
        if matches!(request.risk, AccessRisk::HighRisk { operation_ref } if operation_ref.is_nil())
        {
            return Err(DirectoryAuthError::MfaStepUpRejected);
        }
        transport.preflight(&self.config)?;
        let bind_request = DirectoryBindRequest {
            connector_ref: self.config.connector_ref,
            endpoint: &self.config.endpoint,
            tls: &self.config.tls,
            timeouts: self.config.timeouts,
            username: &request.username,
            credential: &request.credential,
        };
        let result = bounded_bind(transport.bind(bind_request), self.config.timeouts.bind())
            .await
            .ok_or(DirectoryAuthError::Timeout(DirectoryTimeoutPhase::Bind))?
            .map_err(map_transport_error)?;

        if result.connector_ref() != self.config.connector_ref {
            return Err(DirectoryAuthError::ProtocolViolation);
        }

        self.config
            .tls
            .validate_handshake(result.tls_evidence())
            .map_err(map_tls_error)?;

        let identity = result.identity();
        if identity.key().provider() != self.config.provider {
            return Err(DirectoryAuthError::ProtocolViolation);
        }
        match identity.account_status() {
            DirectoryAccountStatus::Enabled => {}
            DirectoryAccountStatus::Disabled => return Err(DirectoryAuthError::AccountDisabled),
            DirectoryAccountStatus::Deleted => return Err(DirectoryAuthError::AccountDeleted),
            DirectoryAccountStatus::Locked => return Err(DirectoryAuthError::AccountLocked),
        }
        let link = ExternalIdentityLinkRef::derive(
            self.config.connector_ref,
            self.config.provider,
            identity.key(),
        )?;
        // Evaluate the MFA receipt only after the protected bind and identity
        // validation complete.  A slow directory response must not extend a
        // receipt's lifetime or make the returned evidence look older/newer
        // than the actual authentication decision.
        let now = Utc::now();
        let mfa = verify_high_risk_step_up(
            request.risk,
            request.step_up.as_ref(),
            mfa_verifier,
            MfaSubject::Directory(link.clone()),
            now,
        )?;

        Ok(AuthenticatedDirectoryIdentity {
            connector_ref: self.config.connector_ref,
            scope: self.config.scope,
            provider: self.config.provider,
            link,
            immutable_value: identity.key().value(),
            tls_evidence: result.tls_evidence().clone(),
            mfa,
            authenticated_at: now,
        })
    }

    /// Explicit local break-glass entry point.  It is never called from
    /// `authenticate_directory`, including on timeout, TLS failure, or bind
    /// rejection.  The caller must have separately selected this mode and the
    /// connector must explicitly permit `LocalOnly`.
    pub fn authenticate_local_break_glass<V: LocalBreakGlassVerifier + ?Sized>(
        &self,
        request: &LocalBreakGlassRequest<'_>,
        verifier: &V,
        mfa_verifier: Option<&dyn MfaStepUpVerifier>,
    ) -> Result<AuthenticatedBreakGlassIdentity, DirectoryAuthError> {
        self.authenticate_local_break_glass_at(request, verifier, mfa_verifier)
    }

    fn authenticate_local_break_glass_at<V: LocalBreakGlassVerifier + ?Sized>(
        &self,
        request: &LocalBreakGlassRequest<'_>,
        verifier: &V,
        mfa_verifier: Option<&dyn MfaStepUpVerifier>,
    ) -> Result<AuthenticatedBreakGlassIdentity, DirectoryAuthError> {
        if self.config.break_glass != BreakGlassPolicy::LocalOnly {
            return Err(DirectoryAuthError::BreakGlassDisabled);
        }

        // Revalidate the origin at the authentication boundary as well as at
        // request construction.  This keeps a future deserialization or
        // adapter path from bypassing the local-only invariant.
        let origin = LocalBreakGlassOrigin::try_from_peer_address(request.origin.peer_address())?;

        // The request timestamp is metadata supplied by the caller.  Rebuild
        // the verifier request with the boundary's current time so a replayed
        // grant cannot be made valid by choosing an earlier timestamp.
        let request_time = Utc::now();
        let authoritative_request = LocalBreakGlassRequest {
            origin,
            principal_ref: request.principal_ref,
            credential: request.credential,
            incident_ref: request.incident_ref,
            risk: request.risk,
            step_up: request.step_up,
            requested_at: request_time,
        };
        let grant = verifier
            .verify(&authoritative_request)
            .map_err(|_| DirectoryAuthError::BreakGlassRejected)?;
        let now = Utc::now();
        if grant.principal_ref != authoritative_request.principal_ref {
            return Err(DirectoryAuthError::BreakGlassSubjectMismatch);
        }
        if now < grant.issued_at
            || grant.expires_at <= now
            || grant.expires_at - now > MAX_BREAK_GLASS_LIFETIME
        {
            return Err(DirectoryAuthError::BreakGlassRejected);
        }
        let mfa = verify_high_risk_step_up(
            authoritative_request.risk,
            authoritative_request.step_up,
            mfa_verifier,
            MfaSubject::LocalPrincipal(authoritative_request.principal_ref),
            now,
        )?;
        Ok(AuthenticatedBreakGlassIdentity {
            principal_ref: grant.principal_ref,
            session_ref: grant.session_ref,
            incident_ref: authoritative_request.incident_ref.to_owned(),
            issued_at: grant.issued_at,
            authenticated_at: now,
            expires_at: grant.expires_at,
            mfa,
        })
    }
}

fn verify_high_risk_step_up(
    risk: AccessRisk,
    proof: Option<&MfaStepUpProof>,
    verifier: Option<&dyn MfaStepUpVerifier>,
    subject: MfaSubject,
    now: DateTime<Utc>,
) -> Result<Option<StepUpReceipt>, DirectoryAuthError> {
    let AccessRisk::HighRisk { operation_ref } = risk else {
        return Ok(None);
    };
    if operation_ref.is_nil() {
        return Err(DirectoryAuthError::MfaStepUpRejected);
    }
    let Some(proof) = proof else {
        return Err(DirectoryAuthError::HighRiskMfaRequired);
    };
    let Some(verifier) = verifier else {
        return Err(DirectoryAuthError::HighRiskMfaRequired);
    };
    let request = StepUpRequest {
        subject: &subject,
        operation_ref,
        proof,
        now,
    };
    let receipt = verifier
        .verify(&request)
        .map_err(|_| DirectoryAuthError::MfaStepUpRejected)?;
    if receipt.subject != subject || receipt.operation_ref != operation_ref {
        return Err(DirectoryAuthError::MfaStepUpRejected);
    }
    receipt.validate_at(now)?;
    Ok(Some(receipt))
}

const fn transport_matches_provider(
    provider: DirectoryProvider,
    transport_mode: DirectoryTransportMode,
) -> bool {
    matches!(
        (provider, transport_mode),
        (
            DirectoryProvider::UniventionCorporateServer,
            DirectoryTransportMode::Https
        ) | (
            DirectoryProvider::GenericLdap,
            DirectoryTransportMode::Ldaps
        ) | (
            DirectoryProvider::GenericLdap,
            DirectoryTransportMode::StartTls
        ) | (
            DirectoryProvider::ActiveDirectoryDomainServices,
            DirectoryTransportMode::Ldaps
        ) | (
            DirectoryProvider::ActiveDirectoryDomainServices,
            DirectoryTransportMode::StartTls,
        )
    )
}

fn map_transport_error(error: DirectoryTransportError) -> DirectoryAuthError {
    match error {
        DirectoryTransportError::TlsHandshakeRejected => DirectoryAuthError::TlsPolicyRejected,
        DirectoryTransportError::CertificateValidationFailed => {
            DirectoryAuthError::TlsCertificateValidationFailed
        }
        DirectoryTransportError::Unavailable => DirectoryAuthError::TransportUnavailable,
        DirectoryTransportError::Rejected => DirectoryAuthError::BindRejected,
        DirectoryTransportError::Disabled => DirectoryAuthError::AccountDisabled,
        DirectoryTransportError::Deleted => DirectoryAuthError::AccountDeleted,
        DirectoryTransportError::Locked => DirectoryAuthError::AccountLocked,
        DirectoryTransportError::ProtocolViolation => DirectoryAuthError::ProtocolViolation,
    }
}

fn map_tls_error(error: TlsPolicyError) -> DirectoryAuthError {
    match error {
        TlsPolicyError::CertificateValidationFailed
        | TlsPolicyError::CertificateValidationRequired
        | TlsPolicyError::PeerCertificateEvidenceRequired => {
            DirectoryAuthError::TlsCertificateValidationFailed
        }
        _ => DirectoryAuthError::TlsPolicyRejected,
    }
}

/// A runtime-neutral deadline wrapper.  The identity crate cannot depend on a
/// particular async runtime in production, so the adapter future is guarded by
/// a small one-shot timer that wakes the task at the policy deadline.  The
/// adapter still receives the complete timeout policy and must apply the
/// connect/StartTLS limits at its protocol boundary.
fn bounded_bind<F>(future: F, timeout: Duration) -> BoundedFuture<F>
where
    F: Future,
{
    BoundedFuture {
        future: Box::pin(future),
        deadline: Instant::now() + timeout,
        timer_started: false,
        done: Arc::new(AtomicBool::new(false)),
    }
}

struct BoundedFuture<F> {
    future: Pin<Box<F>>,
    deadline: Instant,
    timer_started: bool,
    done: Arc<AtomicBool>,
}

impl<F: Future> Future for BoundedFuture<F> {
    type Output = Option<F::Output>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        if self.done.load(Ordering::Acquire) || Instant::now() >= self.deadline {
            self.done.store(true, Ordering::Release);
            return Poll::Ready(None);
        }

        match self.future.as_mut().poll(context) {
            Poll::Ready(value) => {
                self.done.store(true, Ordering::Release);
                // A future is allowed to do work synchronously inside poll.
                // Recheck after polling so a result produced after the
                // deadline cannot cross the authentication boundary merely
                // because the first pre-poll check happened just before it.
                if Instant::now() >= self.deadline {
                    Poll::Ready(None)
                } else {
                    Poll::Ready(Some(value))
                }
            }
            Poll::Pending => {
                if !self.timer_started {
                    self.timer_started = true;
                    let deadline = self.deadline;
                    let done = Arc::clone(&self.done);
                    let waker = context.waker().clone();
                    thread::spawn(move || {
                        let remaining = deadline.saturating_duration_since(Instant::now());
                        if !remaining.is_zero() {
                            thread::sleep(remaining);
                        }
                        if !done.swap(true, Ordering::AcqRel) {
                            waker.wake();
                        }
                    });
                }
                Poll::Pending
            }
        }
    }
}

fn valid_reference(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_MFA_REFERENCE_LENGTH
        && value
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && byte != b' ')
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::directory::tls::{
        CertificateFingerprint, DirectoryTransportMode, TlsProtocolVersion, TrustAnchor,
        sha256_certificate_fingerprint,
    };

    fn test_anchor(reference: &str, generation: u64, _digit: char) -> TrustAnchor {
        TrustAnchor::try_new(
            reference,
            CertificateFingerprint::try_new(sha256_certificate_fingerprint(
                b"verified-directory-peer-certificate",
            ))
            .expect("fingerprint"),
            generation,
        )
        .expect("anchor")
    }

    fn config(
        provider: DirectoryProvider,
        scope: ConnectorScope,
        break_glass: BreakGlassPolicy,
    ) -> (DirectoryConnectorConfig, TrustAnchor) {
        let anchor = test_anchor("directory-ca-v1", 1, 'a');
        let (tls, endpoint) = if provider == DirectoryProvider::UniventionCorporateServer {
            (
                TlsPolicy::https("directory.example.test", anchor.clone()).expect("HTTPS"),
                "https://directory.example.test/univention/provisioning/",
            )
        } else {
            (
                TlsPolicy::ldaps("directory.example.test", anchor.clone()).expect("LDAPS"),
                "ldaps://directory.example.test:636",
            )
        };
        let config = DirectoryConnectorConfig::try_new(
            Uuid::new_v4(),
            scope,
            provider,
            endpoint,
            tls,
            TimeoutPolicy::try_new(
                Duration::from_secs(1),
                Duration::from_secs(1),
                Duration::from_secs(1),
            )
            .expect("timeouts"),
            provider.native_change_feed(),
            break_glass,
        )
        .expect("connector config");
        (config, anchor)
    }

    fn bind_result(
        config: &DirectoryConnectorConfig,
        anchor: &TrustAnchor,
        status: DirectoryAccountStatus,
    ) -> DirectoryBindResult {
        DirectoryBindResult::new(
            config.connector_ref(),
            TlsHandshakeEvidence::from_verified_peer(
                config.tls.mode(),
                "directory.example.test",
                TlsProtocolVersion::Tls1_3,
                b"verified-directory-peer-certificate",
                anchor,
            )
            .expect("verified TLS evidence"),
            DirectoryIdentity::new(
                ExternalIdentityKey::try_for_provider(
                    config.provider(),
                    Uuid::from_u128(0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa),
                )
                .expect("key"),
                status,
            ),
        )
    }

    struct StaticTransport {
        result: Result<DirectoryBindResult, DirectoryTransportError>,
    }

    impl DirectoryTransport for StaticTransport {
        fn preflight(&self, _config: &DirectoryConnectorConfig) -> Result<(), DirectoryAuthError> {
            Ok(())
        }

        fn bind<'a>(&'a self, _request: DirectoryBindRequest<'a>) -> DirectoryBindFuture<'a> {
            let result = self.result.clone();
            Box::pin(async move { result })
        }
    }

    struct DelayedTransport {
        result: Result<DirectoryBindResult, DirectoryTransportError>,
        delay: Duration,
    }

    impl DirectoryTransport for DelayedTransport {
        fn preflight(&self, _config: &DirectoryConnectorConfig) -> Result<(), DirectoryAuthError> {
            Ok(())
        }

        fn bind<'a>(&'a self, _request: DirectoryBindRequest<'a>) -> DirectoryBindFuture<'a> {
            let result = self.result.clone();
            let delay = self.delay;
            Box::pin(async move {
                tokio::time::sleep(delay).await;
                result
            })
        }
    }

    struct StaticMfa {
        result: Result<StepUpReceipt, MfaStepUpError>,
    }

    impl MfaStepUpVerifier for StaticMfa {
        fn verify(&self, _request: &StepUpRequest<'_>) -> Result<StepUpReceipt, MfaStepUpError> {
            self.result.clone()
        }
    }

    struct StaticBreakGlass {
        result: Result<LocalBreakGlassGrant, LocalBreakGlassError>,
    }

    struct PendingTransport;

    impl DirectoryTransport for PendingTransport {
        fn preflight(&self, _config: &DirectoryConnectorConfig) -> Result<(), DirectoryAuthError> {
            Ok(())
        }

        fn bind<'a>(&'a self, _request: DirectoryBindRequest<'a>) -> DirectoryBindFuture<'a> {
            Box::pin(std::future::pending())
        }
    }

    struct BlockingReadyTransport {
        result: Result<DirectoryBindResult, DirectoryTransportError>,
        delay: Duration,
    }

    impl DirectoryTransport for BlockingReadyTransport {
        fn preflight(&self, _config: &DirectoryConnectorConfig) -> Result<(), DirectoryAuthError> {
            Ok(())
        }

        fn bind<'a>(&'a self, _request: DirectoryBindRequest<'a>) -> DirectoryBindFuture<'a> {
            let result = self.result.clone();
            let delay = self.delay;
            // Deliberately block inside the first poll.  This models an
            // adapter that violates cooperative async polling and proves the
            // post-poll deadline check does not accept a late bind result.
            Box::pin(async move {
                thread::sleep(delay);
                result
            })
        }
    }

    impl LocalBreakGlassVerifier for StaticBreakGlass {
        fn verify(
            &self,
            _request: &LocalBreakGlassRequest<'_>,
        ) -> Result<LocalBreakGlassGrant, LocalBreakGlassError> {
            self.result.clone()
        }
    }

    fn request(
        risk: AccessRisk,
        step_up: Option<MfaStepUpProof>,
    ) -> DirectoryAuthenticationRequest {
        DirectoryAuthenticationRequest::new(
            DirectoryUsername::try_new("alice@example.test").expect("username"),
            RedactedSecret::try_new("correct horse battery staple").expect("secret"),
            risk,
            step_up,
        )
    }

    fn local_origin() -> LocalBreakGlassOrigin {
        LocalBreakGlassOrigin::try_from_peer_address(
            "127.0.0.1:43100".parse().expect("loopback peer"),
        )
        .expect("local origin")
    }

    #[tokio::test]
    async fn bind_rejects_unvalidated_or_unattested_tls_and_redacts_credentials() {
        let (config, anchor) = config(
            DirectoryProvider::GenericLdap,
            ConnectorScope::Platform,
            BreakGlassPolicy::Disabled,
        );
        let authenticator = DirectoryAuthenticator::new(config.clone()).expect("authenticator");
        let transport = StaticTransport {
            result: Ok(DirectoryBindResult::new(
                config.connector_ref(),
                TlsHandshakeEvidence::synthetic(
                    DirectoryTransportMode::Ldaps,
                    "directory.example.test",
                    TlsProtocolVersion::Tls1_3,
                    true,
                    &anchor,
                ),
                DirectoryIdentity::new(
                    ExternalIdentityKey::GenericLdapEntryUuid(Uuid::new_v4()),
                    DirectoryAccountStatus::Enabled,
                ),
            )),
        };
        let error = authenticator
            .authenticate_directory(
                request(AccessRisk::Standard, None),
                &transport,
                None,
                Utc::now(),
            )
            .await
            .expect_err("unvalidated TLS must fail");
        assert_eq!(error, DirectoryAuthError::TlsCertificateValidationFailed);
        assert!(!format!("{error:?}").contains("correct horse"));
        assert!(
            !format!(
                "{:?}",
                RedactedSecret::try_new("correct horse battery staple").unwrap()
            )
            .contains("correct horse")
        );
    }

    struct UnreadyTransport;

    impl DirectoryTransport for UnreadyTransport {
        fn preflight(&self, _config: &DirectoryConnectorConfig) -> Result<(), DirectoryAuthError> {
            Err(DirectoryAuthError::ExternalCertificationRequired)
        }

        fn bind<'a>(&'a self, _request: DirectoryBindRequest<'a>) -> DirectoryBindFuture<'a> {
            Box::pin(async { panic!("an unready adapter must not receive a bind request") })
        }
    }

    #[tokio::test]
    async fn external_readiness_preflight_blocks_bind_before_credentials_are_sent() {
        let (config, _) = config(
            DirectoryProvider::GenericLdap,
            ConnectorScope::Platform,
            BreakGlassPolicy::Disabled,
        );
        let authenticator = DirectoryAuthenticator::new(config).expect("structural preflight");
        let error = authenticator
            .authenticate_directory(
                request(AccessRisk::Standard, None),
                &UnreadyTransport,
                None,
                Utc::now(),
            )
            .await
            .expect_err("external readiness must be fail-closed");
        assert_eq!(error, DirectoryAuthError::ExternalCertificationRequired);
    }

    #[tokio::test]
    async fn standard_bind_derives_an_immutable_connector_scoped_link() {
        let (config, anchor) = config(
            DirectoryProvider::ActiveDirectoryDomainServices,
            ConnectorScope::Holding(Uuid::new_v4()),
            BreakGlassPolicy::Disabled,
        );
        let authenticator = DirectoryAuthenticator::new(config.clone()).expect("authenticator");
        let transport = StaticTransport {
            result: Ok(bind_result(
                &config,
                &anchor,
                DirectoryAccountStatus::Enabled,
            )),
        };
        let result = authenticator
            .authenticate_directory(
                request(AccessRisk::Standard, None),
                &transport,
                None,
                Utc::now(),
            )
            .await
            .expect("bind");
        assert!(result.link().as_str().starts_with("erpx:directory-link:"));
        assert!(
            result
                .link()
                .as_str()
                .contains(DirectoryProvider::ActiveDirectoryDomainServices.segment())
        );
        assert_eq!(result.mfa(), None);

        let other = ExternalIdentityLinkRef::derive(
            Uuid::new_v4(),
            DirectoryProvider::ActiveDirectoryDomainServices,
            ExternalIdentityKey::ActiveDirectoryObjectGuid(Uuid::from_u128(
                0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,
            )),
        )
        .expect("other link");
        assert_ne!(result.link(), &other);
    }

    #[tokio::test]
    async fn bind_rejects_disabled_and_deleted_accounts_before_linking() {
        let statuses = [
            (
                DirectoryAccountStatus::Disabled,
                DirectoryAuthError::AccountDisabled,
            ),
            (
                DirectoryAccountStatus::Deleted,
                DirectoryAuthError::AccountDeleted,
            ),
        ];

        for (status, expected) in statuses {
            let (config, anchor) = config(
                DirectoryProvider::GenericLdap,
                ConnectorScope::Platform,
                BreakGlassPolicy::Disabled,
            );
            let authenticator = DirectoryAuthenticator::new(config.clone()).expect("authenticator");
            let error = authenticator
                .authenticate_directory(
                    request(AccessRisk::Standard, None),
                    &StaticTransport {
                        result: Ok(bind_result(&config, &anchor, status)),
                    },
                    None,
                    Utc::now(),
                )
                .await
                .expect_err("non-enabled accounts must not link or authenticate");
            assert_eq!(error, expected);
        }
    }

    #[tokio::test]
    async fn bind_rejects_a_valid_result_attested_to_another_connector() {
        let (config, anchor) = config(
            DirectoryProvider::GenericLdap,
            ConnectorScope::Platform,
            BreakGlassPolicy::Disabled,
        );
        let authenticator = DirectoryAuthenticator::new(config.clone()).expect("authenticator");
        let mut result = bind_result(&config, &anchor, DirectoryAccountStatus::Enabled);
        result.connector_ref = Uuid::new_v4();

        let error = authenticator
            .authenticate_directory(
                request(AccessRisk::Standard, None),
                &StaticTransport { result: Ok(result) },
                None,
                Utc::now(),
            )
            .await
            .expect_err("cross-connector result must fail closed");
        assert_eq!(error, DirectoryAuthError::ProtocolViolation);
    }

    #[tokio::test]
    async fn high_risk_bind_requires_and_binds_mfa_receipt() {
        let (config, anchor) = config(
            DirectoryProvider::UniventionCorporateServer,
            ConnectorScope::Platform,
            BreakGlassPolicy::Disabled,
        );
        let authenticator = DirectoryAuthenticator::new(config.clone()).expect("authenticator");
        let transport = StaticTransport {
            result: Ok(bind_result(
                &config,
                &anchor,
                DirectoryAccountStatus::Enabled,
            )),
        };
        let operation_ref = Uuid::new_v4();
        let now = Utc::now();
        let error = authenticator
            .authenticate_directory(
                request(AccessRisk::HighRisk { operation_ref }, None),
                &transport,
                None,
                now,
            )
            .await
            .expect_err("high-risk access needs MFA");
        assert_eq!(error, DirectoryAuthError::HighRiskMfaRequired);

        let link = ExternalIdentityLinkRef::derive(
            config.connector_ref(),
            config.provider(),
            ExternalIdentityKey::UcsEntryUuid(Uuid::from_u128(0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa)),
        )
        .expect("link");
        let receipt = StepUpReceipt::try_new(
            MfaSubject::Directory(link),
            operation_ref,
            MfaAssurance::WebAuthn,
            now - ChronoDuration::seconds(1),
            now + ChronoDuration::minutes(1),
        )
        .expect("receipt");
        let mfa = StaticMfa {
            result: Ok(receipt.clone()),
        };
        let proof = MfaStepUpProof::try_new("challenge-1", "assertion-1").expect("proof");
        let authenticated = authenticator
            .authenticate_directory(
                request(AccessRisk::HighRisk { operation_ref }, Some(proof)),
                &transport,
                Some(&mfa),
                now,
            )
            .await
            .expect("MFA-bound bind");
        assert_eq!(authenticated.mfa(), Some(&receipt));
    }

    #[tokio::test]
    async fn bind_future_is_bounded_without_a_runtime_specific_dependency() {
        let (mut config, _) = config(
            DirectoryProvider::GenericLdap,
            ConnectorScope::Platform,
            BreakGlassPolicy::Disabled,
        );
        config.timeouts = TimeoutPolicy::try_new(
            Duration::from_secs(1),
            Duration::from_secs(1),
            Duration::from_millis(1),
        )
        .expect("timeouts");
        let authenticator = DirectoryAuthenticator::new(config).expect("authenticator");
        let error = authenticator
            .authenticate_directory(
                request(AccessRisk::Standard, None),
                &PendingTransport,
                None,
                Utc::now(),
            )
            .await
            .expect_err("pending bind must time out");
        assert_eq!(
            error,
            DirectoryAuthError::Timeout(DirectoryTimeoutPhase::Bind)
        );
    }

    #[tokio::test]
    async fn bind_result_produced_after_the_deadline_is_not_accepted() {
        let (mut config, anchor) = config(
            DirectoryProvider::GenericLdap,
            ConnectorScope::Platform,
            BreakGlassPolicy::Disabled,
        );
        config.timeouts = TimeoutPolicy::try_new(
            Duration::from_secs(1),
            Duration::from_secs(1),
            Duration::from_millis(1),
        )
        .expect("timeouts");
        let authenticator = DirectoryAuthenticator::new(config.clone()).expect("authenticator");
        let transport = BlockingReadyTransport {
            result: Ok(bind_result(
                &config,
                &anchor,
                DirectoryAccountStatus::Enabled,
            )),
            delay: Duration::from_millis(10),
        };

        assert_eq!(
            authenticator
                .authenticate_directory(
                    request(AccessRisk::Standard, None),
                    &transport,
                    None,
                    Utc::now(),
                )
                .await,
            Err(DirectoryAuthError::Timeout(DirectoryTimeoutPhase::Bind))
        );
    }

    #[test]
    fn break_glass_is_explicit_local_only_and_never_an_automatic_fallback() {
        let (disabled, _) = config(
            DirectoryProvider::GenericLdap,
            ConnectorScope::Platform,
            BreakGlassPolicy::Disabled,
        );
        let disabled_auth = DirectoryAuthenticator::new(disabled).expect("authenticator");
        let secret = RedactedSecret::try_new("local-break-glass-secret").expect("secret");
        let requested_at = Utc::now();
        let request = LocalBreakGlassRequest::try_new_local(
            local_origin(),
            Uuid::new_v4(),
            &secret,
            "INC-123",
            AccessRisk::Standard,
            None,
            requested_at,
        )
        .expect("request");
        let verifier = StaticBreakGlass {
            result: Err(LocalBreakGlassError::Rejected),
        };
        assert_eq!(
            disabled_auth.authenticate_local_break_glass(&request, &verifier, None),
            Err(DirectoryAuthError::BreakGlassDisabled)
        );

        let (enabled, _) = config(
            DirectoryProvider::GenericLdap,
            ConnectorScope::Platform,
            BreakGlassPolicy::LocalOnly,
        );
        let enabled_auth = DirectoryAuthenticator::new(enabled).expect("authenticator");
        let principal_ref = request.principal_ref();
        let grant = LocalBreakGlassGrant::try_new(
            principal_ref,
            Uuid::new_v4(),
            requested_at + ChronoDuration::minutes(5),
            requested_at,
        )
        .expect("grant");
        let verifier = StaticBreakGlass { result: Ok(grant) };
        let authenticated = enabled_auth
            .authenticate_local_break_glass(&request, &verifier, None)
            .expect("explicit local break-glass");
        assert_eq!(authenticated.principal_ref(), principal_ref);
    }

    #[test]
    fn break_glass_uses_boundary_time_and_rejects_replayed_expired_grants() {
        let (config, _) = config(
            DirectoryProvider::GenericLdap,
            ConnectorScope::Platform,
            BreakGlassPolicy::LocalOnly,
        );
        let authenticator = DirectoryAuthenticator::new(config).expect("authenticator");
        let principal_ref = Uuid::new_v4();
        let stale_requested_at = Utc::now() - ChronoDuration::minutes(10);
        let secret = RedactedSecret::try_new("local-break-glass-secret").expect("secret");
        let request = LocalBreakGlassRequest::try_new_local(
            local_origin(),
            principal_ref,
            &secret,
            "INC-REPLAY",
            AccessRisk::Standard,
            None,
            stale_requested_at,
        )
        .expect("request");
        let grant = LocalBreakGlassGrant::try_new(
            principal_ref,
            Uuid::new_v4(),
            stale_requested_at + ChronoDuration::minutes(5),
            stale_requested_at,
        )
        .expect("historically valid grant");
        let verifier = StaticBreakGlass { result: Ok(grant) };

        assert_eq!(
            authenticator.authenticate_local_break_glass(&request, &verifier, None),
            Err(DirectoryAuthError::BreakGlassRejected)
        );
    }

    #[test]
    fn break_glass_rejects_a_future_dated_grant() {
        let (config, _) = config(
            DirectoryProvider::GenericLdap,
            ConnectorScope::Platform,
            BreakGlassPolicy::LocalOnly,
        );
        let authenticator = DirectoryAuthenticator::new(config).expect("authenticator");
        let principal_ref = Uuid::new_v4();
        let now = Utc::now();
        let secret = RedactedSecret::try_new("local-break-glass-secret").expect("secret");
        let request = LocalBreakGlassRequest::try_new_local(
            local_origin(),
            principal_ref,
            &secret,
            "INC-FUTURE",
            AccessRisk::Standard,
            None,
            now,
        )
        .expect("request");
        let issued_at = now + ChronoDuration::minutes(1);
        let grant = LocalBreakGlassGrant::try_new(
            principal_ref,
            Uuid::new_v4(),
            issued_at + ChronoDuration::minutes(5),
            issued_at,
        )
        .expect("future grant shape");

        assert_eq!(
            authenticator.authenticate_local_break_glass(
                &request,
                &StaticBreakGlass { result: Ok(grant) },
                None,
            ),
            Err(DirectoryAuthError::BreakGlassRejected)
        );
    }

    #[test]
    fn break_glass_origin_rejects_non_loopback_peers() {
        for peer in [
            "192.0.2.10:443".parse().expect("IPv4 peer"),
            "[2001:db8::10]:443".parse().expect("IPv6 peer"),
        ] {
            assert_eq!(
                LocalBreakGlassOrigin::try_from_peer_address(peer),
                Err(DirectoryAuthError::BreakGlassRejected)
            );
        }

        let local = local_origin();
        assert!(local.peer_address().ip().is_loopback());
    }

    #[tokio::test]
    async fn high_risk_authentication_uses_boundary_time_for_mfa_expiry() {
        let (config, anchor) = config(
            DirectoryProvider::GenericLdap,
            ConnectorScope::Platform,
            BreakGlassPolicy::Disabled,
        );
        let authenticator = DirectoryAuthenticator::new(config.clone()).expect("authenticator");
        let transport = StaticTransport {
            result: Ok(bind_result(
                &config,
                &anchor,
                DirectoryAccountStatus::Enabled,
            )),
        };
        let operation_ref = Uuid::new_v4();
        let stale_now = Utc::now() - ChronoDuration::minutes(10);
        let link = ExternalIdentityLinkRef::derive(
            config.connector_ref(),
            config.provider(),
            ExternalIdentityKey::GenericLdapEntryUuid(Uuid::from_u128(
                0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,
            )),
        )
        .expect("link");
        let receipt = StepUpReceipt::try_new(
            MfaSubject::Directory(link),
            operation_ref,
            MfaAssurance::Totp,
            stale_now,
            stale_now + ChronoDuration::minutes(1),
        )
        .expect("stale receipt shape");
        let mfa = StaticMfa {
            result: Ok(receipt),
        };
        let proof =
            MfaStepUpProof::try_new("challenge-expired", "assertion-expired").expect("proof");

        assert_eq!(
            authenticator
                .authenticate_directory(
                    request(AccessRisk::HighRisk { operation_ref }, Some(proof)),
                    &transport,
                    Some(&mfa),
                    stale_now,
                )
                .await,
            Err(DirectoryAuthError::MfaStepUpExpired)
        );
    }

    #[tokio::test]
    async fn high_risk_authentication_cannot_extend_mfa_through_a_slow_bind() {
        let (config, anchor) = config(
            DirectoryProvider::GenericLdap,
            ConnectorScope::Platform,
            BreakGlassPolicy::Disabled,
        );
        let authenticator = DirectoryAuthenticator::new(config.clone()).expect("authenticator");
        let operation_ref = Uuid::new_v4();
        let started_at = Utc::now();
        let link = ExternalIdentityLinkRef::derive(
            config.connector_ref(),
            config.provider(),
            ExternalIdentityKey::GenericLdapEntryUuid(Uuid::from_u128(
                0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,
            )),
        )
        .expect("link");
        let receipt = StepUpReceipt::try_new(
            MfaSubject::Directory(link),
            operation_ref,
            MfaAssurance::HardwareKey,
            started_at,
            started_at + ChronoDuration::milliseconds(10),
        )
        .expect("short-lived receipt");
        let transport = DelayedTransport {
            result: Ok(bind_result(
                &config,
                &anchor,
                DirectoryAccountStatus::Enabled,
            )),
            delay: Duration::from_millis(25),
        };
        let mfa = StaticMfa {
            result: Ok(receipt),
        };
        let proof =
            MfaStepUpProof::try_new("challenge-slow-bind", "assertion-slow-bind").expect("proof");

        assert_eq!(
            authenticator
                .authenticate_directory(
                    request(AccessRisk::HighRisk { operation_ref }, Some(proof)),
                    &transport,
                    Some(&mfa),
                    started_at,
                )
                .await,
            Err(DirectoryAuthError::MfaStepUpExpired)
        );
    }

    #[test]
    fn authentication_preflight_covers_all_providers_without_claiming_certification() {
        let matrix = [
            (
                DirectoryProvider::GenericLdap,
                DirectoryTransportMode::StartTls,
                "ldap://directory.example.test:389",
                ImmutableIdentityAttribute::LdapEntryUuid,
            ),
            (
                DirectoryProvider::ActiveDirectoryDomainServices,
                DirectoryTransportMode::Ldaps,
                "ldaps://directory.example.test:636",
                ImmutableIdentityAttribute::AdObjectGuid,
            ),
            (
                DirectoryProvider::UniventionCorporateServer,
                DirectoryTransportMode::Https,
                "https://directory.example.test/univention/provisioning/",
                ImmutableIdentityAttribute::UcsEntryUuid,
            ),
        ];

        for (provider, mode, endpoint, immutable_attribute) in matrix {
            let anchor = test_anchor("directory-ca-v1", 1, 'a');
            let tls = match mode {
                DirectoryTransportMode::Ldaps => {
                    TlsPolicy::ldaps("directory.example.test", anchor).expect("LDAPS policy")
                }
                DirectoryTransportMode::StartTls => {
                    TlsPolicy::starttls("directory.example.test", anchor).expect("StartTLS policy")
                }
                DirectoryTransportMode::Https => {
                    TlsPolicy::https("directory.example.test", anchor).expect("HTTPS policy")
                }
            };
            let config = DirectoryConnectorConfig::try_new(
                Uuid::new_v4(),
                ConnectorScope::Holding(Uuid::new_v4()),
                provider,
                endpoint,
                tls,
                TimeoutPolicy::conservative(),
                provider.native_change_feed(),
                BreakGlassPolicy::LocalOnly,
            )
            .expect("provider-specific TLS-only configuration");

            let preflight = config.preflight().expect("structural auth preflight");
            assert_eq!(preflight.provider(), provider);
            assert_eq!(preflight.transport_mode(), mode);
            assert_eq!(
                preflight.native_change_feed(),
                provider.native_change_feed()
            );
            assert_eq!(
                preflight.immutable_identity_attribute(),
                immutable_attribute
            );
            assert!(preflight.high_risk_mfa_required());
            assert_eq!(preflight.break_glass(), BreakGlassPolicy::LocalOnly);
            assert!(preflight.automatic_local_fallback_disabled());
            assert!(preflight.external_certification_required());
            DirectoryAuthenticator::new(config).expect("preflight is enforced by construction");
        }
    }

    #[test]
    fn authentication_preflight_rejects_a_plain_or_mismatched_transport_before_bind() {
        let anchor = test_anchor("directory-ca-v1", 1, 'a');
        let tls = TlsPolicy::ldaps("directory.example.test", anchor).expect("LDAPS policy");
        let error = DirectoryConnectorConfig::try_new(
            Uuid::new_v4(),
            ConnectorScope::Platform,
            DirectoryProvider::GenericLdap,
            "ldap://directory.example.test:389",
            tls,
            TimeoutPolicy::conservative(),
            NativeChangeFeed::Rfc4533,
            BreakGlassPolicy::Disabled,
        )
        .expect_err("plaintext endpoint must fail before a transport can bind");
        assert_eq!(error, DirectoryAuthError::InvalidConfiguration);
    }

    #[test]
    fn connector_config_rejects_provider_transport_mismatches_at_construction() {
        let mismatches = [
            (
                DirectoryProvider::GenericLdap,
                DirectoryTransportMode::Https,
                "https://directory.example.test/ldap",
            ),
            (
                DirectoryProvider::ActiveDirectoryDomainServices,
                DirectoryTransportMode::Https,
                "https://directory.example.test/ad",
            ),
            (
                DirectoryProvider::UniventionCorporateServer,
                DirectoryTransportMode::Ldaps,
                "ldaps://directory.example.test:636",
            ),
            (
                DirectoryProvider::UniventionCorporateServer,
                DirectoryTransportMode::StartTls,
                "ldap://directory.example.test:389",
            ),
        ];

        for (provider, mode, endpoint) in mismatches {
            let anchor = test_anchor("directory-ca-v1", 1, 'a');
            let tls = match mode {
                DirectoryTransportMode::Ldaps => {
                    TlsPolicy::ldaps("directory.example.test", anchor).expect("LDAPS policy")
                }
                DirectoryTransportMode::StartTls => {
                    TlsPolicy::starttls("directory.example.test", anchor).expect("StartTLS policy")
                }
                DirectoryTransportMode::Https => {
                    TlsPolicy::https("directory.example.test", anchor).expect("HTTPS policy")
                }
            };

            assert_eq!(
                DirectoryConnectorConfig::try_new(
                    Uuid::new_v4(),
                    ConnectorScope::Platform,
                    provider,
                    endpoint,
                    tls,
                    TimeoutPolicy::conservative(),
                    provider.native_change_feed(),
                    BreakGlassPolicy::Disabled,
                ),
                Err(DirectoryAuthError::InvalidConfiguration),
                "provider/mode mismatch must fail before an authenticator or adapter is built"
            );
        }
    }

    #[test]
    fn provider_feed_and_identity_key_contracts_reject_cross_provider_values() {
        let anchor = test_anchor("directory-ca-v1", 1, 'a');
        let tls = TlsPolicy::starttls("ldap.example.test", anchor).expect("TLS");
        let error = DirectoryConnectorConfig::try_new(
            Uuid::new_v4(),
            ConnectorScope::Platform,
            DirectoryProvider::GenericLdap,
            "ldap://ldap.example.test:389",
            tls,
            TimeoutPolicy::conservative(),
            NativeChangeFeed::AdDirSync,
            BreakGlassPolicy::Disabled,
        )
        .expect_err("generic LDAP cannot claim AD DirSync");
        assert_eq!(error, DirectoryAuthError::InvalidConfiguration);

        let key = ExternalIdentityKey::GenericLdapEntryUuid(Uuid::new_v4());
        assert_eq!(
            ExternalIdentityLinkRef::derive(
                Uuid::new_v4(),
                DirectoryProvider::ActiveDirectoryDomainServices,
                key,
            ),
            Err(DirectoryAuthError::InvalidExternalIdentityLink)
        );
    }

    #[test]
    fn nil_connector_identity_and_mfa_references_fail_closed() {
        let anchor = test_anchor("directory-ca-v1", 1, 'a');
        let tls = TlsPolicy::ldaps("directory.example.test", anchor).expect("TLS");
        assert_eq!(
            DirectoryConnectorConfig::try_new(
                Uuid::nil(),
                ConnectorScope::Platform,
                DirectoryProvider::GenericLdap,
                "ldaps://directory.example.test:636",
                tls,
                TimeoutPolicy::conservative(),
                NativeChangeFeed::Rfc4533,
                BreakGlassPolicy::Disabled,
            ),
            Err(DirectoryAuthError::InvalidConfiguration)
        );
        assert_eq!(
            ExternalIdentityLinkRef::derive(
                Uuid::new_v4(),
                DirectoryProvider::GenericLdap,
                ExternalIdentityKey::GenericLdapEntryUuid(Uuid::nil()),
            ),
            Err(DirectoryAuthError::InvalidExternalIdentityLink)
        );
        assert_eq!(
            StepUpReceipt::try_new(
                MfaSubject::LocalPrincipal(Uuid::new_v4()),
                Uuid::nil(),
                MfaAssurance::Totp,
                Utc::now(),
                Utc::now() + ChronoDuration::minutes(1),
            ),
            Err(DirectoryAuthError::MfaStepUpRejected)
        );
    }

    #[test]
    fn immutable_link_reuse_rejects_mutation() {
        let connector_ref = Uuid::new_v4();
        let first = ExternalIdentityLinkRef::derive(
            connector_ref,
            DirectoryProvider::GenericLdap,
            ExternalIdentityKey::GenericLdapEntryUuid(Uuid::from_u128(1)),
        )
        .expect("link");
        let same = first.clone();
        let changed = ExternalIdentityLinkRef::derive(
            connector_ref,
            DirectoryProvider::GenericLdap,
            ExternalIdentityKey::GenericLdapEntryUuid(Uuid::from_u128(2)),
        )
        .expect("link");
        assert!(first.ensure_unchanged(&same).is_ok());
        assert_eq!(
            first.ensure_unchanged(&changed),
            Err(DirectoryAuthError::ExternalIdentityLinkMutation)
        );
    }

    #[test]
    fn persistence_evidence_is_serializable_and_contains_no_credentials() {
        let (config, anchor) = config(
            DirectoryProvider::GenericLdap,
            ConnectorScope::Platform,
            BreakGlassPolicy::Disabled,
        );
        let link = ExternalIdentityLinkRef::derive(
            config.connector_ref(),
            config.provider(),
            ExternalIdentityKey::GenericLdapEntryUuid(Uuid::from_u128(1)),
        )
        .expect("link");
        let evidence = AuthenticatedDirectoryIdentity {
            connector_ref: config.connector_ref(),
            scope: config.scope(),
            provider: config.provider(),
            link,
            immutable_value: Uuid::from_u128(1),
            tls_evidence: TlsHandshakeEvidence::from_verified_peer(
                DirectoryTransportMode::Ldaps,
                "directory.example.test",
                TlsProtocolVersion::Tls1_3,
                b"verified-directory-peer-certificate",
                &anchor,
            )
            .expect("verified TLS evidence"),
            mfa: None,
            authenticated_at: Utc::now(),
        }
        .persistence_evidence();
        let encoded = serde_json::to_string(&evidence).expect("evidence JSON");
        assert!(encoded.contains("directory-link:"));
        assert!(encoded.contains("peer_certificate_fingerprint"));
        assert!(!encoded.contains("alice@example.test"));
        assert!(!encoded.contains("correct horse battery staple"));
        assert!(!encoded.contains("challenge-1"));
    }

    #[test]
    fn authentication_request_debug_redacts_mfa_references() {
        let proof = MfaStepUpProof::try_new("challenge-secret", "assertion-secret").expect("proof");
        let rendered = format!("{:?}", request(AccessRisk::Standard, Some(proof)));
        assert!(rendered.contains("MfaStepUpProof(REDACTED)"));
        assert!(!rendered.contains("challenge-secret"));
        assert!(!rendered.contains("assertion-secret"));
    }
}
