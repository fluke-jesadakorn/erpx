//! Live, TLS-only directory transports.
//!
//! This module is intentionally small at the worker boundary: protocol
//! decoding produces the existing provider-neutral `DirectorySyncPage`, and
//! the worker then commits that page through `SqlxDirectoryStore`. No
//! transport result is acknowledged before that SQL transaction succeeds.

use std::{collections::BTreeSet, env, panic::AssertUnwindSafe, sync::Arc, time::Duration};

use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use chrono::Utc;
use erpx_identity::directory::auth::{
    AccessRisk, AuthenticatedDirectoryIdentity, BreakGlassPolicy, DirectoryAccountStatus,
    DirectoryAuthenticationRequest, DirectoryAuthenticator, DirectoryBindRequest,
    DirectoryBindResult, DirectoryConnectorConfig as AuthConnectorConfig,
    DirectoryIdentity as AuthDirectoryIdentity, DirectoryProvider as AuthDirectoryProvider,
    DirectoryTransport, DirectoryTransportError, DirectoryUsername, NativeChangeFeed,
    RedactedSecret,
};
use erpx_identity::directory::tls::{
    CertificateFingerprint, TimeoutPolicy, TlsHandshakeEvidence, TlsPolicy, TlsProtocolVersion,
    TrustAnchor,
};
use erpx_identity::{
    DirectoryChange, DirectoryConnector, DirectoryFeedKind, DirectoryProvider, DirectorySyncCursor,
    DirectorySyncPage, DirectoryUser,
};
use ldap3::{
    LdapConnAsync, LdapConnSettings, ResultEntry, Scope, SearchEntry,
    controls::{
        Control, EntryState, PagedResults, RawControl, RefreshMode, SyncInfo, SyncRequest,
        SyncState, parse_syncinfo,
    },
};
use reqwest::{Client, Url, header};
use rustls::{ClientConfig, RootCertStore, pki_types::CertificateDer};
use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;

use crate::directory_adapters::{
    MAX_DIRECTORY_ADAPTER_CURSOR_BYTES, MAX_DIRECTORY_ADAPTER_RESPONSE_BYTES,
};

const MAX_PAGE_SIZE: u32 = 5_000;
const LDAP_TIMEOUT: Duration = Duration::from_secs(30);
const AD_DIRSYNC_OID: &str = "1.2.840.113556.1.4.841";
const LDAP_PAGED_RESULTS_OID: &str = "1.2.840.113556.1.4.319";
const RFC4533_SYNC_REQUEST_OID: &str = "1.3.6.1.4.1.4203.1.9.1.1";
const RFC4533_SYNC_STATE_OID: &str = "1.3.6.1.4.1.4203.1.9.1.2";
const RFC4533_SYNC_DONE_OID: &str = "1.3.6.1.4.1.4203.1.9.1.3";
const RFC4533_FULL_IMPORT_CURSOR_MAGIC: &[u8] = b"ERPX-RFC4533-PAGED\x01";
const AD_SHOW_DELETED_OID: &str = "1.2.840.113556.1.4.417";
const AD_EXTENDED_DN_OID: &str = "1.2.840.113556.1.4.529";
const DIRECTORY_CA_CERT_PATH_ENV: &str = "ERPX_DIRECTORY_CA_CERT_PATH";
const MAX_CA_CERT_PATH_BYTES: usize = 4_096;
const MAX_CA_CERTIFICATE_BYTES: usize = 128 * 1024;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LiveDirectoryError {
    #[error("directory live configuration is incomplete")]
    Configuration,
    #[error("directory live transport failed")]
    Transport,
    #[error("directory live protocol response was rejected")]
    Protocol,
    #[error("directory live response exceeded its bound")]
    ResponseTooLarge,
    #[error(
        "directory TLS peer trust evidence is unavailable or does not match the persisted anchor"
    )]
    TrustEvidenceUnavailable,
    #[error("directory feed has no pending message")]
    NoMessage,
    #[error("directory feed returned an already committed message")]
    AlreadyCommitted(u64),
    #[error("directory feed cursor sequence overflowed")]
    SequenceOverflow,
}

#[derive(Debug)]
pub struct LiveDirectoryPage {
    pub page: DirectorySyncPage,
    pub ucs_confirmation: Option<u64>,
    /// A native adapter may carry a terminal cookie for the next incremental
    /// run even though the current full-import snapshot is complete. Keep the
    /// two facts separate so the durable import cannot mistake that cookie for
    /// another snapshot page.
    pub end_of_snapshot: bool,
}

#[derive(Debug, Clone)]
struct Mapping {
    provider: DirectoryProvider,
    immutable: String,
    login: String,
    email: String,
    display_name: String,
    groups: String,
    disabled: String,
    locked: String,
    deleted: String,
}

impl Mapping {
    fn from_env(provider: DirectoryProvider) -> Result<Self, LiveDirectoryError> {
        let immutable_default = immutable_attribute_for_provider(provider);
        validate_immutable_attribute_override(
            provider,
            env::var("ERPX_DIRECTORY_IMMUTABLE_ATTRIBUTE")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .as_deref(),
        )?;
        let mapping = Self {
            provider,
            // Provider UUIDs are an identity invariant, not a schema mapping
            // knob.  In particular, a mutable uid/sAMAccountName must never
            // become the external identity link through an environment
            // override.
            immutable: immutable_default.into(),
            login: env::var("ERPX_DIRECTORY_LOGIN_ATTRIBUTE").unwrap_or_else(|_| {
                if provider == DirectoryProvider::ActiveDirectoryDomainServices {
                    "sAMAccountName".into()
                } else {
                    "uid".into()
                }
            }),
            email: env::var("ERPX_DIRECTORY_EMAIL_ATTRIBUTE").unwrap_or_else(|_| "mail".into()),
            display_name: env::var("ERPX_DIRECTORY_DISPLAY_NAME_ATTRIBUTE")
                .unwrap_or_else(|_| "displayName".into()),
            groups: env::var("ERPX_DIRECTORY_GROUP_ATTRIBUTE")
                .unwrap_or_else(|_| "memberOf".into()),
            disabled: env::var("ERPX_DIRECTORY_DISABLED_ATTRIBUTE").unwrap_or_else(|_| {
                if provider == DirectoryProvider::ActiveDirectoryDomainServices {
                    "userAccountControl".into()
                } else {
                    "nsAccountLock".into()
                }
            }),
            locked: env::var("ERPX_DIRECTORY_LOCKED_ATTRIBUTE").unwrap_or_else(|_| {
                if provider == DirectoryProvider::ActiveDirectoryDomainServices {
                    "lockoutTime".into()
                } else {
                    "pwdAccountLockedTime".into()
                }
            }),
            deleted: env::var("ERPX_DIRECTORY_DELETED_ATTRIBUTE").unwrap_or_else(|_| {
                if provider == DirectoryProvider::ActiveDirectoryDomainServices {
                    "isDeleted".into()
                } else {
                    "entryDeleted".into()
                }
            }),
        };
        for attribute in [
            &mapping.immutable,
            &mapping.login,
            &mapping.email,
            &mapping.display_name,
            &mapping.groups,
            &mapping.disabled,
            &mapping.locked,
            &mapping.deleted,
        ] {
            validate_ldap_attribute_name(attribute)?;
        }
        Ok(mapping)
    }

    fn attrs(&self) -> Vec<String> {
        [
            self.immutable.clone(),
            self.login.clone(),
            self.email.clone(),
            self.display_name.clone(),
            self.groups.clone(),
            self.disabled.clone(),
            self.locked.clone(),
            self.deleted.clone(),
            "entryCSN".into(),
            "uSNChanged".into(),
        ]
        .into_iter()
        .fold(Vec::new(), |mut attrs, value| {
            if !attrs
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(&value))
            {
                attrs.push(value);
            }
            attrs
        })
    }
}

fn immutable_attribute_for_provider(provider: DirectoryProvider) -> &'static str {
    if provider == DirectoryProvider::ActiveDirectoryDomainServices {
        "objectGUID"
    } else {
        "entryUUID"
    }
}

fn validate_immutable_attribute_override(
    provider: DirectoryProvider,
    configured: Option<&str>,
) -> Result<(), LiveDirectoryError> {
    let expected = immutable_attribute_for_provider(provider);
    if configured.is_some_and(|value| !value.eq_ignore_ascii_case(expected)) {
        return Err(LiveDirectoryError::Configuration);
    }
    Ok(())
}

fn required_env(name: &str) -> Result<String, LiveDirectoryError> {
    let value = env::var(name).map_err(|_| LiveDirectoryError::Configuration)?;
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(LiveDirectoryError::Configuration);
    }
    Ok(value)
}

fn verify_https_peer_trust_evidence(
    response: &reqwest::Response,
    connector: &DirectoryConnector,
) -> Result<(), LiveDirectoryError> {
    // reqwest/rustls exposes the verified leaf certificate when `tls_info` is
    // enabled on the client. Compare that concrete DER certificate with the
    // persisted trust anchor before consuming the response body. A successful
    // HTTPS handshake without a matching fingerprint is not sufficient for a
    // directory connector acceptance.
    verified_https_peer_certificate(response, connector).map(|_| ())
}

fn verified_https_peer_certificate(
    response: &reqwest::Response,
    connector: &DirectoryConnector,
) -> Result<Vec<u8>, LiveDirectoryError> {
    let certificate = response
        .extensions()
        .get::<reqwest::tls::TlsInfo>()
        .and_then(reqwest::tls::TlsInfo::peer_certificate)
        .ok_or(LiveDirectoryError::TrustEvidenceUnavailable)?;
    verify_peer_certificate_fingerprint(certificate, &connector.tls.ca_certificate_sha256)?;
    Ok(certificate.to_vec())
}

fn verify_peer_certificate_fingerprint(
    certificate: &[u8],
    expected_fingerprint: &str,
) -> Result<(), LiveDirectoryError> {
    let peer_fingerprint =
        erpx_identity::directory::tls::sha256_certificate_fingerprint(certificate);
    if !peer_fingerprint.eq_ignore_ascii_case(expected_fingerprint.trim()) {
        return Err(LiveDirectoryError::TrustEvidenceUnavailable);
    }
    Ok(())
}

/// Re-check the persisted TLS contract at the transport boundary. Connector
/// validation happens when the record is written, but a live worker must not
/// assume that a deserialized or legacy record still binds the endpoint to
/// the declared server name, scheme, and trust-anchor fingerprint.
fn validate_tls_policy(
    connector: &DirectoryConnector,
    expected_scheme: &str,
) -> Result<(), LiveDirectoryError> {
    let endpoint =
        Url::parse(&connector.endpoint).map_err(|_| LiveDirectoryError::Configuration)?;
    if endpoint.scheme() != expected_scheme {
        return Err(LiveDirectoryError::Configuration);
    }
    let host = endpoint
        .host_str()
        .ok_or(LiveDirectoryError::Configuration)?;
    if connector.tls.server_name.trim().is_empty()
        || !host.eq_ignore_ascii_case(connector.tls.server_name.trim())
    {
        return Err(LiveDirectoryError::Configuration);
    }
    let fingerprint = connector.tls.ca_certificate_sha256.trim();
    if fingerprint.len() != 64
        || !fingerprint
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(LiveDirectoryError::Configuration);
    }
    if connector.tls.minimum_tls_version != "1.2" && connector.tls.minimum_tls_version != "1.3" {
        return Err(LiveDirectoryError::Configuration);
    }
    if endpoint.username() != ""
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
        || (expected_scheme != "https" && endpoint.path() != "" && endpoint.path() != "/")
    {
        return Err(LiveDirectoryError::Configuration);
    }
    Ok(())
}

fn validate_ucs_connector(connector: &DirectoryConnector) -> Result<(), LiveDirectoryError> {
    connector
        .validate()
        .map_err(|_| LiveDirectoryError::Configuration)?;
    if connector.provider != DirectoryProvider::UniventionCorporateServer
        || connector.feed != DirectoryFeedKind::UcsProvisioningService
    {
        return Err(LiveDirectoryError::Configuration);
    }
    validate_tls_policy(connector, "https")
}

fn bind_credentials() -> Result<(String, String), LiveDirectoryError> {
    Ok((
        required_env("ERPX_DIRECTORY_BIND_DN")?,
        required_env("ERPX_DIRECTORY_PASSWORD")?,
    ))
}

/// Load an optional private-CA bundle for on-premises LDAP.  The bundle is
/// used only as a normal Rustls root store; hostname and certificate-chain
/// verification remain enabled, and `open_ldap` still compares the verified
/// peer leaf fingerprint with the connector's persisted trust anchor.
fn ldap_tls_config(
    minimum_tls_version: &str,
) -> Result<Option<Arc<ClientConfig>>, LiveDirectoryError> {
    let Some(certificates) = private_ca_certificates()? else {
        // Rustls' default ldap3 configuration already permits only TLS 1.2+
        // and is appropriate for the default policy. A connector explicitly
        // requiring TLS 1.3 must provide a private trust bundle so the worker
        // can construct a version-pinned configuration instead of silently
        // falling back to the library default.
        if minimum_tls_version == "1.3" {
            return Err(LiveDirectoryError::Configuration);
        }
        return Ok(None);
    };
    let mut roots = RootCertStore::empty();
    for certificate in certificates {
        roots
            .add(CertificateDer::from(certificate))
            .map_err(|_| LiveDirectoryError::Configuration)?;
    }
    let config = match minimum_tls_version {
        "1.2" => ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
        "1.3" => ClientConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
            .with_root_certificates(roots)
            .with_no_client_auth(),
        _ => return Err(LiveDirectoryError::Configuration),
    };
    Ok(Some(Arc::new(config)))
}

fn private_ca_certificates() -> Result<Option<Vec<Vec<u8>>>, LiveDirectoryError> {
    let Some(path) = env::var(DIRECTORY_CA_CERT_PATH_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(None);
    };
    if path.len() > MAX_CA_CERT_PATH_BYTES || path.chars().any(char::is_control) {
        return Err(LiveDirectoryError::Configuration);
    }
    let pem = std::fs::read(&path).map_err(|_| LiveDirectoryError::Configuration)?;
    if pem.is_empty() || pem.len() > MAX_CA_CERTIFICATE_BYTES {
        return Err(LiveDirectoryError::Configuration);
    }
    Ok(Some(parse_pem_certificates(&pem)?))
}

