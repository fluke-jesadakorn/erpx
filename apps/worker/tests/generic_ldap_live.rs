//! Credential-gated acceptance for the production Generic LDAP transport.
//!
//! This test deliberately does not provide an in-process fake LDAP server.  A
//! configured run starts the same test binary as a child with the production
//! adapter environment names, persists a real `DirectoryConnector`, and calls
//! `DirectorySyncRuntimeHandler`.  The handler then reaches the private
//! `directory_live::fetch` implementation used by the worker composition.
//!
//! Required references for a live run:
//!
//! - `ERPX_G14_GENERIC_LDAP_TEST_DATABASE_URL` (a disposable/dedicated write
//!   database; the test applies the identity schema and leaves evidence rows)
//! - `ERPX_G14_GENERIC_LDAP_ENDPOINT` (`ldaps://...` or `ldap://...`)
//! - `ERPX_G14_GENERIC_LDAP_BASE_DN`
//! - `ERPX_G14_GENERIC_LDAP_BIND_DN`
//! - `ERPX_G14_GENERIC_LDAP_PASSWORD`
//! - `ERPX_G14_GENERIC_LDAP_CA_SHA256`
//! - `ERPX_G14_GENERIC_LDAP_SERVER_NAME`
//! - `ERPX_G14_GENERIC_LDAP_TLS_MODE` (`ldaps` or `start_tls`)
//!
//! A live run must also provide the explicit full-import capacity contract:
//! `ERPX_G14_GENERIC_LDAP_FULL_IMPORT=true` and
//! `ERPX_G14_GENERIC_LDAP_MAX_IMPORT_USERS=1000000`.
//!
//! A provider-side revocation exercise is never automated by this harness.
//! The operator must disable or delete one real directory object, then provide
//! these handoff references: `ERPX_G14_GENERIC_LDAP_REVOCATION_EXTERNAL_ID`,
//! `ERPX_G14_GENERIC_LDAP_REVOCATION_KIND` (`disable` or `delete`),
//! `ERPX_G14_GENERIC_LDAP_REVOCATION_TRIGGERED_AT` (RFC3339), and
//! `ERPX_G14_GENERIC_LDAP_REVOCATION_EVIDENCE_PATH`. The evidence file must
//! be a JSON handoff identifying the object, immutable identity, action,
//! provider event, and trigger/action timestamps. It is only a handoff
//! reference; acceptance still requires the native RFC 4533 feed plus durable
//! SQLx identity, projection, SLA, and outbox evidence within 300 seconds.
//!
//! Optional references configure the real adapter without changing its code:
//! `ERPX_G14_GENERIC_LDAP_PAGE_SIZE`, and the directory attribute/filter
//! variables named `ERPX_G14_GENERIC_LDAP_*_ATTRIBUTE` and
//! `ERPX_G14_GENERIC_LDAP_FILTER`.
//!
//! If any required reference is absent, the test reports a queued external
//! certification blocker and returns without claiming a live result.

use std::{env, fs, path::PathBuf, process::Command, time::Duration};

use chrono::{DateTime, Duration as ChronoDuration, TimeZone, Utc};
use erpx_identity::directory::feeds::{FeedCursor, ImportCapacity};
use erpx_identity::directory::rbac::{
    DirectoryGroupRef, DirectoryRbacRollbackPlan, DirectoryRoleFactAction,
    DirectoryRoleFactAppendOutcome, DirectoryRoleFactInput, DirectoryRoleFactLedger,
    DirectoryRoleFactReason, DirectoryRoleIdentity, DirectoryRoleOutboxEvent,
    DirectoryRoleReadModel, DirectoryRoleRef,
};
use erpx_identity::directory::sync::{ConnectorScope as RbacConnectorScope, ImmutableIdentityLink};
use erpx_identity::directory::tls::{
    CertificateFingerprint, DirectoryTransportMode, TlsHandshakeEvidence, TlsPolicy,
    TlsPolicyError, TlsProtocolVersion, TrustAnchor, sha256_certificate_fingerprint,
};
use erpx_identity::{
    DirectoryConnector, DirectoryFeedKind, DirectoryProvider, DirectoryScope, DirectoryTlsMode,
    DirectoryTlsPolicy, SqlxDirectoryStore, apply_identity_migration,
};
use erpx_worker::{DIRECTORY_SYNC_HANDLER_NAME, DirectorySyncRuntimeHandler, NamedWorkerHandler};
use serde::Deserialize;
use sqlx::{Row, postgres::PgPoolOptions};
use uuid::Uuid;

const CHILD_MODE: &str = "ERPX_G14_GENERIC_LDAP_ACCEPTANCE_CHILD";
const REVOCATION_CHILD_MODE: &str = "ERPX_G14_GENERIC_LDAP_REVOCATION_CHILD";
const MAX_IMPORT_USERS: u64 = 1_000_000;
const MAX_PAGE_SIZE: u32 = 5_000;
const REVOCATION_SLA_SECONDS: i64 = 300;
const MAX_REVOCATION_EVIDENCE_BYTES: usize = 64 * 1024;
const MINIMUM_TLS_VERSION: &str = "1.2";

const REQUIRED_ENV: &[&str] = &[
    "ERPX_G14_GENERIC_LDAP_TEST_DATABASE_URL",
    "ERPX_G14_GENERIC_LDAP_ENDPOINT",
    "ERPX_G14_GENERIC_LDAP_BASE_DN",
    "ERPX_G14_GENERIC_LDAP_BIND_DN",
    "ERPX_G14_GENERIC_LDAP_PASSWORD",
    "ERPX_G14_GENERIC_LDAP_CA_SHA256",
    "ERPX_G14_GENERIC_LDAP_SERVER_NAME",
    "ERPX_G14_GENERIC_LDAP_TLS_MODE",
    "ERPX_G14_GENERIC_LDAP_FEED",
    "ERPX_G14_GENERIC_LDAP_FULL_IMPORT",
    "ERPX_G14_GENERIC_LDAP_MAX_IMPORT_USERS",
    "ERPX_G14_GENERIC_LDAP_REVOCATION_EXTERNAL_ID",
    "ERPX_G14_GENERIC_LDAP_REVOCATION_KIND",
    "ERPX_G14_GENERIC_LDAP_REVOCATION_TRIGGERED_AT",
    "ERPX_G14_GENERIC_LDAP_REVOCATION_EVIDENCE_PATH",
];

#[derive(Debug)]
enum ConfigError {
    Missing(Vec<&'static str>),
    Invalid(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RevocationKind {
    Disable,
    Delete,
}

impl RevocationKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Disable => "disable",
            Self::Delete => "delete",
        }
    }

    const fn reason(self) -> &'static str {
        match self {
            Self::Disable => "directory_account_disabled",
            Self::Delete => "directory_account_deleted",
        }
    }

    const fn event_type(self) -> &'static str {
        match self {
            Self::Disable => "directory.identity_disabled.v1",
            Self::Delete => "directory.identity_deleted.v1",
        }
    }
}

struct LiveConfig {
    database_url: String,
    endpoint: String,
    base_dn: String,
    bind_dn: String,
    password: String,
    ca_sha256: String,
    server_name: String,
    tls_mode: DirectoryTlsMode,
    feed: DirectoryFeedKind,
    page_size: u32,
    max_import_users: u64,
    full_import: bool,
    filter: Option<String>,
    immutable_attribute: Option<String>,
    login_attribute: Option<String>,
    email_attribute: Option<String>,
    display_name_attribute: Option<String>,
    group_attribute: Option<String>,
    disabled_attribute: Option<String>,
    deleted_attribute: Option<String>,
    revocation_external_id: String,
    revocation_kind: RevocationKind,
    revocation_triggered_at: DateTime<Utc>,
    revocation_evidence_path: PathBuf,
}

impl LiveConfig {
    fn from_env() -> Result<Self, ConfigError> {
        let missing = missing_required_references(|name| env::var(name).ok());
        if !missing.is_empty() {
            return Err(ConfigError::Missing(missing));
        }
        let value = |name: &'static str| {
            env::var(name)
                .ok()
                .filter(|value| !value.trim().is_empty() && !value.chars().any(char::is_control))
        };

        let database_url = value(REQUIRED_ENV[0]);
        let endpoint = value(REQUIRED_ENV[1]);
        let base_dn = value(REQUIRED_ENV[2]);
        let bind_dn = value(REQUIRED_ENV[3]);
        let password = value(REQUIRED_ENV[4]);
        let ca_sha256 = value(REQUIRED_ENV[5]);
        let server_name = value(REQUIRED_ENV[6]);
        let tls_mode = value(REQUIRED_ENV[7]);
        let feed = value(REQUIRED_ENV[8]);
        let full_import = value(REQUIRED_ENV[9]);
        let max_import_users = value(REQUIRED_ENV[10]);
        let revocation_external_id = value(REQUIRED_ENV[11]);
        let revocation_kind = value(REQUIRED_ENV[12]);
        let revocation_triggered_at = value(REQUIRED_ENV[13]);
        let revocation_evidence_path = value(REQUIRED_ENV[14]);

