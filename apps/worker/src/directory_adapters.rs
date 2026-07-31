//! Legacy, fail-closed directory transport contracts used by parser tests.
//!
//! The production worker transport lives in `directory_live.rs` and uses
//! rustls-only LDAP/BER and UCS Provisioning API clients.  The typed contracts
//! below remain useful for bounded provider parsing and compatibility tests;
//! they are not the production evidence for a native directory feed.
//!
//! Every constructor requires a concrete TLS endpoint and non-empty live
//! credential material.  There is no anonymous, plaintext, or local fallback.

use std::{collections::HashSet, env, fmt, time::Duration};

use erpx_identity::directory::tls::{
    CertificateFingerprint, DirectoryTransportMode, TlsHandshakeEvidence, TlsPolicy, TrustAnchor,
};
use erpx_identity::directory::{DirectoryFeedKind, DirectoryProvider, DirectoryTlsMode};
use reqwest::{Client, RequestBuilder, Url, header};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

/// Maximum number of records requested from a directory transport in one
/// page.  The identity feed applies its own domain-level bound as well.
pub const MAX_DIRECTORY_ADAPTER_PAGE_SIZE: u32 = 10_000;

/// Maximum size of an opaque native-feed cursor or UCS token.
pub const MAX_DIRECTORY_ADAPTER_CURSOR_BYTES: usize = 64 * 1024;

/// Maximum number of requested attributes in one provider request.
pub const MAX_DIRECTORY_ADAPTER_ATTRIBUTES: usize = 128;

/// Maximum length of one requested LDAP attribute name.
pub const MAX_DIRECTORY_ADAPTER_ATTRIBUTE_BYTES: usize = 256;

/// Maximum endpoint length accepted from deployment configuration.
pub const MAX_DIRECTORY_ADAPTER_ENDPOINT_BYTES: usize = 2 * 1024;

/// Maximum credential material held by one adapter.
pub const MAX_DIRECTORY_ADAPTER_CREDENTIAL_BYTES: usize = 16 * 1024;

/// Maximum UCS HTTP response body retained by the adapter.
pub const MAX_DIRECTORY_ADAPTER_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// Maximum size of an LDAP base DN, AD naming context, or UCS position sent
/// through this adapter seam.
pub const MAX_DIRECTORY_ADAPTER_SEARCH_PARAMETER_BYTES: usize = 2 * 1024;

const DIRECTORY_ADAPTER_TIMEOUT: Duration = Duration::from_secs(30);

/// Errors intentionally contain no endpoint, credential, response body, or
/// provider message.  This makes both `Display` and `Debug` safe to include in
/// worker logs and durable failure records.
#[derive(Debug, Clone, Copy, Error, PartialEq, Eq)]
pub enum DirectoryAdapterError {
    #[error("directory adapter endpoint is unavailable")]
    EndpointUnavailable,
    #[error("directory adapter credential is unavailable")]
    CredentialUnavailable,
    #[error("directory adapter credential is invalid")]
    InvalidCredential,
    #[error("directory adapter endpoint is invalid")]
    InvalidEndpoint,
    #[error("directory adapter requires TLS-only transport")]
    InsecureTransport,
    #[error("directory provider and native feed are not compatible")]
    UnsupportedProviderFeed,
    #[error("directory adapter configuration is invalid")]
    InvalidConfiguration,
    #[error("directory adapter cursor is invalid or too large")]
    InvalidCursor,
    #[error("directory adapter page size is outside its bound")]
    PageSizeOutOfBounds,
    #[error("directory adapter attribute list is outside its bound")]
    AttributesOutOfBounds,
    #[error("directory adapter base DN or filter is missing")]
    MissingSearchParameter,
    #[error("directory adapter search parameter is too large")]
    SearchParameterOutOfBounds,
    #[error("directory adapter response is too large")]
    ResponseTooLarge,
    #[error("directory adapter response is malformed")]
    MalformedResponse,
    #[error("directory adapter response violates its provider contract")]
    ProtocolRejected,
    #[error("directory adapter transport is unavailable")]
    TransportUnavailable,
    #[error("directory adapter returned a non-success HTTP status")]
    HttpRejected,
    #[error("directory adapter TLS trust validation failed")]
    TrustValidationFailed,
}

/// Secret material is zeroized when released and never appears in `Debug`.
pub struct DirectoryCredential(Zeroizing<Vec<u8>>);

impl DirectoryCredential {
    pub fn try_new(material: impl Into<Vec<u8>>) -> Result<Self, DirectoryAdapterError> {
        let mut material = material.into();
        if material.is_empty() || material.len() > MAX_DIRECTORY_ADAPTER_CREDENTIAL_BYTES {
            material.zeroize();
            return Err(DirectoryAdapterError::InvalidCredential);
        }
        if material.iter().any(u8::is_ascii_control) {
            material.zeroize();
            return Err(DirectoryAdapterError::InvalidCredential);
        }
        Ok(Self(Zeroizing::new(material)))
    }

    fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl fmt::Debug for DirectoryCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DirectoryCredential")
            .field("material", &"[redacted]")
            .finish()
    }
}

/// The endpoint policy is explicit because StartTLS is only permitted when
/// the eventual LDAP connection is upgraded before credentials or searches
/// are sent.  There is no plaintext mode in this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryAdapterBinding {
    provider: DirectoryProvider,
    feed: DirectoryFeedKind,
    endpoint: Url,
    tls_mode: DirectoryTlsMode,
}

impl DirectoryAdapterBinding {
    pub fn try_new(
        provider: DirectoryProvider,
        feed: DirectoryFeedKind,
        endpoint: impl Into<String>,
        tls_mode: DirectoryTlsMode,
    ) -> Result<Self, DirectoryAdapterError> {
        if !feed.supported_by(provider) {
            return Err(DirectoryAdapterError::UnsupportedProviderFeed);
        }

        let endpoint = endpoint.into();
        if endpoint.trim().is_empty() || endpoint.len() > MAX_DIRECTORY_ADAPTER_ENDPOINT_BYTES {
            return Err(DirectoryAdapterError::InvalidEndpoint);
        }
        let parsed = Url::parse(&endpoint).map_err(|_| DirectoryAdapterError::InvalidEndpoint)?;
        if parsed.host_str().is_none()
            || parsed.username() != ""
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
            || parsed.port().is_some_and(|port| port == 0)
            || (provider != DirectoryProvider::UniventionCorporateServer && parsed.path() != "")
        {
            return Err(DirectoryAdapterError::InvalidEndpoint);
        }

        let expected = match tls_mode {
            DirectoryTlsMode::Ldaps => "ldaps",
            DirectoryTlsMode::StartTls => "ldap",
            DirectoryTlsMode::Https => "https",
        };
        if parsed.scheme() != expected {
            return Err(DirectoryAdapterError::InsecureTransport);
        }
        if provider == DirectoryProvider::UniventionCorporateServer
            && tls_mode != DirectoryTlsMode::Https
        {
            return Err(DirectoryAdapterError::InvalidConfiguration);
        }
        if provider != DirectoryProvider::UniventionCorporateServer
            && tls_mode == DirectoryTlsMode::Https
        {
            return Err(DirectoryAdapterError::InvalidConfiguration);
        }

        Ok(Self {
            provider,
            feed,
            endpoint: parsed,
            tls_mode,
        })
    }

    pub const fn provider(&self) -> DirectoryProvider {
        self.provider
    }

    pub const fn feed(&self) -> DirectoryFeedKind {
        self.feed
    }

    pub fn endpoint(&self) -> &Url {
        &self.endpoint
    }

    pub const fn tls_mode(&self) -> DirectoryTlsMode {
        self.tls_mode
    }