fn parse_pem_certificates(pem: &[u8]) -> Result<Vec<Vec<u8>>, LiveDirectoryError> {
    let pem = std::str::from_utf8(pem).map_err(|_| LiveDirectoryError::Configuration)?;
    let mut certificates = Vec::new();
    let mut certificate_bytes = 0_usize;
    for section in pem.split("-----BEGIN CERTIFICATE-----").skip(1) {
        let Some((encoded, _)) = section.split_once("-----END CERTIFICATE-----") else {
            return Err(LiveDirectoryError::Configuration);
        };
        let compact = encoded
            .chars()
            .filter(|character| !character.is_ascii_whitespace())
            .collect::<String>();
        let der = STANDARD
            .decode(compact.as_bytes())
            .map_err(|_| LiveDirectoryError::Configuration)?;
        if der.is_empty() || der.len() > MAX_CA_CERTIFICATE_BYTES {
            return Err(LiveDirectoryError::Configuration);
        }
        certificate_bytes = certificate_bytes
            .checked_add(der.len())
            .ok_or(LiveDirectoryError::Configuration)?;
        if certificate_bytes > MAX_CA_CERTIFICATE_BYTES {
            return Err(LiveDirectoryError::Configuration);
        }
        certificates.push(der);
    }
    if certificates.is_empty() {
        return Err(LiveDirectoryError::Configuration);
    }
    Ok(certificates)
}

#[cfg(test)]
fn root_store_from_pem(pem: &[u8]) -> Result<RootCertStore, LiveDirectoryError> {
    let mut roots = RootCertStore::empty();
    for certificate in parse_pem_certificates(pem)? {
        roots
            .add(CertificateDer::from(certificate))
            .map_err(|_| LiveDirectoryError::Configuration)?;
    }
    Ok(roots)
}

fn ucs_http_client(connector: &DirectoryConnector) -> Result<Client, LiveDirectoryError> {
    let mut builder = Client::builder()
        .use_rustls_tls()
        .https_only(true)
        .tls_info(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(LDAP_TIMEOUT);
    builder = builder.min_tls_version(match connector.tls.minimum_tls_version.as_str() {
        "1.2" => reqwest::tls::Version::TLS_1_2,
        "1.3" => reqwest::tls::Version::TLS_1_3,
        _ => return Err(LiveDirectoryError::Configuration),
    });
    if let Some(certificates) = private_ca_certificates()? {
        for certificate in certificates {
            let certificate = reqwest::Certificate::from_der(&certificate)
                .map_err(|_| LiveDirectoryError::Configuration)?;
            builder = builder.add_root_certificate(certificate);
        }
    }
    builder
        .build()
        .map_err(|_| LiveDirectoryError::Configuration)
}

fn page_size(connector: &DirectoryConnector) -> Result<u32, LiveDirectoryError> {
    let configured = env::var("ERPX_DIRECTORY_PAGE_SIZE")
        .ok()
        .map(|value| {
            value
                .parse::<u32>()
                .map_err(|_| LiveDirectoryError::Configuration)
        })
        .transpose()?
        .unwrap_or(500);
    if configured == 0 || configured > MAX_PAGE_SIZE {
        return Err(LiveDirectoryError::Configuration);
    }
    Ok(configured.min(connector.max_import_users as u32))
}

fn decode_cookie(cursor: Option<&String>) -> Result<Vec<u8>, LiveDirectoryError> {
    let Some(cursor) = cursor else {
        return Ok(Vec::new());
    };
    let decoded = URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|_| LiveDirectoryError::Protocol)?;
    if decoded.len() > MAX_DIRECTORY_ADAPTER_CURSOR_BYTES {
        return Err(LiveDirectoryError::Protocol);
    }
    Ok(decoded)
}

fn encode_cookie(cookie: &[u8]) -> Result<Option<String>, LiveDirectoryError> {
    if cookie.len() > MAX_DIRECTORY_ADAPTER_CURSOR_BYTES {
        return Err(LiveDirectoryError::Protocol);
    }
    Ok((!cookie.is_empty()).then(|| URL_SAFE_NO_PAD.encode(cookie)))
}

fn cursor_sequence(previous: Option<&DirectorySyncCursor>) -> Result<u64, LiveDirectoryError> {
    previous.map_or(Ok(1), |cursor| {
        cursor
            .sequence
            .checked_add(1)
            .ok_or(LiveDirectoryError::SequenceOverflow)
    })
}

fn connector_filter(connector: &DirectoryConnector) -> String {
    env::var("ERPX_DIRECTORY_FILTER").unwrap_or_else(|_| {
        if connector.provider == DirectoryProvider::ActiveDirectoryDomainServices {
            "(&(objectCategory=person)(objectClass=user))".into()
        } else {
            "(objectClass=person)".into()
        }
    })
}

fn validate_previous_cursor(
    connector: &DirectoryConnector,
    previous: Option<&DirectorySyncCursor>,
) -> Result<(), LiveDirectoryError> {
    let Some(previous) = previous else {
        return Ok(());
    };
    if previous.connector_id != connector.id || previous.feed != connector.feed {
        return Err(LiveDirectoryError::Configuration);
    }
    previous
        .validate(connector.provider)
        .map_err(|_| LiveDirectoryError::Protocol)
}

async fn open_ldap<'a>(
    connector: &'a DirectoryConnector,
    bind_dn: &'a str,
    password: &'a str,
) -> Result<(ldap3::Ldap, Vec<u8>), LiveDirectoryError> {
    let expected_scheme = match connector.tls.mode {
        erpx_identity::DirectoryTlsMode::Ldaps => "ldaps",
        erpx_identity::DirectoryTlsMode::StartTls => "ldap",
        erpx_identity::DirectoryTlsMode::Https => {
            return Err(LiveDirectoryError::Configuration);
        }
    };
    validate_tls_policy(connector, expected_scheme)?;
    let mut settings = LdapConnSettings::new().set_conn_timeout(LDAP_TIMEOUT);
    if let Some(config) = ldap_tls_config(&connector.tls.minimum_tls_version)? {
        settings = settings.set_config(config);
    }
    let settings = settings.set_starttls(matches!(
        connector.tls.mode,
        erpx_identity::DirectoryTlsMode::StartTls
    ));
    let (connection, mut ldap) = LdapConnAsync::with_settings(settings, &connector.endpoint)
        .await
        .map_err(|_| LiveDirectoryError::Transport)?;
    ldap3::drive!(connection);
    let peer_certificate = ldap
        .get_peer_certificate()
        .await
        .map_err(|_| LiveDirectoryError::TrustEvidenceUnavailable)?
        .ok_or(LiveDirectoryError::TrustEvidenceUnavailable)?;
    let peer_fingerprint =
        erpx_identity::directory::tls::sha256_certificate_fingerprint(&peer_certificate);
    if !peer_fingerprint.eq_ignore_ascii_case(connector.tls.ca_certificate_sha256.trim()) {
        return Err(LiveDirectoryError::TrustEvidenceUnavailable);
    }
    ldap.simple_bind(bind_dn, password)
        .await
        .map_err(|_| LiveDirectoryError::Transport)?
        .success()
        .map_err(|_| LiveDirectoryError::Transport)?;
    Ok((ldap, peer_certificate))
}

#[derive(Clone)]
struct LiveDirectoryAuthTransport {
    connector: DirectoryConnector,
}

impl DirectoryTransport for LiveDirectoryAuthTransport {
    fn preflight(
        &self,
        config: &AuthConnectorConfig,
    ) -> Result<(), erpx_identity::directory::auth::DirectoryAuthError> {
        let expected = auth_connector_config(&self.connector).map_err(|_| {
            erpx_identity::directory::auth::DirectoryAuthError::InvalidConfiguration
        })?;
        if config.connector_ref() != expected.connector_ref()
            || config.scope() != expected.scope()
            || config.endpoint() != expected.endpoint()
            || config.provider() != expected.provider()
            || config.native_change_feed() != expected.native_change_feed()
            || config.tls().mode() != expected.tls().mode()
            || config.tls().server_name() != expected.tls().server_name()
            || config.tls().trust_anchor().fingerprint().as_str()
                != expected.tls().trust_anchor().fingerprint().as_str()
        {
            return Err(erpx_identity::directory::auth::DirectoryAuthError::InvalidConfiguration);
        }
        Ok(())
    }

    fn bind<'a>(
        &'a self,
        request: DirectoryBindRequest<'a>,
    ) -> erpx_identity::directory::auth::DirectoryBindFuture<'a> {
        let connector = self.connector.clone();
        if request.connector_ref() != connector.id {
            return Box::pin(async { Err(DirectoryTransportError::ProtocolViolation) });
        }
        let username = request.username().as_str().to_owned();
        // Keep the asynchronous adapter's owned secret zeroizing.  A plain
        // String copy would outlive the short-lived request boundary and
        // could be retained in a worker heap after the bind completes.
        let password = request.credential().clone();
        let connector_ref = request.connector_ref();
        Box::pin(async move {
            if connector.provider == DirectoryProvider::UniventionCorporateServer {
                return ucs_bind_user(
                    &connector,
                    connector_ref,
                    &username,
                    password.expose_to_transport(),
                )
                .await;
            }
            let (mut ldap, peer_certificate) =
                open_ldap(&connector, &username, password.expose_to_transport())
                    .await
                    .map_err(|error| match error {
                        LiveDirectoryError::TrustEvidenceUnavailable => {
                            DirectoryTransportError::CertificateValidationFailed
                        }
                        LiveDirectoryError::Configuration | LiveDirectoryError::Protocol => {
                            DirectoryTransportError::ProtocolViolation
                        }
                        LiveDirectoryError::Transport => DirectoryTransportError::Unavailable,
                        _ => DirectoryTransportError::Unavailable,
                    })?;
            let mapping = Mapping::from_env(connector.provider)
                .map_err(|_| DirectoryTransportError::ProtocolViolation)?;
            let filter = format!(
                "(&{}({}={}))",
                connector_filter(&connector),
                mapping.login,
                escape_ldap_filter_value(&username),
            );
            let (entries, _) = ldap
                .search(&connector.base_dn, Scope::Subtree, &filter, mapping.attrs())
                .await
                .map_err(|_| DirectoryTransportError::Unavailable)?
                .success()
                .map_err(|_| DirectoryTransportError::Rejected)?;
            if entries.len() != 1 {
                return Err(DirectoryTransportError::Rejected);
            }
            let entry = SearchEntry::construct(
                entries
                    .into_iter()
                    .next()
                    .ok_or(DirectoryTransportError::ProtocolViolation)?,
            );
            let immutable = immutable_uuid_from_entry(&entry, &mapping, connector.provider)
                .ok_or(DirectoryTransportError::ProtocolViolation)?;
            let account_status = directory_account_status(&entry, &mapping)
                .map_err(|_| DirectoryTransportError::ProtocolViolation)?;
            let provider = auth_provider(connector.provider);
            let key = erpx_identity::directory::auth::ExternalIdentityKey::try_for_provider(
                provider, immutable,
            )
            .map_err(|_| DirectoryTransportError::ProtocolViolation)?;
            let tls = auth_tls_policy(&connector)
                .map_err(|_| DirectoryTransportError::CertificateValidationFailed)?;
            let evidence = TlsHandshakeEvidence::from_verified_peer(
                tls.mode(),
                tls.server_name(),
                tls_evidence_protocol(&connector)
                    .map_err(|_| DirectoryTransportError::CertificateValidationFailed)?,
                &peer_certificate,
                tls.trust_anchor(),
            )
            .map_err(|_| DirectoryTransportError::CertificateValidationFailed)?;
            Ok(DirectoryBindResult::new(
                connector_ref,
                evidence,
                AuthDirectoryIdentity::new(key, account_status),
            ))
        })
    }
}

fn auth_provider(provider: DirectoryProvider) -> AuthDirectoryProvider {
    match provider {
        DirectoryProvider::GenericLdap => AuthDirectoryProvider::GenericLdap,
        DirectoryProvider::ActiveDirectoryDomainServices => {
            AuthDirectoryProvider::ActiveDirectoryDomainServices
        }
        DirectoryProvider::UniventionCorporateServer => {
            AuthDirectoryProvider::UniventionCorporateServer
        }
    }
}

fn auth_tls_policy(connector: &DirectoryConnector) -> Result<TlsPolicy, LiveDirectoryError> {
    let fingerprint = CertificateFingerprint::try_new(connector.tls.ca_certificate_sha256.clone())
        .map_err(|_| LiveDirectoryError::Configuration)?;
    let anchor = TrustAnchor::try_new(
        format!("directory-connector:{}", connector.id),
        fingerprint,
        1,
    )
    .map_err(|_| LiveDirectoryError::Configuration)?;
    match connector.tls.mode {
        erpx_identity::DirectoryTlsMode::Ldaps => {
            TlsPolicy::ldaps(connector.tls.server_name.clone(), anchor)
                .map_err(|_| LiveDirectoryError::Configuration)
        }
        erpx_identity::DirectoryTlsMode::StartTls => {
            TlsPolicy::starttls(connector.tls.server_name.clone(), anchor)
                .map_err(|_| LiveDirectoryError::Configuration)
        }
        erpx_identity::DirectoryTlsMode::Https => {
            TlsPolicy::https(connector.tls.server_name.clone(), anchor)
                .map_err(|_| LiveDirectoryError::Configuration)
        }
    }
}

fn tls_evidence_protocol(
    connector: &DirectoryConnector,
) -> Result<TlsProtocolVersion, LiveDirectoryError> {
    match connector.tls.minimum_tls_version.as_str() {
        "1.2" => Ok(TlsProtocolVersion::Tls1_2),
        "1.3" => Ok(TlsProtocolVersion::Tls1_3),
        _ => Err(LiveDirectoryError::Configuration),
    }
}

fn auth_connector_config(
    connector: &DirectoryConnector,
) -> Result<AuthConnectorConfig, LiveDirectoryError> {
    connector
        .validate()
        .map_err(|_| LiveDirectoryError::Configuration)?;
    if !connector.enabled {
        return Err(LiveDirectoryError::Configuration);
    }
    let scope = match &connector.scope {
        erpx_identity::DirectoryScope::Platform => {
            erpx_identity::directory::auth::ConnectorScope::Platform
        }
        erpx_identity::DirectoryScope::Holding { holding_ref } => {
            let holding_id = holding_ref
                .parse::<uuid::Uuid>()
                .map_err(|_| LiveDirectoryError::Configuration)?;
            erpx_identity::directory::auth::ConnectorScope::Holding(holding_id)
        }
    };
    let provider = auth_provider(connector.provider);
    let feed = match connector.provider {
        DirectoryProvider::GenericLdap => NativeChangeFeed::Rfc4533,
        DirectoryProvider::ActiveDirectoryDomainServices => NativeChangeFeed::AdDirSync,
        DirectoryProvider::UniventionCorporateServer => NativeChangeFeed::UcsProvisioningService,
    };
    AuthConnectorConfig::try_new(
        connector.id,
        scope,
        provider,
        connector.endpoint.clone(),
        auth_tls_policy(connector)?,
        TimeoutPolicy::conservative(),
        feed,
        BreakGlassPolicy::LocalOnly,
    )
    .map_err(|_| LiveDirectoryError::Configuration)
}