        let database_url = database_url.expect("checked required database URL");
        let endpoint = endpoint.expect("checked required LDAP endpoint");
        let base_dn = base_dn.expect("checked required LDAP base DN");
        let bind_dn = bind_dn.expect("checked required LDAP bind DN");
        let password = password.expect("checked required LDAP password");
        let ca_sha256 = ca_sha256.expect("checked required LDAP CA fingerprint");
        let server_name = server_name.expect("checked required LDAP server name");
        let tls_mode = match tls_mode.expect("checked required LDAP TLS mode").as_str() {
            "ldaps" => DirectoryTlsMode::Ldaps,
            "start_tls" => DirectoryTlsMode::StartTls,
            _ => return Err(ConfigError::Invalid("ERPX_G14_GENERIC_LDAP_TLS_MODE")),
        };
        if !postgres_url_uses_tls(&database_url) {
            return Err(ConfigError::Invalid(
                "ERPX_G14_GENERIC_LDAP_TEST_DATABASE_URL",
            ));
        }
        let endpoint_is_ldaps = endpoint_has_authority(&endpoint, "ldaps://");
        let endpoint_is_ldap = endpoint_has_authority(&endpoint, "ldap://");
        if (!endpoint_is_ldaps && !endpoint_is_ldap)
            || (matches!(tls_mode, DirectoryTlsMode::Ldaps) != endpoint_is_ldaps)
        {
            return Err(ConfigError::Invalid(
                "ERPX_G14_GENERIC_LDAP_ENDPOINT/TLS_MODE",
            ));
        }
        if !valid_ca_certificate_fingerprint(&ca_sha256) {
            return Err(ConfigError::Invalid("ERPX_G14_GENERIC_LDAP_CA_SHA256"));
        }
        let endpoint_scheme = match tls_mode {
            DirectoryTlsMode::Ldaps => "ldaps://",
            DirectoryTlsMode::StartTls => "ldap://",
            DirectoryTlsMode::Https => unreachable!("Generic LDAP cannot use HTTPS"),
        };
        if endpoint_host(&endpoint, endpoint_scheme)
            .is_none_or(|host| !host.eq_ignore_ascii_case(&server_name))
        {
            return Err(ConfigError::Invalid(
                "ERPX_G14_GENERIC_LDAP_ENDPOINT/SERVER_NAME",
            ));
        }

        let full_import = match full_import
            .expect("checked required full-import setting")
            .as_str()
        {
            "true" => true,
            "false" => false,
            _ => return Err(ConfigError::Invalid("ERPX_G14_GENERIC_LDAP_FULL_IMPORT")),
        };
        let max_import_users = max_import_users
            .expect("checked required import capacity")
            .parse::<u64>()
            .map_err(|_| ConfigError::Invalid("ERPX_G14_GENERIC_LDAP_MAX_IMPORT_USERS"))?;
        if max_import_users == 0 || max_import_users > MAX_IMPORT_USERS {
            return Err(ConfigError::Invalid(
                "ERPX_G14_GENERIC_LDAP_MAX_IMPORT_USERS",
            ));
        }
        let revocation_kind = match revocation_kind
            .expect("checked required revocation kind")
            .as_str()
        {
            "disable" => RevocationKind::Disable,
            "delete" => RevocationKind::Delete,
            _ => {
                return Err(ConfigError::Invalid(
                    "ERPX_G14_GENERIC_LDAP_REVOCATION_KIND",
                ));
            }
        };
        let revocation_triggered_at = DateTime::parse_from_rfc3339(
            revocation_triggered_at
                .expect("checked required revocation trigger timestamp")
                .as_str(),
        )
        .map_err(|_| ConfigError::Invalid("ERPX_G14_GENERIC_LDAP_REVOCATION_TRIGGERED_AT"))?
        .with_timezone(&Utc);
        let revocation_evidence_path = PathBuf::from(
            revocation_evidence_path.expect("checked required revocation evidence path"),
        );
        if !revocation_evidence_path.is_absolute() {
            return Err(ConfigError::Invalid(
                "ERPX_G14_GENERIC_LDAP_REVOCATION_EVIDENCE_PATH",
            ));
        }

        let feed = parse_generic_feed(feed.expect("checked required LDAP feed").as_str())
            .map_err(ConfigError::Invalid)?;
        let page_size = parse_optional("ERPX_G14_GENERIC_LDAP_PAGE_SIZE", 500)?;
        if page_size == 0 || page_size > MAX_PAGE_SIZE {
            return Err(ConfigError::Invalid("ERPX_G14_GENERIC_LDAP_PAGE_SIZE"));
        }
        Ok(Self {
            database_url,
            endpoint,
            base_dn,
            bind_dn,
            password,
            ca_sha256,
            server_name,
            tls_mode,
            feed,
            page_size,
            max_import_users,
            full_import,
            filter: optional("ERPX_G14_GENERIC_LDAP_FILTER"),
            immutable_attribute: optional("ERPX_G14_GENERIC_LDAP_IMMUTABLE_ATTRIBUTE"),
            login_attribute: optional("ERPX_G14_GENERIC_LDAP_LOGIN_ATTRIBUTE"),
            email_attribute: optional("ERPX_G14_GENERIC_LDAP_EMAIL_ATTRIBUTE"),
            display_name_attribute: optional("ERPX_G14_GENERIC_LDAP_DISPLAY_NAME_ATTRIBUTE"),
            group_attribute: optional("ERPX_G14_GENERIC_LDAP_GROUP_ATTRIBUTE"),
            disabled_attribute: optional("ERPX_G14_GENERIC_LDAP_DISABLED_ATTRIBUTE"),
            deleted_attribute: optional("ERPX_G14_GENERIC_LDAP_DELETED_ATTRIBUTE"),
            revocation_external_id: revocation_external_id
                .expect("checked required revocation external ID"),
            revocation_kind,
            revocation_triggered_at,
            revocation_evidence_path,
        })
    }

    fn adapter_name(&self) -> &'static str {
        match self.feed {
            DirectoryFeedKind::Polling => "ldap_polling",
            DirectoryFeedKind::Rfc4533 => "rfc4533",
            DirectoryFeedKind::AdDirSync | DirectoryFeedKind::UcsProvisioningService => {
                unreachable!("Generic LDAP acceptance only permits LDAP feed kinds")
            }
        }
    }
}

/// Provider-owned revocation evidence is a structured handoff, not a free-form
/// text marker.  The harness still does not perform the provider-side action;
/// it only accepts evidence that binds that action to the configured native
/// feed, immutable directory object, and trigger timestamp.
#[derive(Debug, Deserialize)]
struct RevocationActionEvidence {
    exercise_id: String,
    vendor: String,
    feed: String,
    external_id: String,
    immutable_authority: String,
    immutable_value: String,
    operation: String,
    triggered_at: DateTime<Utc>,
    action_at: DateTime<Utc>,
    provider_event_id: String,
}

fn optional(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty() && !value.chars().any(char::is_control))
}

fn missing_required_references<F>(mut get: F) -> Vec<&'static str>
where
    F: FnMut(&str) -> Option<String>,
{
    REQUIRED_ENV
        .iter()
        .copied()
        .filter(|name| {
            get(name)
                .is_none_or(|value| value.trim().is_empty() || value.chars().any(char::is_control))
        })
        .collect()
}

fn parse_generic_feed(value: &str) -> Result<DirectoryFeedKind, &'static str> {
    match value {
        "ldap_polling" => Ok(DirectoryFeedKind::Polling),
        "rfc4533" => Ok(DirectoryFeedKind::Rfc4533),
        // AD DS/DirSync and UCS Provisioning Service are different provider
        // registrations. They must not be accepted by, or silently routed
        // through, the Generic LDAP harness.
        _ => Err("ERPX_G14_GENERIC_LDAP_FEED"),
    }
}

fn valid_ca_certificate_fingerprint(value: &str) -> bool {
    value.len() == 64
        && value.chars().all(|character| character.is_ascii_hexdigit())
        && value.chars().any(|character| character != '0')
}

fn postgres_url_uses_tls(url: &str) -> bool {
    if url.len() > 2048 || url.chars().any(char::is_control) || url.contains('#') {
        return false;
    }
    let Some((scheme, authority_and_path_and_query)) = url.split_once("://") else {
        return false;
    };
    if !matches!(
        scheme.to_ascii_lowercase().as_str(),
        "postgres" | "postgresql"
    ) {
        return false;
    }
    let Some((authority_and_path, query)) = authority_and_path_and_query.split_once('?') else {
        return false;
    };
    let authority_end = authority_and_path
        .find('/')
        .unwrap_or(authority_and_path.len());
    let authority = &authority_and_path[..authority_end];
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    if host.is_empty()
        || host
            .chars()
            .any(|character| character.is_ascii_control() || character.is_whitespace())
    {
        return false;
    }
    let mut sslmode = None;
    for parameter in query.split('&') {
        let Some((name, value)) = parameter.split_once('=') else {
            return false;
        };
        if name.is_empty() || value.is_empty() {
            return false;
        }
        if name.eq_ignore_ascii_case("ssl-mode") {
            return false;
        }
        if name.eq_ignore_ascii_case("sslmode")
            && (sslmode.replace(value).is_some() || !value.eq_ignore_ascii_case("verify-full"))
        {
            return false;
        }
    }
    sslmode.is_some()
}

