//! Credential-gated, non-mock vendor acceptance for the registered directory
//! synchronization worker.
//!
//! The live cases deliberately do not start an LDAP/HTTP fake.  A configured
//! run creates a real connector in the explicitly supplied acceptance database,
//! then invokes the same [`DirectorySyncRuntimeHandler`] that the worker
//! registry installs for `directory_sync`.  The child process is used only to
//! isolate the production `ERPX_DIRECTORY_*` and UCS credential environment
//! from the parent test process.
//!
//! Required AD DS references:
//! `ERPX_G14_AD_TEST_DATABASE_URL`, `ERPX_G14_AD_ENDPOINT`,
//! `ERPX_G14_AD_BASE_DN`, `ERPX_G14_AD_BIND_DN`, `ERPX_G14_AD_PASSWORD`,
//! `ERPX_G14_AD_CA_SHA256`, `ERPX_G14_AD_SERVER_NAME`, and
//! `ERPX_G14_AD_TLS_MODE` (`ldaps` or `start_tls`).
//!
//! Required UCS references:
//! `ERPX_G14_UCS_TEST_DATABASE_URL`, `ERPX_G14_UCS_ENDPOINT`,
//! `ERPX_G14_UCS_BASE_DN`, `ERPX_G14_UCS_SUBSCRIPTION`,
//! `ERPX_G14_UCS_SUBSCRIPTION_PASSWORD`, `ERPX_G14_UCS_CA_SHA256`,
//! `ERPX_G14_UCS_SERVER_NAME`, and `ERPX_G14_UCS_TLS_MODE` (`https`).
//!
//! Both database URLs must specify `sslmode=verify-full`.  AD DS and UCS use
//! fixed native feed semantics
//! (`ad_dirsync` and `ucs_provisioning_service`); their feed kinds are not
//! environment inputs.  `*_MAX_IMPORT_USERS`, `*_FULL_IMPORT`, and the
//! revocation exercise inputs are mandatory.  `*_PAGE_SIZE` is optional but
//! bounded to 5,000, and `*_FILTER` remains optional.
//!
//! The harness does not automate a provider-side disable/delete operation.
//! The environment owner must perform that operation against the real AD DS
//! or UCS system and write provider evidence to the configured JSON path. The
//! harness then consumes the native feed and verifies the append-only identity
//! state, outbox event, revocation SLA row, immutable link, and observation
//! time. Missing or malformed evidence is a queued blocker, never a pass.
//! Missing external references are printed as an explicit queued blocker and
//! return without claiming a vendor certification result.

use std::{collections::BTreeMap, env, fs, process::Command};

use chrono::{DateTime, Duration, Utc};
use erpx_identity::{
    DirectoryConnector, DirectoryFeedKind, DirectoryProvider, DirectoryScope, DirectoryTlsMode,
    DirectoryTlsPolicy, SqlxDirectoryStore, apply_identity_migration,
};
use erpx_worker::{DIRECTORY_SYNC_HANDLER_NAME, DirectorySyncRuntimeHandler, NamedWorkerHandler};
use serde::Deserialize;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

const CHILD_MODE: &str = "ERPX_G14_DIRECTORY_VENDOR_ACCEPTANCE_CHILD";
const REVOCATION_CHILD_MODE: &str = "revocation";
const MAX_IMPORT_USERS: u64 = 1_000_000;
const MAX_PAGE_SIZE: u32 = 5_000;
const REVOCATION_SLA_SECONDS: i64 = 300;
const REVOCATION_POLL_INTERVAL_SECONDS: u64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Vendor {
    Ad,
    Ucs,
}

impl Vendor {
    const fn label(self) -> &'static str {
        match self {
            Self::Ad => "AD DS/DirSync",
            Self::Ucs => "UCS Provisioning Service",
        }
    }

    const fn prefix(self) -> &'static str {
        match self {
            Self::Ad => "ERPX_G14_AD",
            Self::Ucs => "ERPX_G14_UCS",
        }
    }

    const fn required_suffixes(self) -> &'static [&'static str] {
        match self {
            Self::Ad => &[
                "TEST_DATABASE_URL",
                "ENDPOINT",
                "BASE_DN",
                "BIND_DN",
                "PASSWORD",
                "CA_SHA256",
                "SERVER_NAME",
                "TLS_MODE",
                "MAX_IMPORT_USERS",
                "FULL_IMPORT",
                "REVOCATION_MAX_SECONDS",
                "REVOCATION_OPERATION",
                "REVOCATION_TARGET_EXTERNAL_ID",
                "REVOCATION_TARGET_IMMUTABLE_AUTHORITY",
                "REVOCATION_TARGET_IMMUTABLE_VALUE",
                "REVOCATION_ACTION_EVIDENCE_PATH",
            ],
            Self::Ucs => &[
                "TEST_DATABASE_URL",
                "ENDPOINT",
                "BASE_DN",
                "SUBSCRIPTION",
                "SUBSCRIPTION_PASSWORD",
                "CA_SHA256",
                "SERVER_NAME",
                "TLS_MODE",
                "MAX_IMPORT_USERS",
                "FULL_IMPORT",
                "REVOCATION_MAX_SECONDS",
                "REVOCATION_OPERATION",
                "REVOCATION_TARGET_EXTERNAL_ID",
                "REVOCATION_TARGET_IMMUTABLE_AUTHORITY",
                "REVOCATION_TARGET_IMMUTABLE_VALUE",
                "REVOCATION_ACTION_EVIDENCE_PATH",
            ],
        }
    }

    const fn feed(self) -> DirectoryFeedKind {
        match self {
            Self::Ad => DirectoryFeedKind::AdDirSync,
            Self::Ucs => DirectoryFeedKind::UcsProvisioningService,
        }
    }

    const fn provider(self) -> DirectoryProvider {
        match self {
            Self::Ad => DirectoryProvider::ActiveDirectoryDomainServices,
            Self::Ucs => DirectoryProvider::UniventionCorporateServer,
        }
    }

    fn env_name(self, suffix: &str) -> String {
        format!("{}_{}", self.prefix(), suffix)
    }

    const fn adapter_name(self) -> &'static str {
        match self {
            Self::Ad => "ad_dirsync",
            Self::Ucs => "ucs_provisioning_service",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ConfigError {
    Missing(Vec<String>),
    Invalid(String),
}

#[derive(Debug, Clone)]
struct LiveConfig {
    vendor: Vendor,
    database_url: String,
    endpoint: String,
    base_dn: String,
    credential: String,
    secret: String,
    ca_sha256: String,
    server_name: String,
    tls_mode: DirectoryTlsMode,
    feed: DirectoryFeedKind,
    page_size: u32,
    max_import_users: u64,
    full_import: bool,
    revocation_max_seconds: i64,
    revocation_operation: RevocationOperation,
    revocation_target_external_id: String,
    revocation_target_immutable_authority: String,
    revocation_target_immutable_value: String,
    revocation_action_evidence_path: String,
    filter: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RevocationOperation {
    Disable,
    Delete,
}

impl RevocationOperation {
    const fn expected_state(self) -> &'static str {
        match self {
            Self::Disable => "disabled",
            Self::Delete => "deleted",
        }
    }

    const fn event_type(self) -> &'static str {
        match self {
            Self::Disable => "directory.identity_disabled.v1",
            Self::Delete => "directory.identity_deleted.v1",
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Disable => "disable",
            Self::Delete => "delete",
        }
    }
}