    /// Build the certificate-validating policy that a concrete native
    /// transport must apply before sending bind credentials or feed requests.
    /// The trust anchor is supplied by deployment state; this seam never
    /// invents a certificate or treats an endpoint string as trust evidence.
    pub fn tls_policy(
        &self,
        trust_anchor_reference: impl Into<String>,
        trust_anchor_fingerprint: impl Into<String>,
        trust_anchor_generation: u64,
    ) -> Result<TlsPolicy, DirectoryAdapterError> {
        let fingerprint = CertificateFingerprint::try_new(trust_anchor_fingerprint)
            .map_err(|_| DirectoryAdapterError::TrustValidationFailed)?;
        let anchor =
            TrustAnchor::try_new(trust_anchor_reference, fingerprint, trust_anchor_generation)
                .map_err(|_| DirectoryAdapterError::TrustValidationFailed)?;
        let server_name = self
            .endpoint
            .host_str()
            .ok_or(DirectoryAdapterError::InvalidEndpoint)?;
        let policy = match self.tls_mode {
            DirectoryTlsMode::Ldaps => TlsPolicy::ldaps(server_name, anchor),
            DirectoryTlsMode::StartTls => TlsPolicy::starttls(server_name, anchor),
            DirectoryTlsMode::Https => TlsPolicy::https(server_name, anchor),
        }
        .map_err(|_| DirectoryAdapterError::TrustValidationFailed)?;
        policy
            .validate_endpoint(self.endpoint.as_str())
            .map_err(|_| DirectoryAdapterError::TrustValidationFailed)?;
        Ok(policy)
    }

    /// Validate the evidence returned by a completed rustls/native TLS
    /// handshake.  A boolean certificate-valid result without the verified
    /// peer fingerprint is deliberately rejected by the identity policy.
    pub fn validate_tls_handshake(
        &self,
        policy: &TlsPolicy,
        evidence: &TlsHandshakeEvidence,
    ) -> Result<(), DirectoryAdapterError> {
        let expected_mode = match self.tls_mode {
            DirectoryTlsMode::Ldaps => DirectoryTransportMode::Ldaps,
            DirectoryTlsMode::StartTls => DirectoryTransportMode::StartTls,
            DirectoryTlsMode::Https => DirectoryTransportMode::Https,
        };
        if policy.mode() != expected_mode
            || !policy
                .server_name()
                .eq_ignore_ascii_case(self.endpoint.host_str().unwrap_or_default())
        {
            return Err(DirectoryAdapterError::TrustValidationFailed);
        }
        policy
            .validate_handshake(evidence)
            .map_err(|_| DirectoryAdapterError::TrustValidationFailed)
    }
}

/// A validated endpoint plus live credential material.  It is intentionally
/// impossible to construct this configuration without both pieces.
#[derive(Debug)]
pub struct DirectoryAdapterConfig {
    binding: DirectoryAdapterBinding,
    credential: DirectoryCredential,
}

impl DirectoryAdapterConfig {
    pub fn try_new(
        binding: DirectoryAdapterBinding,
        credential: impl Into<Vec<u8>>,
    ) -> Result<Self, DirectoryAdapterError> {
        Ok(Self {
            binding,
            credential: DirectoryCredential::try_new(credential)?,
        })
    }

    /// Resolve deployment-provided material at construction time.  Missing
    /// variables are a hard configuration error, not an invitation to use a
    /// mock, anonymous bind, or Legacy/local-auth fallback.
    pub fn from_env(
        provider: DirectoryProvider,
        feed: DirectoryFeedKind,
        tls_mode: DirectoryTlsMode,
        endpoint_env: &str,
        credential_env: &str,
    ) -> Result<Self, DirectoryAdapterError> {
        let endpoint =
            env::var(endpoint_env).map_err(|_| DirectoryAdapterError::EndpointUnavailable)?;
        let credential =
            env::var(credential_env).map_err(|_| DirectoryAdapterError::CredentialUnavailable)?;
        let binding = DirectoryAdapterBinding::try_new(provider, feed, endpoint, tls_mode)?;
        Self::try_new(binding, credential.into_bytes())
    }

    pub const fn binding(&self) -> &DirectoryAdapterBinding {
        &self.binding
    }

    fn credential(&self) -> &DirectoryCredential {
        &self.credential
    }
}

fn validate_page_size(page_size: u32) -> Result<(), DirectoryAdapterError> {
    if page_size == 0 || page_size > MAX_DIRECTORY_ADAPTER_PAGE_SIZE {
        return Err(DirectoryAdapterError::PageSizeOutOfBounds);
    }
    Ok(())
}

fn validate_cursor(cursor: &[u8]) -> Result<(), DirectoryAdapterError> {
    if cursor.len() > MAX_DIRECTORY_ADAPTER_CURSOR_BYTES {
        return Err(DirectoryAdapterError::InvalidCursor);
    }
    Ok(())
}

fn validate_continuation_cursor(
    cursor: &[u8],
    previous: &[u8],
) -> Result<(), DirectoryAdapterError> {
    validate_cursor(cursor)?;
    if cursor.is_empty() || cursor == previous {
        return Err(DirectoryAdapterError::ProtocolRejected);
    }
    Ok(())
}

fn validate_token(token: &str) -> Result<(), DirectoryAdapterError> {
    if token.len() > MAX_DIRECTORY_ADAPTER_CURSOR_BYTES || token.chars().any(char::is_control) {
        return Err(DirectoryAdapterError::InvalidCursor);
    }
    Ok(())
}

fn validate_attributes(attributes: &[String]) -> Result<(), DirectoryAdapterError> {
    if attributes.is_empty() || attributes.len() > MAX_DIRECTORY_ADAPTER_ATTRIBUTES {
        return Err(DirectoryAdapterError::AttributesOutOfBounds);
    }
    if attributes.iter().any(|attribute| {
        attribute.is_empty()
            || attribute.len() > MAX_DIRECTORY_ADAPTER_ATTRIBUTE_BYTES
            || attribute.chars().any(char::is_control)
    }) {
        return Err(DirectoryAdapterError::AttributesOutOfBounds);
    }
    Ok(())
}

fn validate_search_parameter(value: &str) -> Result<(), DirectoryAdapterError> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(DirectoryAdapterError::MissingSearchParameter);
    }
    if value.len() > MAX_DIRECTORY_ADAPTER_SEARCH_PARAMETER_BYTES {
        return Err(DirectoryAdapterError::SearchParameterOutOfBounds);
    }
    Ok(())
}

/// Common bounded representation returned by each provider parser.  Mapping
/// these entries to immutable identity links and durable changes remains the
/// identity feed's responsibility.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryAdapterEntry {
    pub external_id: String,
    pub immutable_id: String,
    pub operation: DirectoryAdapterOperation,
    #[serde(default)]
    pub login_name: Option<String>,
}

