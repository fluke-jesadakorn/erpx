//! G14 external-directory certification readiness gate.
//!
//! This target is a preflight only.  It never starts a fake directory, makes
//! a network request, stores a secret, or turns configuration presence into a
//! vendor-acceptance claim.  The ignored certification gate is intentionally
//! fail-closed so an environment owner cannot accidentally record a pass with
//! missing references.

use std::{
    collections::BTreeMap,
    env,
    path::{Component, Path},
};

const MAX_IMPORT_USERS: u64 = 1_000_000;
const MAX_PAGE_SIZE: u32 = 5_000;
const REVOCATION_SLA_SECONDS: i64 = 300;
const MAX_CREDENTIAL_BYTES: usize = 16 * 1024;
const MAX_OPTIONAL_CONFIGURATION_BYTES: usize = 2 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Vendor {
    GenericLdap,
    ActiveDirectory,
    Ucs,
}

impl Vendor {
    const ALL: [Self; 3] = [Self::GenericLdap, Self::ActiveDirectory, Self::Ucs];

    const fn label(self) -> &'static str {
        match self {
            Self::GenericLdap => "Generic LDAP",
            Self::ActiveDirectory => "on-premises AD DS/DirSync",
            Self::Ucs => "UCS Provisioning Service",
        }
    }

    const fn prefix(self) -> &'static str {
        match self {
            Self::GenericLdap => "ERPX_G14_GENERIC_LDAP",
            Self::ActiveDirectory => "ERPX_G14_AD",
            Self::Ucs => "ERPX_G14_UCS",
        }
    }

    const fn required(self) -> &'static [&'static str] {
        match self {
            Self::GenericLdap => &[
                "ERPX_G14_GENERIC_LDAP_TEST_DATABASE_URL",
                "ERPX_G14_GENERIC_LDAP_ENDPOINT",
                "ERPX_G14_GENERIC_LDAP_FEED",
                "ERPX_G14_GENERIC_LDAP_BASE_DN",
                "ERPX_G14_GENERIC_LDAP_BIND_DN",
                "ERPX_G14_GENERIC_LDAP_PASSWORD",
                "ERPX_G14_GENERIC_LDAP_CA_SHA256",
                "ERPX_G14_GENERIC_LDAP_SERVER_NAME",
                "ERPX_G14_GENERIC_LDAP_TLS_MODE",
                "ERPX_G14_GENERIC_LDAP_MAX_IMPORT_USERS",
                "ERPX_G14_GENERIC_LDAP_FULL_IMPORT",
                "ERPX_G14_GENERIC_LDAP_REVOCATION_EXTERNAL_ID",
                "ERPX_G14_GENERIC_LDAP_REVOCATION_KIND",
                "ERPX_G14_GENERIC_LDAP_REVOCATION_TRIGGERED_AT",
                "ERPX_G14_GENERIC_LDAP_REVOCATION_EVIDENCE_PATH",
            ],
            Self::ActiveDirectory => &[
                "ERPX_G14_AD_TEST_DATABASE_URL",
                "ERPX_G14_AD_ENDPOINT",
                "ERPX_G14_AD_BASE_DN",
                "ERPX_G14_AD_BIND_DN",
                "ERPX_G14_AD_PASSWORD",
                "ERPX_G14_AD_CA_SHA256",
                "ERPX_G14_AD_SERVER_NAME",
                "ERPX_G14_AD_TLS_MODE",
                "ERPX_G14_AD_MAX_IMPORT_USERS",
                "ERPX_G14_AD_FULL_IMPORT",
                "ERPX_G14_AD_REVOCATION_MAX_SECONDS",
                "ERPX_G14_AD_REVOCATION_OPERATION",
                "ERPX_G14_AD_REVOCATION_TARGET_EXTERNAL_ID",
                "ERPX_G14_AD_REVOCATION_TARGET_IMMUTABLE_AUTHORITY",
                "ERPX_G14_AD_REVOCATION_TARGET_IMMUTABLE_VALUE",
                "ERPX_G14_AD_REVOCATION_ACTION_EVIDENCE_PATH",
            ],
            Self::Ucs => &[
                "ERPX_G14_UCS_TEST_DATABASE_URL",
                "ERPX_G14_UCS_ENDPOINT",
                "ERPX_G14_UCS_BASE_DN",
                "ERPX_G14_UCS_SUBSCRIPTION",
                "ERPX_G14_UCS_SUBSCRIPTION_PASSWORD",
                "ERPX_G14_UCS_CA_SHA256",
                "ERPX_G14_UCS_SERVER_NAME",
                "ERPX_G14_UCS_TLS_MODE",
                "ERPX_G14_UCS_MAX_IMPORT_USERS",
                "ERPX_G14_UCS_FULL_IMPORT",
                "ERPX_G14_UCS_REVOCATION_MAX_SECONDS",
                "ERPX_G14_UCS_REVOCATION_OPERATION",
                "ERPX_G14_UCS_REVOCATION_TARGET_EXTERNAL_ID",
                "ERPX_G14_UCS_REVOCATION_TARGET_IMMUTABLE_AUTHORITY",
                "ERPX_G14_UCS_REVOCATION_TARGET_IMMUTABLE_VALUE",
                "ERPX_G14_UCS_REVOCATION_ACTION_EVIDENCE_PATH",
            ],
        }
    }

    const fn optional(self) -> &'static [&'static str] {
        match self {
            Self::GenericLdap => &[
                "ERPX_G14_GENERIC_LDAP_PAGE_SIZE",
                "ERPX_G14_GENERIC_LDAP_FILTER",
                "ERPX_G14_GENERIC_LDAP_IMMUTABLE_ATTRIBUTE",
                "ERPX_G14_GENERIC_LDAP_LOGIN_ATTRIBUTE",
                "ERPX_G14_GENERIC_LDAP_EMAIL_ATTRIBUTE",
                "ERPX_G14_GENERIC_LDAP_DISPLAY_NAME_ATTRIBUTE",
                "ERPX_G14_GENERIC_LDAP_GROUP_ATTRIBUTE",
                "ERPX_G14_GENERIC_LDAP_DISABLED_ATTRIBUTE",
                "ERPX_G14_GENERIC_LDAP_DELETED_ATTRIBUTE",
            ],
            Self::ActiveDirectory => &["ERPX_G14_AD_PAGE_SIZE", "ERPX_G14_AD_FILTER"],
            Self::Ucs => &["ERPX_G14_UCS_PAGE_SIZE", "ERPX_G14_UCS_FILTER"],
        }
    }

    const fn required_secret(self) -> &'static str {
        match self {
            Self::GenericLdap => "ERPX_G14_GENERIC_LDAP_PASSWORD",
            Self::ActiveDirectory => "ERPX_G14_AD_PASSWORD",
            Self::Ucs => "ERPX_G14_UCS_SUBSCRIPTION_PASSWORD",
        }
    }

    const fn endpoint(self) -> &'static str {
        match self {
            Self::GenericLdap => "ENDPOINT",
            Self::ActiveDirectory => "ENDPOINT",
            Self::Ucs => "ENDPOINT",
        }
    }

    const fn tls_mode(self) -> &'static str {
        match self {
            Self::GenericLdap => "TLS_MODE",
            Self::ActiveDirectory => "TLS_MODE",
            Self::Ucs => "TLS_MODE",
        }
    }

    const fn feed(self) -> &'static str {
        match self {
            Self::GenericLdap | Self::ActiveDirectory | Self::Ucs => "FEED",
        }
    }

    const fn expected_feed(self) -> &'static [&'static str] {
        match self {
            // The live Generic LDAP harness requires this native feed before
            // it will attempt certification; polling is not certification
            // evidence for the requested RFC 4533 path.
            Self::GenericLdap => &["rfc4533"],
            Self::ActiveDirectory => &["ad_dirsync"],
            Self::Ucs => &["ucs_provisioning_service"],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Blocker {
    Missing {
        vendor: Vendor,
        names: Vec<&'static str>,
    },
    Invalid {
        vendor: Vendor,
        fields: Vec<String>,
    },
}

impl Blocker {
    fn describe_without_values(&self) -> String {
        match self {
            Self::Missing { vendor, names } => {
                format!(
                    "{} missing references: {}",
                    vendor.label(),
                    names.join(", ")
                )
            }
            Self::Invalid { vendor, fields } => {
                format!(
                    "{} invalid configuration fields: {}",
                    vendor.label(),
                    fields.join(", ")
                )
            }
        }
    }
}

fn configured<F>(vendor: Vendor, mut get: F) -> Result<(), Blocker>
where
    F: FnMut(&str) -> Option<String>,
{
    let missing = vendor
        .required()
        .iter()
        .copied()
        .filter(|name| {
            get(name)
                .is_none_or(|value| value.trim().is_empty() || value.chars().any(char::is_control))
        })
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(Blocker::Missing {
            vendor,
            names: missing,
        });
    }

    let mut invalid = Vec::new();
    let secret_name = vendor.required_secret();
    // Keep readiness aligned with the native adapter's bounded secret input.
    // Reject oversized material before any external certification attempt,
    // without copying the secret into the blocker report.
    if get(secret_name).is_some_and(|value| value.len() > MAX_CREDENTIAL_BYTES) {
        invalid.push(secret_name.to_owned());
    }
    let value = |name: String, get: &mut F| get(&name).unwrap_or_default();

    for name in vendor.optional() {
        if let Some(raw) = get(name) {
            let valid = !raw.trim().is_empty()
                && raw.len() <= MAX_OPTIONAL_CONFIGURATION_BYTES
                && !raw.chars().any(char::is_control)
                && if name.ends_with("_FILTER") {
                    valid_ldap_filter(&raw)
                } else if name.ends_with("_ATTRIBUTE") {
                    valid_attribute_name(&raw)
                } else {
                    true
                };
            if !valid {
                invalid.push((*name).to_owned());
            }
        }
    }

    let endpoint_name = format!("{}_{}", vendor.prefix(), vendor.endpoint());
    let tls_name = format!("{}_{}", vendor.prefix(), vendor.tls_mode());
    let endpoint = value(endpoint_name.clone(), &mut get);
    let tls_mode = value(tls_name.clone(), &mut get);

    let endpoint_is_tls = match vendor {
        Vendor::GenericLdap | Vendor::ActiveDirectory => {
            (tls_mode == "ldaps" && endpoint.starts_with("ldaps://"))
                || (tls_mode == "start_tls" && endpoint.starts_with("ldap://"))
        }
        Vendor::Ucs => tls_mode == "https" && endpoint.starts_with("https://"),
    } && endpoint_has_authority(&endpoint);
    if !endpoint_is_tls {
        invalid.push(format!("{endpoint_name}/{tls_name}"));
    }

    let server_name_name = format!("{}_SERVER_NAME", vendor.prefix());
    let server_name = value(server_name_name.clone(), &mut get);
    if server_name != server_name.trim()
        || endpoint_host(&endpoint).is_none_or(|host| !host.eq_ignore_ascii_case(&server_name))
    {
        // The live transport revalidates this exact binding immediately
        // before opening the connection. Readiness must not say that a
        // connector is eligible when the worker will reject its persisted
        // endpoint/server-name pair.
        invalid.push(server_name_name);
    }

    // AD DS/DirSync and UCS Provisioning Service are fixed by the actual
    // live harness; requiring unused *_FEED environment variables would
    // create false readiness blockers. Generic LDAP exposes a feed selector,
    // and certification requires its native RFC 4533 feed.
    if matches!(vendor, Vendor::GenericLdap) {
        let feed_name = format!("{}_{}", vendor.prefix(), vendor.feed());
        let feed = value(feed_name.clone(), &mut get);
        if !vendor.expected_feed().contains(&feed.as_str()) {
            invalid.push(feed_name);
        }
    }

    let database_name = format!("{}_TEST_DATABASE_URL", vendor.prefix());
    let database_url = value(database_name.clone(), &mut get);
    if !secure_postgres_database_url(&database_url) {
        invalid.push(database_name);
    }

    let ca_name = format!("{}_CA_SHA256", vendor.prefix());
    let ca = value(ca_name.clone(), &mut get);
    if ca.len() != 64 || !ca.chars().all(|character| character.is_ascii_hexdigit()) {
        invalid.push(ca_name);
    }

    let max_import_name = format!("{}_MAX_IMPORT_USERS", vendor.prefix());
    let max_import = value(max_import_name.clone(), &mut get);
    match max_import.parse::<u64>() {
        Ok(value) if value == MAX_IMPORT_USERS => {}
        _ => invalid.push(max_import_name),
    }

    let full_import_name = format!("{}_FULL_IMPORT", vendor.prefix());
    let full_import = value(full_import_name.clone(), &mut get);
    if full_import != "true" {
        invalid.push(full_import_name);
    }

    let page_size_name = format!("{}_PAGE_SIZE", vendor.prefix());
    if let Some(raw) = get(&page_size_name) {
        match raw.parse::<u32>() {
            Ok(value) if (1..=MAX_PAGE_SIZE).contains(&value) => {}
            _ => invalid.push(page_size_name),
        }
    }

    match vendor {
        Vendor::GenericLdap => {
            let kind_name = format!("{}_REVOCATION_KIND", vendor.prefix());
            let kind = value(kind_name.clone(), &mut get);
            if !matches!(kind.as_str(), "disable" | "delete") {
                invalid.push(kind_name);
            }

            let triggered_at_name = format!("{}_REVOCATION_TRIGGERED_AT", vendor.prefix());
            let triggered_at = value(triggered_at_name.clone(), &mut get);
            if !valid_rfc3339_timestamp(&triggered_at) {
                invalid.push(triggered_at_name);
            }

            let evidence_path_name = format!("{}_REVOCATION_EVIDENCE_PATH", vendor.prefix());
            let evidence_path = value(evidence_path_name.clone(), &mut get);
            if !is_absolute_path(&evidence_path) {
                invalid.push(evidence_path_name);
            }
        }
        Vendor::ActiveDirectory | Vendor::Ucs => {
            let max_seconds_name = format!("{}_REVOCATION_MAX_SECONDS", vendor.prefix());
            let max_seconds = value(max_seconds_name.clone(), &mut get);
            if max_seconds.parse::<i64>() != Ok(REVOCATION_SLA_SECONDS) {
                invalid.push(max_seconds_name);
            }

            let operation_name = format!("{}_REVOCATION_OPERATION", vendor.prefix());
            let operation = value(operation_name.clone(), &mut get);
            if !matches!(operation.as_str(), "disable" | "delete") {
                invalid.push(operation_name);
            }

            let evidence_path_name = format!("{}_REVOCATION_ACTION_EVIDENCE_PATH", vendor.prefix());
            let evidence_path = value(evidence_path_name.clone(), &mut get);
            if !is_absolute_path(&evidence_path) {
                invalid.push(evidence_path_name);
            }
        }
    }

    if invalid.is_empty() {
        Ok(())
    } else {
        Err(Blocker::Invalid {
            vendor,
            fields: invalid,
        })
    }
}

fn endpoint_has_authority(endpoint: &str) -> bool {
    let Some((_, authority_and_path)) = endpoint.split_once("://") else {
        return false;
    };
    let authority_end = authority_and_path
        .find(['/', '?', '#'])
        .unwrap_or(authority_and_path.len());
    let authority = &authority_and_path[..authority_end];
    if authority.is_empty()
        || authority.contains('@')
        || authority.contains('%')
        || authority
            .chars()
            .any(|character| character.is_ascii_control() || character.is_whitespace())
        || endpoint_host(endpoint).is_none()
    {
        return false;
    }

    let suffix = &authority_and_path[authority_end..];
    if suffix.contains('?') || suffix.contains('#') {
        return false;
    }

    // LDAP/StartTLS endpoints are passed directly to ldap3 and the live
    // transport rejects a path other than the empty path or "/". UCS uses an
    // HTTPS base path for the Provisioning Service, so its path remains valid.
    endpoint
        .split_once("://")
        .is_some_and(|(scheme, _)| scheme == "https" || suffix.is_empty() || suffix == "/")
}

fn endpoint_host(endpoint: &str) -> Option<&str> {
    let (_, authority_and_path) = endpoint.split_once("://")?;
    let authority_end = authority_and_path
        .find(['/', '?', '#'])
        .unwrap_or(authority_and_path.len());
    let authority = &authority_and_path[..authority_end];
    if authority.is_empty() || authority.contains('@') || authority.contains('%') {
        return None;
    }

    let host = if let Some(rest) = authority.strip_prefix('[') {
        let closing = rest.find(']')?;
        let host = &rest[..closing];
        let port = &rest[closing + 1..];
        if !port.is_empty() {
            let port = port.strip_prefix(':')?;
            if !valid_port(port) {
                return None;
            }
        }
        host
    } else if authority.matches(':').count() == 1 {
        let (host, port) = authority.split_once(':')?;
        if host.is_empty() || !valid_port(port) {
            return None;
        }
        host
    } else if authority.contains(':') {
        // Unbracketed IPv6 authorities are not valid URL authorities.
        return None;
    } else {
        authority
    };

    if host.is_empty()
        || host
            .chars()
            .any(|character| character.is_ascii_control() || character.is_whitespace())
        || host.contains(['/', '?', '#', '@', '%'])
    {
        None
    } else {
        Some(host)
    }
}

fn valid_port(port: &str) -> bool {
    port.parse::<u32>()
        .is_ok_and(|value| (1..=u32::from(u16::MAX)).contains(&value))
}

fn is_absolute_path(path: &str) -> bool {
    path.starts_with('/')
        && path != "/"
        && path.len() <= 4 * 1024
        && !path
            .chars()
            .any(|character| character.is_ascii_control() || character.is_ascii_whitespace())
        && !Path::new(path)
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
}

fn valid_attribute_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b';'))
}