/// Authenticate a directory user through the same production transport used
/// by the native LDAP/AD feed adapters.  The returned identity is still
/// resolved against the imported write-side projection before a session is
/// issued by the server.
pub async fn authenticate_directory_user(
    connector: &DirectoryConnector,
    username: &str,
    password: &str,
) -> Result<AuthenticatedDirectoryIdentity, LiveDirectoryError> {
    let authenticator = DirectoryAuthenticator::new(auth_connector_config(connector)?)
        .map_err(|_| LiveDirectoryError::Configuration)?;
    let request = DirectoryAuthenticationRequest::new(
        DirectoryUsername::try_new(username).map_err(|_| LiveDirectoryError::Configuration)?,
        RedactedSecret::try_new(password).map_err(|_| LiveDirectoryError::Configuration)?,
        AccessRisk::Standard,
        None,
    );
    authenticator
        .authenticate_directory(
            request,
            &LiveDirectoryAuthTransport {
                connector: connector.clone(),
            },
            None,
            Utc::now(),
        )
        .await
        .map_err(|error| match error {
            erpx_identity::directory::auth::DirectoryAuthError::BindRejected
            | erpx_identity::directory::auth::DirectoryAuthError::AccountDisabled
            | erpx_identity::directory::auth::DirectoryAuthError::AccountLocked => {
                LiveDirectoryError::Transport
            }
            erpx_identity::directory::auth::DirectoryAuthError::TlsCertificateValidationFailed
            | erpx_identity::directory::auth::DirectoryAuthError::TlsPolicyRejected => {
                LiveDirectoryError::TrustEvidenceUnavailable
            }
            _ => LiveDirectoryError::Configuration,
        })
}

/// Authenticate a UCS account through the documented UDM HTTP REST API.  The
/// API uses the submitted user credentials with HTTPS Basic authentication;
/// the response is then queried for exactly one user object so the immutable
/// `entryUUID` and current disabled/locked state are returned together with
/// the verified TLS peer evidence.  Mutable usernames and DNs never become
/// the identity link.
async fn ucs_bind_user(
    connector: &DirectoryConnector,
    connector_ref: uuid::Uuid,
    username: &str,
    password: &str,
) -> Result<DirectoryBindResult, DirectoryTransportError> {
    validate_ucs_connector(connector).map_err(|_| DirectoryTransportError::ProtocolViolation)?;
    let client = ucs_http_client(connector).map_err(|_| DirectoryTransportError::Unavailable)?;
    let mut url =
        Url::parse(&connector.endpoint).map_err(|_| DirectoryTransportError::ProtocolViolation)?;
    url.set_path("/univention/udm/users/user/");
    url.set_query(None);
    url.set_fragment(None);
    let response = client
        .get(url)
        .query(&[("query[username]", username)])
        .basic_auth(username, Some(password))
        .header(header::ACCEPT, "application/hal+json, application/json")
        .send()
        .await
        .map_err(|_| DirectoryTransportError::Unavailable)?;
    let peer_certificate = verified_https_peer_certificate(&response, connector)
        .map_err(|_| DirectoryTransportError::CertificateValidationFailed)?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED
        || response.status() == reqwest::StatusCode::FORBIDDEN
    {
        return Err(DirectoryTransportError::Rejected);
    }
    if !response.status().is_success() {
        return Err(DirectoryTransportError::Unavailable);
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_DIRECTORY_ADAPTER_RESPONSE_BYTES as u64)
    {
        return Err(DirectoryTransportError::ProtocolViolation);
    }
    let body = response
        .bytes()
        .await
        .map_err(|_| DirectoryTransportError::Unavailable)?;
    if body.is_empty() || body.len() > MAX_DIRECTORY_ADAPTER_RESPONSE_BYTES {
        return Err(DirectoryTransportError::ProtocolViolation);
    }
    let body: Value =
        serde_json::from_slice(&body).map_err(|_| DirectoryTransportError::ProtocolViolation)?;
    let objects = body
        .get("_embedded")
        .and_then(|embedded| embedded.get("udm:object"))
        .and_then(Value::as_array)
        .ok_or(DirectoryTransportError::ProtocolViolation)?;
    if objects.len() != 1 {
        return Err(DirectoryTransportError::Rejected);
    }
    let object = &objects[0];
    let immutable = json_string(object, &["entryUUID", "univentionObjectIdentifier"])
        .ok_or(DirectoryTransportError::ProtocolViolation)?;
    let immutable = parse_uuid(&immutable).ok_or(DirectoryTransportError::ProtocolViolation)?;
    let account_status = if json_bool(object, &["deleted"])
        .map_err(|_| DirectoryTransportError::ProtocolViolation)?
    {
        DirectoryAccountStatus::Deleted
    } else if json_bool(object, &["locked"])
        .map_err(|_| DirectoryTransportError::ProtocolViolation)?
    {
        DirectoryAccountStatus::Locked
    } else if json_bool(object, &["disabled"])
        .map_err(|_| DirectoryTransportError::ProtocolViolation)?
    {
        DirectoryAccountStatus::Disabled
    } else {
        DirectoryAccountStatus::Enabled
    };
    let key = erpx_identity::directory::auth::ExternalIdentityKey::try_for_provider(
        AuthDirectoryProvider::UniventionCorporateServer,
        immutable,
    )
    .map_err(|_| DirectoryTransportError::ProtocolViolation)?;
    let tls = auth_tls_policy(connector)
        .map_err(|_| DirectoryTransportError::CertificateValidationFailed)?;
    let evidence = TlsHandshakeEvidence::from_verified_peer(
        tls.mode(),
        tls.server_name(),
        tls_evidence_protocol(connector)
            .map_err(|_| DirectoryTransportError::CertificateValidationFailed)?,
        &peer_certificate,
        tls.trust_anchor(),
    )
    .map_err(|_| DirectoryTransportError::CertificateValidationFailed)?;
    Ok(DirectoryBindResult::new(
        connector_ref,
        evidence,
        AuthDirectoryIdentity::new(key, account_status),
    ))
}

fn parse_uuid(value: &str) -> Option<uuid::Uuid> {
    let value = value.trim();
    let parsed = uuid::Uuid::parse_str(value).ok().or_else(|| {
        (value.len() == 32)
            .then(|| hex::decode(value).ok())
            .flatten()
            .and_then(|bytes| <[u8; 16]>::try_from(bytes).ok())
            .map(uuid::Uuid::from_bytes)
    });
    parsed.filter(|value| !value.is_nil())
}

fn attr_values(entry: &SearchEntry, name: &str) -> Vec<String> {
    if let Some((_, values)) = entry
        .attrs
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
    {
        return values.clone();
    }
    entry
        .bin_attrs
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, values)| values.iter().map(|value| hex_lower(value)).collect())
        .unwrap_or_default()
}

fn attr_value(entry: &SearchEntry, name: &str) -> Option<String> {
    attr_values(entry, name)
        .into_iter()
        .find(|value| !value.trim().is_empty())
}

fn validate_ldap_attribute_name(value: &str) -> Result<(), LiveDirectoryError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(LiveDirectoryError::Configuration);
    }
    Ok(())
}

fn escape_ldap_filter_value(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '*' => escaped.push_str("\\2a"),
            '(' => escaped.push_str("\\28"),
            ')' => escaped.push_str("\\29"),
            '\\' => escaped.push_str("\\5c"),
            '\0' => escaped.push_str("\\00"),
            character if character.is_control() => {
                for byte in character.to_string().as_bytes() {
                    escaped.push_str(&format!("\\{byte:02x}"));
                }
            }
            character => escaped.push(character),
        }
    }
    escaped
}

fn immutable_uuid_from_entry(
    entry: &SearchEntry,
    mapping: &Mapping,
    provider: DirectoryProvider,
) -> Option<uuid::Uuid> {
    let binary = entry
        .bin_attrs
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(&mapping.immutable))
        .and_then(|(_, values)| (values.len() == 1).then(|| &values[0]));
    let textual = entry
        .attrs
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(&mapping.immutable))
        .and_then(|(_, values)| {
            (values
                .iter()
                .filter(|value| !value.trim().is_empty())
                .count()
                == 1)
                .then(|| values.iter().find(|value| !value.trim().is_empty()))
                .flatten()
        });
    // A provider must not be able to present both a binary and textual value
    // for its immutable key and leave the adapter to choose one implicitly.
    if binary.is_some() && textual.is_some() {
        return None;
    }
    let parsed = binary.and_then(|binary| {
        if provider == DirectoryProvider::ActiveDirectoryDomainServices {
            uuid::Uuid::from_slice_le(binary).ok()
        } else {
            uuid::Uuid::from_slice(binary).ok()
        }
    });
    parsed
        .filter(|value| !value.is_nil())
        .or_else(|| {
            textual.and_then(|value| {
                value.parse::<uuid::Uuid>().ok().or_else(|| {
                    let bytes = hex::decode(value).ok()?;
                    let bytes = <[u8; 16]>::try_from(bytes).ok()?;
                    Some(
                        if provider == DirectoryProvider::ActiveDirectoryDomainServices {
                            uuid::Uuid::from_bytes_le(bytes)
                        } else {
                            uuid::Uuid::from_bytes(bytes)
                        },
                    )
                })
            })
        })
        .filter(|value| !value.is_nil())
}

fn canonical_ldap_immutable_id(provider: DirectoryProvider, value: &str) -> Option<String> {
    let value = value.trim();
    let uuid = value.parse::<uuid::Uuid>().ok().or_else(|| {
        let bytes = hex::decode(value).ok()?;
        let bytes = <[u8; 16]>::try_from(bytes).ok()?;
        Some(
            if provider == DirectoryProvider::ActiveDirectoryDomainServices {
                uuid::Uuid::from_bytes_le(bytes)
            } else {
                uuid::Uuid::from_bytes(bytes)
            },
        )
    })?;
    (!uuid.is_nil()).then(|| uuid.hyphenated().to_string())
}

/// Status attributes are part of the authentication contract, not optional
/// presentation data.  Missing values mean "not asserted" for providers
/// that omit false flags; a present but ambiguous value is a protocol error
/// and must never be interpreted as an enabled account.
fn status_attr_value(entry: &SearchEntry, name: &str) -> Result<Option<String>, ()> {
    let values = attr_values(entry, name)
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>();
    match values.as_slice() {
        [] => Ok(None),
        [value] => Ok(Some(value.clone())),
        _ => Err(()),
    }
}

fn parse_status_text(value: &str, true_values: &[&str]) -> Result<bool, ()> {
    let normalized = value.trim().to_ascii_lowercase();
    if true_values.contains(&normalized.as_str()) {
        return Ok(true);
    }
    if matches!(
        normalized.as_str(),
        "0" | "false" | "no" | "none" | "enabled" | "active" | "present" | "unlocked"
    ) {
        return Ok(false);
    }
    Err(())
}

fn disabled_status(entry: &SearchEntry, mapping: &Mapping) -> Result<bool, ()> {
    let Some(value) = status_attr_value(entry, &mapping.disabled)? else {
        return Ok(false);
    };
    if mapping.provider == DirectoryProvider::ActiveDirectoryDomainServices
        && mapping.disabled.eq_ignore_ascii_case("userAccountControl")
    {
        return value
            .trim()
            .parse::<u64>()
            .map(|flags| flags & 0x2 != 0)
            .map_err(|_| ());
    }
    parse_status_text(&value, &["1", "true", "yes", "disabled", "deleted"])
}

fn deleted_status(entry: &SearchEntry, mapping: &Mapping) -> Result<bool, ()> {
    let Some(value) = status_attr_value(entry, &mapping.deleted)? else {
        return Ok(false);
    };
    parse_status_text(&value, &["1", "true", "yes", "deleted"])
}

fn locked_status(entry: &SearchEntry, mapping: &Mapping) -> Result<bool, ()> {
    let Some(value) = status_attr_value(entry, &mapping.locked)? else {
        return Ok(false);
    };
    if mapping.provider == DirectoryProvider::ActiveDirectoryDomainServices
        && mapping.locked.eq_ignore_ascii_case("lockoutTime")
    {
        return value
            .trim()
            .parse::<u64>()
            .map(|timestamp| timestamp != 0)
            .map_err(|_| ());
    }
    // Generic LDAP lock timestamps have provider-specific generalized-time
    // formats.  Only explicit false values are safe to treat as unlocked;
    // any other non-empty value is conservatively locked.
    let normalized = value.trim().to_ascii_lowercase();
    Ok(!matches!(
        normalized.as_str(),
        "0" | "false" | "no" | "none" | "enabled" | "active" | "unlocked"
    ))
}

fn directory_account_status(
    entry: &SearchEntry,
    mapping: &Mapping,
) -> Result<DirectoryAccountStatus, ()> {
    if deleted_status(entry, mapping)? {
        return Ok(DirectoryAccountStatus::Deleted);
    }
    if locked_status(entry, mapping)? {
        return Ok(DirectoryAccountStatus::Locked);
    }
    if disabled_status(entry, mapping)? {
        return Ok(DirectoryAccountStatus::Disabled);
    }
    Ok(DirectoryAccountStatus::Enabled)
}

fn map_entry(
    entry: &SearchEntry,
    mapping: &Mapping,
    source_revision: u64,
    external_fallback: Option<&str>,
) -> Result<DirectoryUser, LiveDirectoryError> {
    let immutable = immutable_uuid_from_entry(entry, mapping, mapping.provider)
        .map(|value| value.hyphenated().to_string())
        .or_else(|| {
            external_fallback.and_then(|value| canonical_ldap_immutable_id(mapping.provider, value))
        })
        .ok_or(LiveDirectoryError::Protocol)?;
    let login = attr_value(entry, &mapping.login).unwrap_or_else(|| immutable.clone());
    let external_object_id = immutable.clone();
    let groups = attr_values(entry, &mapping.groups)
        .into_iter()
        .collect::<BTreeSet<_>>();
    let account_status =
        directory_account_status(entry, mapping).map_err(|_| LiveDirectoryError::Protocol)?;
    Ok(DirectoryUser {
        external_object_id,
        principal_ref: login,
        email: attr_value(entry, &mapping.email),
        display_name: attr_value(entry, &mapping.display_name),
        groups,
        active: account_status == DirectoryAccountStatus::Enabled,
        source_revision,
    })
}

fn map_entries(
    entries: Vec<SearchEntry>,
    mapping: &Mapping,
    sequence: u64,
) -> Result<Vec<DirectoryChange>, LiveDirectoryError> {
    entries
        .into_iter()
        .map(|entry| {
            let account_status = directory_account_status(&entry, mapping)
                .map_err(|_| LiveDirectoryError::Protocol)?;
            let user = map_entry(&entry, mapping, sequence, None)?;
            match account_status {
                DirectoryAccountStatus::Deleted => Ok(DirectoryChange::Delete {
                    external_object_id: user.external_object_id,
                    source_revision: sequence,
                }),
                DirectoryAccountStatus::Disabled | DirectoryAccountStatus::Locked => {
                    Ok(DirectoryChange::Disable {
                        external_object_id: user.external_object_id,
                        source_revision: sequence,
                    })
                }
                DirectoryAccountStatus::Enabled => Ok(DirectoryChange::Upsert(user)),
            }
        })
        .collect()
}