impl DirectoryAdapterEntry {
    fn validate(&self) -> Result<(), DirectoryAdapterError> {
        for value in [&self.external_id, &self.immutable_id] {
            if value.trim().is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
                return Err(DirectoryAdapterError::MalformedResponse);
            }
        }
        if matches!(
            self.operation,
            DirectoryAdapterOperation::Disable | DirectoryAdapterOperation::Delete
        ) && self.login_name.is_some()
        {
            return Err(DirectoryAdapterError::MalformedResponse);
        }
        if self.operation == DirectoryAdapterOperation::Upsert
            && self.login_name.as_deref().is_none_or(|value| {
                value.trim().is_empty() || value.len() > 512 || value.chars().any(char::is_control)
            })
        {
            return Err(DirectoryAdapterError::MalformedResponse);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectoryAdapterOperation {
    Upsert,
    Disable,
    Delete,
}

/// Provider-neutral result after provider-specific envelope validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryAdapterPage {
    pub entries: Vec<DirectoryAdapterEntry>,
    pub next_cursor: Option<DirectoryAdapterCursor>,
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectoryAdapterCursor {
    Opaque(Vec<u8>),
    Ucs { token: String, sequence: u64 },
}

fn validate_entries(
    entries: &[DirectoryAdapterEntry],
    page_size: u32,
    _full_snapshot: bool,
) -> Result<(), DirectoryAdapterError> {
    if entries.len() > page_size as usize {
        return Err(DirectoryAdapterError::PageSizeOutOfBounds);
    }
    let mut external_ids = HashSet::with_capacity(entries.len());
    let mut immutable_ids = HashSet::with_capacity(entries.len());
    for entry in entries {
        entry.validate()?;
        let external_id_is_unique = external_ids.insert(entry.external_id.as_str());
        let immutable_id_is_unique = immutable_ids.insert(entry.immutable_id.as_str());
        if !external_id_is_unique || !immutable_id_is_unique {
            return Err(DirectoryAdapterError::ProtocolRejected);
        }
        // A full snapshot is authoritative for both presence and state.  A
        // disabled or deleted principal must remain in the snapshot so the
        // identity layer can persist its immutable link and apply the
        // five-minute revocation contract.  Do not discard those operations
        // at the provider seam; the identity feed owns their state mapping.
    }
    Ok(())
}

fn decode_bounded<T: DeserializeOwned>(body: &[u8]) -> Result<T, DirectoryAdapterError> {
    if body.len() > MAX_DIRECTORY_ADAPTER_RESPONSE_BYTES {
        return Err(DirectoryAdapterError::ResponseTooLarge);
    }
    serde_json::from_slice(body).map_err(|_| DirectoryAdapterError::MalformedResponse)
}

/// RFC 4533 request emitted by a certified LDAP transport implementation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GenericLdapRfc4533Request {
    pub base_dn: String,
    pub attributes: Vec<String>,
    pub cookie: Vec<u8>,
    pub refresh_and_persist: bool,
    pub reload_hint: bool,
    pub limit: u32,
    pub starttls_required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rfc4533RequestMode {
    RefreshOnly,
    RefreshAndPersist,
}

/// Generic LDAP RFC 4533 adapter.  LDAP BER encoding and socket ownership
/// remain a separate certified transport implementation; this type enforces
/// the request contract before that implementation can send anything.
#[derive(Debug)]
pub struct GenericLdapRfc4533Adapter {
    config: DirectoryAdapterConfig,
    base_dn: String,
    attributes: Vec<String>,
}

impl GenericLdapRfc4533Adapter {
    pub fn new(
        config: DirectoryAdapterConfig,
        base_dn: impl Into<String>,
        attributes: Vec<String>,
    ) -> Result<Self, DirectoryAdapterError> {
        if config.binding().provider() != DirectoryProvider::GenericLdap
            || config.binding().feed() != DirectoryFeedKind::Rfc4533
        {
            return Err(DirectoryAdapterError::UnsupportedProviderFeed);
        }
        let base_dn = base_dn.into();
        validate_search_parameter(&base_dn)?;
        validate_attributes(&attributes)?;
        Ok(Self {
            config,
            base_dn,
            attributes,
        })
    }

    pub fn from_env(
        base_dn: impl Into<String>,
        attributes: Vec<String>,
        endpoint_env: &str,
        credential_env: &str,
        tls_mode: DirectoryTlsMode,
    ) -> Result<Self, DirectoryAdapterError> {
        Self::new(
            DirectoryAdapterConfig::from_env(
                DirectoryProvider::GenericLdap,
                DirectoryFeedKind::Rfc4533,
                tls_mode,
                endpoint_env,
                credential_env,
            )?,
            base_dn,
            attributes,
        )
    }

    pub fn build_request(
        &self,
        cursor: &[u8],
        page_size: u32,
        mode: Rfc4533RequestMode,
    ) -> Result<GenericLdapRfc4533Request, DirectoryAdapterError> {
        validate_cursor(cursor)?;
        validate_page_size(page_size)?;
        Ok(GenericLdapRfc4533Request {
            base_dn: self.base_dn.clone(),
            attributes: self.attributes.clone(),
            cookie: cursor.to_vec(),
            refresh_and_persist: matches!(mode, Rfc4533RequestMode::RefreshAndPersist),
            reload_hint: false,
            limit: page_size,
            starttls_required: self.config.binding().tls_mode() == DirectoryTlsMode::StartTls,
        })
    }

    pub fn parse_response(
        &self,
        body: &[u8],
        request: &GenericLdapRfc4533Request,
    ) -> Result<DirectoryAdapterPage, DirectoryAdapterError> {
        self.validate_request(request)?;
        let response: Rfc4533WireResponse = decode_bounded(body)?;
        let entries = response
            .entries
            .ok_or(DirectoryAdapterError::MalformedResponse)?;
        let cookie = response
            .cookie
            .ok_or(DirectoryAdapterError::MalformedResponse)?;
        if response.refresh_required {
            return Err(DirectoryAdapterError::ProtocolRejected);
        }
        if response.refresh_done {
            // A terminal syncDone must still carry a native cookie. Without
            // it the caller cannot safely persist the source position or
            // resume the feed after a crash.
            validate_continuation_cursor(&cookie, &request.cookie)?;
        } else {
            validate_continuation_cursor(&cookie, &request.cookie)?;
        }
        validate_entries(&entries, request.limit, !request.refresh_and_persist)?;
        let complete = response.refresh_done;
        let next_cursor = Some(DirectoryAdapterCursor::Opaque(cookie));
        Ok(DirectoryAdapterPage {
            entries,
            // Retain a terminal sync cookie. The import finalizer publishes
            // this source position for the following incremental run.
            next_cursor,
            complete,
        })
    }

    pub const fn binding(&self) -> &DirectoryAdapterBinding {
        self.config.binding()
    }

    fn validate_request(
        &self,
        request: &GenericLdapRfc4533Request,
    ) -> Result<(), DirectoryAdapterError> {
        validate_search_parameter(&request.base_dn)?;
        validate_attributes(&request.attributes)?;
        validate_cursor(&request.cookie)?;
        validate_page_size(request.limit)?;
        if request.base_dn != self.base_dn
            || request.attributes != self.attributes
            || request.starttls_required
                != (self.config.binding().tls_mode() == DirectoryTlsMode::StartTls)
        {
            return Err(DirectoryAdapterError::ProtocolRejected);
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct Rfc4533WireResponse {
    entries: Option<Vec<DirectoryAdapterEntry>>,
    cookie: Option<Vec<u8>>,
    refresh_done: bool,
    #[serde(default)]
    refresh_required: bool,
}

/// AD DS DirSync request emitted by a certified LDAP transport.  The flags
/// are fixed to the safe change-feed mode; callers cannot request a weaker
/// object-security or link-value policy through this contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActiveDirectoryDirSyncRequest {
    pub naming_context: String,
    pub attributes: Vec<String>,
    pub cookie: Vec<u8>,
    pub object_security: bool,
    pub incremental_values: bool,
    pub link_values: bool,
    pub page_size: u32,
    pub starttls_required: bool,
}

#[derive(Debug)]
pub struct ActiveDirectoryDirSyncAdapter {
    config: DirectoryAdapterConfig,
    naming_context: String,
    attributes: Vec<String>,
}

impl ActiveDirectoryDirSyncAdapter {
    pub fn new(
        config: DirectoryAdapterConfig,
        naming_context: impl Into<String>,
        attributes: Vec<String>,
    ) -> Result<Self, DirectoryAdapterError> {
        if config.binding().provider() != DirectoryProvider::ActiveDirectoryDomainServices
            || config.binding().feed() != DirectoryFeedKind::AdDirSync
        {
            return Err(DirectoryAdapterError::UnsupportedProviderFeed);
        }
        let naming_context = naming_context.into();
        validate_search_parameter(&naming_context)?;
        validate_attributes(&attributes)?;
        Ok(Self {
            config,
            naming_context,
            attributes,
        })
    }

    pub fn from_env(
        naming_context: impl Into<String>,
        attributes: Vec<String>,
        endpoint_env: &str,
        credential_env: &str,
        tls_mode: DirectoryTlsMode,
    ) -> Result<Self, DirectoryAdapterError> {
        Self::new(
            DirectoryAdapterConfig::from_env(
                DirectoryProvider::ActiveDirectoryDomainServices,
                DirectoryFeedKind::AdDirSync,
                tls_mode,
                endpoint_env,
                credential_env,
            )?,
            naming_context,
            attributes,
        )
    }

    pub fn build_request(
        &self,
        cursor: &[u8],
        page_size: u32,
    ) -> Result<ActiveDirectoryDirSyncRequest, DirectoryAdapterError> {
        validate_cursor(cursor)?;
        validate_page_size(page_size)?;
        Ok(ActiveDirectoryDirSyncRequest {
            naming_context: self.naming_context.clone(),
            attributes: self.attributes.clone(),
            cookie: cursor.to_vec(),
            object_security: true,
            incremental_values: true,
            link_values: true,
            page_size,
            starttls_required: self.config.binding().tls_mode() == DirectoryTlsMode::StartTls,
        })
    }

    pub fn parse_response(
        &self,
        body: &[u8],
        request: &ActiveDirectoryDirSyncRequest,
        full_snapshot: bool,
    ) -> Result<DirectoryAdapterPage, DirectoryAdapterError> {
        self.validate_request(request)?;
        let response: ActiveDirectoryDirSyncWireResponse = decode_bounded(body)?;
        let entries = response
            .entries
            .ok_or(DirectoryAdapterError::MalformedResponse)?;
        let cookie = response
            .cookie
            .ok_or(DirectoryAdapterError::MalformedResponse)?;
        let more_data = response
            .more_data
            .ok_or(DirectoryAdapterError::MalformedResponse)?;
        if more_data {
            validate_continuation_cursor(&cookie, &request.cookie)?;
        } else {
            validate_cursor(&cookie)?;
            // A non-snapshot change page with entries must carry the native
            // DirSync cookie that makes the transaction replayable. A
            // terminal full-import page may legally omit it.
            if !full_snapshot && !entries.is_empty() {
                validate_continuation_cursor(&cookie, &request.cookie)?;
            }
        }
        validate_entries(&entries, request.page_size, full_snapshot)?;
        let complete = !more_data;
        Ok(DirectoryAdapterPage {
            entries,
            // AD may return its final native cookie with moreData=false; do
            // not throw away the checkpoint needed by the next change poll.
            next_cursor: (!cookie.is_empty()).then_some(DirectoryAdapterCursor::Opaque(cookie)),
            complete,
        })
    }

    pub const fn binding(&self) -> &DirectoryAdapterBinding {
        self.config.binding()
    }

    fn validate_request(
        &self,
        request: &ActiveDirectoryDirSyncRequest,
    ) -> Result<(), DirectoryAdapterError> {
        validate_search_parameter(&request.naming_context)?;
        validate_attributes(&request.attributes)?;
        validate_cursor(&request.cookie)?;
        validate_page_size(request.page_size)?;
        if request.naming_context != self.naming_context
            || request.attributes != self.attributes
            || !request.object_security
            || !request.incremental_values
            || !request.link_values
            || request.starttls_required
                != (self.config.binding().tls_mode() == DirectoryTlsMode::StartTls)
        {
            return Err(DirectoryAdapterError::ProtocolRejected);
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct ActiveDirectoryDirSyncWireResponse {
    entries: Option<Vec<DirectoryAdapterEntry>>,
    cookie: Option<Vec<u8>>,
    more_data: Option<bool>,
}

/// UCS Provisioning Service request.  UCS is the only directory transport in
/// this packet with an HTTP wire client because `reqwest` is already a worker
/// dependency and the service is HTTPS-based.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UcsProvisioningServiceRequest {
    pub position: String,
    pub token: String,
    pub sequence: u64,
    pub limit: u32,
    pub full_snapshot: bool,
}

#[derive(Debug)]
pub struct UcsProvisioningServiceAdapter {
    config: DirectoryAdapterConfig,
    position: String,
    client: Client,
}

impl UcsProvisioningServiceAdapter {
    pub fn new(
        config: DirectoryAdapterConfig,
        position: impl Into<String>,
    ) -> Result<Self, DirectoryAdapterError> {
        if config.binding().provider() != DirectoryProvider::UniventionCorporateServer
            || config.binding().feed() != DirectoryFeedKind::UcsProvisioningService
        {
            return Err(DirectoryAdapterError::UnsupportedProviderFeed);
        }
        let position = position.into();
        validate_search_parameter(&position)?;
        let client = Client::builder()
            .use_rustls_tls()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(DIRECTORY_ADAPTER_TIMEOUT)
            .build()
            .map_err(|_| DirectoryAdapterError::InvalidConfiguration)?;
        Ok(Self {
            config,
            position,
            client,
        })
    }

    pub fn from_env(
        position: impl Into<String>,
        endpoint_env: &str,
        credential_env: &str,
    ) -> Result<Self, DirectoryAdapterError> {
        Self::new(
            DirectoryAdapterConfig::from_env(
                DirectoryProvider::UniventionCorporateServer,
                DirectoryFeedKind::UcsProvisioningService,
                DirectoryTlsMode::Https,
                endpoint_env,
                credential_env,
            )?,
            position,
        )
    }

    pub fn build_request(
        &self,
        token: &str,
        sequence: u64,
        page_size: u32,
        full_snapshot: bool,
    ) -> Result<UcsProvisioningServiceRequest, DirectoryAdapterError> {
        validate_token(token)?;
        // UCS starts a new provisioning stream at (empty token, sequence 0).
        // Once a token exists, sequence zero is a stale or malformed
        // continuation and must fail closed.
        if sequence == 0 && !token.is_empty() {
            return Err(DirectoryAdapterError::ProtocolRejected);
        }
        validate_page_size(page_size)?;
        Ok(UcsProvisioningServiceRequest {
            position: self.position.clone(),
            token: token.to_owned(),
            sequence,
            limit: page_size,
            full_snapshot,
        })
    }

    /// Build an HTTPS request with a sensitive bearer header.  The client is
    /// rustls-backed, disables redirects, and rejects non-HTTPS URLs; no
    /// credential is placed in the URL or JSON body.
    pub fn build_http_request(
        &self,
        request: &UcsProvisioningServiceRequest,
    ) -> Result<RequestBuilder, DirectoryAdapterError> {
        self.validate_request(request)?;
        let payload =
            serde_json::to_vec(request).map_err(|_| DirectoryAdapterError::InvalidConfiguration)?;
        let mut authorization = Vec::with_capacity(7 + self.config.credential().as_bytes().len());
        authorization.extend_from_slice(b"Bearer ");
        authorization.extend_from_slice(self.config.credential().as_bytes());
        let authorization = header::HeaderValue::from_bytes(&authorization)
            .map_err(|_| DirectoryAdapterError::InvalidCredential)?;
        let mut authorization = authorization;
        authorization.set_sensitive(true);
        Ok(self
            .client
            .post(self.config.binding().endpoint().clone())
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, authorization)
            .body(payload))
    }

    /// Execute one bounded UCS request.  No response body or transport detail
    /// is included in errors, and the declared/observed response size is
    /// bounded before the parsed envelope is handed to the feed layer.
    pub async fn execute(
        &self,
        request: &UcsProvisioningServiceRequest,
    ) -> Result<DirectoryAdapterPage, DirectoryAdapterError> {
        let mut response = self
            .build_http_request(request)?
            .send()
            .await
            .map_err(|_| DirectoryAdapterError::TransportUnavailable)?;
        if !response.status().is_success() {
            return Err(DirectoryAdapterError::HttpRejected);
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_DIRECTORY_ADAPTER_RESPONSE_BYTES as u64)
        {
            return Err(DirectoryAdapterError::ResponseTooLarge);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| DirectoryAdapterError::TransportUnavailable)?
        {
            let next_len = body
                .len()
                .checked_add(chunk.len())
                .ok_or(DirectoryAdapterError::ResponseTooLarge)?;
            if next_len > MAX_DIRECTORY_ADAPTER_RESPONSE_BYTES {
                return Err(DirectoryAdapterError::ResponseTooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        self.parse_response(&body, request)
    }

    pub fn parse_response(
        &self,
        body: &[u8],
        request: &UcsProvisioningServiceRequest,
    ) -> Result<DirectoryAdapterPage, DirectoryAdapterError> {
        self.validate_request(request)?;
        let response: UcsProvisioningServiceWireResponse = decode_bounded(body)?;
        let entries = response
            .entries
            .ok_or(DirectoryAdapterError::MalformedResponse)?;
        let token = response
            .token
            .ok_or(DirectoryAdapterError::MalformedResponse)?;
        let sequence = response
            .sequence
            .ok_or(DirectoryAdapterError::MalformedResponse)?;
        let end_of_snapshot = response
            .end_of_snapshot
            .ok_or(DirectoryAdapterError::MalformedResponse)?;
        let has_more = response
            .has_more
            .ok_or(DirectoryAdapterError::MalformedResponse)?;
        if sequence == 0 {
            return Err(DirectoryAdapterError::ProtocolRejected);
        }
        validate_token(&token)?;
        validate_entries(&entries, request.limit, request.full_snapshot)?;
        let complete = end_of_snapshot && !has_more;
        if request.sequence == 0 {
            // The first UCS message may have any positive source sequence;
            // there is no local predecessor from which to derive a number.
        } else if complete {
            // A terminal snapshot marker may repeat the last acknowledged
            // source position, or advance exactly once. A gap would make a
            // restart skip an uncommitted provisioning message.
            let next_sequence = request
                .sequence
                .checked_add(1)
                .ok_or(DirectoryAdapterError::ProtocolRejected)?;
            let same_position = sequence == request.sequence && token == request.token;
            if sequence != next_sequence && !same_position {
                return Err(DirectoryAdapterError::ProtocolRejected);
            }
        } else {
            let next_sequence = request
                .sequence
                .checked_add(1)
                .ok_or(DirectoryAdapterError::ProtocolRejected)?;
            if sequence != next_sequence {
                return Err(DirectoryAdapterError::ProtocolRejected);
            }
        }
        Ok(DirectoryAdapterPage {
            entries,
            // UCS' terminal marker still carries the source sequence/token
            // needed to resume the native stream after snapshot finalization.
            next_cursor: Some(DirectoryAdapterCursor::Ucs { token, sequence }),
            complete,
        })
    }

    pub const fn binding(&self) -> &DirectoryAdapterBinding {
        self.config.binding()
    }

    fn validate_request(
        &self,
        request: &UcsProvisioningServiceRequest,
    ) -> Result<(), DirectoryAdapterError> {
        validate_search_parameter(&request.position)?;
        validate_token(&request.token)?;
        validate_page_size(request.limit)?;
        if request.position != self.position || (request.sequence == 0 && !request.token.is_empty())
        {
            return Err(DirectoryAdapterError::ProtocolRejected);
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct UcsProvisioningServiceWireResponse {
    entries: Option<Vec<DirectoryAdapterEntry>>,
    token: Option<String>,
    sequence: Option<u64>,
    end_of_snapshot: Option<bool>,
    has_more: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credential() -> Vec<u8> {
        b"directory-live-credential".to_vec()
    }

    fn binding(
        provider: DirectoryProvider,
        feed: DirectoryFeedKind,
        endpoint: &str,
        tls_mode: DirectoryTlsMode,
    ) -> DirectoryAdapterBinding {
        DirectoryAdapterBinding::try_new(provider, feed, endpoint, tls_mode).expect("binding")
    }

    fn entry(operation: DirectoryAdapterOperation) -> DirectoryAdapterEntry {
        DirectoryAdapterEntry {
            external_id: "directory-id-1".into(),
            immutable_id: "immutable-id-1".into(),
            operation,
            login_name: Some("alice".into()),
        }
    }

    #[test]
    fn provider_and_feed_binding_is_closed() {
        let generic = binding(
            DirectoryProvider::GenericLdap,
            DirectoryFeedKind::Rfc4533,
            "ldaps://ldap.example.test",
            DirectoryTlsMode::Ldaps,
        );
        let config = DirectoryAdapterConfig::try_new(generic, credential()).expect("config");
        let adapter =
            GenericLdapRfc4533Adapter::new(config, "dc=example,dc=test", vec!["uid".into()])
                .expect("generic RFC 4533 adapter");
        assert_eq!(adapter.binding().provider(), DirectoryProvider::GenericLdap);
        assert_eq!(adapter.binding().feed(), DirectoryFeedKind::Rfc4533);

        let wrong = DirectoryAdapterBinding::try_new(
            DirectoryProvider::GenericLdap,
            DirectoryFeedKind::AdDirSync,
            "ldaps://ldap.example.test",
            DirectoryTlsMode::Ldaps,
        );
        assert_eq!(wrong, Err(DirectoryAdapterError::UnsupportedProviderFeed));
    }

    #[test]
    fn endpoints_are_tls_only_and_starttls_is_explicit() {
        assert_eq!(
            DirectoryAdapterBinding::try_new(
                DirectoryProvider::UniventionCorporateServer,
                DirectoryFeedKind::UcsProvisioningService,
                "http://ucs.example.test/provision",
                DirectoryTlsMode::Https,
            ),
            Err(DirectoryAdapterError::InsecureTransport)
        );
        assert_eq!(
            DirectoryAdapterBinding::try_new(
                DirectoryProvider::GenericLdap,
                DirectoryFeedKind::Rfc4533,
                "ldap://ldap.example.test",
                DirectoryTlsMode::Ldaps,
            ),
            Err(DirectoryAdapterError::InsecureTransport)
        );
        let starttls = DirectoryAdapterBinding::try_new(
            DirectoryProvider::GenericLdap,
            DirectoryFeedKind::Rfc4533,
            "ldap://ldap.example.test",
            DirectoryTlsMode::StartTls,
        )
        .expect("explicit StartTLS binding");
        assert_eq!(starttls.tls_mode(), DirectoryTlsMode::StartTls);

        assert_eq!(
            DirectoryAdapterBinding::try_new(
                DirectoryProvider::GenericLdap,
                DirectoryFeedKind::Rfc4533,
                "ldaps://ldap.example.test/directory",
                DirectoryTlsMode::Ldaps,
            ),
            Err(DirectoryAdapterError::InvalidEndpoint)
        );
        assert_eq!(
            DirectoryAdapterBinding::try_new(
                DirectoryProvider::GenericLdap,
                DirectoryFeedKind::Rfc4533,
                "ldaps://ldap.example.test/",
                DirectoryTlsMode::Ldaps,
            ),
            Err(DirectoryAdapterError::InvalidEndpoint)
        );
        assert_eq!(
            DirectoryAdapterBinding::try_new(
                DirectoryProvider::GenericLdap,
                DirectoryFeedKind::Rfc4533,
                "ldaps://ldap.example.test:0",
                DirectoryTlsMode::Ldaps,
            ),
            Err(DirectoryAdapterError::InvalidEndpoint)
        );
        assert_eq!(
            DirectoryAdapterBinding::try_new(
                DirectoryProvider::UniventionCorporateServer,
                DirectoryFeedKind::UcsProvisioningService,
                "https://ucs.example.test/provision?insecure=false",
                DirectoryTlsMode::Https,
            ),
            Err(DirectoryAdapterError::InvalidEndpoint)
        );
    }

    #[test]
    fn tls_policy_rejects_invalid_anchor_and_mismatched_peer_evidence() {
        let binding = binding(
            DirectoryProvider::ActiveDirectoryDomainServices,
            DirectoryFeedKind::AdDirSync,
            "ldaps://ad.example.test",
            DirectoryTlsMode::Ldaps,
        );
        let policy = binding
            .tls_policy("ad-ca-v1", "a".repeat(64), 1)
            .expect("TLS policy");
        assert_eq!(policy.mode(), DirectoryTransportMode::Ldaps);
        assert_eq!(policy.server_name(), "ad.example.test");

        assert_eq!(
            binding.tls_policy("ad-ca-v1", "not-a-fingerprint", 1),
            Err(DirectoryAdapterError::TrustValidationFailed)
        );

        let anchor = TrustAnchor::try_new(
            "ad-ca-v1",
            CertificateFingerprint::try_new("a".repeat(64)).expect("fingerprint"),
            1,
        )
        .expect("anchor");
        let evidence = TlsHandshakeEvidence::from_verified_peer(
            DirectoryTransportMode::Ldaps,
            "ad.example.test",
            erpx_identity::directory::tls::TlsProtocolVersion::Tls1_3,
            b"peer-certificate-from-another-anchor",
            &anchor,
        )
        .expect("verified evidence");
        assert_eq!(
            binding.validate_tls_handshake(&policy, &evidence),
            Err(DirectoryAdapterError::TrustValidationFailed)
        );
    }

    #[test]
    fn search_parameters_are_bounded_for_each_native_adapter() {
        let oversized = "x".repeat(MAX_DIRECTORY_ADAPTER_SEARCH_PARAMETER_BYTES + 1);

        let generic = DirectoryAdapterConfig::try_new(
            binding(
                DirectoryProvider::GenericLdap,
                DirectoryFeedKind::Rfc4533,
                "ldaps://ldap.example.test",
                DirectoryTlsMode::Ldaps,
            ),
            credential(),
        )
        .expect("generic config");
        assert!(matches!(
            GenericLdapRfc4533Adapter::new(generic, oversized.clone(), vec!["uid".into()]),
            Err(DirectoryAdapterError::SearchParameterOutOfBounds)
        ));

        let ad = DirectoryAdapterConfig::try_new(
            binding(
                DirectoryProvider::ActiveDirectoryDomainServices,
                DirectoryFeedKind::AdDirSync,
                "ldaps://ad.example.test",
                DirectoryTlsMode::Ldaps,
            ),
            credential(),
        )
        .expect("AD config");
        assert!(matches!(
            ActiveDirectoryDirSyncAdapter::new(ad, oversized.clone(), vec!["objectGUID".into()]),
            Err(DirectoryAdapterError::SearchParameterOutOfBounds)
        ));

        let ucs = DirectoryAdapterConfig::try_new(
            binding(
                DirectoryProvider::UniventionCorporateServer,
                DirectoryFeedKind::UcsProvisioningService,
                "https://ucs.example.test/provision",
                DirectoryTlsMode::Https,
            ),
            credential(),
        )
        .expect("UCS config");
        assert!(matches!(
            UcsProvisioningServiceAdapter::new(ucs, oversized),
            Err(DirectoryAdapterError::SearchParameterOutOfBounds)
        ));
    }

    #[test]
    fn wire_envelopes_require_authoritative_native_fields() {
        let generic = GenericLdapRfc4533Adapter::new(
            DirectoryAdapterConfig::try_new(
                binding(
                    DirectoryProvider::GenericLdap,
                    DirectoryFeedKind::Rfc4533,
                    "ldaps://ldap.example.test",
                    DirectoryTlsMode::Ldaps,
                ),
                credential(),
            )
            .expect("generic config"),
            "dc=example,dc=test",
            vec!["uid".into()],
        )
        .expect("generic adapter");
        let generic_request = generic
            .build_request(&[], 10, Rfc4533RequestMode::RefreshOnly)
            .expect("generic request");
        let missing_rfc_cookie = serde_json::to_vec(&serde_json::json!({
            "entries": [],
            "refresh_done": true,
            "refresh_required": false
        }))
        .expect("RFC response");
        assert_eq!(
            generic.parse_response(&missing_rfc_cookie, &generic_request),
            Err(DirectoryAdapterError::MalformedResponse)
        );

        let ad = ActiveDirectoryDirSyncAdapter::new(
            DirectoryAdapterConfig::try_new(
                binding(
                    DirectoryProvider::ActiveDirectoryDomainServices,
                    DirectoryFeedKind::AdDirSync,
                    "ldaps://ad.example.test",
                    DirectoryTlsMode::Ldaps,
                ),
                credential(),
            )
            .expect("AD config"),
            "DC=example,DC=test",
            vec!["objectGUID".into()],
        )
        .expect("AD adapter");
        let ad_request = ad.build_request(&[], 10).expect("AD request");
        let missing_ad_cookie = serde_json::to_vec(&serde_json::json!({
            "entries": [],
            "more_data": false
        }))
        .expect("AD response");
        assert_eq!(
            ad.parse_response(&missing_ad_cookie, &ad_request, true),
            Err(DirectoryAdapterError::MalformedResponse)
        );

        let ucs = UcsProvisioningServiceAdapter::new(
            DirectoryAdapterConfig::try_new(
                binding(
                    DirectoryProvider::UniventionCorporateServer,
                    DirectoryFeedKind::UcsProvisioningService,
                    "https://ucs.example.test/provision",
                    DirectoryTlsMode::Https,
                ),
                credential(),
            )
            .expect("UCS config"),
            "users",
        )
        .expect("UCS adapter");
        let ucs_request = ucs.build_request("", 0, 10, true).expect("UCS request");
        let missing_ucs_terminal_flags = serde_json::to_vec(&serde_json::json!({
            "entries": [],
            "token": "next-token",
            "sequence": 1
        }))
        .expect("UCS response");
        assert_eq!(
            ucs.parse_response(&missing_ucs_terminal_flags, &ucs_request),
            Err(DirectoryAdapterError::MalformedResponse)
        );

        assert_eq!(
            decode_bounded::<serde_json::Value>(&vec![
                b' ';
                MAX_DIRECTORY_ADAPTER_RESPONSE_BYTES + 1
            ]),
            Err(DirectoryAdapterError::ResponseTooLarge)
        );
    }

    #[test]
    fn pages_and_cursors_are_bounded_before_transport_handoff() {
        let config = DirectoryAdapterConfig::try_new(
            binding(
                DirectoryProvider::ActiveDirectoryDomainServices,
                DirectoryFeedKind::AdDirSync,
                "ldaps://ad.example.test",
                DirectoryTlsMode::Ldaps,
            ),
            credential(),
        )
        .expect("config");
        let adapter = ActiveDirectoryDirSyncAdapter::new(
            config,
            "DC=example,DC=test",
            vec!["objectGUID".into(), "sAMAccountName".into()],
        )
        .expect("AD adapter");
        assert_eq!(
            adapter.build_request(&[], 0),
            Err(DirectoryAdapterError::PageSizeOutOfBounds)
        );
        assert_eq!(
            adapter.build_request(&[], MAX_DIRECTORY_ADAPTER_PAGE_SIZE + 1),
            Err(DirectoryAdapterError::PageSizeOutOfBounds)
        );
        assert_eq!(
            adapter.build_request(&vec![0_u8; MAX_DIRECTORY_ADAPTER_CURSOR_BYTES + 1], 100,),
            Err(DirectoryAdapterError::InvalidCursor)
        );

        let request = adapter.build_request(&[], 1).expect("request");
        let response = serde_json::to_vec(&serde_json::json!({
            "entries": [entry(DirectoryAdapterOperation::Upsert), entry(DirectoryAdapterOperation::Upsert)],
            "cookie": [1, 2, 3],
            "more_data": true
        }))
        .expect("response");
        assert_eq!(
            adapter.parse_response(&response, &request, false),
            Err(DirectoryAdapterError::PageSizeOutOfBounds)
        );
    }

    #[test]
    fn provider_parsers_validate_native_cursor_progression() {
        let config = DirectoryAdapterConfig::try_new(
            binding(
                DirectoryProvider::UniventionCorporateServer,
                DirectoryFeedKind::UcsProvisioningService,
                "https://ucs.example.test/provision",
                DirectoryTlsMode::Https,
            ),
            credential(),
        )
        .expect("config");
        let adapter = UcsProvisioningServiceAdapter::new(config, "users").expect("UCS adapter");
        let request = adapter
            .build_request("old-token", 4, 10, false)
            .expect("request");
        let mut disabled_entry = entry(DirectoryAdapterOperation::Disable);
        disabled_entry.login_name = None;
        let response = serde_json::to_vec(&serde_json::json!({
            "entries": [disabled_entry],
            "token": "new-token",
            "sequence": 5,
            "end_of_snapshot": false,
            "has_more": true
        }))
        .expect("response");
        let page = adapter.parse_response(&response, &request).expect("page");
        assert_eq!(
            page.next_cursor,
            Some(DirectoryAdapterCursor::Ucs {
                token: "new-token".into(),
                sequence: 5
            })
        );

        let stale = serde_json::to_vec(&serde_json::json!({
            "entries": [],
            "token": "stale-token",
            "sequence": 3,
            "end_of_snapshot": false,
            "has_more": true
        }))
        .expect("stale response");
        assert_eq!(
            adapter.parse_response(&stale, &request),
            Err(DirectoryAdapterError::ProtocolRejected)
        );

        let gap = serde_json::to_vec(&serde_json::json!({
            "entries": [],
            "token": "gap-token",
            "sequence": 6,
            "end_of_snapshot": false,
            "has_more": true
        }))
        .expect("gapped response");
        assert_eq!(
            adapter.parse_response(&gap, &request),
            Err(DirectoryAdapterError::ProtocolRejected)
        );

        let rotated_same_sequence = serde_json::to_vec(&serde_json::json!({
            "entries": [],
            "token": "rotated-token",
            "sequence": 4,
            "end_of_snapshot": false,
            "has_more": true
        }))
        .expect("rotated response");
        assert_eq!(
            adapter.parse_response(&rotated_same_sequence, &request),
            Err(DirectoryAdapterError::ProtocolRejected)
        );
    }

    #[test]
    fn ucs_requests_and_responses_require_positive_sequences() {
        let config = DirectoryAdapterConfig::try_new(
            binding(
                DirectoryProvider::UniventionCorporateServer,
                DirectoryFeedKind::UcsProvisioningService,
                "https://ucs.example.test/provision",
                DirectoryTlsMode::Https,
            ),
            credential(),
        )
        .expect("config");
        let adapter = UcsProvisioningServiceAdapter::new(config, "users").expect("adapter");
        assert_eq!(
            adapter.build_request("", 0, 10, false),
            Ok(UcsProvisioningServiceRequest {
                position: "users".into(),
                token: String::new(),
                sequence: 0,
                limit: 10,
                full_snapshot: false,
            })
        );
        let request = adapter
            .build_request("initial-token", 1, 10, false)
            .expect("positive sequence request");
        let response = serde_json::to_vec(&serde_json::json!({
            "entries": [],
            "token": "next-token",
            "sequence": 0,
            "end_of_snapshot": true,
            "has_more": false
        }))
        .expect("response");
        assert_eq!(
            adapter.parse_response(&response, &request),
            Err(DirectoryAdapterError::ProtocolRejected)
        );
        assert_eq!(
            adapter.build_request("stale-token", 0, 10, false),
            Err(DirectoryAdapterError::ProtocolRejected)
        );
    }

    #[test]
    fn full_snapshots_retain_revocation_operations() {
        let mut disabled = entry(DirectoryAdapterOperation::Disable);
        disabled.login_name = None;
        let mut deleted = entry(DirectoryAdapterOperation::Delete);
        deleted.external_id = "directory-id-2".into();
        deleted.immutable_id = "immutable-id-2".into();
        deleted.login_name = None;
        assert!(validate_entries(&[disabled, deleted], 2, true).is_ok());
    }

    #[test]
    fn errors_and_debug_output_are_redacted() {
        let secret = "super-secret-token";
        let credential = DirectoryCredential::try_new(secret).expect("credential");
        let debug = format!("{credential:?}");
        assert!(!debug.contains(secret));

        let error = DirectoryAdapterBinding::try_new(
            DirectoryProvider::UniventionCorporateServer,
            DirectoryFeedKind::UcsProvisioningService,
            "http://user:super-secret-token@ucs.example.test/provision",
            DirectoryTlsMode::Https,
        )
        .expect_err("plaintext and URL credentials are rejected");
        let rendered = format!("{error:?} {error}");
        assert!(!rendered.contains("super-secret-token"));
        assert!(!rendered.contains("ucs.example.test"));
    }

    #[test]
    fn response_entries_reject_control_characters_in_login_names() {
        let mut value = entry(DirectoryAdapterOperation::Upsert);
        value.login_name = Some("alice\nadmin".into());
        assert_eq!(
            value.validate(),
            Err(DirectoryAdapterError::MalformedResponse)
        );
    }

    #[test]
    fn response_entries_reject_whitespace_only_identity_fields() {
        let mut external = entry(DirectoryAdapterOperation::Upsert);
        external.external_id = " \t".into();
        assert_eq!(
            external.validate(),
            Err(DirectoryAdapterError::MalformedResponse)
        );

        let mut login = entry(DirectoryAdapterOperation::Upsert);
        login.login_name = Some(" \n".into());
        assert_eq!(
            login.validate(),
            Err(DirectoryAdapterError::MalformedResponse)
        );
    }

    #[test]
    fn disable_and_delete_entries_reject_mutable_login_fields() {
        for operation in [
            DirectoryAdapterOperation::Disable,
            DirectoryAdapterOperation::Delete,
        ] {
            let mut value = entry(operation);
            value.login_name = None;
            assert!(value.validate().is_ok());

            value.login_name = Some("stale-mutable-login".into());
            assert_eq!(
                value.validate(),
                Err(DirectoryAdapterError::MalformedResponse)
            );
        }
    }

    #[test]
    fn provider_pages_reject_duplicate_external_or_immutable_links() {
        let first = entry(DirectoryAdapterOperation::Upsert);

        let mut duplicate_external = first.clone();
        duplicate_external.immutable_id = "immutable-id-2".into();
        assert_eq!(
            validate_entries(&[first.clone(), duplicate_external], 2, false),
            Err(DirectoryAdapterError::ProtocolRejected)
        );

        let mut duplicate_immutable = first;
        duplicate_immutable.external_id = "directory-id-2".into();
        assert_eq!(
            validate_entries(
                &[
                    entry(DirectoryAdapterOperation::Upsert),
                    duplicate_immutable
                ],
                2,
                false
            ),
            Err(DirectoryAdapterError::ProtocolRejected)
        );
    }

    #[test]
    fn non_terminal_provider_pages_must_advance_a_native_cursor() {
        let generic_config = DirectoryAdapterConfig::try_new(
            binding(
                DirectoryProvider::GenericLdap,
                DirectoryFeedKind::Rfc4533,
                "ldaps://ldap.example.test",
                DirectoryTlsMode::Ldaps,
            ),
            credential(),
        )
        .expect("generic config");
        let generic = GenericLdapRfc4533Adapter::new(
            generic_config,
            "dc=example,dc=test",
            vec!["uid".into()],
        )
        .expect("generic adapter");
        let request = generic
            .build_request(&[1, 2], 10, Rfc4533RequestMode::RefreshOnly)
            .expect("generic request");
        let repeated = serde_json::to_vec(&serde_json::json!({
            "entries": [],
            "cookie": [1, 2],
            "refresh_done": false,
            "refresh_required": false
        }))
        .expect("generic response");
        assert_eq!(
            generic.parse_response(&repeated, &request),
            Err(DirectoryAdapterError::ProtocolRejected)
        );

        let ad_config = DirectoryAdapterConfig::try_new(
            binding(
                DirectoryProvider::ActiveDirectoryDomainServices,
                DirectoryFeedKind::AdDirSync,
                "ldaps://ad.example.test",
                DirectoryTlsMode::Ldaps,
            ),
            credential(),
        )
        .expect("AD config");
        let ad = ActiveDirectoryDirSyncAdapter::new(
            ad_config,
            "DC=example,DC=test",
            vec!["objectGUID".into()],
        )
        .expect("AD adapter");
        let ad_request = ad.build_request(&[3], 10).expect("AD request");
        let empty_ad_cookie = serde_json::to_vec(&serde_json::json!({
            "entries": [],
            "cookie": [],
            "more_data": true
        }))
        .expect("AD response");
        assert_eq!(
            ad.parse_response(&empty_ad_cookie, &ad_request, false),
            Err(DirectoryAdapterError::ProtocolRejected)
        );
    }

    #[test]
    fn terminal_native_cookies_are_retained_and_change_rows_need_one() {
        let generic_config = DirectoryAdapterConfig::try_new(
            binding(
                DirectoryProvider::GenericLdap,
                DirectoryFeedKind::Rfc4533,
                "ldaps://ldap.example.test",
                DirectoryTlsMode::Ldaps,
            ),
            credential(),
        )
        .expect("generic config");
        let generic = GenericLdapRfc4533Adapter::new(
            generic_config,
            "dc=example,dc=test",
            vec!["uid".into()],
        )
        .expect("generic adapter");
        let refresh_request = generic
            .build_request(&[1], 10, Rfc4533RequestMode::RefreshOnly)
            .expect("refresh request");
        let terminal = serde_json::to_vec(&serde_json::json!({
            "entries": [entry(DirectoryAdapterOperation::Upsert)],
            "cookie": [2],
            "refresh_done": true,
            "refresh_required": false
        }))
        .expect("terminal refresh response");
        let page = generic
            .parse_response(&terminal, &refresh_request)
            .expect("terminal refresh page");
        assert!(page.complete);
        assert_eq!(
            page.next_cursor,
            Some(DirectoryAdapterCursor::Opaque(vec![2]))
        );

        let change_request = generic
            .build_request(&[1], 10, Rfc4533RequestMode::RefreshAndPersist)
            .expect("change request");
        let missing_cookie = serde_json::to_vec(&serde_json::json!({
            "entries": [entry(DirectoryAdapterOperation::Upsert)],
            "cookie": [],
            "refresh_done": true,
            "refresh_required": false
        }))
        .expect("cookie-less change response");
        assert_eq!(
            generic.parse_response(&missing_cookie, &change_request),
            Err(DirectoryAdapterError::ProtocolRejected)
        );

        let ad_config = DirectoryAdapterConfig::try_new(
            binding(
                DirectoryProvider::ActiveDirectoryDomainServices,
                DirectoryFeedKind::AdDirSync,
                "ldaps://ad.example.test",
                DirectoryTlsMode::Ldaps,
            ),
            credential(),
        )
        .expect("AD config");
        let ad = ActiveDirectoryDirSyncAdapter::new(
            ad_config,
            "DC=example,DC=test",
            vec!["objectGUID".into()],
        )
        .expect("AD adapter");
        let ad_request = ad.build_request(&[1], 10).expect("AD request");
        let terminal_ad = serde_json::to_vec(&serde_json::json!({
            "entries": [entry(DirectoryAdapterOperation::Upsert)],
            "cookie": [2],
            "more_data": false
        }))
        .expect("terminal AD response");
        let page = ad
            .parse_response(&terminal_ad, &ad_request, true)
            .expect("terminal AD page");
        assert!(page.complete);
        assert_eq!(
            page.next_cursor,
            Some(DirectoryAdapterCursor::Opaque(vec![2]))
        );
        let missing_cookie_ad = serde_json::to_vec(&serde_json::json!({
            "entries": [entry(DirectoryAdapterOperation::Upsert)],
            "cookie": [],
            "more_data": false
        }))
        .expect("cookie-less AD response");
        assert_eq!(
            ad.parse_response(&missing_cookie_ad, &ad_request, false),
            Err(DirectoryAdapterError::ProtocolRejected)
        );

        let ucs_config = DirectoryAdapterConfig::try_new(
            binding(
                DirectoryProvider::UniventionCorporateServer,
                DirectoryFeedKind::UcsProvisioningService,
                "https://ucs.example.test/provision",
                DirectoryTlsMode::Https,
            ),
            credential(),
        )
        .expect("UCS config");
        let ucs = UcsProvisioningServiceAdapter::new(ucs_config, "users").expect("UCS adapter");
        let ucs_request = ucs
            .build_request("old-token", 4, 10, true)
            .expect("UCS snapshot request");
        let terminal_ucs = serde_json::to_vec(&serde_json::json!({
            "entries": [],
            "token": "old-token",
            "sequence": 4,
            "end_of_snapshot": true,
            "has_more": false
        }))
        .expect("terminal UCS response");
        let page = ucs
            .parse_response(&terminal_ucs, &ucs_request)
            .expect("terminal UCS page");
        assert!(page.complete);
        assert_eq!(
            page.next_cursor,
            Some(DirectoryAdapterCursor::Ucs {
                token: "old-token".into(),
                sequence: 4,
            })
        );
    }

    #[test]
    fn missing_live_environment_material_fails_closed() {
        let result = DirectoryAdapterConfig::from_env(
            DirectoryProvider::GenericLdap,
            DirectoryFeedKind::Rfc4533,
            DirectoryTlsMode::Ldaps,
            "ERPX_TEST_DIRECTORY_ENDPOINT_MISSING",
            "ERPX_TEST_DIRECTORY_CREDENTIAL_MISSING",
        );
        assert!(matches!(
            result,
            Err(DirectoryAdapterError::EndpointUnavailable)
        ));
    }
}