fn valid_ldap_filter(value: &str) -> bool {
    if value.len() > MAX_OPTIONAL_CONFIGURATION_BYTES
        || !value.starts_with('(')
        || !value.ends_with(')')
    {
        return false;
    }

    let mut depth = 0_u32;
    for character in value.chars() {
        match character {
            '(' => depth = depth.saturating_add(1),
            ')' => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            }
            character if character.is_control() => return false,
            _ => {}
        }
    }
    depth == 0
}

fn valid_rfc3339_timestamp(value: &str) -> bool {
    let bytes = value.as_bytes();
    if value != value.trim() || bytes.iter().any(|byte| !byte.is_ascii()) || bytes.len() < 20 {
        return false;
    }

    let digits = |range: std::ops::Range<usize>| {
        range.end <= bytes.len() && bytes[range].iter().all(u8::is_ascii_digit)
    };
    if !digits(0..4)
        || bytes.get(4) != Some(&b'-')
        || !digits(5..7)
        || bytes.get(7) != Some(&b'-')
        || !digits(8..10)
        || bytes.get(10) != Some(&b'T')
        || !digits(11..13)
        || bytes.get(13) != Some(&b':')
        || !digits(14..16)
        || bytes.get(16) != Some(&b':')
        || !digits(17..19)
    {
        return false;
    }

    let number = |range: std::ops::Range<usize>| {
        value[range]
            .parse::<u32>()
            .expect("timestamp digit ranges are ASCII decimal")
    };
    let year = number(0..4);
    let month = number(5..7);
    let day = number(8..10);
    let hour = number(11..13);
    let minute = number(14..16);
    let second = number(17..19);
    if !(1..=12).contains(&month)
        || day == 0
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return false;
    }

    let mut index = 19;
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        let fraction_start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if index == fraction_start {
            return false;
        }
    }

    match bytes.get(index) {
        Some(b'Z') => index += 1,
        Some(b'+') | Some(b'-') => {
            if index + 6 != bytes.len()
                || !bytes[index + 1..index + 3].iter().all(u8::is_ascii_digit)
                || bytes.get(index + 3) != Some(&b':')
                || !bytes[index + 4..index + 6].iter().all(u8::is_ascii_digit)
            {
                return false;
            }
            let offset_hour =
                u32::from(bytes[index + 1] - b'0') * 10 + u32::from(bytes[index + 2] - b'0');
            let offset_minute =
                u32::from(bytes[index + 4] - b'0') * 10 + u32::from(bytes[index + 5] - b'0');
            if offset_hour > 23 || offset_minute > 59 {
                return false;
            }
            index += 6;
        }
        _ => return false,
    }
    index == bytes.len()
}

const fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        2 if year.is_multiple_of(400) || (year.is_multiple_of(4) && !year.is_multiple_of(100)) => {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn secure_postgres_database_url(value: &str) -> bool {
    if value.len() > 2048
        || value.chars().any(char::is_control)
        || value.chars().any(char::is_whitespace)
        || value.contains('#')
    {
        return false;
    }
    let Some((scheme, authority_and_query)) = value.split_once("://") else {
        return false;
    };
    if !matches!(
        scheme.to_ascii_lowercase().as_str(),
        "postgres" | "postgresql"
    ) {
        return false;
    }
    let Some((authority_and_database, query)) = authority_and_query.split_once('?') else {
        return false;
    };
    let Some((authority, database)) = authority_and_database.split_once('/') else {
        return false;
    };
    if authority.is_empty() || database.is_empty() || database.contains('/') {
        return false;
    }
    // A secure value must be the sole sslmode declaration. Accepting any
    // secure occurrence would make a URL such as
    // `sslmode=disable&sslmode=require` appear ready even though clients may
    // honor the insecure declaration. A fragment is not sent to PostgreSQL,
    // so it must not be allowed to alter the value certified here.
    let mut sslmodes = Vec::new();
    for parameter in query.split('&') {
        let Some((key, value)) = parameter.split_once('=') else {
            return false;
        };
        if key.eq_ignore_ascii_case("ssl-mode") {
            return false;
        }
        if key.eq_ignore_ascii_case("sslmode") {
            sslmodes.push(value);
        }
    }
    // The Legacy bridge and every credentialed G14 harness require hostname
    // verification. `require` and `verify-ca` encrypt the channel but do not
    // prove that the configured database is the intended endpoint.
    sslmodes.len() == 1 && sslmodes[0].eq_ignore_ascii_case("verify-full")
}

fn process_environment() -> Vec<Blocker> {
    Vendor::ALL
        .into_iter()
        .filter_map(|vendor| configured(vendor, |name| env::var(name).ok()).err())
        .collect()
}

fn certification_queue_report<F>(mut get: F) -> String
where
    F: FnMut(&str) -> Option<String>,
{
    let mut entries = Vec::with_capacity(Vendor::ALL.len());
    for vendor in Vendor::ALL {
        let entry = match configured(vendor, |name| get(name)) {
            Ok(()) => format!(
                "{}: READY FOR EXPLICIT LIVE ATTEMPT (certification unclaimed)",
                vendor.label()
            ),
            Err(blocker) => format!(
                "{}: QUEUED ({})",
                vendor.label(),
                blocker.describe_without_values()
            ),
        };
        entries.push(entry);
    }
    entries.join("; ")
}

fn queued_blocker_report(blockers: &[Blocker]) -> String {
    if blockers.is_empty() {
        return "no external directory blocker was recorded; certification is still unclaimed"
            .to_owned();
    }
    blockers
        .iter()
        .map(Blocker::describe_without_values)
        .collect::<Vec<_>>()
        .join("; ")
}

fn valid_contract_configuration(vendor: Vendor) -> BTreeMap<String, String> {
    let (endpoint, tls_mode) = match vendor {
        Vendor::GenericLdap => ("ldaps://ldap.example.test:636", "ldaps"),
        Vendor::ActiveDirectory => ("ldap://ad.example.test:389", "start_tls"),
        Vendor::Ucs => ("https://ucs.example.test/provision", "https"),
    };
    let mut values = BTreeMap::new();
    for name in vendor.required() {
        let value = if name.ends_with("_TEST_DATABASE_URL") {
            "postgres://localhost/erpx_g14_candidate?sslmode=verify-full"
        } else if name.ends_with("_ENDPOINT") {
            endpoint
        } else if name.ends_with("_FEED") {
            vendor.expected_feed()[0]
        } else if name.ends_with("_BASE_DN") {
            "dc=example,dc=test"
        } else if name.ends_with("_BIND_DN") {
            "cn=erpx-g14-bind,dc=example,dc=test"
        } else if name.ends_with("_PASSWORD") {
            "opaque-test-only-secret-reference"
        } else if name.ends_with("_SUBSCRIPTION") {
            "subscription-reference"
        } else if name.ends_with("_CA_SHA256") {
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        } else if name.ends_with("_SERVER_NAME") {
            match vendor {
                Vendor::GenericLdap => "ldap.example.test",
                Vendor::ActiveDirectory => "ad.example.test",
                Vendor::Ucs => "ucs.example.test",
            }
        } else if name.ends_with("_TLS_MODE") {
            tls_mode
        } else if name.ends_with("_REVOCATION_EXTERNAL_ID") {
            "target-user"
        } else if name.ends_with("_REVOCATION_KIND") {
            "disable"
        } else if name.ends_with("_REVOCATION_TRIGGERED_AT") {
            "2026-07-27T00:00:00Z"
        } else if name.ends_with("_REVOCATION_EVIDENCE_PATH") {
            "/tmp/erpx-g14-revocation-evidence.json"
        } else if name.ends_with("_REVOCATION_MAX_SECONDS") {
            "300"
        } else if name.ends_with("_REVOCATION_OPERATION") {
            "disable"
        } else if name.ends_with("_REVOCATION_TARGET_EXTERNAL_ID") {
            "target-user"
        } else if name.ends_with("_REVOCATION_TARGET_IMMUTABLE_AUTHORITY") {
            "directory:immutable"
        } else if name.ends_with("_REVOCATION_TARGET_IMMUTABLE_VALUE") {
            "immutable-value"
        } else if name.ends_with("_REVOCATION_ACTION_EVIDENCE_PATH") {
            "/tmp/erpx-g14-revocation-evidence.json"
        } else {
            "configured-reference"
        };
        values.insert((*name).to_owned(), value.to_owned());
    }
    values.insert(
        format!("{}_MAX_IMPORT_USERS", vendor.prefix()),
        MAX_IMPORT_USERS.to_string(),
    );
    values.insert(
        format!("{}_PAGE_SIZE", vendor.prefix()),
        MAX_PAGE_SIZE.to_string(),
    );
    values.insert(
        format!("{}_FULL_IMPORT", vendor.prefix()),
        "true".to_owned(),
    );
    values
}

#[test]
fn g14_external_readiness_has_exact_non_secret_reference_contract() {
    for vendor in Vendor::ALL {
        let mut names = vendor.required().to_vec();
        names.extend_from_slice(vendor.optional());
        names.push(vendor.required_secret());
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names.len(),
            vendor.required().len() + vendor.optional().len()
        );
        assert!(
            vendor
                .required()
                .iter()
                .all(|name| name.starts_with("ERPX_G14_"))
        );
    }
}