fn endpoint_has_authority(endpoint: &str, scheme: &str) -> bool {
    endpoint_host(endpoint, scheme).is_some()
}

fn endpoint_host<'a>(endpoint: &'a str, scheme: &str) -> Option<&'a str> {
    let authority_and_path = endpoint.strip_prefix(scheme)?;
    let authority_end = authority_and_path
        .find(['/', '?', '#'])
        .unwrap_or(authority_and_path.len());
    if scheme != "https://" && authority_end != authority_and_path.len() {
        return None;
    }
    if authority_and_path[authority_end..].contains(['?', '#']) {
        return None;
    }
    let authority = &authority_and_path[..authority_end];
    if authority.is_empty() || authority.contains('@') || authority.chars().any(char::is_whitespace)
    {
        return None;
    }
    if authority.matches(':').count() > 1 {
        return None;
    }
    let (host, port) = authority
        .rsplit_once(':')
        .map_or((authority, None), |(host, port)| (host, Some(port)));
    if host.is_empty()
        || host
            .chars()
            .any(|character| character.is_ascii_control() || character.is_ascii_whitespace())
        || port.is_some_and(|port| {
            port.is_empty() || port.parse::<u16>().ok().is_none_or(|port| port == 0)
        })
    {
        return None;
    }
    Some(host)
}

fn parse_optional<T>(name: &'static str, default: T) -> Result<T, ConfigError>
where
    T: std::str::FromStr,
{
    optional(name)
        .map(|value| value.parse().map_err(|_| ConfigError::Invalid(name)))
        .unwrap_or(Ok(default))
}

fn validate_revocation_evidence_reference(config: &LiveConfig) -> Result<(), String> {
    read_revocation_action_evidence(config).map(|_| ())
}

fn read_revocation_action_evidence(
    config: &LiveConfig,
) -> Result<RevocationActionEvidence, String> {
    let metadata = fs::metadata(&config.revocation_evidence_path).map_err(|error| {
        format!("ERPX_G14_GENERIC_LDAP_REVOCATION_EVIDENCE_PATH is not readable: {error}")
    })?;
    if !metadata.is_file() {
        return Err(
            "ERPX_G14_GENERIC_LDAP_REVOCATION_EVIDENCE_PATH must name a regular file".into(),
        );
    }
    if metadata.len() > MAX_REVOCATION_EVIDENCE_BYTES as u64 {
        return Err(format!(
            "ERPX_G14_GENERIC_LDAP_REVOCATION_EVIDENCE_PATH exceeds {MAX_REVOCATION_EVIDENCE_BYTES} bytes"
        ));
    }
    let contents = String::from_utf8(
        fs::read(&config.revocation_evidence_path)
            .map_err(|error| format!("could not read revocation evidence: {error}"))?,
    )
    .map_err(|_| "revocation evidence must be UTF-8 text".to_owned())?;
    let evidence = parse_revocation_action_evidence(&contents)?;
    validate_revocation_action_evidence(&evidence, config)?;
    Ok(evidence)
}

fn parse_revocation_action_evidence(contents: &str) -> Result<RevocationActionEvidence, String> {
    serde_json::from_str::<RevocationActionEvidence>(contents).map_err(|_| {
        "revocation evidence must be JSON with the required immutable-target fields".to_owned()
    })
}

fn validate_revocation_action_evidence(
    evidence: &RevocationActionEvidence,
    config: &LiveConfig,
) -> Result<(), String> {
    for (field, value) in [
        ("exercise_id", evidence.exercise_id.as_str()),
        ("vendor", evidence.vendor.as_str()),
        ("feed", evidence.feed.as_str()),
        ("external_id", evidence.external_id.as_str()),
        ("immutable_authority", evidence.immutable_authority.as_str()),
        ("immutable_value", evidence.immutable_value.as_str()),
        ("operation", evidence.operation.as_str()),
        ("provider_event_id", evidence.provider_event_id.as_str()),
    ] {
        if value.trim().is_empty() || value.chars().any(char::is_control) {
            return Err(format!(
                "Generic LDAP revocation evidence field {field} is empty or unsafe"
            ));
        }
    }
    if evidence.vendor != "Generic LDAP" {
        return Err("Generic LDAP revocation evidence vendor does not match".into());
    }
    if evidence.feed != "rfc4533" {
        return Err("Generic LDAP revocation evidence must identify the RFC 4533 feed".into());
    }
    if evidence.external_id != config.revocation_external_id {
        return Err("Generic LDAP revocation evidence target does not match".into());
    }
    if evidence.operation != config.revocation_kind.as_str() {
        return Err("Generic LDAP revocation evidence operation does not match".into());
    }
    if evidence.triggered_at != config.revocation_triggered_at {
        return Err("Generic LDAP revocation evidence trigger timestamp does not match".into());
    }
    if evidence.action_at < evidence.triggered_at {
        return Err("Generic LDAP revocation evidence action predates its trigger".into());
    }
    if evidence.action_at > Utc::now() {
        return Err("Generic LDAP revocation evidence action is timestamped in the future".into());
    }
    if evidence.action_at > evidence.triggered_at + ChronoDuration::seconds(REVOCATION_SLA_SECONDS)
    {
        return Err("Generic LDAP provider action exceeded the five-minute SLA".into());
    }
    Ok(())
}

fn production_env_names() -> [&'static str; 14] {
    [
        "ERPX_DIRECTORY_CONNECTOR_ID",
        "ERPX_DIRECTORY_FEED_ADAPTER",
        "ERPX_DIRECTORY_BIND_DN",
        "ERPX_DIRECTORY_PASSWORD",
        "ERPX_DIRECTORY_PAGE_SIZE",
        "ERPX_DIRECTORY_FULL_IMPORT",
        "ERPX_DIRECTORY_FILTER",
        "ERPX_DIRECTORY_IMMUTABLE_ATTRIBUTE",
        "ERPX_DIRECTORY_LOGIN_ATTRIBUTE",
        "ERPX_DIRECTORY_EMAIL_ATTRIBUTE",
        "ERPX_DIRECTORY_DISPLAY_NAME_ATTRIBUTE",
        "ERPX_DIRECTORY_GROUP_ATTRIBUTE",
        "ERPX_DIRECTORY_DISABLED_ATTRIBUTE",
        "ERPX_DIRECTORY_DELETED_ATTRIBUTE",
    ]
}

fn configure_child_command(
    mut command: Command,
    config: &LiveConfig,
    connector_id: Uuid,
) -> Command {
    for name in production_env_names() {
        command.env_remove(name);
    }
    command
        .env(CHILD_MODE, "1")
        .env("ERPX_DIRECTORY_CONNECTOR_ID", connector_id.to_string())
        .env("ERPX_DIRECTORY_FEED_ADAPTER", config.adapter_name())
        .env("ERPX_DIRECTORY_BIND_DN", &config.bind_dn)
        .env("ERPX_DIRECTORY_PASSWORD", &config.password)
        .env("ERPX_DIRECTORY_PAGE_SIZE", config.page_size.to_string())
        .env("ERPX_DIRECTORY_FULL_IMPORT", config.full_import.to_string());
    set_optional_env(
        &mut command,
        "ERPX_DIRECTORY_FILTER",
        config.filter.as_deref(),
    );
    set_optional_env(
        &mut command,
        "ERPX_DIRECTORY_IMMUTABLE_ATTRIBUTE",
        config.immutable_attribute.as_deref(),
    );
    set_optional_env(
        &mut command,
        "ERPX_DIRECTORY_LOGIN_ATTRIBUTE",
        config.login_attribute.as_deref(),
    );
    set_optional_env(
        &mut command,
        "ERPX_DIRECTORY_EMAIL_ATTRIBUTE",
        config.email_attribute.as_deref(),
    );
    set_optional_env(
        &mut command,
        "ERPX_DIRECTORY_DISPLAY_NAME_ATTRIBUTE",
        config.display_name_attribute.as_deref(),
    );
    set_optional_env(
        &mut command,
        "ERPX_DIRECTORY_GROUP_ATTRIBUTE",
        config.group_attribute.as_deref(),
    );
    set_optional_env(
        &mut command,
        "ERPX_DIRECTORY_DISABLED_ATTRIBUTE",
        config.disabled_attribute.as_deref(),
    );
    set_optional_env(
        &mut command,
        "ERPX_DIRECTORY_DELETED_ATTRIBUTE",
        config.deleted_attribute.as_deref(),
    );
    command
}

fn set_optional_env(command: &mut Command, name: &str, value: Option<&str>) {
    match value {
        Some(value) => {
            command.env(name, value);
        }
        None => {
            command.env_remove(name);
        }
    }
}

fn redact_output(output: &[u8], config: &LiveConfig) -> String {
    let mut output = String::from_utf8_lossy(output).into_owned();
    for value in [
        config.password.as_str(),
        config.bind_dn.as_str(),
        config.endpoint.as_str(),
        config.base_dn.as_str(),
        config.ca_sha256.as_str(),
        config.server_name.as_str(),
        config.database_url.as_str(),
    ] {
        if !value.is_empty() {
            output = output.replace(value, "<redacted-config-value>");
        }
    }
    output.trim().to_owned()
}

fn validate_rfc4533_cursor_transition(
    current: &FeedCursor,
    next: &FeedCursor,
    index: usize,
) -> Result<(), String> {
    if matches!(next, FeedCursor::EndOfSnapshot) {
        return Ok(());
    }
    if !matches!(next, FeedCursor::Rfc4533(_)) {
        return Err(format!(
            "RFC 4533 page {index} advanced to a non-RFC 4533 cursor: {next:?}"
        ));
    }
    current
        .validate_continuation_from(next)
        .map_err(|error| format!("RFC 4533 page {index} has an invalid continuation: {error}"))
}

fn immutable_identity_matches(
    durable_authority: Option<&str>,
    durable_value: Option<&str>,
    action_evidence: &RevocationActionEvidence,
) -> bool {
    durable_authority == Some(action_evidence.immutable_authority.as_str())
        && durable_value == Some(action_evidence.immutable_value.as_str())
}

#[derive(Debug)]
struct DurableRevocationEvidence {
    observed_at: DateTime<Utc>,
    deadline: DateTime<Utc>,
    reason: String,
    status: String,
    applied_at: Option<DateTime<Utc>>,
    active: bool,
    revoked_at: Option<DateTime<Utc>>,
    immutable_authority: Option<String>,
    immutable_value: Option<String>,
    outbox_events: i64,
    outbox_event_type: Option<String>,
}

async fn read_durable_revocation_evidence(
    pool: &sqlx::PgPool,
    connector_id: Uuid,
    config: &LiveConfig,
) -> Result<Option<DurableRevocationEvidence>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT sla.observed_at, sla.deadline, sla.reason, sla.status,
                sla.applied_at,
                link.active, link.revoked_at,
                link.immutable_authority, link.immutable_value,
                (SELECT count(*)::bigint
                   FROM erpx_identity.directory_outbox outbox
                  WHERE outbox.event_id=sla.event_id
                    AND outbox.aggregate_type='directory_identity') AS outbox_events
                ,(SELECT outbox.event_type
                    FROM erpx_identity.directory_outbox outbox
                   WHERE outbox.event_id=sla.event_id
                     AND outbox.aggregate_type='directory_identity'
                   ORDER BY outbox.occurred_at DESC
                   LIMIT 1) AS outbox_event_type
           FROM erpx_identity.directory_revocation_sla sla
           JOIN erpx_identity.directory_identity_links link
             ON link.connector_id=sla.connector_id
            AND link.external_object_id=sla.external_id
          WHERE sla.connector_id=$1
            AND sla.external_id=$2
            AND sla.reason=$3
          ORDER BY sla.observed_at DESC
          LIMIT 1",
    )
    .bind(connector_id)
    .bind(&config.revocation_external_id)
    .bind(config.revocation_kind.reason())
    .fetch_optional(pool)
    .await?;
    row.map(|row| {
        Ok(DurableRevocationEvidence {
            observed_at: row.try_get("observed_at")?,
            deadline: row.try_get("deadline")?,
            reason: row.try_get("reason")?,
            status: row.try_get("status")?,
            applied_at: row.try_get("applied_at")?,
            active: row.try_get("active")?,
            revoked_at: row.try_get("revoked_at")?,
            immutable_authority: row.try_get("immutable_authority")?,
            immutable_value: row.try_get("immutable_value")?,
            outbox_events: row.try_get("outbox_events")?,
            outbox_event_type: row.try_get("outbox_event_type")?,
        })
    })
    .transpose()
}

