//! Live, TLS-only directory authentication and connector preflight.
//!
//! This adapter is owned by Identity rather than a particular executable, so
//! Server and Worker can compose the same fail-closed transport without a
//! runtime-to-runtime dependency.

use std::{env, sync::Arc, time::Duration};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::Utc;
use ldap3::{LdapConnAsync, LdapConnSettings, Scope, SearchEntry};
use reqwest::{Client, Url, header};
use rustls::{ClientConfig, RootCertStore, pki_types::CertificateDer};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use zeroize::Zeroizing;

use super::auth::{
    AccessRisk, AuthenticatedDirectoryIdentity, BreakGlassPolicy, DirectoryAccountStatus,
    DirectoryAuthenticationRequest, DirectoryAuthenticator, DirectoryBindRequest,
    DirectoryBindResult, DirectoryConnectorConfig as AuthConnectorConfig,
    DirectoryIdentity as AuthDirectoryIdentity, DirectoryProvider as AuthDirectoryProvider,
    DirectoryTransport, DirectoryTransportError, DirectoryUsername, NativeChangeFeed,
    RedactedSecret,
};
use super::secrets::DirectorySecretResolver;
use super::tls::{
    CertificateFingerprint, TimeoutPolicy, TlsHandshakeEvidence, TlsPolicy, TlsProtocolVersion,
    TrustAnchor,
};
use super::{
    DirectoryConnector, DirectoryConnectorRuntimeSettings, DirectoryFeedKind, DirectoryProvider,
    DirectoryScope, DirectoryTlsMode,
};