#[test]
fn g14_external_readiness_is_fail_closed_without_external_references() {
    let blockers = Vendor::ALL
        .into_iter()
        .map(|vendor| {
            configured(vendor, |_| None).expect_err("missing external configuration must block")
        })
        .collect::<Vec<_>>();

    assert_eq!(blockers.len(), Vendor::ALL.len());
    let report = queued_blocker_report(&blockers);
    assert!(report.contains("Generic LDAP missing references"));
    assert!(report.contains("on-premises AD DS/DirSync missing references"));
    assert!(report.contains("UCS Provisioning Service missing references"));
    assert!(report.contains("ERPX_G14_GENERIC_LDAP_PASSWORD"));
    assert!(report.contains("ERPX_G14_AD_PASSWORD"));
    assert!(report.contains("ERPX_G14_UCS_SUBSCRIPTION_PASSWORD"));
    assert!(report.contains("ERPX_G14_GENERIC_LDAP_REVOCATION_EXTERNAL_ID"));
    assert!(report.contains("ERPX_G14_GENERIC_LDAP_REVOCATION_EVIDENCE_PATH"));
    assert!(report.contains("ERPX_G14_AD_REVOCATION_MAX_SECONDS"));
    assert!(report.contains("ERPX_G14_AD_REVOCATION_ACTION_EVIDENCE_PATH"));
    assert!(report.contains("ERPX_G14_UCS_REVOCATION_MAX_SECONDS"));
    assert!(report.contains("ERPX_G14_UCS_REVOCATION_ACTION_EVIDENCE_PATH"));
    for vendor in Vendor::ALL {
        for suffix in [
            "_TEST_DATABASE_URL",
            "_CA_SHA256",
            "_TLS_MODE",
            "_MAX_IMPORT_USERS",
            "_FULL_IMPORT",
        ] {
            assert!(
                report.contains(&format!("{}{}", vendor.prefix(), suffix)),
                "queued {} blocker must retain the {} reference",
                vendor.label(),
                suffix
            );
        }
        for name in vendor
            .required()
            .iter()
            .filter(|name| name.contains("REVOCATION"))
        {
            assert!(
                report.contains(name),
                "queued {} blocker must retain the {} reference",
                vendor.label(),
                name
            );
        }
    }
    assert!(!report.contains("opaque-test-only-secret-reference"));
    assert!(!report.contains("certified"));
    eprintln!("QUEUED G14 CERTIFICATION BLOCKER (not a pass): {report}");
}