fn page(
    connector: &DirectoryConnector,
    feed: DirectoryFeedKind,
    sequence: u64,
    cookie: Vec<u8>,
    changes: Vec<DirectoryChange>,
) -> Result<DirectorySyncPage, LiveDirectoryError> {
    if connector.feed != feed || !feed.supported_by(connector.provider) || sequence == 0 {
        return Err(LiveDirectoryError::Configuration);
    }
    if changes.len() > MAX_PAGE_SIZE as usize {
        return Err(LiveDirectoryError::Protocol);
    }
    Ok(DirectorySyncPage {
        connector_id: connector.id,
        cursor: DirectorySyncCursor {
            connector_id: connector.id,
            feed,
            sequence,
            opaque_cookie: encode_cookie(&cookie)?,
            observed_at: Utc::now(),
        },
        changes,
    })
}

async fn ldap_search(
    connector: &DirectoryConnector,
    previous: Option<&DirectorySyncCursor>,
    page_size: u32,
    rfc4533: bool,
) -> Result<DirectorySyncPage, LiveDirectoryError> {
    ldap_search_with_mode(connector, previous, page_size, rfc4533, false)
        .await
        .map(|(page, _)| page)
}

fn root_dse_advertises_rfc4533(entry: &SearchEntry) -> bool {
    attr_values(entry, "supportedControl")
        .into_iter()
        .any(|value| value.trim() == RFC4533_SYNC_REQUEST_OID)
}

async fn rfc4533_persistent_supported(ldap: &mut ldap3::Ldap) -> Result<bool, LiveDirectoryError> {
    ldap.with_timeout(LDAP_TIMEOUT);
    let (entries, _) = ldap
        .search("", Scope::Base, "(objectClass=*)", vec!["supportedControl"])
        .await
        .map_err(|_| LiveDirectoryError::Transport)?
        .success()
        .map_err(|_| LiveDirectoryError::Transport)?;
    let Some(raw) = entries.into_iter().next() else {
        return Ok(false);
    };
    Ok(root_dse_advertises_rfc4533(&SearchEntry::construct(raw)))
}

struct Rfc4533EntryContext<'a> {
    mapping: &'a Mapping,
    sequence: u64,
    page_size: u32,
    changes: &'a mut Vec<DirectoryChange>,
}

struct Rfc4533SearchRequest<'a> {
    connector: &'a DirectoryConnector,
    page_size: u32,
    ldap: ldap3::Ldap,
    mapping: Mapping,
    sync_cookie: Vec<u8>,
    page_cookie: Vec<u8>,
    sequence: u64,
    filter: String,
    attributes: Vec<String>,
    full_import: bool,
}

fn append_rfc4533_entry(
    result_entry: ResultEntry,
    context: Rfc4533EntryContext<'_>,
) -> Result<(), LiveDirectoryError> {
    match std::panic::catch_unwind(AssertUnwindSafe(|| {
        let sync_state = parse_rfc4533_entry_sync_state(&result_entry.1)?;
        let entry = SearchEntry::construct(result_entry);
        let (state, uuid, _entry_cookie) = sync_state;
        match state {
            EntryState::Present => {}
            EntryState::Delete => {
                let external =
                    canonical_ldap_immutable_id(context.mapping.provider, &hex_lower(&uuid))
                        .ok_or(LiveDirectoryError::Protocol)?;
                context.changes.push(DirectoryChange::Delete {
                    external_object_id: external,
                    source_revision: context.sequence,
                });
            }
            EntryState::Add | EntryState::Modify => {
                let user = map_entry(
                    &entry,
                    context.mapping,
                    context.sequence,
                    Some(&hex_lower(&uuid)),
                )?;
                if user.active {
                    context.changes.push(DirectoryChange::Upsert(user));
                } else {
                    context.changes.push(DirectoryChange::Disable {
                        external_object_id: user.external_object_id,
                        source_revision: context.sequence,
                    });
                }
            }
        }
        if context.changes.len() > context.page_size as usize {
            return Err(LiveDirectoryError::Protocol);
        }
        Ok(())
    })) {
        Ok(result) => result,
        Err(_) => Err(LiveDirectoryError::Protocol),
    }
}

async fn ldap_search_rfc4533_persistent(
    request: Rfc4533SearchRequest<'_>,
) -> Result<(DirectorySyncPage, bool), LiveDirectoryError> {
    let Rfc4533SearchRequest {
        connector,
        page_size,
        mut ldap,
        mapping,
        sync_cookie: cookie,
        sequence,
        filter,
        attributes,
        ..
    } = request;
    ldap.with_controls(vec![
        SyncRequest {
            mode: rfc4533_refresh_mode(false, true),
            cookie: (!cookie.is_empty()).then_some(cookie),
            reload_hint: false,
        }
        .into(),
    ]);
    ldap.with_timeout(LDAP_TIMEOUT);
    let mut stream = ldap
        .streaming_search(&connector.base_dn, Scope::Subtree, &filter, attributes)
        .await
        .map_err(|_| LiveDirectoryError::Transport)?;
    let mut changes = Vec::new();
    let mut sync_cookie = None;
    let mut refresh_complete = false;

    macro_rules! abort_search {
        ($error:expr) => {{
            let request_id = stream.ldap_handle().last_id();
            let abandon = stream.ldap_handle().abandon(request_id).await;
            let _ = stream.finish().await;
            if abandon.is_err() {
                return Err(LiveDirectoryError::Transport);
            }
            return Err($error);
        }};
    }

    loop {
        let result_entry = match stream.next().await {
            Ok(Some(result_entry)) => result_entry,
            Ok(None) => break,
            Err(_) => abort_search!(LiveDirectoryError::Transport),
        };
        if result_entry.is_intermediate() {
            let sync_info =
                std::panic::catch_unwind(AssertUnwindSafe(|| parse_syncinfo(result_entry)))
                    .map_err(|_| LiveDirectoryError::Protocol);
            let sync_info = match sync_info {
                Ok(sync_info) => sync_info,
                Err(error) => abort_search!(error),
            };
            match sync_info {
                SyncInfo::NewCookie(cookie) => {
                    if cookie.is_empty() {
                        abort_search!(LiveDirectoryError::Protocol);
                    }
                    sync_cookie = Some(cookie);
                }
                SyncInfo::RefreshDelete {
                    cookie,
                    refresh_done,
                }
                | SyncInfo::RefreshPresent {
                    cookie,
                    refresh_done,
                } => {
                    if let Some(cookie) = cookie {
                        if cookie.is_empty() {
                            abort_search!(LiveDirectoryError::Protocol);
                        }
                        sync_cookie = Some(cookie);
                    }
                    refresh_complete = refresh_done;
                    if refresh_complete {
                        break;
                    }
                }
                SyncInfo::SyncIdSet {
                    cookie,
                    refresh_deletes,
                    sync_uuids,
                } => {
                    if let Some(cookie) = cookie {
                        if cookie.is_empty() {
                            abort_search!(LiveDirectoryError::Protocol);
                        }
                        sync_cookie = Some(cookie);
                    }
                    if refresh_deletes {
                        for uuid in sync_uuids {
                            if uuid.len() != 16 {
                                abort_search!(LiveDirectoryError::Protocol);
                            }
                            let external =
                                canonical_ldap_immutable_id(mapping.provider, &hex_lower(&uuid))
                                    .ok_or(LiveDirectoryError::Protocol);
                            let external = match external {
                                Ok(external) => external,
                                Err(error) => abort_search!(error),
                            };
                            changes.push(DirectoryChange::Delete {
                                external_object_id: external,
                                source_revision: sequence,
                            });
                            if changes.len() > page_size as usize {
                                abort_search!(LiveDirectoryError::Protocol);
                            }
                        }
                    }
                }
            }
            continue;
        }
        if result_entry.is_ref() {
            abort_search!(LiveDirectoryError::Protocol);
        }
        let context = Rfc4533EntryContext {
            mapping: &mapping,
            sequence,
            page_size,
            changes: &mut changes,
        };
        if let Err(error) = append_rfc4533_entry(result_entry, context) {
            abort_search!(error);
        }
    }

    if !refresh_complete {
        let result = stream.finish().await;
        sync_cookie =
            Some(native_ldap_cookie(&result.ctrls, true)?.ok_or(LiveDirectoryError::Protocol)?);
    } else {
        let request_id = stream.ldap_handle().last_id();
        let abandon = stream.ldap_handle().abandon(request_id).await;
        let _ = stream.finish().await;
        abandon.map_err(|_| LiveDirectoryError::Transport)?;
    }
    let cookie = sync_cookie
        .filter(|cookie| !cookie.is_empty())
        .ok_or(LiveDirectoryError::Protocol)?;
    page(
        connector,
        DirectoryFeedKind::Rfc4533,
        sequence,
        cookie,
        changes,
    )
    .map(|page| (page, false))
}

async fn ldap_search_rfc4533_refresh_only(
    request: Rfc4533SearchRequest<'_>,
) -> Result<(DirectorySyncPage, bool), LiveDirectoryError> {
    let Rfc4533SearchRequest {
        connector,
        page_size,
        mut ldap,
        mapping,
        sync_cookie,
        page_cookie,
        sequence,
        filter,
        attributes,
        full_import,
    } = request;
    let mut controls: Vec<RawControl> = vec![
        SyncRequest {
            mode: rfc4533_refresh_mode(full_import, false),
            cookie: (!sync_cookie.is_empty()).then_some(sync_cookie),
            reload_hint: false,
        }
        .into(),
    ];
    if full_import {
        controls.push(
            PagedResults {
                size: page_size as i32,
                cookie: page_cookie,
            }
            .into(),
        );
    }
    ldap.with_controls(controls);
    ldap.with_timeout(LDAP_TIMEOUT);
    let mut stream = ldap
        .streaming_search(&connector.base_dn, Scope::Subtree, &filter, attributes)
        .await
        .map_err(|_| LiveDirectoryError::Transport)?;
    let mut changes = Vec::new();
    macro_rules! abort_search {
        ($error:expr) => {{
            let request_id = stream.ldap_handle().last_id();
            let abandon = stream.ldap_handle().abandon(request_id).await;
            let _ = stream.finish().await;
            if abandon.is_err() {
                return Err(LiveDirectoryError::Transport);
            }
            return Err($error);
        }};
    }

    loop {
        let result_entry = match stream.next().await {
            Ok(Some(result_entry)) => result_entry,
            Ok(None) => break,
            Err(_) => abort_search!(LiveDirectoryError::Transport),
        };
        if result_entry.is_intermediate() {
            let sync_info =
                std::panic::catch_unwind(AssertUnwindSafe(|| parse_syncinfo(result_entry)))
                    .map_err(|_| LiveDirectoryError::Protocol);
            let sync_info = match sync_info {
                Ok(sync_info) => sync_info,
                Err(error) => abort_search!(error),
            };
            match sync_info {
                SyncInfo::NewCookie(cookie) => {
                    if cookie.is_empty() {
                        abort_search!(LiveDirectoryError::Protocol);
                    }
                }
                SyncInfo::RefreshDelete { cookie, .. }
                | SyncInfo::RefreshPresent { cookie, .. } => {
                    if let Some(cookie) = cookie
                        && cookie.is_empty()
                    {
                        abort_search!(LiveDirectoryError::Protocol);
                    }
                }
                SyncInfo::SyncIdSet {
                    cookie,
                    refresh_deletes,
                    sync_uuids,
                } => {
                    if let Some(cookie) = cookie
                        && cookie.is_empty()
                    {
                        abort_search!(LiveDirectoryError::Protocol);
                    }
                    if refresh_deletes {
                        for uuid in sync_uuids {
                            if uuid.len() != 16 {
                                abort_search!(LiveDirectoryError::Protocol);
                            }
                            let external =
                                canonical_ldap_immutable_id(mapping.provider, &hex_lower(&uuid))
                                    .ok_or(LiveDirectoryError::Protocol);
                            let external = match external {
                                Ok(external) => external,
                                Err(error) => abort_search!(error),
                            };
                            changes.push(DirectoryChange::Delete {
                                external_object_id: external,
                                source_revision: sequence,
                            });
                            if changes.len() > page_size as usize {
                                abort_search!(LiveDirectoryError::Protocol);
                            }
                        }
                    }
                }
            }
            continue;
        }
        if result_entry.is_ref() {
            abort_search!(LiveDirectoryError::Protocol);
        }
        let context = Rfc4533EntryContext {
            mapping: &mapping,
            sequence,
            page_size,
            changes: &mut changes,
        };
        if let Err(error) = append_rfc4533_entry(result_entry, context) {
            abort_search!(error);
        }
    }

    let result = stream.finish().await;
    if result.rc != 0 {
        return Err(LiveDirectoryError::Transport);
    }
    let sync_cookie = rfc4533_sync_done_cookie(&result.ctrls)?
        .filter(|cookie| !cookie.is_empty())
        .ok_or(LiveDirectoryError::Protocol)?;
    let (next_cookie, end_of_snapshot) = if full_import {
        let page_cookie = rfc4533_paged_cookie(&result.ctrls)?;
        if page_cookie.is_empty() {
            (sync_cookie, true)
        } else {
            (
                encode_rfc4533_full_import_cursor(&sync_cookie, &page_cookie)?,
                false,
            )
        }
    } else {
        (sync_cookie, false)
    };
    page(
        connector,
        DirectoryFeedKind::Rfc4533,
        sequence,
        next_cookie,
        changes,
    )
    .map(|page| (page, end_of_snapshot))
}

fn rfc4533_refresh_mode(full_import: bool, persistent_supported: bool) -> RefreshMode {
    if !full_import && persistent_supported {
        RefreshMode::RefreshAndPersist
    } else {
        RefreshMode::RefreshOnly
    }
}