const LDAP_TIMEOUT: Duration = Duration::from_secs(30);
const AD_DIRSYNC_OID: &str = "1.2.840.113556.1.4.841";
const RFC4533_SYNC_REQUEST_OID: &str = "1.3.6.1.4.1.4203.1.9.1.1";
const DIRECTORY_CA_CERT_PATH_ENV: &str = "ERPX_DIRECTORY_CA_CERT_PATH";
const MAX_CA_CERT_PATH_BYTES: usize = 4_096;
const MAX_CA_CERTIFICATE_BYTES: usize = 128 * 1024;
const MAX_DIRECTORY_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const MAX_PAGE_SIZE: u32 = 5_000;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LiveDirectoryError {
    #[error("directory live configuration is incomplete")]
    Configuration,
    #[error("directory live transport failed")]
    Transport,
    #[error("directory credentials were rejected")]
    InvalidCredentials,
    #[error("directory account policy denied authentication")]
    AccountDenied,
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
    fn from_persisted(
        provider: DirectoryProvider,
        configured: &Value,
    ) -> Result<Self, LiveDirectoryError> {
        let object = configured
            .as_object()
            .ok_or(LiveDirectoryError::Configuration)?;
        let value = |key: &str, default: &str| -> Result<String, LiveDirectoryError> {
            match object.get(key) {
                None => Ok(default.to_owned()),
                Some(Value::String(value)) if !value.trim().is_empty() => {
                    Ok(value.trim().to_owned())
                }
                Some(_) => Err(LiveDirectoryError::Configuration),
            }
        };
        let immutable = immutable_attribute_for_provider(provider);
        validate_immutable_attribute_override(
            provider,
            object.get("immutable").and_then(Value::as_str),
        )?;
        let mapping = Self {
            provider,
            immutable: immutable.into(),
            login: value(
                "login",
                if provider == DirectoryProvider::ActiveDirectoryDomainServices {
                    "sAMAccountName"
                } else {
                    "uid"
                },
            )?,
            email: value("email", "mail")?,
            display_name: value("display_name", "displayName")?,
            groups: value("groups", "memberOf")?,
            disabled: value(
                "disabled",
                if provider == DirectoryProvider::ActiveDirectoryDomainServices {
                    "userAccountControl"
                } else {
                    "nsAccountLock"
                },
            )?,
            locked: value(
                "locked",
                if provider == DirectoryProvider::ActiveDirectoryDomainServices {
                    "lockoutTime"
                } else {
                    "pwdAccountLockedTime"
                },
            )?,
            deleted: value(
                "deleted",
                if provider == DirectoryProvider::ActiveDirectoryDomainServices {
                    "isDeleted"
                } else {
                    "entryDeleted"
                },
            )?,
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

fn default_connector_filter(connector: &DirectoryConnector) -> String {
    if connector.provider == DirectoryProvider::ActiveDirectoryDomainServices {
        "(&(objectCategory=person)(objectClass=user))".into()
    } else {
        "(objectClass=person)".into()
    }
}

fn validate_runtime_settings(
    connector: &DirectoryConnector,
    settings: &DirectoryConnectorRuntimeSettings,
) -> Result<(), LiveDirectoryError> {
    if settings.user_filter.trim().is_empty()
        || settings.user_filter.len() > 2_048
        || settings.user_filter.chars().any(char::is_control)
        || !settings.user_filter.starts_with('(')
        || !settings.user_filter.ends_with(')')
        || settings.page_size == 0
        || settings.page_size > MAX_PAGE_SIZE
        || settings.page_size > connector.max_import_users as u32
        || settings.ca_reference.trim().is_empty()
        || !settings.attribute_mapping.is_object()
        || !settings.feed_configuration.is_object()
    {
        return Err(LiveDirectoryError::Configuration);
    }
    Mapping::from_persisted(connector.provider, &settings.attribute_mapping)?;
    Ok(())
}

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
        || !fingerprint.chars().all(|value| value.is_ascii_hexdigit())
        || !matches!(connector.tls.minimum_tls_version.as_str(), "1.2" | "1.3")
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
        || (expected_scheme != "https" && !matches!(endpoint.path(), "" | "/"))
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

fn ldap_tls_config(
    minimum_tls_version: &str,
    ca_certificate_pem: Option<&[u8]>,
) -> Result<Option<Arc<ClientConfig>>, LiveDirectoryError> {
    let Some(certificates) = private_ca_certificates(ca_certificate_pem)? else {
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

fn private_ca_certificates(
    ca_certificate_pem: Option<&[u8]>,
) -> Result<Option<Vec<Vec<u8>>>, LiveDirectoryError> {
    if let Some(pem) = ca_certificate_pem {
        if pem.is_empty() || pem.len() > MAX_CA_CERTIFICATE_BYTES {
            return Err(LiveDirectoryError::Configuration);
        }
        return Ok(Some(parse_pem_certificates(pem)?));
    }
    let Some(path) = env::var(DIRECTORY_CA_CERT_PATH_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(None);
    };
    if path.len() > MAX_CA_CERT_PATH_BYTES || path.chars().any(char::is_control) {
        return Err(LiveDirectoryError::Configuration);
    }
    let pem = std::fs::read(path).map_err(|_| LiveDirectoryError::Configuration)?;
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

fn ucs_http_client(
    connector: &DirectoryConnector,
    ca_certificate_pem: Option<&[u8]>,
) -> Result<Client, LiveDirectoryError> {
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
    if let Some(certificates) = private_ca_certificates(ca_certificate_pem)? {
        for certificate in certificates {
            builder = builder.add_root_certificate(
                reqwest::Certificate::from_der(&certificate)
                    .map_err(|_| LiveDirectoryError::Configuration)?,
            );
        }
    }
    builder
        .build()
        .map_err(|_| LiveDirectoryError::Configuration)
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
    let peer_fingerprint = super::tls::sha256_certificate_fingerprint(certificate);
    if !peer_fingerprint.eq_ignore_ascii_case(connector.tls.ca_certificate_sha256.trim()) {
        return Err(LiveDirectoryError::TrustEvidenceUnavailable);
    }
    Ok(certificate.to_vec())
}

async fn open_ldap<'a>(
    connector: &'a DirectoryConnector,
    bind_dn: &'a str,
    password: &'a str,
    ca_certificate_pem: Option<&[u8]>,
) -> Result<(ldap3::Ldap, Vec<u8>), LiveDirectoryError> {
    let expected_scheme = match connector.tls.mode {
        DirectoryTlsMode::Ldaps => "ldaps",
        DirectoryTlsMode::StartTls => "ldap",
        DirectoryTlsMode::Https => return Err(LiveDirectoryError::Configuration),
    };
    validate_tls_policy(connector, expected_scheme)?;
    let mut settings = LdapConnSettings::new().set_conn_timeout(LDAP_TIMEOUT);
    if let Some(config) = ldap_tls_config(&connector.tls.minimum_tls_version, ca_certificate_pem)? {
        settings = settings.set_config(config);
    }
    settings = settings.set_starttls(matches!(connector.tls.mode, DirectoryTlsMode::StartTls));
    let (connection, mut ldap) = LdapConnAsync::with_settings(settings, &connector.endpoint)
        .await
        .map_err(|_| LiveDirectoryError::Transport)?;
    ldap3::drive!(connection);
    let peer_certificate = ldap
        .get_peer_certificate()
        .await
        .map_err(|_| LiveDirectoryError::TrustEvidenceUnavailable)?
        .ok_or(LiveDirectoryError::TrustEvidenceUnavailable)?;
    let peer_fingerprint = super::tls::sha256_certificate_fingerprint(&peer_certificate);
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
    ca_certificate_pem: Option<Arc<Vec<u8>>>,
    mapping: Mapping,
    user_filter: String,
    service_bind: Option<Arc<ServiceBind>>,
}

struct ServiceBind {
    bind_dn: Zeroizing<String>,
    password: Zeroizing<String>,
}

impl DirectoryTransport for LiveDirectoryAuthTransport {
    fn preflight(
        &self,
        config: &AuthConnectorConfig,
    ) -> Result<(), super::auth::DirectoryAuthError> {
        let expected = auth_connector_config(&self.connector)
            .map_err(|_| super::auth::DirectoryAuthError::InvalidConfiguration)?;
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
            return Err(super::auth::DirectoryAuthError::InvalidConfiguration);
        }
        Ok(())
    }

    fn bind<'a>(
        &'a self,
        request: DirectoryBindRequest<'a>,
    ) -> super::auth::DirectoryBindFuture<'a> {
        let connector = self.connector.clone();
        if request.connector_ref() != connector.id {
            return Box::pin(async { Err(DirectoryTransportError::ProtocolViolation) });
        }
        let username = request.username().as_str().to_owned();
        let password = request.credential().clone();
        let connector_ref = request.connector_ref();
        let mapping = self.mapping.clone();
        let user_filter = self.user_filter.clone();
        let service_bind = self.service_bind.clone();
        Box::pin(async move {
            if connector.provider == DirectoryProvider::UniventionCorporateServer {
                return ucs_bind_user(
                    &connector,
                    connector_ref,
                    &username,
                    password.expose_to_transport(),
                    self.ca_certificate_pem.as_deref().map(Vec::as_slice),
                )
                .await;
            }
            let map_transport_error = |error| match error {
                LiveDirectoryError::TrustEvidenceUnavailable => {
                    DirectoryTransportError::CertificateValidationFailed
                }
                LiveDirectoryError::Configuration | LiveDirectoryError::Protocol => {
                    DirectoryTransportError::ProtocolViolation
                }
                _ => DirectoryTransportError::Unavailable,
            };
            let filter = format!(
                "(&{}({}={}))",
                user_filter,
                mapping.login,
                escape_ldap_filter_value(&username),
            );
            let (entry, peer_certificate) = if let Some(service_bind) = service_bind {
                let (mut service_ldap, _) = open_ldap(
                    &connector,
                    service_bind.bind_dn.as_str(),
                    service_bind.password.as_str(),
                    self.ca_certificate_pem.as_deref().map(Vec::as_slice),
                )
                .await
                .map_err(map_transport_error)?;
                let entry =
                    find_auth_entry(&mut service_ldap, &connector, &mapping, &filter).await?;
                let user_dn = entry.dn.clone();
                let _ = service_ldap.unbind().await;
                let (mut user_ldap, peer_certificate) = open_ldap(
                    &connector,
                    &user_dn,
                    password.expose_to_transport(),
                    self.ca_certificate_pem.as_deref().map(Vec::as_slice),
                )
                .await
                .map_err(map_transport_error)?;
                let _ = user_ldap.unbind().await;
                (entry, peer_certificate)
            } else {
                let (mut user_ldap, peer_certificate) = open_ldap(
                    &connector,
                    &username,
                    password.expose_to_transport(),
                    self.ca_certificate_pem.as_deref().map(Vec::as_slice),
                )
                .await
                .map_err(map_transport_error)?;
                let entry = find_auth_entry(&mut user_ldap, &connector, &mapping, &filter).await?;
                (entry, peer_certificate)
            };
            let immutable = immutable_uuid_from_entry(&entry, &mapping, connector.provider)
                .ok_or(DirectoryTransportError::ProtocolViolation)?;
            let account_status = directory_account_status(&entry, &mapping)
                .map_err(|_| DirectoryTransportError::ProtocolViolation)?;
            let provider = auth_provider(connector.provider);
            let key = super::auth::ExternalIdentityKey::try_for_provider(provider, immutable)
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

async fn find_auth_entry(
    ldap: &mut ldap3::Ldap,
    connector: &DirectoryConnector,
    mapping: &Mapping,
    filter: &str,
) -> Result<SearchEntry, DirectoryTransportError> {
    let (entries, _) = ldap
        .search(&connector.base_dn, Scope::Subtree, filter, mapping.attrs())
        .await
        .map_err(|_| DirectoryTransportError::Unavailable)?
        .success()
        .map_err(|_| DirectoryTransportError::Rejected)?;
    if entries.len() != 1 {
        return Err(DirectoryTransportError::Rejected);
    }
    Ok(SearchEntry::construct(
        entries
            .into_iter()
            .next()
            .ok_or(DirectoryTransportError::ProtocolViolation)?,
    ))
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
        DirectoryTlsMode::Ldaps => TlsPolicy::ldaps(connector.tls.server_name.clone(), anchor),
        DirectoryTlsMode::StartTls => {
            TlsPolicy::starttls(connector.tls.server_name.clone(), anchor)
        }
        DirectoryTlsMode::Https => TlsPolicy::https(connector.tls.server_name.clone(), anchor),
    }
    .map_err(|_| LiveDirectoryError::Configuration)
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
        DirectoryScope::Platform => super::auth::ConnectorScope::Platform,
        DirectoryScope::Holding { data_profile_key } => {
            super::auth::ConnectorScope::Holding(stable_holding_id(data_profile_key))
        }
        DirectoryScope::Branch {
            data_profile_key,
            company_id,
            branch_id,
        } => super::auth::ConnectorScope::Branch {
            holding_id: stable_holding_id(data_profile_key),
            company_id: *company_id,
            branch_id: *branch_id,
        },
    };
    let feed = match connector.provider {
        DirectoryProvider::GenericLdap => NativeChangeFeed::Rfc4533,
        DirectoryProvider::ActiveDirectoryDomainServices => NativeChangeFeed::AdDirSync,
        DirectoryProvider::UniventionCorporateServer => NativeChangeFeed::UcsProvisioningService,
    };
    AuthConnectorConfig::try_new(
        connector.id,
        scope,
        auth_provider(connector.provider),
        connector.endpoint.clone(),
        auth_tls_policy(connector)?,
        TimeoutPolicy::conservative(),
        feed,
        BreakGlassPolicy::LocalOnly,
    )
    .map_err(|_| LiveDirectoryError::Configuration)
}

fn stable_holding_id(data_profile_key: &str) -> uuid::Uuid {
    data_profile_key.parse::<uuid::Uuid>().unwrap_or_else(|_| {
        uuid::Uuid::new_v5(
            &uuid::Uuid::NAMESPACE_URL,
            format!("erpx:holding-scope:{data_profile_key}").as_bytes(),
        )
    })
}

/// Evidence produced by a real, non-mutating bind/feed capability probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryPreflightEvidence {
    pub authority_fingerprint: String,
}

pub async fn preflight_directory_connector(
    connector: &DirectoryConnector,
    resolver: &dyn DirectorySecretResolver,
) -> Result<DirectoryPreflightEvidence, LiveDirectoryError> {
    connector
        .validate()
        .map_err(|_| LiveDirectoryError::Configuration)?;
    let secret = resolver
        .resolve(directory_credential_resolver_key(connector))
        .await
        .map_err(|_| LiveDirectoryError::Configuration)?;
    if connector.provider == DirectoryProvider::UniventionCorporateServer {
        validate_ucs_connector(connector)?;
        let subscription = secret
            .subscription()
            .ok_or(LiveDirectoryError::Configuration)?;
        let client = ucs_http_client(connector, Some(secret.ca_certificate_pem()))?;
        let url = ucs_path(&connector.endpoint, subscription, "/messages/next")?;
        let response = client
            .get(url)
            .query(&[("timeout", "0"), ("pop", "false")])
            .basic_auth(subscription, Some(secret.password()))
            .header(header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|_| LiveDirectoryError::Transport)?;
        verified_https_peer_certificate(&response, connector)?;
        if response.status().is_success() || response.status() == reqwest::StatusCode::NO_CONTENT {
            return Ok(DirectoryPreflightEvidence {
                authority_fingerprint: directory_authority_fingerprint(
                    connector,
                    &[connector.tls.server_name.as_str()],
                )?,
            });
        }
        return Err(LiveDirectoryError::Transport);
    }
    let (mut ldap, _) = open_ldap(
        connector,
        secret.bind_dn(),
        secret.password(),
        Some(secret.ca_certificate_pem()),
    )
    .await?;
    let (entries, _) = ldap
        .search(
            "",
            Scope::Base,
            "(objectClass=*)",
            vec![
                "supportedControl",
                "namingContexts",
                "rootDomainNamingContext",
                "configurationNamingContext",
                "schemaNamingContext",
            ],
        )
        .await
        .map_err(|_| LiveDirectoryError::Transport)?
        .success()
        .map_err(|_| LiveDirectoryError::Transport)?;
    if entries.len() != 1 {
        return Err(LiveDirectoryError::Protocol);
    }
    let root = SearchEntry::construct(
        entries
            .into_iter()
            .next()
            .ok_or(LiveDirectoryError::Protocol)?,
    );
    let controls = root
        .attrs
        .get("supportedControl")
        .cloned()
        .unwrap_or_default();
    let feed_supported = match connector.feed {
        DirectoryFeedKind::Polling => true,
        DirectoryFeedKind::Rfc4533 => controls
            .iter()
            .any(|value| value == RFC4533_SYNC_REQUEST_OID),
        DirectoryFeedKind::AdDirSync => controls.iter().any(|value| value == AD_DIRSYNC_OID),
        DirectoryFeedKind::UcsProvisioningService => false,
    };
    if !feed_supported {
        return Err(LiveDirectoryError::Protocol);
    }
    let mut authority_values = root
        .attrs
        .iter()
        .filter(|(key, _)| {
            matches!(
                key.to_ascii_lowercase().as_str(),
                "namingcontexts"
                    | "rootdomainnamingcontext"
                    | "configurationnamingcontext"
                    | "schemanamingcontext"
            )
        })
        .flat_map(|(_, values)| values.iter().map(String::as_str))
        .collect::<Vec<_>>();
    authority_values.sort_unstable_by_key(|value| value.to_ascii_lowercase());
    authority_values.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    if authority_values.is_empty() {
        return Err(LiveDirectoryError::Protocol);
    }
    Ok(DirectoryPreflightEvidence {
        authority_fingerprint: directory_authority_fingerprint(connector, &authority_values)?,
    })
}

fn directory_authority_fingerprint(
    connector: &DirectoryConnector,
    values: &[&str],
) -> Result<String, LiveDirectoryError> {
    if values.is_empty()
        || values.iter().any(|value| {
            value.trim().is_empty() || value.len() > 2_048 || value.chars().any(char::is_control)
        })
    {
        return Err(LiveDirectoryError::Protocol);
    }
    let mut normalized_values = values
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();
    normalized_values.sort_unstable();
    normalized_values.dedup();
    let mut digest = Sha256::new();
    digest.update(b"erpx-directory-authority\0");
    digest.update(match connector.provider {
        DirectoryProvider::GenericLdap => b"generic_ldap".as_slice(),
        DirectoryProvider::ActiveDirectoryDomainServices => {
            b"active_directory_domain_services".as_slice()
        }
        DirectoryProvider::UniventionCorporateServer => b"univention_corporate_server".as_slice(),
    });
    for value in normalized_values {
        digest.update([0]);
        digest.update(value.as_bytes());
    }
    Ok(hex_lower(&digest.finalize()))
}

fn directory_credential_resolver_key(connector: &DirectoryConnector) -> &str {
    let is_migrated_legacy = connector.credential_profile_key == "legacy-unconfigured"
        || connector
            .credential_profile_key
            .strip_prefix("legacy-")
            .is_some_and(|suffix| {
                suffix.len() == 32 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
            });
    if is_migrated_legacy {
        &connector.credential_ref
    } else {
        &connector.credential_profile_key
    }
}

pub async fn authenticate_directory_user(
    connector: &DirectoryConnector,
    username: &str,
    password: &str,
) -> Result<AuthenticatedDirectoryIdentity, LiveDirectoryError> {
    authenticate_directory_user_with_ca(
        connector,
        username,
        password,
        None,
        Mapping::from_persisted(connector.provider, &serde_json::json!({}))?,
        default_connector_filter(connector),
        None,
    )
    .await
}

pub async fn authenticate_directory_user_with_resolver(
    connector: &DirectoryConnector,
    username: &str,
    password: &str,
    resolver: &dyn DirectorySecretResolver,
) -> Result<AuthenticatedDirectoryIdentity, LiveDirectoryError> {
    let secret = resolver
        .resolve(directory_credential_resolver_key(connector))
        .await
        .map_err(|_| LiveDirectoryError::Configuration)?;
    authenticate_directory_user_with_ca(
        connector,
        username,
        password,
        Some(Arc::new(secret.ca_certificate_pem().to_vec())),
        Mapping::from_persisted(connector.provider, &serde_json::json!({}))?,
        default_connector_filter(connector),
        Some(Arc::new(ServiceBind {
            bind_dn: Zeroizing::new(secret.bind_dn().to_owned()),
            password: Zeroizing::new(secret.password().to_owned()),
        })),
    )
    .await
}

pub async fn authenticate_directory_user_with_runtime(
    connector: &DirectoryConnector,
    settings: &DirectoryConnectorRuntimeSettings,
    username: &str,
    password: &str,
    resolver: &dyn DirectorySecretResolver,
) -> Result<AuthenticatedDirectoryIdentity, LiveDirectoryError> {
    validate_runtime_settings(connector, settings)?;
    let secret = resolver
        .resolve(directory_credential_resolver_key(connector))
        .await
        .map_err(|_| LiveDirectoryError::Configuration)?;
    authenticate_directory_user_with_ca(
        connector,
        username,
        password,
        Some(Arc::new(secret.ca_certificate_pem().to_vec())),
        Mapping::from_persisted(connector.provider, &settings.attribute_mapping)?,
        settings.user_filter.clone(),
        Some(Arc::new(ServiceBind {
            bind_dn: Zeroizing::new(secret.bind_dn().to_owned()),
            password: Zeroizing::new(secret.password().to_owned()),
        })),
    )
    .await
}

async fn authenticate_directory_user_with_ca(
    connector: &DirectoryConnector,
    username: &str,
    password: &str,
    ca_certificate_pem: Option<Arc<Vec<u8>>>,
    mapping: Mapping,
    user_filter: String,
    service_bind: Option<Arc<ServiceBind>>,
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
                ca_certificate_pem,
                mapping,
                user_filter,
                service_bind,
            },
            None,
            Utc::now(),
        )
        .await
        .map_err(|error| match error {
            super::auth::DirectoryAuthError::BindRejected => LiveDirectoryError::InvalidCredentials,
            super::auth::DirectoryAuthError::AccountDisabled
            | super::auth::DirectoryAuthError::AccountDeleted
            | super::auth::DirectoryAuthError::AccountLocked => LiveDirectoryError::AccountDenied,
            super::auth::DirectoryAuthError::Timeout(_)
            | super::auth::DirectoryAuthError::TransportUnavailable => {
                LiveDirectoryError::Transport
            }
            super::auth::DirectoryAuthError::TlsCertificateValidationFailed
            | super::auth::DirectoryAuthError::TlsPolicyRejected => {
                LiveDirectoryError::TrustEvidenceUnavailable
            }
            _ => LiveDirectoryError::Configuration,
        })
}

async fn ucs_bind_user(
    connector: &DirectoryConnector,
    connector_ref: uuid::Uuid,
    username: &str,
    password: &str,
    ca_certificate_pem: Option<&[u8]>,
) -> Result<DirectoryBindResult, DirectoryTransportError> {
    validate_ucs_connector(connector).map_err(|_| DirectoryTransportError::ProtocolViolation)?;
    let client = ucs_http_client(connector, ca_certificate_pem)
        .map_err(|_| DirectoryTransportError::Unavailable)?;
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
    if matches!(
        response.status(),
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
    ) {
        return Err(DirectoryTransportError::Rejected);
    }
    if !response.status().is_success() {
        return Err(DirectoryTransportError::Unavailable);
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_DIRECTORY_RESPONSE_BYTES as u64)
    {
        return Err(DirectoryTransportError::ProtocolViolation);
    }
    let body = response
        .bytes()
        .await
        .map_err(|_| DirectoryTransportError::Unavailable)?;
    if body.is_empty() || body.len() > MAX_DIRECTORY_RESPONSE_BYTES {
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
        .and_then(|value| parse_uuid(&value))
        .ok_or(DirectoryTransportError::ProtocolViolation)?;
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
    let key = super::auth::ExternalIdentityKey::try_for_provider(
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
    let properties = value.get("properties")?;
    for name in names {
        let Some(found) = properties.get(*name) else {
            continue;
        };
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
    None
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
    if binary.is_some() && textual.is_some() {
        return None;
    }
    binary
        .and_then(|binary| {
            if provider == DirectoryProvider::ActiveDirectoryDomainServices {
                uuid::Uuid::from_slice_le(binary).ok()
            } else {
                uuid::Uuid::from_slice(binary).ok()
            }
        })
        .filter(|value| !value.is_nil())
        .or_else(|| {
            textual.and_then(|value| {
                value.parse::<uuid::Uuid>().ok().or_else(|| {
                    let bytes = <[u8; 16]>::try_from(hex::decode(value).ok()?).ok()?;
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
    let normalized = value.trim().to_ascii_lowercase();
    Ok(!matches!(
        normalized.as_str(),
        "0" | "false" | "no" | "none" | "enabled" | "active" | "unlocked"
    ))
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

fn hex_lower(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn ldap_filter_values_are_escaped_at_the_transport_boundary() {
        assert_eq!(
            escape_ldap_filter_value("alice*)(uid=*)"),
            "alice\\2a\\29\\28uid=\\2a\\29"
        );
    }

    #[test]
    fn malformed_account_status_fails_closed() {
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
            attrs: HashMap::from([("userAccountControl".into(), vec!["not-a-number".into()])]),
            bin_attrs: HashMap::new(),
        };
        assert!(directory_account_status(&entry, &mapping).is_err());
    }

    #[test]
    fn secret_profile_resolution_preserves_legacy_rows_only() {
        fn connector(profile: &str, credential: &str) -> DirectoryConnector {
            DirectoryConnector {
                id: uuid::Uuid::from_u128(1),
                scope: DirectoryScope::Platform,
                provider: DirectoryProvider::GenericLdap,
                feed: DirectoryFeedKind::Rfc4533,
                endpoint: "ldaps://ldap.example.test:636".into(),
                base_dn: "dc=example,dc=test".into(),
                credential_ref: credential.into(),
                credential_profile_key: profile.into(),
                parent_connector_id: None,
                directory_authority_fingerprint: None,
                fallback_to_holding: false,
                tls: super::super::DirectoryTlsPolicy {
                    mode: DirectoryTlsMode::Ldaps,
                    server_name: "ldap.example.test".into(),
                    ca_certificate_sha256: "a".repeat(64),
                    minimum_tls_version: "1.2".into(),
                },
                max_import_users: 100,
                enabled: true,
                revision: 1,
            }
        }

        let modern = connector("directory-profile-1", "legacy-secret");
        assert_eq!(
            directory_credential_resolver_key(&modern),
            "directory-profile-1"
        );
        let legacy = connector("legacy-unconfigured", "legacy-secret");
        assert_eq!(directory_credential_resolver_key(&legacy), "legacy-secret");
    }
}