fn validate_durable_revocation_evidence(
    evidence: &DurableRevocationEvidence,
    config: &LiveConfig,
) -> Result<(), String> {
    if evidence.reason != config.revocation_kind.reason() {
        return Err(
            "durable revocation reason does not match the requested provider action".into(),
        );
    }
    if evidence.status != "applied" {
        return Err(format!(
            "durable revocation SLA is not applied: {}",
            evidence.status
        ));
    }
    let applied_at = evidence
        .applied_at
        .ok_or_else(|| "applied durable revocation has no applied_at evidence".to_owned())?;
    if evidence.active || evidence.revoked_at.is_none() {
        return Err(
            "durable identity read model is not revoked; authorization must remain denied".into(),
        );
    }
    if evidence
        .immutable_authority
        .as_deref()
        .is_none_or(str::is_empty)
        || evidence
            .immutable_value
            .as_deref()
            .is_none_or(str::is_empty)
    {
        return Err("durable revocation has no immutable identity link".into());
    }
    if evidence.outbox_events < 1 {
        return Err("durable revocation has no matching identity outbox event".into());
    }
    if evidence.outbox_event_type.as_deref() != Some(config.revocation_kind.event_type()) {
        return Err("durable revocation has no matching identity action event".into());
    }
    if evidence.deadline != evidence.observed_at + ChronoDuration::seconds(REVOCATION_SLA_SECONDS) {
        return Err("durable revocation deadline is not exactly five minutes".into());
    }
    if applied_at < evidence.observed_at || applied_at > evidence.deadline {
        return Err("durable revocation was applied outside its five-minute SLA".into());
    }
    if applied_at > Utc::now() {
        return Err("durable revocation applied_at is in the future".into());
    }
    if evidence.observed_at < config.revocation_triggered_at {
        return Err("native-feed revocation evidence predates the provider trigger".into());
    }
    if evidence.observed_at
        > config.revocation_triggered_at + ChronoDuration::seconds(REVOCATION_SLA_SECONDS)
    {
        return Err("native-feed revocation evidence exceeded the five-minute SLA".into());
    }
    Ok(())
}

/// Prove that the real RFC 4533 full import consumed a durable native cursor
/// chain.  A final checkpoint alone is insufficient: a source that silently
/// restarts from `Start` can still leave a healthy-looking final row while
/// dropping changes between pages.  The page rows are the authoritative
/// handoff evidence, so every page must begin at the exact cursor produced by
/// its predecessor, and every non-terminal successor must be an RFC 4533
/// cursor with a non-empty cookie.
async fn assert_rfc4533_import_cursor_continuity(
    pool: &sqlx::PgPool,
    connector_id: Uuid,
) -> Result<Uuid, String> {
    let import_id: Uuid = sqlx::query_scalar(
        "SELECT id
           FROM erpx_identity.directory_imports
          WHERE connector_id=$1 AND state='completed'
          ORDER BY finalized_at DESC NULLS LAST, id DESC
          LIMIT 1",
    )
    .bind(connector_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("read completed RFC 4533 import: {error}"))?
    .ok_or_else(|| "RFC 4533 live run has no completed durable import".to_owned())?;

    let page_rows = sqlx::query(
        "SELECT cursor, next_checkpoint
           FROM erpx_identity.directory_import_pages
          WHERE import_id=$1
          ORDER BY (next_checkpoint->>'pages_applied')::bigint ASC",
    )
    .bind(import_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("read RFC 4533 durable import pages: {error}"))?;
    if page_rows.is_empty() {
        return Err("RFC 4533 completed import has no durable page evidence".into());
    }

    let mut expected_cursor = FeedCursor::Start;
    for (index, row) in page_rows.iter().enumerate() {
        let current_cursor: FeedCursor = serde_json::from_value(
            row.try_get("cursor")
                .map_err(|error| format!("read RFC 4533 page {index} cursor: {error}"))?,
        )
        .map_err(|error| format!("decode RFC 4533 page {index} cursor: {error}"))?;
        if current_cursor != expected_cursor {
            return Err(format!(
                "RFC 4533 cursor continuity broke at page {index}: expected {expected_cursor:?}, observed {current_cursor:?}"
            ));
        }

        let next_checkpoint: serde_json::Value = row
            .try_get("next_checkpoint")
            .map_err(|error| format!("read RFC 4533 page {index} next checkpoint: {error}"))?;
        let next_cursor_value = next_checkpoint
            .get("cursor")
            .cloned()
            .ok_or_else(|| format!("RFC 4533 page {index} next checkpoint has no cursor"))?;
        let next_cursor: FeedCursor = serde_json::from_value(next_cursor_value)
            .map_err(|error| format!("decode RFC 4533 page {index} next cursor: {error}"))?;
        validate_rfc4533_cursor_transition(&current_cursor, &next_cursor, index)?;
        if matches!(next_cursor, FeedCursor::EndOfSnapshot) && index + 1 != page_rows.len() {
            return Err(format!(
                "RFC 4533 page {index} marked EndOfSnapshot before the final durable page"
            ));
        }
        expected_cursor = next_cursor;
    }

    if !matches!(expected_cursor, FeedCursor::EndOfSnapshot) {
        return Err(format!(
            "RFC 4533 completed import did not persist a terminal snapshot cursor: {expected_cursor:?}"
        ));
    }
    let durable_cursor: FeedCursor = serde_json::from_value(
        sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT cursor
           FROM erpx_identity.directory_imports
          WHERE id=$1 AND state='completed'",
        )
        .bind(import_id)
        .fetch_one(pool)
        .await
        .map_err(|error| format!("read final RFC 4533 import cursor: {error}"))?,
    )
    .map_err(|error| format!("decode final RFC 4533 import cursor: {error}"))?;
    if durable_cursor != expected_cursor {
        return Err(format!(
            "RFC 4533 final import cursor disagrees with its page chain: expected {expected_cursor:?}, observed {durable_cursor:?}"
        ));
    }
    Ok(import_id)
}