async fn ldap_search_with_mode(
    connector: &DirectoryConnector,
    previous: Option<&DirectorySyncCursor>,
    page_size: u32,
    rfc4533: bool,
    full_import: bool,
) -> Result<(DirectorySyncPage, bool), LiveDirectoryError> {
    let (bind_dn, password) = bind_credentials()?;
    let (mut ldap, _peer_certificate) = open_ldap(connector, &bind_dn, &password).await?;
    let mapping = Mapping::from_env(connector.provider)?;
    let cookie = decode_cookie(previous.and_then(|cursor| cursor.opaque_cookie.as_ref()))?;
    let persistent = rfc4533 && !full_import && rfc4533_persistent_supported(&mut ldap).await?;
    let (rfc_sync_cookie, rfc_page_cookie) = if rfc4533 && full_import {
        decode_rfc4533_full_import_cursor(&cookie)?
    } else {
        (cookie.clone(), Vec::new())
    };
    let sequence = cursor_sequence(previous)?;
    let filter = connector_filter(connector);
    let attributes = mapping.attrs();
    if rfc4533 {
        let request = Rfc4533SearchRequest {
            connector,
            page_size,
            ldap,
            mapping,
            sync_cookie: rfc_sync_cookie,
            page_cookie: rfc_page_cookie,
            sequence,
            filter,
            attributes,
            full_import,
        };
        if persistent {
            return ldap_search_rfc4533_persistent(request).await;
        }
        return ldap_search_rfc4533_refresh_only(request).await;
    }
    ldap.with_controls(PagedResults {
        size: page_size as i32,
        cookie: cookie.clone(),
    });
    let (entries, result) = ldap
        .search(&connector.base_dn, Scope::Subtree, &filter, attributes)
        .await
        .map_err(|_| LiveDirectoryError::Transport)?
        .success()
        .map_err(|_| LiveDirectoryError::Transport)?;
    if entries.len() > page_size as usize {
        return Err(LiveDirectoryError::Protocol);
    }
    let mut changes = Vec::with_capacity(entries.len());
    for raw in entries {
        let entry = SearchEntry::construct(raw);
        changes.extend(map_entries(vec![entry], &mapping, sequence)?);
    }
    let next_cookie =
        native_ldap_cookie(&result.ctrls, false)?.ok_or(LiveDirectoryError::Protocol)?;
    page(
        connector,
        DirectoryFeedKind::Polling,
        sequence,
        next_cookie,
        changes,
    )
    .map(|page| (page, false))
}

type Rfc4533EntrySyncState = (EntryState, Vec<u8>, Option<Vec<u8>>);

fn parse_sync_state(control: &RawControl) -> Option<(EntryState, Vec<u8>, Option<Vec<u8>>)> {
    let value = control.val.as_deref()?;
    std::panic::catch_unwind(AssertUnwindSafe(|| control.parse::<SyncState>()))
        .ok()
        .map(|state| (state.state, state.entry_uuid, state.cookie))
        .filter(|(_, uuid, _)| {
            uuid.len() == 16 && value.len() <= MAX_DIRECTORY_ADAPTER_CURSOR_BYTES
        })
}

fn parse_rfc4533_entry_sync_state(
    controls: &[Control],
) -> Result<Rfc4533EntrySyncState, LiveDirectoryError> {
    let mut matching = controls
        .iter()
        .filter(|control| control.1.ctype == RFC4533_SYNC_STATE_OID);
    let control = matching.next().ok_or(LiveDirectoryError::Protocol)?;
    if matching.next().is_some() {
        return Err(LiveDirectoryError::Protocol);
    }
    parse_sync_state(&control.1).ok_or(LiveDirectoryError::Protocol)
}

fn parse_sync_done(control: &RawControl) -> Option<Vec<u8>> {
    let value = control.val.as_deref()?;
    let mut outer = 0;
    let (tag, sequence) = read_tlv(value, &mut outer)?;
    if tag != 0x30 || outer != value.len() {
        return None;
    }
    let mut offset = 0;
    let mut cookie = None;
    if offset < sequence.len() {
        let (tag, field) = read_tlv(sequence, &mut offset)?;
        match tag {
            0x04 if field.len() <= MAX_DIRECTORY_ADAPTER_CURSOR_BYTES => {
                cookie = Some(field.to_vec());
                if offset < sequence.len() {
                    let (tag, field) = read_tlv(sequence, &mut offset)?;
                    // refreshDeletes BOOLEAN DEFAULT FALSE is the only
                    // optional field after an explicitly supplied cookie.
                    if tag != 0x01 || field.len() != 1 {
                        return None;
                    }
                }
            }
            // The cookie is OPTIONAL, so a present refreshDeletes BOOLEAN may
            // legally be the first and only field in syncDoneValue.
            0x01 if field.len() == 1 => {}
            _ => return None,
        }
    }
    if offset != sequence.len() {
        return None;
    }
    Some(cookie.unwrap_or_default())
}

fn parse_paged_cookie(control: &RawControl) -> Option<Vec<u8>> {
    control.val.as_deref().and_then(|value| {
        std::panic::catch_unwind(AssertUnwindSafe(|| control.parse::<PagedResults>()))
            .ok()
            .map(|result| result.cookie)
            .filter(|_cookie| value.len() <= MAX_DIRECTORY_ADAPTER_CURSOR_BYTES)
    })
}

fn rfc4533_sync_done_cookie(controls: &[Control]) -> Result<Option<Vec<u8>>, LiveDirectoryError> {
    let mut matching = controls
        .iter()
        .filter(|control| control.1.ctype == RFC4533_SYNC_DONE_OID);
    let Some(control) = matching.next() else {
        return Ok(None);
    };
    if matching.next().is_some() {
        return Err(LiveDirectoryError::Protocol);
    }
    parse_sync_done(&control.1)
        .map(Some)
        .ok_or(LiveDirectoryError::Protocol)
}

fn rfc4533_paged_cookie(controls: &[Control]) -> Result<Vec<u8>, LiveDirectoryError> {
    let mut matching = controls
        .iter()
        .filter(|control| control.1.ctype == LDAP_PAGED_RESULTS_OID);
    let control = matching.next().ok_or(LiveDirectoryError::Protocol)?;
    if matching.next().is_some() {
        return Err(LiveDirectoryError::Protocol);
    }
    parse_paged_cookie(&control.1).ok_or(LiveDirectoryError::Protocol)
}

fn decode_rfc4533_full_import_cursor(
    cursor: &[u8],
) -> Result<(Vec<u8>, Vec<u8>), LiveDirectoryError> {
    if !cursor.starts_with(RFC4533_FULL_IMPORT_CURSOR_MAGIC) {
        return Ok((cursor.to_vec(), Vec::new()));
    }
    let mut offset = RFC4533_FULL_IMPORT_CURSOR_MAGIC.len();
    let sync_len = read_cursor_length(cursor, &mut offset)?;
    let page_len = read_cursor_length(cursor, &mut offset)?;
    let total = offset
        .checked_add(sync_len)
        .and_then(|total| total.checked_add(page_len))
        .ok_or(LiveDirectoryError::Protocol)?;
    if total != cursor.len() || total > MAX_DIRECTORY_ADAPTER_CURSOR_BYTES {
        return Err(LiveDirectoryError::Protocol);
    }
    let sync_end = offset + sync_len;
    let page_end = sync_end + page_len;
    Ok((
        cursor[offset..sync_end].to_vec(),
        cursor[sync_end..page_end].to_vec(),
    ))
}

fn encode_rfc4533_full_import_cursor(
    sync_cookie: &[u8],
    page_cookie: &[u8],
) -> Result<Vec<u8>, LiveDirectoryError> {
    let total = RFC4533_FULL_IMPORT_CURSOR_MAGIC
        .len()
        .checked_add(8)
        .and_then(|total| total.checked_add(sync_cookie.len()))
        .and_then(|total| total.checked_add(page_cookie.len()))
        .ok_or(LiveDirectoryError::Protocol)?;
    if sync_cookie.len() > u32::MAX as usize
        || page_cookie.len() > u32::MAX as usize
        || total > MAX_DIRECTORY_ADAPTER_CURSOR_BYTES
    {
        return Err(LiveDirectoryError::Protocol);
    }
    let mut encoded = Vec::with_capacity(total);
    encoded.extend_from_slice(RFC4533_FULL_IMPORT_CURSOR_MAGIC);
    encoded.extend_from_slice(&(sync_cookie.len() as u32).to_be_bytes());
    encoded.extend_from_slice(&(page_cookie.len() as u32).to_be_bytes());
    encoded.extend_from_slice(sync_cookie);
    encoded.extend_from_slice(page_cookie);
    Ok(encoded)
}

fn read_cursor_length(cursor: &[u8], offset: &mut usize) -> Result<usize, LiveDirectoryError> {
    let end = offset.checked_add(4).ok_or(LiveDirectoryError::Protocol)?;
    let bytes = cursor
        .get(*offset..end)
        .ok_or(LiveDirectoryError::Protocol)?;
    *offset = end;
    usize::try_from(u32::from_be_bytes(
        bytes.try_into().map_err(|_| LiveDirectoryError::Protocol)?,
    ))
    .map_err(|_| LiveDirectoryError::Protocol)
}

/// A successful LDAP search without its native continuation control is not a
/// terminal snapshot. Treating it as terminal would silently drop the rest of
/// a full import or advance an RFC 4533 feed without a durable cookie.
fn native_ldap_cookie(
    controls: &[Control],
    rfc4533: bool,
) -> Result<Option<Vec<u8>>, LiveDirectoryError> {
    let expected = if rfc4533 {
        RFC4533_SYNC_DONE_OID
    } else {
        LDAP_PAGED_RESULTS_OID
    };
    let mut found = None;
    for control in controls
        .iter()
        .filter(|control| control.1.ctype == expected)
    {
        if found.is_some() {
            return Err(LiveDirectoryError::Protocol);
        }
        let cookie = if rfc4533 {
            parse_sync_done(&control.1)
        } else {
            parse_paged_cookie(&control.1)
        }
        .ok_or(LiveDirectoryError::Protocol)?;
        if rfc4533 && cookie.is_empty() {
            return Err(LiveDirectoryError::Protocol);
        }
        found = Some(cookie);
    }
    found.map(Some).ok_or(LiveDirectoryError::Protocol)
}

fn ber_length(length: usize) -> Vec<u8> {
    if length < 128 {
        return vec![length as u8];
    }
    let bytes = length.to_be_bytes();
    let first = bytes
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(bytes.len() - 1);
    let value = &bytes[first..];
    let mut result = vec![0x80 | value.len() as u8];
    result.extend_from_slice(value);
    result
}

fn ber_tlv(tag: u8, value: &[u8]) -> Vec<u8> {
    let mut result = vec![tag];
    result.extend(ber_length(value.len()));
    result.extend_from_slice(value);
    result
}

fn ber_integer(value: u64) -> Vec<u8> {
    let mut bytes = value.to_be_bytes().to_vec();
    while bytes.len() > 1 && bytes[0] == 0 && bytes[1] & 0x80 == 0 {
        bytes.remove(0);
    }
    if bytes[0] & 0x80 != 0 {
        bytes.insert(0, 0);
    }
    ber_tlv(0x02, &bytes)
}

fn ad_dirsync_request(cookie: &[u8], page_size: u32) -> RawControl {
    let mut body = Vec::new();
    body.extend(ber_integer(0x8000_0001));
    let max_bytes = u64::from(page_size).saturating_mul(4096).max(0x10_0000);
    body.extend(ber_integer(max_bytes));
    body.extend(ber_tlv(0x04, cookie));
    RawControl {
        ctype: AD_DIRSYNC_OID.into(),
        crit: true,
        val: Some(ber_tlv(0x30, &body)),
    }
}

fn read_tlv<'a>(bytes: &'a [u8], offset: &mut usize) -> Option<(u8, &'a [u8])> {
    let tag = *bytes.get(*offset)?;
    *offset += 1;
    let first = *bytes.get(*offset)?;
    *offset += 1;
    let length = if first & 0x80 == 0 {
        usize::from(first)
    } else {
        let count = usize::from(first & 0x7f);
        let end = (*offset).checked_add(count)?;
        if count == 0 || count > 8 || end > bytes.len() {
            return None;
        }
        let mut length = 0_usize;
        for byte in &bytes[*offset..end] {
            length = length.checked_mul(256)?.checked_add(usize::from(*byte))?;
        }
        *offset = end;
        length
    };
    let end = offset.checked_add(length)?;
    let value = bytes.get(*offset..end)?;
    *offset = end;
    Some((tag, value))
}

fn ad_dirsync_response(value: &[u8]) -> Option<(bool, Vec<u8>)> {
    let mut outer = 0;
    let (tag, sequence) = read_tlv(value, &mut outer)?;
    if tag != 0x30 || outer != value.len() {
        return None;
    }
    let mut offset = 0;
    let (more_results_tag, more_results) = read_tlv(sequence, &mut offset)?;
    // The AD DS DirSync response control is
    //   SEQUENCE { moreData INTEGER, unused INTEGER, cookie OCTET STRING }.
    // MoreResults is nonzero when the server has another page. Both integer
    // fields are required by the AD response contract; accepting a BOOLEAN or
    // an omitted reserved field would make malformed responses look terminal.
    if more_results_tag != 0x02 || decode_nonnegative_integer(more_results).is_none() {
        return None;
    }
    let (unused_tag, unused) = read_tlv(sequence, &mut offset)?;
    if unused_tag != 0x02 || decode_nonnegative_integer(unused).is_none() {
        return None;
    }
    let (cookie_tag, cookie) = read_tlv(sequence, &mut offset)?;
    if cookie_tag != 0x04
        || cookie.len() > MAX_DIRECTORY_ADAPTER_CURSOR_BYTES
        || offset != sequence.len()
    {
        return None;
    }
    Some((
        decode_nonnegative_integer(more_results)? != 0,
        cookie.to_vec(),
    ))
}

fn decode_nonnegative_integer(value: &[u8]) -> Option<u64> {
    if value.is_empty() || value.len() > 8 || (value[0] & 0x80) != 0 {
        return None;
    }
    if value.len() > 1 && value[0] == 0 && value[1] & 0x80 == 0 {
        return None;
    }
    let mut number = 0_u64;
    for byte in value {
        number = number.checked_mul(256)?.checked_add(u64::from(*byte))?;
    }
    Some(number)
}

fn ad_optional_control(ctype: &str) -> RawControl {
    RawControl {
        ctype: ctype.into(),
        // These controls are required for complete AD deletion semantics. A
        // server that cannot honor them must fail the feed rather than return
        // a superficially healthy stream with tombstones omitted.
        crit: true,
        val: None,
    }
}

fn ad_dirsync_controls(cookie: &[u8], page_size: u32) -> Vec<RawControl> {
    vec![
        ad_dirsync_request(cookie, page_size),
        ad_optional_control(AD_SHOW_DELETED_OID),
        ad_optional_control(AD_EXTENDED_DN_OID),
    ]
}

fn ad_dirsync_control(controls: &[Control]) -> Result<(bool, Vec<u8>), LiveDirectoryError> {
    let mut found = None;
    for control in controls
        .iter()
        .filter(|control| control.1.ctype == AD_DIRSYNC_OID)
    {
        if found.is_some() {
            return Err(LiveDirectoryError::Protocol);
        }
        let value = control
            .1
            .val
            .as_deref()
            .ok_or(LiveDirectoryError::Protocol)?;
        found = Some(ad_dirsync_response(value).ok_or(LiveDirectoryError::Protocol)?);
    }
    found.ok_or(LiveDirectoryError::Protocol)
}