#[derive(Debug, Deserialize)]
struct RevocationActionEvidence {
    exercise_id: String,
    vendor: String,
    feed: String,
    external_id: String,
    immutable_authority: String,
    immutable_value: String,
    operation: String,
    action_at: DateTime<Utc>,
    provider_event_id: String,
}

impl LiveConfig {
    fn from_env(vendor: Vendor) -> Result<Self, ConfigError> {
        parse_config(vendor, optional_env)
    }
}

fn optional_env(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty() && !value.chars().any(char::is_control))
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
    let sslmodes = query
        .split('&')
        .filter_map(|parameter| parameter.split_once('='))
        .filter(|(name, _)| name.eq_ignore_ascii_case("sslmode"))
        .map(|(_, value)| value)
        .collect::<Vec<_>>();
    sslmodes.len() == 1 && sslmodes[0].eq_ignore_ascii_case("verify-full")
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

fn parse_config<F>(vendor: Vendor, mut get: F) -> Result<LiveConfig, ConfigError>
where
    F: FnMut(&str) -> Option<String>,
{
    let required = |suffix: &'static str, get: &mut F, missing: &mut Vec<String>| {
        let name = format!("{}_{}", vendor.prefix(), suffix);
        match get(&name) {
            Some(value) => Some(value),
            None => {
                missing.push(name);
                None
            }
        }
    };

    let mut missing = Vec::new();
    let database_url = required("TEST_DATABASE_URL", &mut get, &mut missing);
    let endpoint = required("ENDPOINT", &mut get, &mut missing);
    let base_dn = required("BASE_DN", &mut get, &mut missing);
    let credential_suffix = match vendor {
        Vendor::Ad => "BIND_DN",
        Vendor::Ucs => "SUBSCRIPTION",
    };
    let secret_suffix = match vendor {
        Vendor::Ad => "PASSWORD",
        Vendor::Ucs => "SUBSCRIPTION_PASSWORD",
    };
    let credential = required(credential_suffix, &mut get, &mut missing);
    let secret = required(secret_suffix, &mut get, &mut missing);
    let ca_sha256 = required("CA_SHA256", &mut get, &mut missing);
    let server_name = required("SERVER_NAME", &mut get, &mut missing);
    let tls_mode = required("TLS_MODE", &mut get, &mut missing);
    let max_import_users = required("MAX_IMPORT_USERS", &mut get, &mut missing);
    let full_import = required("FULL_IMPORT", &mut get, &mut missing);
    let revocation_max_seconds = required("REVOCATION_MAX_SECONDS", &mut get, &mut missing);
    let revocation_operation = required("REVOCATION_OPERATION", &mut get, &mut missing);
    let revocation_target_external_id =
        required("REVOCATION_TARGET_EXTERNAL_ID", &mut get, &mut missing);
    let revocation_target_immutable_authority = required(
        "REVOCATION_TARGET_IMMUTABLE_AUTHORITY",
        &mut get,
        &mut missing,
    );
    let revocation_target_immutable_value =
        required("REVOCATION_TARGET_IMMUTABLE_VALUE", &mut get, &mut missing);
    let revocation_action_evidence_path =
        required("REVOCATION_ACTION_EVIDENCE_PATH", &mut get, &mut missing);
    if !missing.is_empty() {
        return Err(ConfigError::Missing(missing));
    }

    let database_url = database_url.expect("required database URL was checked");
    let endpoint = endpoint.expect("required endpoint was checked");
    let base_dn = base_dn.expect("required base DN was checked");
    let credential = credential.expect("required credential was checked");
    let secret = secret.expect("required secret was checked");
    let ca_sha256 = ca_sha256.expect("required CA fingerprint was checked");
    let server_name = server_name.expect("required TLS server name was checked");
    let tls_mode_value = tls_mode.expect("required TLS mode was checked");
    let max_import_users_value =
        max_import_users.expect("required full-import capacity was checked");
    let full_import_value = full_import.expect("required full-import switch was checked");
    let revocation_max_seconds_value =
        revocation_max_seconds.expect("required revocation SLA was checked");
    let revocation_operation_value =
        revocation_operation.expect("required revocation operation was checked");
    let revocation_target_external_id =
        revocation_target_external_id.expect("required revocation target external ID was checked");
    let revocation_target_immutable_authority = revocation_target_immutable_authority
        .expect("required revocation target immutable authority was checked");
    let revocation_target_immutable_value = revocation_target_immutable_value
        .expect("required revocation target immutable value was checked");
    let revocation_action_evidence_path = revocation_action_evidence_path
        .expect("required provider revocation evidence path was checked");

    if !postgres_url_uses_tls(&database_url) {
        return Err(ConfigError::Invalid(format!(
            "{}_TEST_DATABASE_URL must specify sslmode=verify-full",
            vendor.prefix()
        )));
    }
    if ca_sha256.len() != 64 || !ca_sha256.chars().all(|value| value.is_ascii_hexdigit()) {
        return Err(ConfigError::Invalid(format!(
            "{}_CA_SHA256 must be a 64-character hexadecimal fingerprint",
            vendor.prefix()
        )));
    }

    let tls_mode = match vendor {
        Vendor::Ad => match tls_mode_value.as_str() {
            "ldaps" if endpoint_has_authority(&endpoint, "ldaps://") => DirectoryTlsMode::Ldaps,
            "start_tls" if endpoint_has_authority(&endpoint, "ldap://") => {
                DirectoryTlsMode::StartTls
            }
            _ => {
                return Err(ConfigError::Invalid(format!(
                    "{}_TLS_MODE and endpoint must select ldaps:// or ldap:// with StartTLS",
                    vendor.prefix()
                )));
            }
        },
        Vendor::Ucs
            if tls_mode_value == "https" && endpoint_has_authority(&endpoint, "https://") =>
        {
            DirectoryTlsMode::Https
        }
        Vendor::Ucs => {
            return Err(ConfigError::Invalid(
                "ERPX_G14_UCS_TLS_MODE and endpoint must select https://".into(),
            ));
        }
    };

    let endpoint_scheme = match tls_mode {
        DirectoryTlsMode::Ldaps => "ldaps://",
        DirectoryTlsMode::StartTls => "ldap://",
        DirectoryTlsMode::Https => "https://",
    };
    if endpoint_host(&endpoint, endpoint_scheme)
        .is_none_or(|host| !host.eq_ignore_ascii_case(&server_name))
    {
        return Err(ConfigError::Invalid(format!(
            "{}_ENDPOINT/SERVER_NAME must match exactly",
            vendor.prefix()
        )));
    }

    let feed = vendor.feed();

    let page_size = optional_value(&mut get, vendor.env_name("PAGE_SIZE"), 500)?;
    if page_size == 0 || page_size > MAX_PAGE_SIZE {
        return Err(ConfigError::Invalid(format!(
            "{}_PAGE_SIZE must be between 1 and {MAX_PAGE_SIZE}",
            vendor.prefix()
        )));
    }
    let max_import_users = max_import_users_value
        .parse::<u64>()
        .map_err(|_| ConfigError::Invalid(vendor.env_name("MAX_IMPORT_USERS")))?;
    if max_import_users == 0 || max_import_users > MAX_IMPORT_USERS {
        return Err(ConfigError::Invalid(format!(
            "{}_MAX_IMPORT_USERS must be between 1 and {MAX_IMPORT_USERS}",
            vendor.prefix()
        )));
    }
    let full_import = match full_import_value.as_str() {
        "true" => true,
        "false" => false,
        _ => return Err(ConfigError::Invalid(vendor.env_name("FULL_IMPORT"))),
    };
    let revocation_max_seconds = revocation_max_seconds_value
        .parse::<i64>()
        .map_err(|_| ConfigError::Invalid(vendor.env_name("REVOCATION_MAX_SECONDS")))?;
    if revocation_max_seconds != REVOCATION_SLA_SECONDS {
        return Err(ConfigError::Invalid(format!(
            "{}_REVOCATION_MAX_SECONDS must be {REVOCATION_SLA_SECONDS}",
            vendor.prefix()
        )));
    }
    let revocation_operation = match revocation_operation_value.as_str() {
        "disable" => RevocationOperation::Disable,
        "delete" => RevocationOperation::Delete,
        _ => {
            return Err(ConfigError::Invalid(format!(
                "{}_REVOCATION_OPERATION must be disable or delete",
                vendor.prefix()
            )));
        }
    };
    for (suffix, value) in [
        (
            "REVOCATION_TARGET_EXTERNAL_ID",
            &revocation_target_external_id,
        ),
        (
            "REVOCATION_TARGET_IMMUTABLE_AUTHORITY",
            &revocation_target_immutable_authority,
        ),
        (
            "REVOCATION_TARGET_IMMUTABLE_VALUE",
            &revocation_target_immutable_value,
        ),
        (
            "REVOCATION_ACTION_EVIDENCE_PATH",
            &revocation_action_evidence_path,
        ),
    ] {
        if value.trim().is_empty() || value.chars().any(char::is_control) {
            return Err(ConfigError::Invalid(vendor.env_name(suffix)));
        }
    }

    Ok(LiveConfig {
        vendor,
        database_url,
        endpoint,
        base_dn,
        credential,
        secret,
        ca_sha256,
        server_name,
        tls_mode,
        feed,
        page_size,
        max_import_users,
        full_import,
        revocation_max_seconds,
        revocation_operation,
        revocation_target_external_id,
        revocation_target_immutable_authority,
        revocation_target_immutable_value,
        revocation_action_evidence_path,
        filter: get(&vendor.env_name("FILTER")),
    })
}