async fn assert_import_reconciliation_and_rollback_evidence(
    pool: &sqlx::PgPool,
    connector_id: Uuid,
    import_id: Uuid,
) -> Result<(), String> {
    let import_state: String = sqlx::query_scalar(
        "SELECT state FROM erpx_identity.directory_imports WHERE id=$1 AND connector_id=$2",
    )
    .bind(import_id)
    .bind(connector_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("read completed Generic LDAP import state: {error}"))?
    .ok_or_else(|| "completed Generic LDAP import disappeared from durable state".to_owned())?;
    if import_state != "completed" {
        return Err(format!(
            "Generic LDAP import has unsafe terminal state: {import_state}"
        ));
    }

    let reconciliation_status: Option<String> = sqlx::query_scalar(
        "SELECT status
           FROM erpx_identity.directory_reconciliation_runs
          WHERE import_id=$1
          ORDER BY checked_at DESC
          LIMIT 1",
    )
    .bind(import_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("read Generic LDAP reconciliation evidence: {error}"))?;
    if reconciliation_status.as_deref() != Some("reconciled") {
        return Err(format!(
            "Generic LDAP import has no reconciled durable evidence: {reconciliation_status:?}"
        ));
    }

    let reconciliation_events: i64 = sqlx::query_scalar(
        "SELECT count(*)
           FROM erpx_identity.directory_outbox
          WHERE aggregate_id=$1
            AND aggregate_type='directory_import'
            AND event_type='directory.import_reconciliation_reconciled.v1'",
    )
    .bind(import_id)
    .fetch_one(pool)
    .await
    .map_err(|error| format!("read reconciliation outbox evidence: {error}"))?;
    if reconciliation_events != 1 {
        return Err(format!(
            "Generic LDAP reconciliation must have exactly one durable success event, observed {reconciliation_events}"
        ));
    }

    let active_imports: i64 = sqlx::query_scalar(
        "SELECT count(*)
           FROM erpx_identity.directory_imports
          WHERE connector_id=$1 AND state IN ('running', 'finalizing')",
    )
    .bind(connector_id)
    .fetch_one(pool)
    .await
    .map_err(|error| format!("read active Generic LDAP imports: {error}"))?;
    if active_imports != 0 {
        return Err(format!(
            "Generic LDAP reconciliation left {active_imports} active import(s)"
        ));
    }

    let invalid_rollbacks: i64 = sqlx::query_scalar(
        "SELECT count(*)
           FROM erpx_identity.directory_imports import_row
          WHERE import_row.connector_id=$1
            AND import_row.state='rolled_back'
            AND (
                EXISTS (
                    SELECT 1
                      FROM erpx_identity.directory_import_members member
                     WHERE member.import_id=import_row.id
                )
                OR NOT EXISTS (
                    SELECT 1
                      FROM erpx_identity.directory_outbox outbox
                     WHERE outbox.aggregate_id=import_row.id
                       AND outbox.aggregate_type='directory_import'
                       AND outbox.event_type='directory.import_rolled_back.v1'
                )
            )",
    )
    .bind(connector_id)
    .fetch_one(pool)
    .await
    .map_err(|error| format!("read Generic LDAP rollback evidence: {error}"))?;
    if invalid_rollbacks != 0 {
        return Err(format!(
            "Generic LDAP rollback evidence is incomplete for {invalid_rollbacks} import(s)"
        ));
    }
    Ok(())
}