#[test]
fn g14_external_readiness_emits_one_explicit_queue_entry_per_vendor() {
    let queued = certification_queue_report(|_| None);
    assert_eq!(queued.matches("QUEUED (").count(), Vendor::ALL.len());
    for vendor in Vendor::ALL {
        assert!(queued.contains(&format!("{}: QUEUED", vendor.label())));
    }
    assert!(!queued.contains("opaque-test-only-secret-reference"));
    assert!(!queued.contains("certified"));

    let mut complete = BTreeMap::new();
    for vendor in Vendor::ALL {
        complete.extend(valid_contract_configuration(vendor));
    }
    let ready = certification_queue_report(|name| complete.get(name).cloned());
    assert_eq!(
        ready
            .matches("READY FOR EXPLICIT LIVE ATTEMPT (certification unclaimed)")
            .count(),
        Vendor::ALL.len()
    );
    assert!(!ready.contains("QUEUED ("));
    assert!(!ready.contains("certified"));
}

#[test]
fn g14_external_readiness_validates_the_closed_vendor_feed_tls_capacity_and_revocation_contract() {
    for vendor in Vendor::ALL {
        let values = valid_contract_configuration(vendor);
        assert!(configured(vendor, |name| values.get(name).cloned()).is_ok());
    }

    let mut over_capacity = valid_contract_configuration(Vendor::GenericLdap);
    over_capacity.insert(
        "ERPX_G14_GENERIC_LDAP_MAX_IMPORT_USERS".to_owned(),
        (MAX_IMPORT_USERS + 1).to_string(),
    );
    assert!(matches!(
        configured(Vendor::GenericLdap, |name| over_capacity.get(name).cloned()),
        Err(Blocker::Invalid { fields, .. }) if fields.iter().any(|field| field == "ERPX_G14_GENERIC_LDAP_MAX_IMPORT_USERS")
    ));

    let mut wrong_feed = valid_contract_configuration(Vendor::GenericLdap);
    wrong_feed.insert(
        "ERPX_G14_GENERIC_LDAP_FEED".to_owned(),
        "ldap_polling".to_owned(),
    );
    assert!(matches!(
        configured(Vendor::GenericLdap, |name| wrong_feed.get(name).cloned()),
        Err(Blocker::Invalid { fields, .. }) if fields.iter().any(|field| field == "ERPX_G14_GENERIC_LDAP_FEED")
    ));

    let mut plaintext_endpoint = valid_contract_configuration(Vendor::Ucs);
    plaintext_endpoint.insert(
        "ERPX_G14_UCS_ENDPOINT".to_owned(),
        "http://ucs.example.test/provision".to_owned(),
    );
    assert!(matches!(
        configured(Vendor::Ucs, |name| plaintext_endpoint.get(name).cloned()),
        Err(Blocker::Invalid { fields, .. }) if fields.iter().any(|field| field == "ERPX_G14_UCS_ENDPOINT/ERPX_G14_UCS_TLS_MODE")
    ));

    for vendor in [Vendor::GenericLdap, Vendor::ActiveDirectory] {
        let endpoint_name = format!("{}_ENDPOINT", vendor.prefix());
        let mut endpoint_with_path = valid_contract_configuration(vendor);
        endpoint_with_path.insert(
            endpoint_name,
            match vendor {
                Vendor::GenericLdap => "ldaps://ldap.example.test:636/directory".to_owned(),
                Vendor::ActiveDirectory => "ldap://ad.example.test:389/directory".to_owned(),
                Vendor::Ucs => unreachable!("UCS is not a LDAP endpoint"),
            },
        );
        let endpoint_tls_field =
            format!("{}_ENDPOINT/{}_TLS_MODE", vendor.prefix(), vendor.prefix());
        assert!(matches!(
            configured(vendor, |name| endpoint_with_path.get(name).cloned()),
            Err(Blocker::Invalid { fields, .. })
                if fields.iter().any(|field| field == &endpoint_tls_field)
        ));
    }

    for (vendor, endpoint) in [
        (Vendor::GenericLdap, "ldaps://ldap.example.test:0"),
        (Vendor::GenericLdap, "ldaps://ldap.example.test:65536"),
        (Vendor::ActiveDirectory, "ldap://ad.example.test:99999"),
    ] {
        let endpoint_name = format!("{}_ENDPOINT", vendor.prefix());
        let tls_name = format!("{}_TLS_MODE", vendor.prefix());
        let mut invalid_port = valid_contract_configuration(vendor);
        invalid_port.insert(endpoint_name.clone(), endpoint.to_owned());
        invalid_port.insert(
            format!("{}_SERVER_NAME", vendor.prefix()),
            endpoint
                .split_once("://")
                .and_then(|(_, rest)| rest.split(':').next())
                .unwrap_or_default()
                .to_owned(),
        );
        assert!(matches!(
            configured(vendor, |name| invalid_port.get(name).cloned()),
            Err(Blocker::Invalid { fields, .. })
                if fields.iter().any(|field| field == &format!("{endpoint_name}/{tls_name}"))
        ));
    }

    for vendor in Vendor::ALL {
        let server_name = format!("{}_SERVER_NAME", vendor.prefix());
        let mut wrong_server_name = valid_contract_configuration(vendor);
        wrong_server_name.insert(server_name.clone(), "other.example.test".to_owned());
        assert!(matches!(
            configured(vendor, |name| wrong_server_name.get(name).cloned()),
            Err(Blocker::Invalid { fields, .. }) if fields.iter().any(|field| field == &server_name)
        ));

        let mut whitespace_server_name = valid_contract_configuration(vendor);
        let configured_server_name = whitespace_server_name
            .get(&server_name)
            .expect("valid fixture has a server name")
            .clone();
        whitespace_server_name.insert(server_name.clone(), format!("{configured_server_name} "));
        assert!(matches!(
            configured(vendor, |name| whitespace_server_name.get(name).cloned()),
            Err(Blocker::Invalid { fields, .. }) if fields.iter().any(|field| field == &server_name)
        ));
    }

    for vendor in [Vendor::ActiveDirectory, Vendor::Ucs] {
        let evidence_name = format!("{}_REVOCATION_ACTION_EVIDENCE_PATH", vendor.prefix());
        let mut relative_evidence_path = valid_contract_configuration(vendor);
        relative_evidence_path.insert(evidence_name.clone(), "evidence.json".to_owned());
        assert!(matches!(
            configured(vendor, |name| relative_evidence_path.get(name).cloned()),
            Err(Blocker::Invalid { fields, .. }) if fields.iter().any(|field| field == &evidence_name)
        ));
    }

    let mut insecure_database = valid_contract_configuration(Vendor::GenericLdap);
    insecure_database.insert(
        "ERPX_G14_GENERIC_LDAP_TEST_DATABASE_URL".to_owned(),
        "postgres://localhost/erpx_g14_candidate?sslmode=disable".to_owned(),
    );
    assert!(matches!(
        configured(Vendor::GenericLdap, |name| insecure_database.get(name).cloned()),
        Err(Blocker::Invalid { fields, .. }) if fields.iter().any(|field| field == "ERPX_G14_GENERIC_LDAP_TEST_DATABASE_URL")
    ));

    for insecure_database_url in [
        "postgres://localhost/erpx_g14_candidate?sslmode=disable&sslmode=require",
        "postgres://localhost/erpx_g14_candidate?sslmode=require&sslmode=disable",
        "postgres://localhost/erpx_g14_candidate?sslmode=verify-full&sslmode=verify-full",
        "postgres://localhost/erpx_g14_candidate?sslmode=require",
        "postgres://localhost/erpx_g14_candidate?sslmode=verify-ca",
        "postgres://localhost/erpx_g14_candidate?sslmode=require#insecure-fragment",
        "postgres://localhost/erpx_g14_candidate?sslmode=verify-full&connect_timeout=5#fragment",
        "postgres://localhost/erpx_g14_candidate#fragment?sslmode=verify-full",
        "postgres://localhost/erpx_g14_candidate?ssl-mode=verify-full",
        "postgres://localhost?sslmode=verify-full",
    ] {
        let mut mixed_tls_database = valid_contract_configuration(Vendor::GenericLdap);
        mixed_tls_database.insert(
            "ERPX_G14_GENERIC_LDAP_TEST_DATABASE_URL".to_owned(),
            insecure_database_url.to_owned(),
        );
        assert!(
            matches!(
                configured(Vendor::GenericLdap, |name| mixed_tls_database
                    .get(name)
                    .cloned()),
                Err(Blocker::Invalid { fields, .. })
                    if fields.iter().any(|field| field == "ERPX_G14_GENERIC_LDAP_TEST_DATABASE_URL")
            ),
            "database URL must reject ambiguous TLS declarations: {insecure_database_url}"
        );
    }

    for (name, value) in [
        ("ERPX_G14_GENERIC_LDAP_FILTER", "objectClass=person"),
        ("ERPX_G14_GENERIC_LDAP_FILTER", "(objectClass=person"),
        ("ERPX_G14_GENERIC_LDAP_IMMUTABLE_ATTRIBUTE", "uid\nmail"),
        (
            "ERPX_G14_GENERIC_LDAP_LOGIN_ATTRIBUTE",
            "access_token=secret",
        ),
    ] {
        let mut invalid_optional = valid_contract_configuration(Vendor::GenericLdap);
        invalid_optional.insert(name.to_owned(), value.to_owned());
        assert!(matches!(
            configured(Vendor::GenericLdap, |candidate| invalid_optional
                .get(candidate)
                .cloned()),
            Err(Blocker::Invalid { fields, .. }) if fields.iter().any(|field| field == name)
        ));
    }
}