fn optional_value<T, F>(get: &mut F, name: String, default: T) -> Result<T, ConfigError>
where
    T: std::str::FromStr,
    F: FnMut(&str) -> Option<String>,
{
    get(&name)
        .map(|value| {
            value
                .parse()
                .map_err(|_| ConfigError::Invalid(name.clone()))
        })
        .unwrap_or(Ok(default))
}

fn full_import_evidence_is_acceptable(
    state: &str,
    max_users: i64,
    page_size: i32,
    users_seen: i64,
    pages_applied: i64,
    reported_users: u64,
) -> bool {
    state == "completed"
        && max_users == i64::try_from(MAX_IMPORT_USERS).expect("one-million bound fits BIGINT")
        && (1..=i64::from(MAX_PAGE_SIZE)).contains(&i64::from(page_size))
        && (1..=i64::try_from(MAX_IMPORT_USERS).expect("one-million bound fits BIGINT"))
            .contains(&users_seen)
        && pages_applied >= 1
        && u64::try_from(users_seen).ok() == Some(reported_users)
}

fn connector(config: &LiveConfig, connector_id: Uuid) -> DirectoryConnector {
    DirectoryConnector {
        id: connector_id,
        provider: config.vendor.provider(),
        scope: DirectoryScope::Holding {
            holding_ref: format!(
                "g14-{}-{}",
                config.vendor.prefix().to_ascii_lowercase(),
                connector_id.simple()
            ),
        },
        endpoint: config.endpoint.clone(),
        base_dn: config.base_dn.clone(),
        tls: DirectoryTlsPolicy {
            mode: config.tls_mode,
            ca_certificate_sha256: config.ca_sha256.clone(),
            server_name: config.server_name.clone(),
            minimum_tls_version: "1.2".into(),
        },
        feed: config.feed,
        credential_ref: format!(
            "acceptance-env:ERPX_G14_{}_{}",
            match config.vendor {
                Vendor::Ad => "AD",
                Vendor::Ucs => "UCS",
            },
            match config.vendor {
                Vendor::Ad => "PASSWORD",
                Vendor::Ucs => "SUBSCRIPTION_PASSWORD",
            }
        ),
        enabled: true,
        max_import_users: config.max_import_users,
        revision: 1,
    }
}

fn production_env_names() -> &'static [&'static str] {
    &[
        "ERPX_DIRECTORY_CONNECTOR_ID",
        "ERPX_DIRECTORY_FEED_ADAPTER",
        "ERPX_DIRECTORY_BIND_DN",
        "ERPX_DIRECTORY_PASSWORD",
        "ERPX_DIRECTORY_PAGE_SIZE",
        "ERPX_DIRECTORY_FULL_IMPORT",
        "ERPX_DIRECTORY_FILTER",
        "ERPX_UCS_SUBSCRIPTION",
        "ERPX_UCS_SUBSCRIPTION_PASSWORD",
    ]
}

fn configure_child(mut command: Command, config: &LiveConfig, connector_id: Uuid) -> Command {
    for name in production_env_names() {
        command.env_remove(name);
    }
    command
        .env(CHILD_MODE, "1")
        .env("ERPX_DIRECTORY_CONNECTOR_ID", connector_id.to_string())
        .env("ERPX_DIRECTORY_FEED_ADAPTER", config.vendor.adapter_name())
        .env("ERPX_DIRECTORY_PAGE_SIZE", config.page_size.to_string())
        .env("ERPX_DIRECTORY_FULL_IMPORT", config.full_import.to_string());
    if let Some(filter) = &config.filter {
        command.env("ERPX_DIRECTORY_FILTER", filter);
    }
    match config.vendor {
        Vendor::Ad => {
            command
                .env("ERPX_DIRECTORY_BIND_DN", &config.credential)
                .env("ERPX_DIRECTORY_PASSWORD", &config.secret);
        }
        Vendor::Ucs => {
            command
                .env("ERPX_UCS_SUBSCRIPTION", &config.credential)
                .env("ERPX_UCS_SUBSCRIPTION_PASSWORD", &config.secret);
        }
    }
    command
}