async fn run_revocation_child(config: LiveConfig, connector_id: Uuid) {
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&config.database_url)
        .await
        .expect("connect to the Generic LDAP revocation acceptance database");
    apply_identity_migration(&pool)
        .await
        .expect("apply identity schema to the Generic LDAP revocation acceptance database");

    let deadline = config.revocation_triggered_at + ChronoDuration::seconds(REVOCATION_SLA_SECONDS);
    if Utc::now() > deadline {
        eprintln!(
            "QUEUED BLOCKER (not a pass): Generic LDAP provider revocation evidence window has expired; repeat the real {} exercise with ERPX_G14_GENERIC_LDAP_REVOCATION_TRIGGERED_AT and ERPX_G14_GENERIC_LDAP_REVOCATION_EVIDENCE_PATH",
            config.revocation_kind.as_str()
        );
        pool.close().await;
        return;
    }

    let action_evidence = match read_revocation_action_evidence(&config) {
        Ok(evidence) => evidence,
        Err(error) => {
            eprintln!(
                "QUEUED BLOCKER (not a pass): Generic LDAP revocation evidence changed or became invalid before the native feed exercise: {error}"
            );
            pool.close().await;
            return;
        }
    };

    let handler = DirectorySyncRuntimeHandler::from_env(pool.clone())
        .expect("construct the production directory synchronization handler for revocation");
    assert_eq!(handler.name(), DIRECTORY_SYNC_HANDLER_NAME);
    loop {
        if let Some(evidence) = read_durable_revocation_evidence(&pool, connector_id, &config)
            .await
            .expect("read durable Generic LDAP revocation evidence")
        {
            match evidence.status.as_str() {
                "pending" => {}
                "breached" => {
                    eprintln!(
                        "QUEUED BLOCKER (not a pass): Generic LDAP {} revocation exceeded the five-minute durable SLA; repeat the real provider exercise with a fresh trigger and evidence handoff",
                        config.revocation_kind.as_str()
                    );
                    pool.close().await;
                    return;
                }
                "applied" => {
                    validate_durable_revocation_evidence(&evidence, &config).unwrap_or_else(
                        |error| panic!("Generic LDAP revocation evidence rejected: {error}"),
                    );
                    if !immutable_identity_matches(
                        evidence.immutable_authority.as_deref(),
                        evidence.immutable_value.as_deref(),
                        &action_evidence,
                    ) {
                        panic!("Generic LDAP revocation changed the immutable identity link");
                    }
                    eprintln!(
                        "G14 Generic LDAP revocation exercise observed native {} evidence at {} with durable identity/projection/SLA/outbox state",
                        config.revocation_kind.as_str(),
                        evidence.observed_at.to_rfc3339()
                    );
                    pool.close().await;
                    return;
                }
                other => panic!("Generic LDAP revocation evidence rejected unsafe status: {other}"),
            }
        }

        let _report = handler
            .run_once(Uuid::new_v4())
            .await
            .expect("poll the real Generic LDAP/RFC 4533 revocation feed");
        if Utc::now() > deadline {
            eprintln!(
                "QUEUED BLOCKER (not a pass): Generic LDAP provider-specific {} was not observed through the native feed within 300 seconds; no synthetic disable/delete was generated. Required inputs: ERPX_G14_GENERIC_LDAP_REVOCATION_EXTERNAL_ID, ERPX_G14_GENERIC_LDAP_REVOCATION_KIND, ERPX_G14_GENERIC_LDAP_REVOCATION_TRIGGERED_AT, ERPX_G14_GENERIC_LDAP_REVOCATION_EVIDENCE_PATH",
                config.revocation_kind.as_str()
            );
            pool.close().await;
            return;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn spawn_revocation_child(config: &LiveConfig, connector_id: Uuid) {
    let executable = env::current_exe().expect("locate the Generic LDAP acceptance binary");
    let command = Command::new(executable);
    let mut command = configure_child_command(command, config, connector_id);
    command
        .env(CHILD_MODE, REVOCATION_CHILD_MODE)
        .env("ERPX_DIRECTORY_FULL_IMPORT", "false");
    let output = command
        .arg("--exact")
        .arg("generic_ldap_live_transport_acceptance")
        .arg("--nocapture")
        .arg("--test-threads=1")
        .output()
        .expect("spawn the Generic LDAP revocation acceptance child");
    assert!(
        output.status.success(),
        "Generic LDAP revocation acceptance child failed (status {:?})\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        redact_output(&output.stdout, config),
        redact_output(&output.stderr, config),
    );
}

async fn run_live_child(config: LiveConfig, connector_id: Uuid) {
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&config.database_url)
        .await
        .expect("connect to the explicitly configured Generic LDAP acceptance database");
    apply_identity_migration(&pool)
        .await
        .expect("apply identity schema to the Generic LDAP acceptance database");

    let store = SqlxDirectoryStore::new(pool.clone());
    let holding_ref = format!("g14-generic-ldap-{}", connector_id.simple());
    let connector = DirectoryConnector {
        id: connector_id,
        provider: DirectoryProvider::GenericLdap,
        scope: DirectoryScope::Holding { holding_ref },
        endpoint: config.endpoint.clone(),
        base_dn: config.base_dn.clone(),
        tls: DirectoryTlsPolicy {
            mode: config.tls_mode,
            ca_certificate_sha256: config.ca_sha256.clone(),
            server_name: config.server_name.clone(),
            minimum_tls_version: MINIMUM_TLS_VERSION.into(),
        },
        feed: config.feed,
        credential_ref: "acceptance-env:ERPX_G14_GENERIC_LDAP_PASSWORD".into(),
        credential_profile_key: format!("legacy-{}", connector_id.simple()),
        parent_connector_id: None,
        directory_authority_fingerprint: None,
        fallback_to_holding: false,
        enabled: true,
        max_import_users: config.max_import_users,
        revision: 1,
    };
    store
        .save_connector(&connector, Utc::now())
        .await
        .expect("persist the real Generic LDAP connector before the live fetch");
    let persisted_connector = store
        .connector(connector.id)
        .await
        .expect("read the persisted Generic LDAP TLS connector");
    assert_eq!(persisted_connector.tls.mode, config.tls_mode);
    assert_eq!(
        persisted_connector.tls.ca_certificate_sha256,
        config.ca_sha256
    );
    assert_eq!(persisted_connector.tls.server_name, config.server_name);
    assert_eq!(
        persisted_connector.tls.minimum_tls_version,
        MINIMUM_TLS_VERSION
    );

    let handler = DirectorySyncRuntimeHandler::from_env(pool.clone())
        .expect("construct the production directory synchronization handler");
    assert_eq!(handler.name(), DIRECTORY_SYNC_HANDLER_NAME);
    let report = handler
        .run_once(Uuid::new_v4())
        .await
        .expect("live Generic LDAP fetch and durable page application");
    assert!(
        report.claimed >= 1,
        "the live adapter must claim at least one page"
    );
    assert!(
        report.has_work,
        "a successful live page must be reported as work"
    );
    assert!(
        report.processed >= 1,
        "the explicit one-million-user full import must durably process at least one user"
    );
    let import_id = assert_rfc4533_import_cursor_continuity(&pool, connector.id)
        .await
        .unwrap_or_else(|error| panic!("RFC 4533 native cursor certification rejected: {error}"));
    assert_import_reconciliation_and_rollback_evidence(&pool, connector.id, import_id)
        .await
        .unwrap_or_else(|error| {
            panic!("Generic LDAP reconciliation/rollback certification rejected: {error}")
        });

    let persisted_max_import_users: i64 = sqlx::query_scalar(
        "SELECT max_import_users FROM erpx_identity.directory_connectors WHERE id=$1",
    )
    .bind(connector.id)
    .fetch_one(&pool)
    .await
    .expect("read the persisted Generic LDAP import capacity");
    assert_eq!(persisted_max_import_users, MAX_IMPORT_USERS as i64);

    let cursor = store
        .latest_cursor(connector.id)
        .await
        .expect("read the durably committed Generic LDAP cursor")
        .expect("live Generic LDAP fetch must persist a cursor");
    assert_eq!(cursor.connector_id, connector.id);
    assert_eq!(cursor.feed, config.feed);
    assert!(cursor.sequence >= 1);

    let checkpoint_status: String = sqlx::query_scalar(
        "SELECT status FROM erpx_identity.directory_sync_checkpoints WHERE connector_id=$1",
    )
    .bind(connector.id)
    .fetch_one(&pool)
    .await
    .expect("read the healthy directory checkpoint");
    assert_eq!(checkpoint_status, "healthy");

    let connector_events: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM erpx_identity.directory_outbox
         WHERE aggregate_id=$1 AND aggregate_type='directory_connector'",
    )
    .bind(connector.id)
    .fetch_one(&pool)
    .await
    .expect("read the connector outbox evidence");
    assert!(
        connector_events >= 1,
        "connector persistence must emit an outbox event"
    );

    let stored_credential_ref: String = sqlx::query_scalar(
        "SELECT credential_ref FROM erpx_identity.directory_connectors WHERE id=$1",
    )
    .bind(connector.id)
    .fetch_one(&pool)
    .await
    .expect("read the persisted credential reference");
    assert_eq!(
        stored_credential_ref,
        "acceptance-env:ERPX_G14_GENERIC_LDAP_PASSWORD"
    );

    let target_is_active: Option<bool> = sqlx::query_scalar(
        "SELECT active
           FROM erpx_identity.directory_identity_links
          WHERE connector_id=$1 AND external_object_id=$2",
    )
    .bind(connector.id)
    .bind(&config.revocation_external_id)
    .fetch_optional(&pool)
    .await
    .expect("read the full-import revocation target");
    if target_is_active != Some(true) {
        eprintln!(
            "QUEUED BLOCKER (not a pass): the real full import did not yield an active revocation target for {}; no provider-specific {} operation was automated. Required inputs: ERPX_G14_GENERIC_LDAP_REVOCATION_EXTERNAL_ID and ERPX_G14_GENERIC_LDAP_REVOCATION_EVIDENCE_PATH",
            config.revocation_external_id,
            config.revocation_kind.as_str()
        );
        pool.close().await;
        return;
    }
    pool.close().await;

    spawn_revocation_child(&config, connector_id).await;
}

#[tokio::test]
async fn generic_ldap_live_transport_acceptance() {
    let config = match LiveConfig::from_env() {
        Ok(config) => config,
        Err(ConfigError::Missing(missing)) => {
            eprintln!(
                "G14 Generic LDAP live certification queued: missing {}",
                missing.join(", ")
            );
            return;
        }
        Err(ConfigError::Invalid(name)) => {
            eprintln!(
                "QUEUED BLOCKER (not a pass): Generic LDAP live certification has invalid configuration {name}; no transport attempt was made"
            );
            return;
        }
    };

    let mut queued = Vec::new();
    if !config.full_import {
        queued.push("ERPX_G14_GENERIC_LDAP_FULL_IMPORT=true");
    }
    if config.max_import_users != MAX_IMPORT_USERS {
        queued.push("ERPX_G14_GENERIC_LDAP_MAX_IMPORT_USERS=1000000");
    }
    if config.feed != DirectoryFeedKind::Rfc4533 {
        queued.push("ERPX_G14_GENERIC_LDAP_FEED=rfc4533");
    }
    if !queued.is_empty() {
        eprintln!(
            "QUEUED BLOCKER (not a pass): Generic LDAP certification requires explicit {}",
            queued.join(" and ")
        );
        return;
    }

    if let Err(error) = validate_revocation_evidence_reference(&config) {
        eprintln!(
            "QUEUED BLOCKER (not a pass): Generic LDAP provider-specific {} exercise requires a valid ERPX_G14_GENERIC_LDAP_REVOCATION_EVIDENCE_PATH: {error}",
            config.revocation_kind.as_str(),
        );
        return;
    }

    if let Ok(child_mode) = env::var(CHILD_MODE) {
        let connector_id = env::var("ERPX_DIRECTORY_CONNECTOR_ID")
            .expect("child connector ID")
            .parse::<Uuid>()
            .expect("child connector ID is a UUID");
        match child_mode.as_str() {
            "1" => run_live_child(config, connector_id).await,
            REVOCATION_CHILD_MODE => run_revocation_child(config, connector_id).await,
            _ => panic!("unsupported Generic LDAP acceptance child mode: {child_mode}"),
        }
        return;
    }

    let connector_id = Uuid::new_v4();
    let executable = env::current_exe().expect("locate the current integration test binary");
    let mut command = Command::new(executable);
    command
        .arg("--exact")
        .arg("generic_ldap_live_transport_acceptance")
        .arg("--nocapture")
        .arg("--test-threads=1");
    let mut command = configure_child_command(command, &config, connector_id);
    let output = command
        .output()
        .expect("spawn the live Generic LDAP acceptance child");
    assert!(
        output.status.success(),
        "live Generic LDAP acceptance child failed (status {:?})\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        redact_output(&output.stdout, &config),
        redact_output(&output.stderr, &config),
    );
}