#[test]
fn g14_external_readiness_requires_and_validates_five_minute_revocation_inputs() {
    for vendor in Vendor::ALL {
        let values = valid_contract_configuration(vendor);
        let revocation_names = vendor
            .required()
            .iter()
            .copied()
            .filter(|name| name.contains("REVOCATION"))
            .collect::<Vec<_>>();
        assert!(
            !revocation_names.is_empty(),
            "{} has no revocation contract",
            vendor.label()
        );

        for name in revocation_names {
            let mut missing = values.clone();
            missing.remove(name);
            match configured(vendor, |candidate| missing.get(candidate).cloned()) {
                Err(Blocker::Missing { names, .. }) => assert!(names.contains(&name)),
                other => panic!("missing {name} must block, got {other:?}"),
            }
        }
    }

    let mut generic_wrong_kind = valid_contract_configuration(Vendor::GenericLdap);
    generic_wrong_kind.insert(
        "ERPX_G14_GENERIC_LDAP_REVOCATION_KIND".to_owned(),
        "suspend".to_owned(),
    );
    assert!(matches!(
        configured(Vendor::GenericLdap, |name| generic_wrong_kind.get(name).cloned()),
        Err(Blocker::Invalid { fields, .. })
            if fields.iter().any(|field| field == "ERPX_G14_GENERIC_LDAP_REVOCATION_KIND")
    ));

    let mut generic_wrong_timestamp = valid_contract_configuration(Vendor::GenericLdap);
    generic_wrong_timestamp.insert(
        "ERPX_G14_GENERIC_LDAP_REVOCATION_TRIGGERED_AT".to_owned(),
        "not-a-timestamp".to_owned(),
    );
    assert!(matches!(
        configured(Vendor::GenericLdap, |name| generic_wrong_timestamp
            .get(name)
            .cloned()),
        Err(Blocker::Invalid { fields, .. })
            if fields.iter().any(|field| field == "ERPX_G14_GENERIC_LDAP_REVOCATION_TRIGGERED_AT")
    ));

    let mut generic_relative_path = valid_contract_configuration(Vendor::GenericLdap);
    generic_relative_path.insert(
        "ERPX_G14_GENERIC_LDAP_REVOCATION_EVIDENCE_PATH".to_owned(),
        "evidence.json".to_owned(),
    );
    assert!(matches!(
        configured(Vendor::GenericLdap, |name| generic_relative_path
            .get(name)
            .cloned()),
        Err(Blocker::Invalid { fields, .. })
            if fields.iter().any(|field| field == "ERPX_G14_GENERIC_LDAP_REVOCATION_EVIDENCE_PATH")
    ));

    for path in ["/", "/tmp/../evidence.json", "relative/evidence.json"] {
        let mut unsafe_path = valid_contract_configuration(Vendor::GenericLdap);
        unsafe_path.insert(
            "ERPX_G14_GENERIC_LDAP_REVOCATION_EVIDENCE_PATH".to_owned(),
            path.to_owned(),
        );
        assert!(matches!(
            configured(Vendor::GenericLdap, |name| unsafe_path.get(name).cloned()),
            Err(Blocker::Invalid { fields, .. })
                if fields.iter().any(|field| field == "ERPX_G14_GENERIC_LDAP_REVOCATION_EVIDENCE_PATH")
        ));
    }

    let mut control_path = valid_contract_configuration(Vendor::GenericLdap);
    control_path.insert(
        "ERPX_G14_GENERIC_LDAP_REVOCATION_EVIDENCE_PATH".to_owned(),
        "/tmp/evidence\n.json".to_owned(),
    );
    assert!(matches!(
        configured(Vendor::GenericLdap, |name| control_path.get(name).cloned()),
        Err(Blocker::Missing { names, .. })
            if names.contains(&"ERPX_G14_GENERIC_LDAP_REVOCATION_EVIDENCE_PATH")
    ));

    for vendor in [Vendor::ActiveDirectory, Vendor::Ucs] {
        let max_name = format!("{}_REVOCATION_MAX_SECONDS", vendor.prefix());
        let mut wrong_max = valid_contract_configuration(vendor);
        wrong_max.insert(max_name.clone(), "299".to_owned());
        assert!(matches!(
            configured(vendor, |name| wrong_max.get(name).cloned()),
            Err(Blocker::Invalid { fields, .. }) if fields.iter().any(|field| field == &max_name)
        ));

        let operation_name = format!("{}_REVOCATION_OPERATION", vendor.prefix());
        let mut wrong_operation = valid_contract_configuration(vendor);
        wrong_operation.insert(operation_name.clone(), "suspend".to_owned());
        assert!(matches!(
            configured(vendor, |name| wrong_operation.get(name).cloned()),
            Err(Blocker::Invalid { fields, .. })
                if fields.iter().any(|field| field == &operation_name)
        ));
    }
}