async fn ad_dirsync(
    connector: &DirectoryConnector,
    previous: Option<&DirectorySyncCursor>,
    page_size: u32,
) -> Result<LiveDirectoryPage, LiveDirectoryError> {
    let (bind_dn, password) = bind_credentials()?;
    let (mut ldap, _peer_certificate) = open_ldap(connector, &bind_dn, &password).await?;
    let mapping = Mapping::from_env(connector.provider)?;
    let cookie = decode_cookie(previous.and_then(|cursor| cursor.opaque_cookie.as_ref()))?;
    let sequence = cursor_sequence(previous)?;
    ldap.with_controls(ad_dirsync_controls(&cookie, page_size));
    let (entries, result) = ldap
        .search(
            &connector.base_dn,
            Scope::Subtree,
            &connector_filter(connector),
            mapping.attrs(),
        )
        .await
        .map_err(|_| LiveDirectoryError::Transport)?
        .success()
        .map_err(|_| LiveDirectoryError::Transport)?;
    if entries.len() > page_size as usize {
        return Err(LiveDirectoryError::Protocol);
    }
    let (more_data, next_cookie) = ad_dirsync_control(&result.ctrls)?;
    let changes = map_entries(
        entries.into_iter().map(SearchEntry::construct).collect(),
        &mapping,
        sequence,
    )?;
    if next_cookie.is_empty() && more_data {
        return Err(LiveDirectoryError::Protocol);
    }
    let page = page(
        connector,
        DirectoryFeedKind::AdDirSync,
        sequence,
        next_cookie,
        changes,
    )?;
    Ok(LiveDirectoryPage {
        page,
        ucs_confirmation: None,
        end_of_snapshot: !more_data,
    })
}

#[derive(Debug, Deserialize)]
struct UcsMessage {
    body: Option<Value>,
    sequence_number: u64,
    #[serde(default)]
    end_of_snapshot: bool,
    #[serde(default)]
    has_more: bool,
}

fn decode_ucs_change(message: UcsMessage) -> Result<DirectoryChange, LiveDirectoryError> {
    let body = message.body.ok_or(LiveDirectoryError::Protocol)?;
    ucs_change(&body, message.sequence_number)
}

fn ucs_path(endpoint: &str, subscription: &str, suffix: &str) -> Result<Url, LiveDirectoryError> {
    if subscription.is_empty()
        || subscription.len() > 256
        || !subscription
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._-".contains(character))
    {
        return Err(LiveDirectoryError::Configuration);
    }
    let mut url = Url::parse(endpoint).map_err(|_| LiveDirectoryError::Configuration)?;
    if url.scheme() != "https" || url.host_str().is_none() {
        return Err(LiveDirectoryError::Configuration);
    }
    let base = url.path().trim_end_matches('/');
    let prefix = if base.ends_with("/v1") {
        base.to_owned()
    } else {
        format!("{base}/v1")
    };
    url.set_path(&format!("{prefix}/subscriptions/{subscription}{suffix}"));
    Ok(url)
}

fn json_string(value: &Value, names: &[&str]) -> Option<String> {
    for name in names {
        if let Some(found) = value.get(*name).and_then(Value::as_str)
            && !found.trim().is_empty()
        {
            return Some(found.to_owned());
        }
    }
    if let Some(properties) = value.get("properties") {
        for name in names {
            if let Some(found) = properties.get(*name) {
                if let Some(found) = found.as_str() {
                    return Some(found.to_owned());
                }
                if let Some(found) = found
                    .as_array()
                    .and_then(|values| values.first())
                    .and_then(Value::as_str)
                {
                    return Some(found.to_owned());
                }
            }
        }
    }
    None
}

/// UCS change envelopes may repeat the immutable UUID at the top level and
/// under `properties`. Treat those as one value, but reject aliases or
/// multi-valued attributes that disagree. A replacement object must be
/// represented as a delete plus upsert; silently rebinding an existing
/// immutable identity to a new UUID is never accepted.
fn ucs_immutable_id(value: &Value) -> Result<Option<String>, LiveDirectoryError> {
    let mut values = BTreeSet::new();
    let properties = value.get("properties");
    for name in ["univentionObjectIdentifier", "entryUUID", "uuid", "id"] {
        for container in [Some(value), properties] {
            let Some(candidate) = container.and_then(|container| container.get(name)) else {
                continue;
            };
            match candidate {
                Value::String(candidate) => {
                    if candidate.trim().is_empty() {
                        return Err(LiveDirectoryError::Protocol);
                    }
                    values.insert(candidate.trim().to_owned());
                }
                Value::Array(candidates) => {
                    if candidates.is_empty() {
                        return Err(LiveDirectoryError::Protocol);
                    }
                    for candidate in candidates {
                        let Some(candidate) = candidate.as_str() else {
                            return Err(LiveDirectoryError::Protocol);
                        };
                        if candidate.trim().is_empty() {
                            return Err(LiveDirectoryError::Protocol);
                        }
                        values.insert(candidate.trim().to_owned());
                    }
                }
                Value::Null => {}
                _ => return Err(LiveDirectoryError::Protocol),
            }
        }
    }
    if values.is_empty() {
        return Ok(None);
    }
    if values.len() != 1 {
        return Err(LiveDirectoryError::Protocol);
    }
    let value = values
        .into_iter()
        .next()
        .ok_or(LiveDirectoryError::Protocol)?;
    let uuid = parse_uuid(&value).ok_or(LiveDirectoryError::Protocol)?;
    Ok(Some(uuid.hyphenated().to_string()))
}

fn parse_json_bool(value: &Value) -> Result<bool, LiveDirectoryError> {
    match value {
        Value::Bool(value) => Ok(*value),
        Value::Number(value) => match value.as_u64() {
            Some(0) => Ok(false),
            Some(1) => Ok(true),
            _ => Err(LiveDirectoryError::Protocol),
        },
        Value::String(value) => parse_status_text(
            value,
            &["1", "true", "yes", "disabled", "deleted", "locked"],
        )
        .map_err(|_| LiveDirectoryError::Protocol),
        Value::Array(values) if values.len() == 1 => parse_json_bool(&values[0]),
        _ => Err(LiveDirectoryError::Protocol),
    }
}

fn json_bool(value: &Value, names: &[&str]) -> Result<bool, LiveDirectoryError> {
    let mut result = false;
    for name in names {
        let Some(candidate) = value.get(*name).or_else(|| {
            value
                .get("properties")
                .and_then(|properties| properties.get(*name))
        }) else {
            continue;
        };
        result |= parse_json_bool(candidate)?;
    }
    Ok(result)
}

fn ucs_change(body: &Value, sequence: u64) -> Result<DirectoryChange, LiveDirectoryError> {
    let old = body
        .get("old")
        .filter(|value| !value.is_null())
        .unwrap_or(&Value::Null);
    let new = body
        .get("new")
        .filter(|value| !value.is_null())
        .unwrap_or(&Value::Null);
    let old_immutable = ucs_immutable_id(old)?;
    let new_immutable = ucs_immutable_id(new)?;
    if old_immutable.is_some() && new_immutable.is_some() && old_immutable != new_immutable {
        return Err(LiveDirectoryError::Protocol);
    }
    let current = if !new.is_null() && !new.as_object().is_some_and(|object| object.is_empty()) {
        new
    } else {
        old
    };
    let immutable = new_immutable
        .or(old_immutable)
        .ok_or(LiveDirectoryError::Protocol)?;
    if new.is_null() || (new.as_object().is_some_and(|object| object.is_empty()) && !old.is_null())
    {
        return Ok(DirectoryChange::Delete {
            external_object_id: immutable,
            source_revision: sequence,
        });
    }
    if json_bool(current, &["deleted"])? {
        return Ok(DirectoryChange::Delete {
            external_object_id: immutable,
            source_revision: sequence,
        });
    }
    let principal = json_string(current, &["username", "uid", "name", "mail"])
        .unwrap_or_else(|| immutable.clone());
    let groups = current
        .get("groups")
        .or_else(|| current.get("groupMembership"))
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let user = DirectoryUser {
        external_object_id: immutable,
        principal_ref: principal,
        email: json_string(current, &["mail", "email"]),
        display_name: json_string(current, &["displayName", "name"]),
        groups,
        active: !json_bool(current, &["disabled", "deleted", "locked"])?,
        source_revision: sequence,
    };
    if user.active {
        Ok(DirectoryChange::Upsert(user))
    } else {
        Ok(DirectoryChange::Disable {
            external_object_id: user.external_object_id,
            source_revision: sequence,
        })
    }
}

fn validate_ucs_sequence(previous: u64, current: u64) -> Result<(), LiveDirectoryError> {
    if current == 0 {
        return Err(LiveDirectoryError::Protocol);
    }
    if current < previous {
        return Err(LiveDirectoryError::Protocol);
    }
    if current == previous {
        return Err(LiveDirectoryError::AlreadyCommitted(current));
    }
    if previous != 0
        && current
            != previous
                .checked_add(1)
                .ok_or(LiveDirectoryError::SequenceOverflow)?
    {
        return Err(LiveDirectoryError::Protocol);
    }
    Ok(())
}

async fn ucs(
    connector: &DirectoryConnector,
    previous: Option<&DirectorySyncCursor>,
    full_import: bool,
) -> Result<LiveDirectoryPage, LiveDirectoryError> {
    validate_ucs_connector(connector)?;
    let subscription = required_env("ERPX_UCS_SUBSCRIPTION")?;
    let password = required_env("ERPX_UCS_SUBSCRIPTION_PASSWORD")?;
    let client = ucs_http_client(connector)?;
    let url = ucs_path(&connector.endpoint, &subscription, "/messages/next")?;
    let response = client
        .get(url)
        .query(&[("timeout", "0"), ("pop", "false")])
        .basic_auth(&subscription, Some(&password))
        .header(header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|_| LiveDirectoryError::Transport)?;
    verify_https_peer_trust_evidence(&response, connector)?;
    if response.status() == reqwest::StatusCode::NO_CONTENT {
        return Err(LiveDirectoryError::NoMessage);
    }
    if !response.status().is_success() {
        return Err(LiveDirectoryError::Transport);
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_DIRECTORY_ADAPTER_RESPONSE_BYTES as u64)
    {
        return Err(LiveDirectoryError::ResponseTooLarge);
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| LiveDirectoryError::Transport)?;
    if bytes.is_empty() || bytes.len() > MAX_DIRECTORY_ADAPTER_RESPONSE_BYTES {
        return Err(LiveDirectoryError::ResponseTooLarge);
    }
    if bytes.as_ref() == b"null" {
        return Err(LiveDirectoryError::NoMessage);
    }
    let message: UcsMessage =
        serde_json::from_slice(&bytes).map_err(|_| LiveDirectoryError::Protocol)?;
    let previous_sequence = previous.map_or(0, |cursor| cursor.sequence);
    let sequence = message.sequence_number;
    validate_ucs_sequence(previous_sequence, sequence)?;
    let end_of_snapshot = full_import && message.end_of_snapshot && !message.has_more;
    let change = decode_ucs_change(message)?;
    let page = page(
        connector,
        DirectoryFeedKind::UcsProvisioningService,
        sequence,
        sequence.to_be_bytes().to_vec(),
        vec![change],
    )?;
    Ok(LiveDirectoryPage {
        page,
        ucs_confirmation: Some(sequence),
        end_of_snapshot,
    })
}

pub async fn fetch(
    connector: &DirectoryConnector,
    adapter_name: &str,
    previous: Option<&DirectorySyncCursor>,
) -> Result<LiveDirectoryPage, LiveDirectoryError> {
    fetch_with_mode(connector, adapter_name, previous, false).await
}

/// Fetch one bounded native page. A normal synchronization run uses the
/// RFC 4533 refresh-and-persist mode; full imports explicitly use the
/// refresh-only mode so the source can complete its snapshot before the
/// durable worker finalizes it.
pub async fn fetch_with_mode(
    connector: &DirectoryConnector,
    adapter_name: &str,
    previous: Option<&DirectorySyncCursor>,
    full_import: bool,
) -> Result<LiveDirectoryPage, LiveDirectoryError> {
    // Connector rows are persisted and may outlive the code that originally
    // validated them. Revalidate before any socket, bind, or HTTP request so
    // a malformed legacy row cannot use the live adapter as a bypass around
    // provider/feed, TLS, scope, or one-million-user invariants.
    connector
        .validate()
        .map_err(|_| LiveDirectoryError::Configuration)?;
    if !connector.enabled {
        return Err(LiveDirectoryError::Configuration);
    }
    validate_previous_cursor(connector, previous)?;
    let expected = match connector.feed {
        DirectoryFeedKind::Polling => "ldap_polling",
        DirectoryFeedKind::AdDirSync => "ad_dirsync",
        DirectoryFeedKind::Rfc4533 => "rfc4533",
        DirectoryFeedKind::UcsProvisioningService => "ucs_provisioning_service",
    };
    if expected != adapter_name {
        return Err(LiveDirectoryError::Configuration);
    }
    match connector.feed {
        DirectoryFeedKind::Polling => Ok(LiveDirectoryPage {
            page: ldap_search(connector, previous, page_size(connector)?, false).await?,
            ucs_confirmation: None,
            end_of_snapshot: false,
        }),
        DirectoryFeedKind::Rfc4533 => {
            if full_import {
                let (page, end_of_snapshot) =
                    ldap_search_with_mode(connector, previous, page_size(connector)?, true, true)
                        .await?;
                Ok(LiveDirectoryPage {
                    page,
                    ucs_confirmation: None,
                    end_of_snapshot,
                })
            } else {
                Ok(LiveDirectoryPage {
                    page: ldap_search(connector, previous, page_size(connector)?, true).await?,
                    ucs_confirmation: None,
                    end_of_snapshot: false,
                })
            }
        }
        DirectoryFeedKind::AdDirSync => {
            let live = ad_dirsync(connector, previous, page_size(connector)?).await?;
            Ok(live)
        }
        DirectoryFeedKind::UcsProvisioningService => ucs(connector, previous, full_import).await,
    }
}

pub async fn confirm_ucs(
    connector: &DirectoryConnector,
    sequence: u64,
) -> Result<(), LiveDirectoryError> {
    validate_ucs_connector(connector)?;
    if sequence == 0 {
        return Err(LiveDirectoryError::Protocol);
    }
    let subscription = required_env("ERPX_UCS_SUBSCRIPTION")?;
    let password = required_env("ERPX_UCS_SUBSCRIPTION_PASSWORD")?;
    let client = ucs_http_client(connector)?;
    let url = ucs_path(
        &connector.endpoint,
        &subscription,
        &format!("/messages/{sequence}/status"),
    )?;
    let response = client
        .patch(url)
        .basic_auth(&subscription, Some(&password))
        .header(header::ACCEPT, "application/json")
        .header(header::CONTENT_TYPE, "application/json")
        .json(&serde_json::json!({ "status": "ok" }))
        .send()
        .await
        .map_err(|_| LiveDirectoryError::Transport)?;
    verify_https_peer_trust_evidence(&response, connector)?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(LiveDirectoryError::Transport)
    }
}