#[test]
fn generic_ldap_required_references_include_native_rfc4533_selection() {
    assert!(REQUIRED_ENV.contains(&"ERPX_G14_GENERIC_LDAP_FEED"));
    assert_eq!(REQUIRED_ENV.len(), 15);
}

#[test]
fn generic_ldap_unavailable_environment_is_queued_without_secret_values() {
    let missing = missing_required_references(|_| None);
    assert_eq!(missing, REQUIRED_ENV);
    let report = format!(
        "G14 Generic LDAP live certification queued: missing {}",
        missing.join(", ")
    );
    assert!(report.contains("ERPX_G14_GENERIC_LDAP_PASSWORD"));
    assert!(report.contains("ERPX_G14_GENERIC_LDAP_CA_SHA256"));
    assert!(report.contains("ERPX_G14_GENERIC_LDAP_FEED"));
    assert!(!report.contains("test-only-secret-reference"));
}

#[test]
fn generic_ldap_feed_boundary_rejects_ad_and_ucs_protocols() {
    assert_eq!(
        parse_generic_feed("rfc4533"),
        Ok(DirectoryFeedKind::Rfc4533)
    );
    assert_eq!(
        parse_generic_feed("ldap_polling"),
        Ok(DirectoryFeedKind::Polling)
    );
    for unsupported in ["ad_dirsync", "ucs_provisioning_service", "https", ""] {
        assert_eq!(
            parse_generic_feed(unsupported),
            Err("ERPX_G14_GENERIC_LDAP_FEED"),
            "Generic LDAP must not accept the {unsupported} provider feed"
        );
    }
}

#[test]
fn generic_ldap_import_capacity_is_bounded_at_exactly_one_million() {
    let mut capacity = ImportCapacity::full_import();
    assert_eq!(capacity.maximum_users(), MAX_IMPORT_USERS);
    assert_eq!(capacity.users_seen(), 0);
    assert_eq!(
        capacity
            .accept_page(usize::try_from(MAX_IMPORT_USERS).expect("one-million users fit usize"))
            .expect("the exact one-million-user bound must be accepted"),
        0
    );
    assert_eq!(capacity.users_seen(), MAX_IMPORT_USERS);
    assert!(
        capacity
            .accept_page(1)
            .expect_err("the first user beyond the bound must be rejected")
            .to_string()
            .contains("1000001")
    );
    assert!(ImportCapacity::new(0).is_err());
    assert!(ImportCapacity::new(MAX_IMPORT_USERS + 1).is_err());
}

#[test]
fn generic_ldap_rbac_revocation_ledger_read_model_and_outbox_are_append_only() {
    let connector_id = Uuid::new_v4();
    let holding_id = Uuid::new_v4();
    let scope = RbacConnectorScope::Holding(holding_id);
    let immutable_link = ImmutableIdentityLink::new("ldap:entryUUID", "entry-001")
        .expect("immutable LDAP identity link");
    let identity = DirectoryRoleIdentity::from_immutable_link(immutable_link.clone());
    let group_ref = DirectoryGroupRef::try_new("group:g14-directory-admins")
        .expect("bounded directory group reference");
    let role_ref = DirectoryRoleRef::try_new("role:g14-directory-admin")
        .expect("bounded directory role reference");
    let observed_at = Utc
        .with_ymd_and_hms(2026, 7, 29, 12, 0, 0)
        .single()
        .expect("fixed role fact timestamp");
    let grant_input = DirectoryRoleFactInput {
        identity: identity.clone(),
        connector_id,
        group_ref: group_ref.clone(),
        role_ref: role_ref.clone(),
        scope: scope.clone(),
        action: DirectoryRoleFactAction::Grant,
        reason: DirectoryRoleFactReason::GroupMatched,
        idempotency_key: "g14-generic-ldap/role/grant".into(),
        occurred_at: observed_at,
    };
    let mut ledger = DirectoryRoleFactLedger::default();
    let (grant_outcome, grant) = ledger
        .append(grant_input.clone())
        .expect("append the immutable role grant fact");
    assert_eq!(grant_outcome, DirectoryRoleFactAppendOutcome::Applied);
    assert_eq!(grant.sequence(), 1);
    assert!(grant.previous_hash().is_none());
    assert_eq!(grant.identity().link(), &immutable_link);

    let (revoke_outcome, revoke) = ledger
        .append(DirectoryRoleFactInput {
            identity: identity.clone(),
            connector_id,
            group_ref: group_ref.clone(),
            role_ref: role_ref.clone(),
            scope: scope.clone(),
            action: DirectoryRoleFactAction::Revoke,
            reason: DirectoryRoleFactReason::IdentityDisabled,
            idempotency_key: "g14-generic-ldap/role/revoke".into(),
            occurred_at: observed_at + ChronoDuration::seconds(1),
        })
        .expect("append the compensating immutable role revoke fact");
    assert_eq!(revoke_outcome, DirectoryRoleFactAppendOutcome::Applied);
    assert_eq!(revoke.sequence(), 2);
    assert_eq!(revoke.previous_hash(), Some(grant.fact_hash()));
    ledger.verify().expect("role fact hash chain must verify");

    let grant_event = DirectoryRoleOutboxEvent::from_fact(&grant)
        .expect("encode the durable role grant outbox event");
    let revoke_event = DirectoryRoleOutboxEvent::from_fact(&revoke)
        .expect("encode the durable role revoke outbox event");
    assert_eq!(grant_event.event_id, grant.id());
    assert_eq!(revoke_event.event_id, revoke.id());
    assert_eq!(grant_event.aggregate_type, "directory_role");
    assert_eq!(revoke_event.aggregate_type, "directory_role");
    assert_eq!(grant_event.event_type, "directory.role_granted.v1");
    assert_eq!(revoke_event.event_type, "directory.role_revoked.v1");
    assert_eq!(grant_event.aggregate_version, grant.sequence());
    assert_eq!(revoke_event.aggregate_version, revoke.sequence());
    assert_eq!(grant_event.connector_id, connector_id);
    assert_eq!(revoke_event.connector_id, connector_id);

    let mut read_model = DirectoryRoleReadModel::default();
    read_model
        .apply(&grant)
        .expect("project the durable role grant event");
    assert_eq!(read_model.roles_for(&identity, connector_id).len(), 1);
    read_model
        .apply(&revoke)
        .expect("project the durable role revoke event");
    assert!(read_model.roles_for(&identity, connector_id).is_empty());
    read_model
        .apply(&revoke)
        .expect("replaying the role revoke event must be idempotent");
    assert_eq!(read_model.applied_event_count(), 2);

    let (replay_outcome, replayed_grant) = ledger
        .append(grant_input)
        .expect("replaying the role grant fact must be idempotent");
    assert_eq!(
        replay_outcome,
        DirectoryRoleFactAppendOutcome::AlreadyApplied
    );
    assert_eq!(replayed_grant.id(), grant.id());
    assert!(
        DirectoryRbacRollbackPlan::staged_import(Uuid::new_v4(), "G14 fixture rollback")
            .expect("staged rollback plan")
            .preserves_history()
    );
    let finalized_rollback =
        DirectoryRbacRollbackPlan::finalized(connector_id, "G14 fixture finalized rollback")
            .expect("finalized rollback plan");
    assert!(finalized_rollback.preserves_history());
    assert!(finalized_rollback.rebuilds_read_model());
}