#[test]
fn g14_external_readiness_requires_explicit_full_import_at_the_one_million_bound() {
    for vendor in Vendor::ALL {
        let max_name = format!("{}_MAX_IMPORT_USERS", vendor.prefix());
        let mut missing_max = valid_contract_configuration(vendor);
        missing_max.remove(&max_name);
        match configured(vendor, |name| missing_max.get(name).cloned()) {
            Err(Blocker::Missing { names, .. }) => {
                assert!(names.contains(&max_name.as_str()));
            }
            other => panic!("missing {max_name} must block, got {other:?}"),
        }

        let full_name = format!("{}_FULL_IMPORT", vendor.prefix());
        let mut missing_full = valid_contract_configuration(vendor);
        missing_full.remove(&full_name);
        match configured(vendor, |name| missing_full.get(name).cloned()) {
            Err(Blocker::Missing { names, .. }) => {
                assert!(names.contains(&full_name.as_str()));
            }
            other => panic!("missing {full_name} must block, got {other:?}"),
        }

        let mut non_maximum = valid_contract_configuration(vendor);
        non_maximum.insert(max_name.clone(), (MAX_IMPORT_USERS - 1).to_string());
        match configured(vendor, |name| non_maximum.get(name).cloned()) {
            Err(Blocker::Invalid { fields, .. }) => {
                assert!(fields.iter().any(|field| field == &max_name));
            }
            other => panic!("non-maximum {max_name} must block, got {other:?}"),
        }

        let mut partial_import = valid_contract_configuration(vendor);
        partial_import.insert(full_name.clone(), "false".to_owned());
        match configured(vendor, |name| partial_import.get(name).cloned()) {
            Err(Blocker::Invalid { fields, .. }) => {
                assert!(fields.iter().any(|field| field == &full_name));
            }
            other => panic!("false {full_name} must block, got {other:?}"),
        }
    }
}