fn hex_lower(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn connector(
        endpoint: &str,
        server_name: &str,
        ca_certificate_sha256: &str,
    ) -> DirectoryConnector {
        DirectoryConnector {
            id: uuid::Uuid::from_u128(1),
            provider: DirectoryProvider::GenericLdap,
            scope: erpx_identity::directory::DirectoryScope::Platform,
            endpoint: endpoint.into(),
            base_dn: "dc=example,dc=test".into(),
            tls: erpx_identity::directory::DirectoryTlsPolicy {
                mode: erpx_identity::DirectoryTlsMode::Ldaps,
                ca_certificate_sha256: ca_certificate_sha256.into(),
                server_name: server_name.into(),
                minimum_tls_version: "1.2".into(),
            },
            feed: DirectoryFeedKind::Rfc4533,
            credential_ref: "secret-ref".into(),
            enabled: true,
            max_import_users: 1_000_000,
            revision: 1,
        }
    }

    #[test]
    fn transport_tls_policy_binds_endpoint_name_and_trust_fingerprint() {
        let valid = connector(
            "ldaps://ldap.example.test:636",
            "ldap.example.test",
            &"a".repeat(64),
        );
        assert!(validate_tls_policy(&valid, "ldaps").is_ok());

        let mut wrong_name = valid.clone();
        wrong_name.tls.server_name = "other.example.test".into();
        assert!(matches!(
            validate_tls_policy(&wrong_name, "ldaps"),
            Err(LiveDirectoryError::Configuration)
        ));

        let mut wrong_fingerprint = valid.clone();
        wrong_fingerprint.tls.ca_certificate_sha256 = "not-a-fingerprint".into();
        assert!(matches!(
            validate_tls_policy(&wrong_fingerprint, "ldaps"),
            Err(LiveDirectoryError::Configuration)
        ));

        let mut with_query = valid.clone();
        with_query.endpoint.push_str("?starttls=0");
        assert!(matches!(
            validate_tls_policy(&with_query, "ldaps"),
            Err(LiveDirectoryError::Configuration)
        ));

        let mut with_path = valid;
        with_path.endpoint.push_str("/directory");
        assert!(matches!(
            validate_tls_policy(&with_path, "ldaps"),
            Err(LiveDirectoryError::Configuration)
        ));
    }

    #[test]
    fn ucs_transport_requires_the_ucs_provider_and_native_feed() {
        let generic = connector(
            "ldaps://ldap.example.test:636",
            "ldap.example.test",
            &"a".repeat(64),
        );
        assert!(matches!(
            validate_ucs_connector(&generic),
            Err(LiveDirectoryError::Configuration)
        ));

        let ucs = DirectoryConnector {
            provider: DirectoryProvider::UniventionCorporateServer,
            feed: DirectoryFeedKind::UcsProvisioningService,
            endpoint: "https://ucs.example.test".into(),
            tls: erpx_identity::directory::DirectoryTlsPolicy {
                mode: erpx_identity::DirectoryTlsMode::Https,
                ca_certificate_sha256: "a".repeat(64),
                server_name: "ucs.example.test".into(),
                minimum_tls_version: "1.2".into(),
            },
            ..generic
        };
        assert!(validate_ucs_connector(&ucs).is_ok());
    }

    #[test]
    fn https_peer_fingerprint_requires_the_verified_leaf_certificate() {
        let certificate = b"verified-ucs-leaf";
        let fingerprint =
            erpx_identity::directory::tls::sha256_certificate_fingerprint(certificate);
        assert!(verify_peer_certificate_fingerprint(certificate, &fingerprint).is_ok());
        assert!(matches!(
            verify_peer_certificate_fingerprint(certificate, &"0".repeat(64)),
            Err(LiveDirectoryError::TrustEvidenceUnavailable)
        ));
        assert!(matches!(
            verify_peer_certificate_fingerprint(&[], &fingerprint),
            Err(LiveDirectoryError::TrustEvidenceUnavailable)
        ));
    }

    #[test]
    fn ldap_lock_attributes_disable_ad_and_generic_users() {
        let mapping = Mapping {
            provider: DirectoryProvider::ActiveDirectoryDomainServices,
            immutable: "objectGUID".into(),
            login: "sAMAccountName".into(),
            email: "mail".into(),
            display_name: "displayName".into(),
            groups: "memberOf".into(),
            disabled: "userAccountControl".into(),
            locked: "lockoutTime".into(),
            deleted: "isDeleted".into(),
        };
        let entry = SearchEntry {
            dn: "CN=alice,DC=example,DC=test".into(),
            attrs: HashMap::from([
                (
                    "objectGUID".into(),
                    vec![uuid::Uuid::from_u128(1).to_string()],
                ),
                ("sAMAccountName".into(), vec!["alice".into()]),
                ("lockoutTime".into(), vec!["1".into()]),
            ]),
            bin_attrs: HashMap::new(),
        };
        assert_eq!(
            directory_account_status(&entry, &mapping).expect("AD status"),
            DirectoryAccountStatus::Locked
        );
        assert!(
            !map_entry(&entry, &mapping, 1, None)
                .expect("mapped AD user")
                .active
        );

        let generic_mapping = Mapping {
            provider: DirectoryProvider::GenericLdap,
            immutable: "entryUUID".into(),
            login: "uid".into(),
            locked: "pwdAccountLockedTime".into(),
            ..mapping
        };
        let generic_entry = SearchEntry {
            dn: "uid=alice,dc=example,dc=test".into(),
            attrs: HashMap::from([
                (
                    "entryUUID".into(),
                    vec![uuid::Uuid::from_u128(2).to_string()],
                ),
                ("pwdAccountLockedTime".into(), vec!["000001010000Z".into()]),
            ]),
            bin_attrs: HashMap::new(),
        };
        assert_eq!(
            directory_account_status(&generic_entry, &generic_mapping).expect("LDAP status"),
            DirectoryAccountStatus::Locked
        );
        assert!(
            !map_entry(&generic_entry, &generic_mapping, 1, None)
                .expect("mapped LDAP user")
                .active
        );
    }

    #[test]
    fn binary_ldap_immutable_attributes_are_canonicalized_for_auth_linking() {
        let ad_uuid = uuid::Uuid::from_u128(0x0011_2233_4455_6677_8899_aabb_ccdd_eeff);
        let ad_mapping = Mapping {
            provider: DirectoryProvider::ActiveDirectoryDomainServices,
            immutable: "objectGUID".into(),
            login: "sAMAccountName".into(),
            email: "mail".into(),
            display_name: "displayName".into(),
            groups: "memberOf".into(),
            disabled: "userAccountControl".into(),
            locked: "lockoutTime".into(),
            deleted: "isDeleted".into(),
        };
        let ad_entry = SearchEntry {
            dn: "CN=alice,DC=example,DC=test".into(),
            attrs: HashMap::from([("sAMAccountName".into(), vec!["alice".into()])]),
            bin_attrs: HashMap::from([("objectGUID".into(), vec![ad_uuid.to_bytes_le().to_vec()])]),
        };
        let ad_user = map_entry(&ad_entry, &ad_mapping, 1, None).expect("AD user");
        assert_eq!(ad_user.external_object_id, ad_uuid.hyphenated().to_string());

        let ldap_uuid = uuid::Uuid::from_u128(0x1020_3040_5060_7080_90a0_b0c0_d0e0_f000);
        let ldap_mapping = Mapping {
            provider: DirectoryProvider::GenericLdap,
            immutable: "entryUUID".into(),
            login: "uid".into(),
            ..ad_mapping
        };
        let ldap_entry = SearchEntry {
            dn: "uid=alice,dc=example,dc=test".into(),
            attrs: HashMap::from([("uid".into(), vec!["alice".into()])]),
            bin_attrs: HashMap::from([("entryUUID".into(), vec![ldap_uuid.into_bytes().to_vec()])]),
        };
        let ldap_user = map_entry(&ldap_entry, &ldap_mapping, 1, None).expect("LDAP user");
        assert_eq!(
            ldap_user.external_object_id,
            ldap_uuid.hyphenated().to_string()
        );
    }

    #[test]
    fn ambiguous_or_malformed_directory_status_fails_closed() {
        let mapping = Mapping {
            provider: DirectoryProvider::ActiveDirectoryDomainServices,
            immutable: "objectGUID".into(),
            login: "sAMAccountName".into(),
            email: "mail".into(),
            display_name: "displayName".into(),
            groups: "memberOf".into(),
            disabled: "userAccountControl".into(),
            locked: "lockoutTime".into(),
            deleted: "isDeleted".into(),
        };
        let mut malformed = SearchEntry {
            dn: "CN=alice,DC=example,DC=test".into(),
            attrs: HashMap::from([
                (
                    "objectGUID".into(),
                    vec![uuid::Uuid::from_u128(1).to_string()],
                ),
                ("sAMAccountName".into(), vec!["alice".into()]),
                ("userAccountControl".into(), vec!["not-a-number".into()]),
            ]),
            bin_attrs: HashMap::new(),
        };
        assert!(directory_account_status(&malformed, &mapping).is_err());
        assert!(map_entry(&malformed, &mapping, 1, None).is_err());

        malformed
            .attrs
            .insert("userAccountControl".into(), vec!["0".into(), "2".into()]);
        assert!(directory_account_status(&malformed, &mapping).is_err());

        malformed
            .attrs
            .insert("userAccountControl".into(), vec!["0".into()]);
        malformed
            .attrs
            .insert("isDeleted".into(), vec!["indeterminate".into()]);
        assert!(directory_account_status(&malformed, &mapping).is_err());
    }

    #[test]
    fn immutable_directory_uuid_requires_one_unambiguous_provider_value() {
        let mapping = Mapping {
            provider: DirectoryProvider::ActiveDirectoryDomainServices,
            immutable: "objectGUID".into(),
            login: "sAMAccountName".into(),
            email: "mail".into(),
            display_name: "displayName".into(),
            groups: "memberOf".into(),
            disabled: "userAccountControl".into(),
            locked: "lockoutTime".into(),
            deleted: "isDeleted".into(),
        };
        let uuid = uuid::Uuid::from_u128(2);
        let mut entry = SearchEntry {
            dn: "CN=bob,DC=example,DC=test".into(),
            attrs: HashMap::from([("sAMAccountName".into(), vec!["bob".into()])]),
            bin_attrs: HashMap::from([("objectGUID".into(), vec![uuid.to_bytes_le().to_vec()])]),
        };
        assert_eq!(
            immutable_uuid_from_entry(&entry, &mapping, mapping.provider),
            Some(uuid)
        );
        entry.bin_attrs.insert(
            "objectGUID".into(),
            vec![uuid.to_bytes_le().to_vec(), vec![0; 16]],
        );
        assert!(immutable_uuid_from_entry(&entry, &mapping, mapping.provider).is_none());
    }

    #[test]
    fn ucs_boolean_status_rejects_ambiguous_values() {
        let object = serde_json::json!({
            "properties": {"disabled": "possibly"}
        });
        assert!(json_bool(&object, &["disabled"]).is_err());
        assert_eq!(
            json_bool(&serde_json::json!({"disabled": "false"}), &["disabled"]),
            Ok(false)
        );
    }

    #[test]
    fn disabled_connectors_cannot_receive_directory_authentication() {
        let mut connector = connector(
            "ldaps://ldap.example.test:636",
            "ldap.example.test",
            &"a".repeat(64),
        );
        connector.enabled = false;
        assert!(matches!(
            auth_connector_config(&connector),
            Err(LiveDirectoryError::Configuration)
        ));
    }

    #[test]
    fn auth_transport_preflight_rejects_connector_scope_or_identity_drift() {
        let connector = connector(
            "ldaps://ldap.example.test:636",
            "ldap.example.test",
            &"a".repeat(64),
        );
        let transport = LiveDirectoryAuthTransport {
            connector: connector.clone(),
        };
        let expected = auth_connector_config(&connector).expect("auth config");
        assert!(transport.preflight(&expected).is_ok());

        let drifted = AuthConnectorConfig::try_new(
            uuid::Uuid::new_v4(),
            expected.scope(),
            expected.provider(),
            expected.endpoint().to_owned(),
            expected.tls().clone(),
            expected.timeouts(),
            expected.native_change_feed(),
            BreakGlassPolicy::LocalOnly,
        )
        .expect("drifted structural config");
        assert_eq!(
            transport.preflight(&drifted),
            Err(erpx_identity::directory::auth::DirectoryAuthError::InvalidConfiguration)
        );
    }

    #[test]
    fn provider_immutable_uuid_attributes_cannot_be_repointed_to_mutable_logins() {
        assert_eq!(
            immutable_attribute_for_provider(DirectoryProvider::GenericLdap),
            "entryUUID"
        );
        assert_eq!(
            immutable_attribute_for_provider(DirectoryProvider::ActiveDirectoryDomainServices),
            "objectGUID"
        );
        assert!(
            validate_immutable_attribute_override(
                DirectoryProvider::GenericLdap,
                Some("ENTRYuuid")
            )
            .is_ok()
        );
        assert!(
            validate_immutable_attribute_override(DirectoryProvider::GenericLdap, Some("uid"))
                .is_err()
        );
        assert!(
            validate_immutable_attribute_override(
                DirectoryProvider::ActiveDirectoryDomainServices,
                Some("sAMAccountName")
            )
            .is_err()
        );
    }

    #[test]
    fn tls_evidence_protocol_matches_the_enforced_connector_minimum() {
        let mut connector = connector(
            "ldaps://ldap.example.test:636",
            "ldap.example.test",
            &"a".repeat(64),
        );
        assert_eq!(
            tls_evidence_protocol(&connector).expect("TLS 1.2 policy"),
            TlsProtocolVersion::Tls1_2
        );
        connector.tls.minimum_tls_version = "1.3".into();
        assert_eq!(
            tls_evidence_protocol(&connector).expect("TLS 1.3 policy"),
            TlsProtocolVersion::Tls1_3
        );
    }

    #[test]
    fn private_ca_bundle_parser_fails_closed_on_malformed_or_empty_input() {
        assert!(matches!(
            root_store_from_pem(b""),
            Err(LiveDirectoryError::Configuration)
        ));
        assert!(matches!(
            root_store_from_pem(b"not a certificate"),
            Err(LiveDirectoryError::Configuration)
        ));
        assert!(matches!(
            root_store_from_pem(
                b"-----BEGIN CERTIFICATE-----\nnot-base64\n-----END CERTIFICATE-----"
            ),
            Err(LiveDirectoryError::Configuration)
        ));
    }

    #[tokio::test]
    async fn fetch_revalidates_connector_before_network_access() {
        let mut invalid = connector(
            "https://ldap.example.test",
            "ldap.example.test",
            &"a".repeat(64),
        );
        invalid.tls.mode = erpx_identity::DirectoryTlsMode::Https;
        assert!(matches!(
            fetch(&invalid, "rfc4533", None).await,
            Err(LiveDirectoryError::Configuration)
        ));
    }

    #[test]
    fn dirsync_request_is_critical_and_ber_encoded() {
        let control = ad_dirsync_request(&[1, 2, 3], 500);
        assert_eq!(control.ctype, AD_DIRSYNC_OID);
        assert!(control.crit);
        assert!(
            control
                .val
                .as_deref()
                .is_some_and(|value| value.starts_with(&[0x30]))
        );
    }

    #[test]
    fn dirsync_response_parser_rejects_trailing_bytes() {
        assert!(ad_dirsync_response(&[0x30, 0x00, 0x00]).is_none());

        let mut body = Vec::new();
        body.extend(ber_integer(1));
        body.extend(ber_integer(0));
        body.extend(ber_tlv(0x04, &[7, 8, 9]));
        body.push(0x00);
        let response = ber_tlv(0x30, &body);
        assert!(ad_dirsync_response(&response).is_none());
    }

    #[test]
    fn dirsync_response_parser_reads_integer_more_results_and_opaque_cookie() {
        let mut body = Vec::new();
        body.extend(ber_integer(1));
        body.extend(ber_integer(0));
        body.extend(ber_tlv(0x04, &[7, 8, 9]));
        let response = ber_tlv(0x30, &body);
        assert_eq!(ad_dirsync_response(&response), Some((true, vec![7, 8, 9])));
    }

    #[test]
    fn dirsync_response_parser_accepts_integer_false() {
        let mut body = Vec::new();
        body.extend(ber_integer(0));
        body.extend(ber_integer(0));
        body.extend(ber_tlv(0x04, &[]));
        let response = ber_tlv(0x30, &body);
        assert_eq!(ad_dirsync_response(&response), Some((false, vec![])));
    }

    #[test]
    fn dirsync_request_includes_critical_delete_controls() {
        let controls = ad_dirsync_controls(&[1, 2, 3], 500);
        assert_eq!(controls.len(), 3);
        assert_eq!(controls[0].ctype, AD_DIRSYNC_OID);
        for (control, oid) in controls
            .iter()
            .skip(1)
            .zip([AD_SHOW_DELETED_OID, AD_EXTENDED_DN_OID])
        {
            assert_eq!(control.ctype, oid);
            assert!(control.crit);
            assert!(control.val.is_none());
        }
    }

    #[test]
    fn dirsync_response_parser_rejects_boolean_more_results() {
        let mut body = Vec::new();
        body.extend(ber_tlv(0x01, &[0xff]));
        body.extend(ber_integer(0));
        body.extend(ber_tlv(0x04, &[7, 8, 9]));
        assert!(ad_dirsync_response(&ber_tlv(0x30, &body)).is_none());
    }

    #[test]
    fn native_feed_continuation_controls_are_required() {
        assert!(matches!(
            native_ldap_cookie(&[], false),
            Err(LiveDirectoryError::Protocol)
        ));
        assert!(matches!(
            native_ldap_cookie(&[], true),
            Err(LiveDirectoryError::Protocol)
        ));
        assert!(matches!(
            ad_dirsync_control(&[]),
            Err(LiveDirectoryError::Protocol)
        ));
    }

    #[test]
    fn rfc4533_entry_requires_one_well_formed_sync_state_control() {
        assert!(matches!(
            parse_rfc4533_entry_sync_state(&[]),
            Err(LiveDirectoryError::Protocol)
        ));

        let mut fields = Vec::new();
        fields.extend(ber_tlv(0x0a, &[0]));
        fields.extend(ber_tlv(0x04, &[1; 16]));
        let valid = Control(
            None,
            RawControl {
                ctype: RFC4533_SYNC_STATE_OID.into(),
                crit: false,
                val: Some(ber_tlv(0x30, &fields)),
            },
        );
        assert!(parse_rfc4533_entry_sync_state(std::slice::from_ref(&valid)).is_ok());
        assert!(matches!(
            parse_rfc4533_entry_sync_state(&[valid.clone(), valid]),
            Err(LiveDirectoryError::Protocol)
        ));
        assert!(matches!(
            parse_rfc4533_entry_sync_state(&[Control(
                None,
                RawControl {
                    ctype: RFC4533_SYNC_STATE_OID.into(),
                    crit: false,
                    val: Some(vec![0x30, 0x00]),
                },
            )]),
            Err(LiveDirectoryError::Protocol)
        ));
    }

    #[test]
    fn rfc4533_uses_persistent_mode_for_incremental_polls_when_advertised() {
        assert!(matches!(
            rfc4533_refresh_mode(true, true),
            RefreshMode::RefreshOnly
        ));
        assert!(matches!(
            rfc4533_refresh_mode(false, true),
            RefreshMode::RefreshAndPersist
        ));
        assert!(matches!(
            rfc4533_refresh_mode(false, false),
            RefreshMode::RefreshOnly
        ));
    }

    #[test]
    fn rfc4533_persistence_requires_root_dse_advertisement() {
        let advertised = SearchEntry {
            dn: "".into(),
            attrs: HashMap::from([(
                "supportedControl".into(),
                vec![RFC4533_SYNC_REQUEST_OID.into()],
            )]),
            bin_attrs: HashMap::new(),
        };
        assert!(root_dse_advertises_rfc4533(&advertised));

        let not_advertised = SearchEntry {
            attrs: HashMap::from([("supportedControl".into(), vec!["1.2.3".into()])]),
            ..advertised
        };
        assert!(!root_dse_advertises_rfc4533(&not_advertised));
    }

    #[test]
    fn rfc4533_full_import_cursor_round_trips_both_native_cookies() {
        let encoded = encode_rfc4533_full_import_cursor(&[1, 2, 3], &[9, 8])
            .expect("bounded composite cursor");
        assert_eq!(
            decode_rfc4533_full_import_cursor(&encoded).expect("composite cursor"),
            (vec![1, 2, 3], vec![9, 8])
        );
        assert_eq!(
            decode_rfc4533_full_import_cursor(&[4, 5]).expect("legacy native cursor"),
            (vec![4, 5], Vec::new())
        );
    }

    #[test]
    fn rfc4533_full_import_cursor_rejects_truncated_or_oversized_frames() {
        let mut truncated = RFC4533_FULL_IMPORT_CURSOR_MAGIC.to_vec();
        truncated.extend_from_slice(&1_u32.to_be_bytes());
        assert!(matches!(
            decode_rfc4533_full_import_cursor(&truncated),
            Err(LiveDirectoryError::Protocol)
        ));
        assert!(matches!(
            encode_rfc4533_full_import_cursor(&[0; MAX_DIRECTORY_ADAPTER_CURSOR_BYTES], &[1]),
            Err(LiveDirectoryError::Protocol)
        ));
    }

    #[test]
    fn rfc4533_terminal_refresh_rejects_an_empty_cookie() {
        let control = RawControl {
            ctype: RFC4533_SYNC_DONE_OID.into(),
            crit: false,
            val: Some(ber_tlv(0x30, &[])),
        };
        assert!(matches!(
            native_ldap_cookie(&[Control(None, control)], true),
            Err(LiveDirectoryError::Protocol)
        ));
    }

    #[test]
    fn rfc4533_sync_done_obeys_optional_cookie_and_rejects_extra_fields() {
        let mut valid_fields = Vec::new();
        valid_fields.extend(ber_tlv(0x04, &[1, 2]));
        valid_fields.extend(ber_tlv(0x01, &[0]));
        let valid = RawControl {
            ctype: RFC4533_SYNC_DONE_OID.into(),
            crit: false,
            val: Some(ber_tlv(0x30, &valid_fields)),
        };
        assert!(matches!(
            native_ldap_cookie(&[Control(None, valid)], true),
            Ok(Some(cookie)) if cookie == vec![1, 2]
        ));

        let mut invalid_fields = Vec::new();
        invalid_fields.extend(ber_tlv(0x01, &[0]));
        invalid_fields.extend(ber_tlv(0x01, &[0]));
        let invalid = RawControl {
            ctype: RFC4533_SYNC_DONE_OID.into(),
            crit: false,
            val: Some(ber_tlv(0x30, &invalid_fields)),
        };
        assert!(matches!(
            native_ldap_cookie(&[Control(None, invalid)], true),
            Err(LiveDirectoryError::Protocol)
        ));

        let mut no_cookie_fields = Vec::new();
        no_cookie_fields.extend(ber_tlv(0x01, &[0]));
        let no_cookie = RawControl {
            ctype: RFC4533_SYNC_DONE_OID.into(),
            crit: false,
            val: Some(ber_tlv(0x30, &no_cookie_fields)),
        };
        assert!(matches!(
            native_ldap_cookie(&[Control(None, no_cookie)], true),
            Err(LiveDirectoryError::Protocol)
        ));
    }

    #[test]
    fn ucs_change_uses_immutable_id_and_append_only_state() {
        let change = ucs_change(
            &serde_json::json!({
                "old": {},
                "new": {"id": "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa", "username": "alice", "mail": "a@example.test"}
            }),
            7,
        )
        .expect("change");
        assert!(
            matches!(change, DirectoryChange::Upsert(DirectoryUser { external_object_id, .. }) if external_object_id == "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa")
        );
    }

    #[test]
    fn ucs_deleted_flag_emits_a_delete_for_the_immutable_uuid() {
        let change = ucs_change(
            &serde_json::json!({
                "old": {"id": "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb"},
                "new": {"id": "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb", "deleted": true}
            }),
            9,
        )
        .expect("deleted change");
        assert!(matches!(
            change,
            DirectoryChange::Delete { external_object_id, source_revision }
                if external_object_id == "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb"
                    && source_revision == 9
        ));
    }

    #[test]
    fn ucs_replacement_cannot_rebind_an_immutable_identity() {
        let result = ucs_change(
            &serde_json::json!({
                "old": {"id": "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa"},
                "new": {"id": "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb", "username": "alice"}
            }),
            10,
        );
        assert_eq!(result, Err(LiveDirectoryError::Protocol));
    }

    #[test]
    fn ucs_change_reads_nested_properties_for_disable_and_delete_state() {
        let change = ucs_change(
            &serde_json::json!({
                "old": {"id": "22222222-2222-2222-2222-222222222222"},
                "new": {"id": "22222222-2222-2222-2222-222222222222", "properties": {"disabled": true}}
            }),
            8,
        )
        .expect("nested disabled change");
        assert!(matches!(
            change,
            DirectoryChange::Disable { external_object_id, source_revision }
                if external_object_id == "22222222-2222-2222-2222-222222222222" && source_revision == 8
        ));
    }

    #[test]
    fn ucs_empty_snapshot_message_fails_closed() {
        let message = UcsMessage {
            body: None,
            sequence_number: 1,
            end_of_snapshot: true,
            has_more: false,
        };
        assert_eq!(
            decode_ucs_change(message),
            Err(LiveDirectoryError::Protocol)
        );
        assert_eq!(
            ucs_change(&serde_json::json!({}), 1),
            Err(LiveDirectoryError::Protocol)
        );
    }

    #[test]
    fn ucs_sequence_requires_exact_continuity_after_the_initial_message() {
        assert!(matches!(
            validate_ucs_sequence(0, 0),
            Err(LiveDirectoryError::Protocol)
        ));
        assert!(validate_ucs_sequence(0, 41).is_ok());
        assert!(matches!(
            validate_ucs_sequence(41, 41),
            Err(LiveDirectoryError::AlreadyCommitted(41))
        ));
        assert!(validate_ucs_sequence(41, 42).is_ok());
        assert!(matches!(
            validate_ucs_sequence(41, 43),
            Err(LiveDirectoryError::Protocol)
        ));
        assert!(matches!(
            validate_ucs_sequence(41, 40),
            Err(LiveDirectoryError::Protocol)
        ));
    }

    #[tokio::test]
    async fn ucs_confirmation_rejects_zero_without_network_or_credentials() {
        let connector = DirectoryConnector {
            provider: DirectoryProvider::UniventionCorporateServer,
            feed: DirectoryFeedKind::UcsProvisioningService,
            endpoint: "https://ucs.example.test".into(),
            tls: erpx_identity::DirectoryTlsPolicy {
                mode: erpx_identity::DirectoryTlsMode::Https,
                ca_certificate_sha256: "a".repeat(64),
                server_name: "ucs.example.test".into(),
                minimum_tls_version: "1.2".into(),
            },
            scope: erpx_identity::directory::DirectoryScope::Platform,
            base_dn: "dc=example,dc=test".into(),
            credential_ref: "ucs-secret".into(),
            enabled: true,
            max_import_users: 1_000_000,
            revision: 1,
            id: uuid::Uuid::from_u128(12),
        };
        assert!(matches!(
            confirm_ucs(&connector, 0).await,
            Err(LiveDirectoryError::Protocol)
        ));
    }

    #[test]
    fn directory_cursor_sequence_overflow_fails_closed() {
        let previous = DirectorySyncCursor {
            connector_id: uuid::Uuid::from_u128(1),
            feed: DirectoryFeedKind::Rfc4533,
            sequence: u64::MAX,
            opaque_cookie: None,
            observed_at: Utc::now(),
        };
        assert!(matches!(
            cursor_sequence(Some(&previous)),
            Err(LiveDirectoryError::SequenceOverflow)
        ));
        assert_eq!(cursor_sequence(None).expect("initial sequence"), 1);
    }

    #[test]
    fn predecessor_cursor_must_belong_to_the_same_connector_and_feed() {
        let valid = connector(
            "ldaps://ldap.example.test:636",
            "ldap.example.test",
            &"a".repeat(64),
        );
        let mut wrong_connector = DirectorySyncCursor {
            connector_id: uuid::Uuid::from_u128(2),
            feed: DirectoryFeedKind::Rfc4533,
            sequence: 1,
            opaque_cookie: Some(URL_SAFE_NO_PAD.encode([1, 2, 3])),
            observed_at: Utc::now(),
        };
        assert!(matches!(
            validate_previous_cursor(&valid, Some(&wrong_connector)),
            Err(LiveDirectoryError::Configuration)
        ));
        wrong_connector.connector_id = valid.id;
        wrong_connector.feed = DirectoryFeedKind::Polling;
        assert!(matches!(
            validate_previous_cursor(&valid, Some(&wrong_connector)),
            Err(LiveDirectoryError::Configuration)
        ));
    }
}