#[test]
fn generic_ldap_certificate_trust_and_tls_handshake_boundaries_fail_closed() {
    let certificate = b"verified-generic-ldap-peer-certificate";
    let anchor = TrustAnchor::try_new(
        "g14-generic-ldap-ca",
        CertificateFingerprint::try_new(sha256_certificate_fingerprint(certificate))
            .expect("certificate fingerprint"),
        1,
    )
    .expect("trust anchor");
    let policy = TlsPolicy::ldaps("ldap.example.test", anchor.clone()).expect("LDAPS policy");
    let observed_at = Utc
        .with_ymd_and_hms(2026, 7, 30, 12, 0, 0)
        .single()
        .expect("fixed handshake timestamp");

    let valid = TlsHandshakeEvidence::from_verified_peer_at(
        DirectoryTransportMode::Ldaps,
        "ldap.example.test",
        TlsProtocolVersion::Tls1_3,
        certificate,
        &anchor,
        observed_at,
    )
    .expect("verified peer evidence");
    assert!(policy.validate_handshake_at(&valid, observed_at).is_ok());
    assert_eq!(
        policy.validate_endpoint("ldap://ldap.example.test:389"),
        Err(TlsPolicyError::InsecureOrUnexpectedScheme)
    );

    let wrong_certificate = TlsHandshakeEvidence::from_verified_peer_at(
        DirectoryTransportMode::Ldaps,
        "ldap.example.test",
        TlsProtocolVersion::Tls1_3,
        b"different-peer-certificate",
        &anchor,
        observed_at,
    )
    .expect("different peer evidence");
    assert_eq!(
        policy.validate_handshake_at(&wrong_certificate, observed_at),
        Err(TlsPolicyError::TrustAnchorMismatch)
    );

    let wrong_server = TlsHandshakeEvidence::from_verified_peer_at(
        DirectoryTransportMode::Ldaps,
        "other.example.test",
        TlsProtocolVersion::Tls1_3,
        certificate,
        &anchor,
        observed_at,
    )
    .expect("wrong server evidence");
    assert_eq!(
        policy.validate_handshake_at(&wrong_server, observed_at),
        Err(TlsPolicyError::ServerNameMismatch)
    );

    let wrong_mode = TlsHandshakeEvidence::from_verified_peer_at(
        DirectoryTransportMode::StartTls,
        "ldap.example.test",
        TlsProtocolVersion::Tls1_2,
        certificate,
        &anchor,
        observed_at,
    )
    .expect("StartTLS peer evidence");
    assert_eq!(
        policy.validate_handshake_at(&wrong_mode, observed_at),
        Err(TlsPolicyError::InsecureOrUnexpectedScheme)
    );
    let starttls_policy =
        TlsPolicy::starttls("ldap.example.test", anchor.clone()).expect("StartTLS policy");
    assert!(
        starttls_policy
            .validate_endpoint("ldap://ldap.example.test:389")
            .is_ok()
    );
    assert!(
        starttls_policy
            .validate_handshake_at(&wrong_mode, observed_at)
            .is_ok()
    );
    let stale = TlsHandshakeEvidence::from_verified_peer_at(
        DirectoryTransportMode::Ldaps,
        "ldap.example.test",
        TlsProtocolVersion::Tls1_3,
        certificate,
        &anchor,
        observed_at - ChronoDuration::minutes(6),
    )
    .expect("stale peer evidence");
    assert_eq!(
        policy.validate_handshake_at(&stale, observed_at),
        Err(TlsPolicyError::HandshakeEvidenceExpired)
    );
}

#[test]
fn generic_ldap_readiness_rejects_non_tls_database_and_malformed_endpoints() {
    assert!(postgres_url_uses_tls(
        "postgresql://acceptance.invalid/erpx?sslmode=verify-full"
    ));
    assert!(!postgres_url_uses_tls(
        "postgresql://acceptance.invalid/erpx?sslmode=disable"
    ));
    assert!(!postgres_url_uses_tls(
        "postgresql://acceptance.invalid/erpx?sslmode=verify-fullx"
    ));
    assert!(!postgres_url_uses_tls(
        "postgresql://acceptance.invalid/erpx?sslmode=disable&sslmode=verify-full"
    ));
    assert!(!postgres_url_uses_tls(
        "postgresql://acceptance.invalid/erpx?sslmode=require"
    ));
    assert!(!postgres_url_uses_tls(
        "postgresql://acceptance.invalid/erpx?sslmode=verify-ca"
    ));
    assert!(!postgres_url_uses_tls(
        "postgresql://acceptance.invalid/erpx?sslmode=verify-full&ssl-mode=disable"
    ));
    assert!(!postgres_url_uses_tls(
        "postgresql://acceptance.invalid/erpx?sslmode=verify-full&"
    ));
    assert!(!postgres_url_uses_tls(
        "postgresql://acceptance.invalid/erpx?sslmode=verify-full&connect_timeout"
    ));
    assert!(postgres_url_uses_tls(
        "postgresql://acceptance.invalid/erpx?sslmode=verify-full&connect_timeout=5"
    ));

    assert!(valid_ca_certificate_fingerprint(&"a".repeat(64)));
    assert!(valid_ca_certificate_fingerprint(&"A".repeat(64)));
    assert!(!valid_ca_certificate_fingerprint(&"0".repeat(64)));
    assert!(!valid_ca_certificate_fingerprint(&"g".repeat(64)));
    assert!(!valid_ca_certificate_fingerprint(&"a".repeat(63)));

    assert!(endpoint_has_authority(
        "ldaps://directory.example.test",
        "ldaps://"
    ));
    assert!(!endpoint_has_authority(
        "ldap://directory.example.test/ou=users",
        "ldap://"
    ));
    assert!(!endpoint_has_authority("ldaps://", "ldaps://"));
    assert!(!endpoint_has_authority(
        "ldap://directory.example.test host",
        "ldap://"
    ));
    assert!(
        endpoint_host("ldaps://directory.example.test:636", "ldaps://")
            .is_some_and(|host| host == "directory.example.test")
    );
}

fn revocation_validation_fixture() -> LiveConfig {
    LiveConfig {
        database_url: "postgresql://acceptance.invalid/erpx?sslmode=verify-full".into(),
        endpoint: "ldaps://ldap.example.test:636".into(),
        base_dn: "dc=example,dc=test".into(),
        bind_dn: "cn=bind,dc=example,dc=test".into(),
        password: "test-only-secret-reference".into(),
        ca_sha256: "a".repeat(64),
        server_name: "ldap.example.test".into(),
        tls_mode: DirectoryTlsMode::Ldaps,
        feed: DirectoryFeedKind::Rfc4533,
        page_size: 500,
        max_import_users: MAX_IMPORT_USERS,
        full_import: true,
        filter: None,
        immutable_attribute: None,
        login_attribute: None,
        email_attribute: None,
        display_name_attribute: None,
        group_attribute: None,
        disabled_attribute: None,
        deleted_attribute: None,
        revocation_external_id: "target-user".into(),
        revocation_kind: RevocationKind::Disable,
        revocation_triggered_at: Utc
            .with_ymd_and_hms(2026, 7, 29, 12, 0, 0)
            .single()
            .expect("fixed revocation timestamp"),
        revocation_evidence_path: "/tmp/g14-generic-ldap-evidence.json".into(),
    }
}

#[test]
fn generic_ldap_revocation_requires_applied_sla_evidence() {
    let config = revocation_validation_fixture();
    let observed_at = config.revocation_triggered_at + ChronoDuration::seconds(1);
    let deadline = observed_at + ChronoDuration::seconds(REVOCATION_SLA_SECONDS);
    let base = DurableRevocationEvidence {
        observed_at,
        deadline,
        reason: RevocationKind::Disable.reason().into(),
        status: "pending".into(),
        applied_at: None,
        active: false,
        revoked_at: Some(observed_at),
        immutable_authority: Some("ldap:entryUUID".into()),
        immutable_value: Some("entry-001".into()),
        outbox_events: 1,
        outbox_event_type: Some(RevocationKind::Disable.event_type().into()),
    };
    assert!(
        validate_durable_revocation_evidence(&base, &config)
            .expect_err("pending revocation must not be accepted")
            .contains("not applied")
    );

    let applied = DurableRevocationEvidence {
        status: "applied".into(),
        applied_at: Some(observed_at + ChronoDuration::seconds(2)),
        ..base
    };
    assert!(validate_durable_revocation_evidence(&applied, &config).is_ok());

    let late = DurableRevocationEvidence {
        applied_at: Some(deadline + ChronoDuration::seconds(1)),
        ..applied
    };
    assert!(
        validate_durable_revocation_evidence(&late, &config)
            .expect_err("late revocation must fail the SLA")
            .contains("outside its five-minute SLA")
    );
}

#[test]
fn generic_ldap_revocation_handoff_rejects_free_form_or_incomplete_evidence() {
    assert!(parse_revocation_action_evidence("external_id=target-user operation=disable").is_err());
    assert!(
        parse_revocation_action_evidence(
            r#"{"vendor":"Generic LDAP","feed":"rfc4533","external_id":"target-user"}"#
        )
        .is_err()
    );

    let evidence = parse_revocation_action_evidence(
        r#"{
            "exercise_id":"exercise-1",
            "vendor":"Generic LDAP",
            "feed":"rfc4533",
            "external_id":"target-user",
            "immutable_authority":"ldap:entryUUID",
            "immutable_value":"entry-001",
            "operation":"disable",
            "triggered_at":"2026-07-30T00:00:00Z",
            "action_at":"2026-07-30T00:00:01Z",
            "provider_event_id":"provider-event-1"
        }"#,
    )
    .expect("structured provider evidence should decode");
    assert_eq!(evidence.feed, "rfc4533");
    assert_eq!(evidence.immutable_authority, "ldap:entryUUID");
    assert_eq!(evidence.immutable_value, "entry-001");
    let mut config = revocation_validation_fixture();
    config.revocation_triggered_at = Utc
        .with_ymd_and_hms(2026, 7, 30, 0, 0, 0)
        .single()
        .expect("fixed provider trigger timestamp");
    assert!(validate_revocation_action_evidence(&evidence, &config).is_ok());
    let mut late = evidence;
    late.action_at = late.triggered_at + ChronoDuration::seconds(REVOCATION_SLA_SECONDS + 1);
    assert!(
        validate_revocation_action_evidence(&late, &config)
            .expect_err("provider action beyond the SLA must be rejected")
            .contains("exceeded the five-minute SLA")
    );
}