#[test]
fn g14_external_readiness_rejects_oversized_credentials_fail_closed() {
    for vendor in Vendor::ALL {
        let mut values = valid_contract_configuration(vendor);
        let secret_name = vendor.required_secret();
        values.insert(secret_name.to_owned(), "x".repeat(MAX_CREDENTIAL_BYTES + 1));

        let error = configured(vendor, |name| values.get(name).cloned())
            .expect_err("oversized credentials must block external readiness");
        assert!(matches!(
            error,
            Blocker::Invalid {
                vendor: actual,
                fields
            } if actual == vendor && fields.iter().any(|field| field == secret_name)
        ));
    }
}

#[test]
fn g14_external_readiness_does_not_require_fixed_ad_or_ucs_feed_variables() {
    for vendor in [Vendor::ActiveDirectory, Vendor::Ucs] {
        let values = valid_contract_configuration(vendor);
        assert!(configured(vendor, |name| values.get(name).cloned()).is_ok());
        assert!(
            vendor
                .required()
                .iter()
                .all(|name| !name.ends_with("_FEED")),
            "{} feed is fixed by the live harness and must not be an environment blocker",
            vendor.label()
        );
    }
}

#[test]
fn g14_optional_directory_contract_helpers_are_bounded_and_structural() {
    assert!(valid_attribute_name("uid").then_some(()).is_some());
    assert!(!valid_attribute_name("uid mail"));
    assert!(valid_ldap_filter("(&(objectClass=person)(uid=*))"));
    assert!(!valid_ldap_filter("objectClass=person"));
    assert!(!valid_ldap_filter("(objectClass=person"));
    assert!(!valid_ldap_filter("(objectClass=person)\u{0000}"));
}

#[test]
fn g14_external_readiness_reports_current_environment_without_claiming_vendor_acceptance() {
    let blockers = process_environment();
    eprintln!(
        "G14 CERTIFICATION QUEUE (not vendor acceptance): {}",
        certification_queue_report(|name| env::var(name).ok())
    );
    if blockers.is_empty() {
        eprintln!(
            "G14 external references are complete enough for a live attempt; this test still does not claim Generic LDAP, AD DS, or UCS certification"
        );
    }
}

#[test]
#[ignore = "requires credentialed real Generic LDAP, AD DS, and UCS environments; run explicitly as a certification preflight"]
fn g14_external_certification_preflight_fails_closed_when_not_ready() {
    let blockers = process_environment();
    assert!(
        blockers.is_empty(),
        "G14 certification preflight blocked; no vendor acceptance is claimed: {}",
        queued_blocker_report(&blockers)
    );
}