fn redact(output: &[u8], config: &LiveConfig) -> String {
    let mut output = String::from_utf8_lossy(output).into_owned();
    for value in [
        config.secret.as_str(),
        config.credential.as_str(),
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

fn queue_revocation_blocker(config: &LiveConfig, detail: &str) {
    eprintln!(
        "QUEUED BLOCKER (not a pass): {} native {} revocation exercise requires a real provider-side {} operation and evidence; required inputs are {}_REVOCATION_OPERATION, {}_REVOCATION_TARGET_EXTERNAL_ID, {}_REVOCATION_TARGET_IMMUTABLE_AUTHORITY, {}_REVOCATION_TARGET_IMMUTABLE_VALUE, {}_REVOCATION_MAX_SECONDS=300, and {}_REVOCATION_ACTION_EVIDENCE_PATH; {}",
        config.vendor.label(),
        config.vendor.adapter_name(),
        config.revocation_operation.as_str(),
        config.vendor.prefix(),
        config.vendor.prefix(),
        config.vendor.prefix(),
        config.vendor.prefix(),
        config.vendor.prefix(),
        config.vendor.prefix(),
        detail,
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RevocationTimingError {
    MissingActionEvidence,
    MissingObservationEvidence,
    NegativeElapsed,
    InvalidSla,
    TimestampOverflow,
    ExceededSla,
}

fn revocation_deadline(
    action_at: DateTime<Utc>,
    max_seconds: i64,
) -> Result<DateTime<Utc>, RevocationTimingError> {
    if max_seconds < 0 {
        return Err(RevocationTimingError::InvalidSla);
    }
    let duration =
        Duration::try_seconds(max_seconds).ok_or(RevocationTimingError::TimestampOverflow)?;
    action_at
        .checked_add_signed(duration)
        .ok_or(RevocationTimingError::TimestampOverflow)
}

fn validate_revocation_observation_timing(
    action_at: Option<DateTime<Utc>>,
    observed_at: Option<DateTime<Utc>>,
    max_seconds: i64,
) -> Result<Duration, RevocationTimingError> {
    let action_at = action_at.ok_or(RevocationTimingError::MissingActionEvidence)?;
    let observed_at = observed_at.ok_or(RevocationTimingError::MissingObservationEvidence)?;
    let deadline = revocation_deadline(action_at, max_seconds)?;
    if observed_at < action_at {
        return Err(RevocationTimingError::NegativeElapsed);
    }
    if observed_at > deadline {
        return Err(RevocationTimingError::ExceededSla);
    }
    Ok(observed_at.signed_duration_since(action_at))
}

fn parse_revocation_evidence(
    config: &LiveConfig,
    contents: &str,
) -> Result<RevocationActionEvidence, String> {
    let evidence = serde_json::from_str::<RevocationActionEvidence>(contents)
        .map_err(|_| "provider evidence JSON does not match the required schema".to_owned())?;
    for (field, value) in [
        ("exercise_id", &evidence.exercise_id),
        ("vendor", &evidence.vendor),
        ("feed", &evidence.feed),
        ("external_id", &evidence.external_id),
        ("immutable_authority", &evidence.immutable_authority),
        ("immutable_value", &evidence.immutable_value),
        ("operation", &evidence.operation),
        ("provider_event_id", &evidence.provider_event_id),
    ] {
        if value.trim().is_empty() || value.chars().any(char::is_control) {
            return Err(format!(
                "provider evidence field {field} is empty or unsafe"
            ));
        }
    }
    if evidence.vendor != config.vendor.label() {
        return Err("provider evidence vendor does not match the configured vendor".into());
    }
    if evidence.feed != config.vendor.adapter_name() {
        return Err("provider evidence feed does not match the native feed kind".into());
    }
    if evidence.operation != config.revocation_operation.as_str() {
        return Err("provider evidence operation does not match the configured operation".into());
    }
    if evidence.external_id != config.revocation_target_external_id
        || evidence.immutable_authority != config.revocation_target_immutable_authority
        || evidence.immutable_value != config.revocation_target_immutable_value
    {
        return Err("provider evidence target does not match the immutable target contract".into());
    }
    Ok(evidence)
}

async fn wait_for_revocation_evidence(
    config: &LiveConfig,
    baseline_at: DateTime<Utc>,
) -> Option<RevocationActionEvidence> {
    let wait_deadline = match revocation_deadline(baseline_at, config.revocation_max_seconds) {
        Ok(deadline) => deadline,
        Err(error) => {
            queue_revocation_blocker(
                config,
                &format!("provider evidence wait window failed closed: {error:?}"),
            );
            return None;
        }
    };
    loop {
        match fs::read_to_string(&config.revocation_action_evidence_path) {
            Ok(contents) => match parse_revocation_evidence(config, &contents) {
                Ok(evidence) if evidence.action_at < baseline_at => {
                    queue_revocation_blocker(
                        config,
                        "provider action evidence predates the active baseline; repeat the exercise after the harness establishes the target",
                    );
                    return None;
                }
                Ok(evidence) if evidence.action_at > Utc::now() => {
                    queue_revocation_blocker(
                        config,
                        "provider action evidence is timestamped in the future; synchronized clocks are required",
                    );
                    return None;
                }
                Ok(evidence) => return Some(evidence),
                Err(detail) => {
                    queue_revocation_blocker(config, &detail);
                    return None;
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if Utc::now() >= wait_deadline {
                    queue_revocation_blocker(
                        config,
                        "the provider-generated evidence file did not appear within the five-minute exercise window; no disable/delete automation is available",
                    );
                    return None;
                }
            }
            Err(_) => {
                queue_revocation_blocker(
                    config,
                    "the configured provider evidence path could not be read",
                );
                return None;
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

#[derive(Debug)]
struct NativeRevocationObservation {
    state: String,
    immutable_authority: String,
    immutable_value: String,
    immutable_link_consistent: bool,
    observed_at: DateTime<Utc>,
    status: String,
    event_type: String,
    native_cursor: bool,
}

fn immutable_link_matches(
    identity_authority: &str,
    identity_value: &str,
    link_authority: Option<&str>,
    link_value: Option<&str>,
) -> bool {
    link_authority == Some(identity_authority) && link_value == Some(identity_value)
}

async fn native_revocation_observation(
    pool: &sqlx::PgPool,
    config: &LiveConfig,
    connector_id: Uuid,
) -> Result<Option<NativeRevocationObservation>, sqlx::Error> {
    let row = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            DateTime<Utc>,
            String,
            String,
            serde_json::Value,
            Option<String>,
            Option<String>,
        ),
    >(
        "SELECT i.state, i.immutable_authority, i.immutable_value,
                s.observed_at, s.status, o.event_type, o.payload_json,
                link.immutable_authority, link.immutable_value
         FROM erpx_identity.directory_identities i
         JOIN erpx_identity.directory_revocation_sla s
           ON s.connector_id=i.connector_id AND s.external_id=i.external_id
         JOIN erpx_identity.directory_outbox o ON o.event_id=s.event_id
         LEFT JOIN erpx_identity.directory_identity_links link
           ON link.connector_id=i.connector_id
          AND link.external_object_id=i.external_id
         WHERE i.connector_id=$1 AND i.external_id=$2
           AND o.aggregate_type='directory_identity' AND o.event_type=$3
         ORDER BY s.observed_at DESC
         LIMIT 1",
    )
    .bind(connector_id)
    .bind(&config.revocation_target_external_id)
    .bind(config.revocation_operation.event_type())
    .fetch_optional(pool)
    .await?;

    Ok(row.map(
        |(
            state,
            immutable_authority,
            immutable_value,
            observed_at,
            status,
            event_type,
            payload,
            link_authority,
            link_value,
        )| {
            let immutable_link_consistent = immutable_link_matches(
                &immutable_authority,
                &immutable_value,
                link_authority.as_deref(),
                link_value.as_deref(),
            );
            NativeRevocationObservation {
                state,
                immutable_authority,
                immutable_value,
                immutable_link_consistent,
                observed_at,
                status,
                event_type,
                native_cursor: payload
                    .get("cursor")
                    .and_then(|cursor| {
                        cursor.get(match config.feed {
                            DirectoryFeedKind::AdDirSync => "AdDirSync",
                            DirectoryFeedKind::UcsProvisioningService => "Ucs",
                            DirectoryFeedKind::Polling | DirectoryFeedKind::Rfc4533 => {
                                "unsupported"
                            }
                        })
                    })
                    .is_some(),
            }
        },
    ))
}

async fn run_revocation_live_child(config: LiveConfig, connector_id: Uuid) {
    let baseline_at = match env::var("ERPX_G14_DIRECTORY_VENDOR_BASELINE_AT")
        .ok()
        .and_then(|value| value.parse::<DateTime<Utc>>().ok())
    {
        Some(value) => value,
        None => {
            queue_revocation_blocker(
                &config,
                "the harness baseline timestamp was not transferred to the native-feed exercise child",
            );
            return;
        }
    };
    let pool = match PgPoolOptions::new()
        .max_connections(4)
        .connect(&config.database_url)
        .await
    {
        Ok(pool) => pool,
        Err(_) => {
            queue_revocation_blocker(&config, "the dedicated acceptance database is unavailable");
            return;
        }
    };
    let handler = match DirectorySyncRuntimeHandler::from_env(pool.clone()) {
        Ok(handler) => handler,
        Err(_) => {
            queue_revocation_blocker(&config, "the registered native-feed worker is unavailable");
            pool.close().await;
            return;
        }
    };
    if handler.name() != DIRECTORY_SYNC_HANDLER_NAME {
        queue_revocation_blocker(&config, "the registered worker name is not directory_sync");
        pool.close().await;
        return;
    }
    let Some(evidence) = wait_for_revocation_evidence(&config, baseline_at).await else {
        pool.close().await;
        return;
    };
    let deadline = match revocation_deadline(evidence.action_at, config.revocation_max_seconds) {
        Ok(deadline) => deadline,
        Err(error) => {
            queue_revocation_blocker(
                &config,
                &format!("provider evidence deadline failed closed: {error:?}"),
            );
            pool.close().await;
            return;
        }
    };
    loop {
        if let Err(error) = handler.run_once(Uuid::new_v4()).await {
            queue_revocation_blocker(
                &config,
                &format!(
                    "native {} delivery failed closed: {error}",
                    config.vendor.adapter_name()
                ),
            );
            pool.close().await;
            return;
        }
        match native_revocation_observation(&pool, &config, connector_id).await {
            Ok(Some(observation)) => {
                let elapsed = match validate_revocation_observation_timing(
                    Some(evidence.action_at),
                    Some(observation.observed_at),
                    config.revocation_max_seconds,
                ) {
                    Ok(elapsed) => elapsed,
                    Err(error) => {
                        queue_revocation_blocker(
                            &config,
                            &format!("native revocation timing failed closed: {error:?}"),
                        );
                        pool.close().await;
                        return;
                    }
                };
                if observation.state != config.revocation_operation.expected_state()
                    || observation.immutable_authority
                        != config.revocation_target_immutable_authority
                    || observation.immutable_value != config.revocation_target_immutable_value
                    || observation.status == "breached"
                    || observation.event_type != config.revocation_operation.event_type()
                    || !observation.immutable_link_consistent
                    || !observation.native_cursor
                {
                    queue_revocation_blocker(
                        &config,
                        "native revocation evidence failed the immutable target, native cursor, append-only state, or five-minute SLA checks",
                    );
                    pool.close().await;
                    return;
                }
                eprintln!(
                    "G14 {} native {} revocation evidence observed in {} seconds for exercise {} (provider event {})",
                    config.vendor.label(),
                    config.vendor.adapter_name(),
                    elapsed.num_seconds(),
                    evidence.exercise_id,
                    evidence.provider_event_id,
                );
                pool.close().await;
                return;
            }
            Ok(None) if Utc::now() >= deadline => {
                queue_revocation_blocker(
                    &config,
                    "the native disable/delete event was not durably observed within 300 seconds of the provider action",
                );
                pool.close().await;
                return;
            }
            Ok(None) => {}
            Err(_) => {
                queue_revocation_blocker(
                    &config,
                    "revocation evidence could not be read from SQLx",
                );
                pool.close().await;
                return;
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(
            REVOCATION_POLL_INTERVAL_SECONDS,
        ))
        .await;
    }
}

async fn run_revocation_child_process(
    config: &LiveConfig,
    connector_id: Uuid,
    test_name: &'static str,
    baseline_at: DateTime<Utc>,
) {
    let executable = env::current_exe().expect("locate the vendor acceptance binary");
    let mut command = Command::new(executable);
    command
        .arg("--exact")
        .arg(test_name)
        .arg("--nocapture")
        .arg("--test-threads=1");
    let output = configure_child(command, config, connector_id)
        .env(CHILD_MODE, REVOCATION_CHILD_MODE)
        .env("ERPX_DIRECTORY_FULL_IMPORT", "false")
        .env(
            "ERPX_G14_DIRECTORY_VENDOR_BASELINE_AT",
            baseline_at.to_rfc3339(),
        )
        .output()
        .expect("spawn the native-feed revocation child");
    if !redact(&output.stdout, config).is_empty() {
        eprintln!("{}", redact(&output.stdout, config));
    }
    if !redact(&output.stderr, config).is_empty() {
        eprintln!("{}", redact(&output.stderr, config));
    }
    assert!(
        output.status.success(),
        "native {} revocation exercise child failed with status {:?}",
        config.vendor.adapter_name(),
        output.status.code()
    );
}

async fn run_live_child(config: LiveConfig, connector_id: Uuid, test_name: &'static str) {
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&config.database_url)
        .await
        .unwrap_or_else(|error| {
            panic!(
                "connect to the {0} acceptance database: {error}",
                config.vendor.label()
            )
        });
    apply_identity_migration(&pool)
        .await
        .unwrap_or_else(|error| {
            panic!(
                "apply identity schema for {0}: {error}",
                config.vendor.label()
            )
        });

    let connector = connector(&config, connector_id);
    let store = SqlxDirectoryStore::new(pool.clone());
    store
        .save_connector(&connector, Utc::now())
        .await
        .unwrap_or_else(|error| {
            panic!("persist the {0} connector: {error}", config.vendor.label())
        });

    let handler = DirectorySyncRuntimeHandler::from_env(pool.clone())
        .unwrap_or_else(|error| panic!("construct the registered directory sync handler: {error}"));
    assert_eq!(handler.name(), DIRECTORY_SYNC_HANDLER_NAME);
    let report = handler
        .run_once(Uuid::new_v4())
        .await
        .unwrap_or_else(|error| {
            panic!(
                "run live {0} synchronization: {error}",
                config.vendor.label()
            )
        });
    assert!(
        report.claimed >= 1,
        "{0} must claim a live page",
        config.vendor.label()
    );
    assert!(
        report.processed >= 1,
        "{0} must durably process at least one live directory change",
        config.vendor.label()
    );

    // A configured full-import acceptance is not proven by observing one
    // live record.  Require the durable import receipt to be completed and
    // bounded, and bind its count to the registered worker's report so a
    // partial, rolled-back, or non-full-import execution cannot look like
    // vendor evidence.
    let (import_state, import_max_users, import_page_size, import_users_seen, import_pages): (
        String,
        i64,
        i32,
        i64,
        i64,
    ) = sqlx::query_as(
        "SELECT state, max_users, page_size, users_seen, pages_applied
         FROM erpx_identity.directory_imports
         WHERE connector_id=$1
         ORDER BY started_at DESC
         LIMIT 1",
    )
    .bind(connector.id)
    .fetch_one(&pool)
    .await
    .expect("read durable full-import evidence");
    assert!(
        full_import_evidence_is_acceptable(
            &import_state,
            import_max_users,
            import_page_size,
            import_users_seen,
            import_pages,
            report.processed,
        ),
        "{} full import must persist a completed, bounded receipt whose user count matches the registered worker report",
        config.vendor.label()
    );
    let baseline_at = Utc::now();

    let cursor = store
        .latest_cursor(connector.id)
        .await
        .expect("read the durably committed vendor cursor")
        .expect("live vendor synchronization must persist a cursor");
    assert_eq!(cursor.connector_id, connector.id);
    assert_eq!(cursor.feed, config.feed);
    assert!(cursor.sequence >= 1);

    let checkpoint_status: String = sqlx::query_scalar(
        "SELECT status FROM erpx_identity.directory_sync_checkpoints WHERE connector_id=$1",
    )
    .bind(connector.id)
    .fetch_one(&pool)
    .await
    .expect("read the vendor synchronization checkpoint");
    assert_eq!(checkpoint_status, "healthy");

    let identities: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM erpx_identity.directory_identities WHERE connector_id=$1",
    )
    .bind(connector.id)
    .fetch_one(&pool)
    .await
    .expect("read durably imported vendor identities");
    assert!(
        identities >= 1,
        "live vendor changes must reach the identity store"
    );
    let baseline_target_state: Option<String> = sqlx::query_scalar(
        "SELECT state FROM erpx_identity.directory_identities
         WHERE connector_id=$1 AND external_id=$2",
    )
    .bind(connector.id)
    .bind(&config.revocation_target_external_id)
    .fetch_optional(&pool)
    .await
    .expect("read the active revocation target baseline");
    if baseline_target_state.as_deref() != Some("active") {
        queue_revocation_blocker(
            &config,
            "the configured immutable target was not active after the baseline native import; provider action cannot be measured safely",
        );
        pool.close().await;
        return;
    }

    let connector_events: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM erpx_identity.directory_outbox
         WHERE aggregate_id=$1 AND aggregate_type='directory_connector'",
    )
    .bind(connector.id)
    .fetch_one(&pool)
    .await
    .expect("read connector outbox evidence");
    assert!(
        connector_events >= 1,
        "connector persistence must emit outbox evidence"
    );

    let stored_credential_ref: String = sqlx::query_scalar(
        "SELECT credential_ref FROM erpx_identity.directory_connectors WHERE id=$1",
    )
    .bind(connector.id)
    .fetch_one(&pool)
    .await
    .expect("read persisted vendor credential reference");
    assert_eq!(stored_credential_ref, connector.credential_ref);
    run_revocation_child_process(&config, connector.id, test_name, baseline_at).await;
    pool.close().await;
}

async fn run_live(vendor: Vendor, test_name: &'static str) {
    let config = match LiveConfig::from_env(vendor) {
        Ok(config) => config,
        Err(ConfigError::Missing(missing)) => {
            eprintln!(
                "QUEUED BLOCKER (not a pass): G14 {} certification requires an external vendor and dedicated TLS database; missing references: {}",
                vendor.label(),
                missing.join(", ")
            );
            return;
        }
        Err(ConfigError::Invalid(name)) => {
            panic!(
                "invalid G14 {} acceptance configuration: {name}",
                vendor.label()
            );
        }
    };

    let mut queued = Vec::new();
    if !config.full_import {
        queued.push(format!("{}_FULL_IMPORT=true", vendor.prefix()));
    }
    if config.max_import_users != MAX_IMPORT_USERS {
        queued.push(format!("{}_MAX_IMPORT_USERS=1000000", vendor.prefix()));
    }
    if !queued.is_empty() {
        eprintln!(
            "QUEUED BLOCKER (not a pass): {} certification requires explicit {}",
            vendor.label(),
            queued.join(" and ")
        );
        return;
    }

    if env::var(CHILD_MODE).as_deref() == Ok("1") {
        let connector_id = env::var("ERPX_DIRECTORY_CONNECTOR_ID")
            .expect("child connector ID")
            .parse::<Uuid>()
            .expect("child connector ID must be a UUID");
        run_live_child(config, connector_id, test_name).await;
        return;
    }
    if env::var(CHILD_MODE).as_deref() == Ok(REVOCATION_CHILD_MODE) {
        let connector_id = env::var("ERPX_DIRECTORY_CONNECTOR_ID")
            .expect("revocation child connector ID")
            .parse::<Uuid>()
            .expect("revocation child connector ID must be a UUID");
        run_revocation_live_child(config, connector_id).await;
        return;
    }

    let connector_id = Uuid::new_v4();
    let executable = env::current_exe().expect("locate the vendor acceptance binary");
    let mut command = Command::new(executable);
    command
        .arg("--exact")
        .arg(test_name)
        .arg("--nocapture")
        .arg("--test-threads=1");
    let output = configure_child(command, &config, connector_id)
        .output()
        .expect("spawn the live vendor acceptance child");
    assert!(
        output.status.success(),
        "live {} acceptance child failed (status {:?})\nstdout:\n{}\nstderr:\n{}",
        vendor.label(),
        output.status.code(),
        redact(&output.stdout, &config),
        redact(&output.stderr, &config),
    );
}

#[tokio::test]
async fn ad_ds_dirsync_live_acceptance() {
    run_live(Vendor::Ad, "ad_ds_dirsync_live_acceptance").await;
}

#[tokio::test]
async fn ucs_provisioning_service_live_acceptance() {
    run_live(Vendor::Ucs, "ucs_provisioning_service_live_acceptance").await;
}

#[test]
fn ad_missing_environment_is_queued_with_exact_references() {
    let result = parse_config(Vendor::Ad, |_| None);
    let ConfigError::Missing(missing) = result.expect_err("AD configuration must be incomplete")
    else {
        panic!("expected missing AD references");
    };
    let expected = Vendor::Ad
        .required_suffixes()
        .iter()
        .map(|suffix| format!("ERPX_G14_AD_{suffix}"))
        .collect::<Vec<_>>();
    assert_eq!(missing, expected);
    eprintln!(
        "QUEUED BLOCKER (not a pass): AD DS/DirSync certification is waiting for {}",
        missing.join(", ")
    );
}

#[test]
fn ucs_missing_environment_is_queued_with_exact_references() {
    let result = parse_config(Vendor::Ucs, |_| None);
    let ConfigError::Missing(missing) = result.expect_err("UCS configuration must be incomplete")
    else {
        panic!("expected missing UCS references");
    };
    let expected = Vendor::Ucs
        .required_suffixes()
        .iter()
        .map(|suffix| format!("ERPX_G14_UCS_{suffix}"))
        .collect::<Vec<_>>();
    assert_eq!(missing, expected);
    eprintln!(
        "QUEUED BLOCKER (not a pass): UCS Provisioning Service certification is waiting for {}",
        missing.join(", ")
    );
}

#[test]
fn vendor_configuration_rejects_cleartext_transport_and_unbounded_imports() {
    let mut ad = complete_values(Vendor::Ad);
    ad.insert("ERPX_G14_AD_ENDPOINT", "ldap://ad.example.test".into());
    ad.insert("ERPX_G14_AD_TLS_MODE", "ldaps".into());
    assert!(matches!(
        parse_config(Vendor::Ad, |name| ad.get(name).cloned()),
        Err(ConfigError::Invalid(name)) if name.contains("TLS_MODE")
    ));

    let mut ucs = complete_values(Vendor::Ucs);
    ucs.insert("ERPX_G14_UCS_ENDPOINT", "http://ucs.example.test".into());
    ucs.insert("ERPX_G14_UCS_TLS_MODE", "http".into());
    assert!(matches!(
        parse_config(Vendor::Ucs, |name| ucs.get(name).cloned()),
        Err(ConfigError::Invalid(name)) if name.contains("TLS_MODE")
    ));

    let mut bounded = complete_values(Vendor::Ad);
    bounded.insert("ERPX_G14_AD_MAX_IMPORT_USERS", "1000001".into());
    assert!(matches!(
        parse_config(Vendor::Ad, |name| bounded.get(name).cloned()),
        Err(ConfigError::Invalid(name)) if name.contains("MAX_IMPORT_USERS")
    ));

    let mut insecure_database = complete_values(Vendor::Ad);
    insecure_database.insert(
        "ERPX_G14_AD_TEST_DATABASE_URL",
        "postgresql://acceptance.invalid/erpx?sslmode=verify-fullx".into(),
    );
    assert!(matches!(
        parse_config(Vendor::Ad, |name| insecure_database.get(name).cloned()),
        Err(ConfigError::Invalid(name)) if name.contains("TEST_DATABASE_URL")
    ));

    for vendor in [Vendor::Ad, Vendor::Ucs] {
        let mut ambiguous_database = complete_values(vendor);
        let database_name = match vendor {
            Vendor::Ad => "ERPX_G14_AD_TEST_DATABASE_URL",
            Vendor::Ucs => "ERPX_G14_UCS_TEST_DATABASE_URL",
        };
        ambiguous_database.insert(
            database_name,
            "postgresql://acceptance.invalid/erpx?sslmode=disable&sslmode=verify-full".into(),
        );
        assert!(matches!(
            parse_config(vendor, |name| ambiguous_database.get(name).cloned()),
            Err(ConfigError::Invalid(name)) if name.contains("TEST_DATABASE_URL")
        ));
    }

    let mut unbounded_ucs_page = complete_values(Vendor::Ucs);
    unbounded_ucs_page.insert("ERPX_G14_UCS_PAGE_SIZE", "5001".into());
    assert!(matches!(
        parse_config(Vendor::Ucs, |name| unbounded_ucs_page.get(name).cloned()),
        Err(ConfigError::Invalid(name)) if name.contains("PAGE_SIZE")
    ));
}

#[test]
fn vendor_readiness_accepts_only_authoritative_tls_database_and_endpoint_shapes() {
    assert!(postgres_url_uses_tls(
        "postgresql://acceptance.invalid/erpx?sslmode=verify-full"
    ));
    assert!(!postgres_url_uses_tls(
        "postgresql://acceptance.invalid/erpx?sslmode=require"
    ));
    assert!(!postgres_url_uses_tls(
        "postgresql://acceptance.invalid/erpx?sslmode=verify-ca"
    ));
    assert!(!postgres_url_uses_tls(
        "postgresql://acceptance.invalid/erpx?sslmode=disable"
    ));
    assert!(endpoint_has_authority(
        "ldaps://ad.example.test",
        "ldaps://"
    ));
    assert!(endpoint_has_authority(
        "https://ucs.example.test/provisioning",
        "https://"
    ));
    assert!(!endpoint_has_authority("https://", "https://"));
}

#[test]
fn vendor_config_uses_fixed_native_feed_kinds_without_feed_environment_inputs() {
    let ad = complete_values(Vendor::Ad);
    assert!(!ad.contains_key("ERPX_G14_AD_FEED"));
    let ad_config = parse_config(Vendor::Ad, |name| ad.get(name).cloned())
        .expect("AD fixed native feed configuration");
    assert_eq!(ad_config.feed, DirectoryFeedKind::AdDirSync);

    let ucs = complete_values(Vendor::Ucs);
    assert!(!ucs.contains_key("ERPX_G14_UCS_FEED"));
    let ucs_config = parse_config(Vendor::Ucs, |name| ucs.get(name).cloned())
        .expect("UCS fixed native feed configuration");
    assert_eq!(ucs_config.feed, DirectoryFeedKind::UcsProvisioningService);
}

#[test]
fn full_import_evidence_contract_rejects_partial_or_unbounded_receipts() {
    let valid = ("completed", 1_000_000_i64, 500_i32, 2_i64, 1_i64);
    assert!(full_import_evidence_is_acceptable(
        valid.0, valid.1, valid.2, valid.3, valid.4, 2
    ));
    assert!(!full_import_evidence_is_acceptable(
        valid.0, valid.1, valid.2, valid.3, valid.4, 3
    ));

    for partial in [
        ("running", 1_000_000_i64, 500_i32, 2_i64, 1_i64),
        ("rolled_back", 1_000_000_i64, 500_i32, 2_i64, 1_i64),
        ("completed", 1_000_001_i64, 500_i32, 2_i64, 1_i64),
        ("completed", 1_000_000_i64, 5_001_i32, 2_i64, 1_i64),
        ("completed", 1_000_000_i64, 500_i32, 0_i64, 1_i64),
    ] {
        assert!(!full_import_evidence_is_acceptable(
            partial.0, partial.1, partial.2, partial.3, partial.4, 2
        ));
    }
}

#[test]
fn revocation_sla_rejects_invalid_timing_and_queues_unavailable_provider_evidence() {
    let action_at = Utc::now();
    assert_eq!(
        validate_revocation_observation_timing(
            Some(action_at),
            Some(action_at + Duration::seconds(REVOCATION_SLA_SECONDS)),
            REVOCATION_SLA_SECONDS,
        ),
        Ok(Duration::seconds(REVOCATION_SLA_SECONDS))
    );
    assert_eq!(
        validate_revocation_observation_timing(
            Some(action_at),
            Some(action_at + Duration::seconds(REVOCATION_SLA_SECONDS) + Duration::nanoseconds(1)),
            REVOCATION_SLA_SECONDS,
        ),
        Err(RevocationTimingError::ExceededSla)
    );
    assert_eq!(
        validate_revocation_observation_timing(
            Some(action_at),
            Some(action_at - Duration::nanoseconds(1)),
            REVOCATION_SLA_SECONDS,
        ),
        Err(RevocationTimingError::NegativeElapsed)
    );
    assert_eq!(
        validate_revocation_observation_timing(Some(action_at), Some(action_at), -1),
        Err(RevocationTimingError::InvalidSla)
    );
    assert_eq!(
        validate_revocation_observation_timing(
            Some(DateTime::<Utc>::MAX_UTC),
            Some(DateTime::<Utc>::MAX_UTC),
            1,
        ),
        Err(RevocationTimingError::TimestampOverflow)
    );
    assert_eq!(
        validate_revocation_observation_timing(None, Some(action_at), REVOCATION_SLA_SECONDS),
        Err(RevocationTimingError::MissingActionEvidence)
    );
    assert_eq!(
        validate_revocation_observation_timing(Some(action_at), None, REVOCATION_SLA_SECONDS),
        Err(RevocationTimingError::MissingObservationEvidence)
    );

    let ConfigError::Missing(missing) =
        parse_config(Vendor::Ad, |_| None).expect_err("missing live provider evidence is queued")
    else {
        panic!("missing live provider configuration must remain a queued blocker");
    };
    assert!(missing.contains(&"ERPX_G14_AD_REVOCATION_ACTION_EVIDENCE_PATH".to_owned()));
    eprintln!(
        "QUEUED BLOCKER (not a pass): AD DS/DirSync provider revocation evidence is unavailable; missing references: {}",
        missing.join(", ")
    );
}

#[test]
fn immutable_link_tampering_fails_closed_during_vendor_revocation() {
    assert!(immutable_link_matches(
        "directory:immutable",
        "immutable-value",
        Some("directory:immutable"),
        Some("immutable-value"),
    ));
    assert!(!immutable_link_matches(
        "directory:immutable",
        "immutable-value",
        Some("directory:immutable"),
        Some("tampered-value"),
    ));
    assert!(!immutable_link_matches(
        "directory:immutable",
        "immutable-value",
        None,
        None,
    ));
}

fn complete_values(vendor: Vendor) -> BTreeMap<&'static str, String> {
    let mut values = BTreeMap::new();
    for suffix in vendor.required_suffixes() {
        let name = match (vendor, *suffix) {
            (Vendor::Ad, "BIND_DN") => "ERPX_G14_AD_BIND_DN",
            (Vendor::Ad, "PASSWORD") => "ERPX_G14_AD_PASSWORD",
            (Vendor::Ucs, "SUBSCRIPTION") => "ERPX_G14_UCS_SUBSCRIPTION",
            (Vendor::Ucs, "SUBSCRIPTION_PASSWORD") => "ERPX_G14_UCS_SUBSCRIPTION_PASSWORD",
            _ => match vendor {
                Vendor::Ad => match *suffix {
                    "TEST_DATABASE_URL" => "ERPX_G14_AD_TEST_DATABASE_URL",
                    "ENDPOINT" => "ERPX_G14_AD_ENDPOINT",
                    "BASE_DN" => "ERPX_G14_AD_BASE_DN",
                    "CA_SHA256" => "ERPX_G14_AD_CA_SHA256",
                    "SERVER_NAME" => "ERPX_G14_AD_SERVER_NAME",
                    "TLS_MODE" => "ERPX_G14_AD_TLS_MODE",
                    "MAX_IMPORT_USERS" => "ERPX_G14_AD_MAX_IMPORT_USERS",
                    "FULL_IMPORT" => "ERPX_G14_AD_FULL_IMPORT",
                    "REVOCATION_MAX_SECONDS" => "ERPX_G14_AD_REVOCATION_MAX_SECONDS",
                    "REVOCATION_OPERATION" => "ERPX_G14_AD_REVOCATION_OPERATION",
                    "REVOCATION_TARGET_EXTERNAL_ID" => "ERPX_G14_AD_REVOCATION_TARGET_EXTERNAL_ID",
                    "REVOCATION_TARGET_IMMUTABLE_AUTHORITY" => {
                        "ERPX_G14_AD_REVOCATION_TARGET_IMMUTABLE_AUTHORITY"
                    }
                    "REVOCATION_TARGET_IMMUTABLE_VALUE" => {
                        "ERPX_G14_AD_REVOCATION_TARGET_IMMUTABLE_VALUE"
                    }
                    "REVOCATION_ACTION_EVIDENCE_PATH" => {
                        "ERPX_G14_AD_REVOCATION_ACTION_EVIDENCE_PATH"
                    }
                    _ => unreachable!(),
                },
                Vendor::Ucs => match *suffix {
                    "TEST_DATABASE_URL" => "ERPX_G14_UCS_TEST_DATABASE_URL",
                    "ENDPOINT" => "ERPX_G14_UCS_ENDPOINT",
                    "BASE_DN" => "ERPX_G14_UCS_BASE_DN",
                    "CA_SHA256" => "ERPX_G14_UCS_CA_SHA256",
                    "SERVER_NAME" => "ERPX_G14_UCS_SERVER_NAME",
                    "TLS_MODE" => "ERPX_G14_UCS_TLS_MODE",
                    "MAX_IMPORT_USERS" => "ERPX_G14_UCS_MAX_IMPORT_USERS",
                    "FULL_IMPORT" => "ERPX_G14_UCS_FULL_IMPORT",
                    "REVOCATION_MAX_SECONDS" => "ERPX_G14_UCS_REVOCATION_MAX_SECONDS",
                    "REVOCATION_OPERATION" => "ERPX_G14_UCS_REVOCATION_OPERATION",
                    "REVOCATION_TARGET_EXTERNAL_ID" => "ERPX_G14_UCS_REVOCATION_TARGET_EXTERNAL_ID",
                    "REVOCATION_TARGET_IMMUTABLE_AUTHORITY" => {
                        "ERPX_G14_UCS_REVOCATION_TARGET_IMMUTABLE_AUTHORITY"
                    }
                    "REVOCATION_TARGET_IMMUTABLE_VALUE" => {
                        "ERPX_G14_UCS_REVOCATION_TARGET_IMMUTABLE_VALUE"
                    }
                    "REVOCATION_ACTION_EVIDENCE_PATH" => {
                        "ERPX_G14_UCS_REVOCATION_ACTION_EVIDENCE_PATH"
                    }
                    _ => unreachable!(),
                },
            },
        };
        let value = match name {
            name if name.ends_with("TEST_DATABASE_URL") => {
                "postgresql://acceptance.invalid/erpx?sslmode=verify-full"
            }
            name if name.ends_with("ENDPOINT") => match vendor {
                Vendor::Ad => "ldaps://ad.example.test",
                Vendor::Ucs => "https://ucs.example.test",
            },
            name if name.ends_with("SERVER_NAME") => match vendor {
                Vendor::Ad => "ad.example.test",
                Vendor::Ucs => "ucs.example.test",
            },
            name if name.ends_with("TLS_MODE") => match vendor {
                Vendor::Ad => "ldaps",
                Vendor::Ucs => "https",
            },
            name if name.ends_with("MAX_IMPORT_USERS") => "1000000",
            name if name.ends_with("FULL_IMPORT") => "true",
            name if name.ends_with("REVOCATION_MAX_SECONDS") => "300",
            name if name.ends_with("REVOCATION_OPERATION") => "disable",
            name if name.ends_with("REVOCATION_TARGET_EXTERNAL_ID") => "target-user",
            name if name.ends_with("REVOCATION_TARGET_IMMUTABLE_AUTHORITY") => {
                "directory:immutable"
            }
            name if name.ends_with("REVOCATION_TARGET_IMMUTABLE_VALUE") => "immutable-value",
            name if name.ends_with("REVOCATION_ACTION_EVIDENCE_PATH") => {
                "/tmp/erpx-g14-revocation-evidence.json"
            }
            name if name.ends_with("CA_SHA256") => {
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            }
            _ => "acceptance-value",
        };
        values.insert(name, value.into());
    }
    values
}
