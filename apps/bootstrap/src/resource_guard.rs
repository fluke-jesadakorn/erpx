use std::{
    fmt,
    fs::{self, File},
    io::{self, Read},
    net::IpAddr,
    path::{Path, PathBuf},
    time::Duration,
};

use chrono::{DateTime, Utc};
use erpx_infrastructure::{MinioObjectStore, MinioObjectStoreConfig};
use serde::{Serialize, Serializer};
use sha2::{Digest, Sha256};

const N8N_CONFIG_COMPONENTS: [&str; 4] = [".runtime", "n8n", ".n8n", "config"];
const MAX_N8N_CONFIG_BYTES: u64 = 1024 * 1024;
const MAX_NAMESPACE_OPERATION_SECONDS: u64 = 60;

type GuardResult<T> = std::result::Result<T, ResourceGuardError>;

/// Fail-closed resource guard errors. Messages deliberately contain neither
/// endpoints, resource references, credentials, nor file contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResourceGuardError {
    InvalidLoopbackEndpoint,
    InvalidInstallationPrefix,
    InvalidObjectNamespace,
    ReusedInstallationPrefix,
    ReusedObjectNamespace,
    InvalidLegacyInventory,
    InvalidRollbackMetadata,
    ObjectStoreConfigurationFailed,
    ObjectStoreEndpointMismatch,
    ObjectNamespaceOperationFailed,
    ObjectNamespaceOperationTimedOut,
    N8nConfigPathInvalid,
    N8nConfigReadFailed,
    N8nConfigTooLarge,
    N8nConfigChanged,
}

impl fmt::Display for ResourceGuardError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLoopbackEndpoint => {
                "MinIO endpoint must be an exact HTTPS loopback endpoint with an explicit port"
            }
            Self::InvalidInstallationPrefix => "fresh installation prefix is invalid",
            Self::InvalidObjectNamespace => "fresh object namespace is invalid",
            Self::ReusedInstallationPrefix => {
                "fresh installation prefix overlaps the retained resource inventory"
            }
            Self::ReusedObjectNamespace => {
                "fresh object namespace overlaps the retained resource inventory"
            }
            Self::InvalidLegacyInventory => "retained resource inventory is invalid",
            Self::InvalidRollbackMetadata => "rollback metadata is invalid",
            Self::ObjectStoreConfigurationFailed => "MinIO client configuration failed",
            Self::ObjectStoreEndpointMismatch => {
                "MinIO client endpoint does not match the validated endpoint"
            }
            Self::ObjectNamespaceOperationFailed => {
                "MinIO namespace creation or verification failed"
            }
            Self::ObjectNamespaceOperationTimedOut => {
                "MinIO namespace creation or verification timed out"
            }
            Self::N8nConfigPathInvalid => "n8n configuration path is not an exact regular file",
            Self::N8nConfigReadFailed => "n8n configuration fingerprint could not be read",
            Self::N8nConfigTooLarge => "n8n configuration exceeds the fingerprint size bound",
            Self::N8nConfigChanged => "n8n configuration fingerprint changed during cutover",
        })
    }
}

impl std::error::Error for ResourceGuardError {}

/// An exact, root-only MinIO URL. DNS aliases are intentionally rejected: the
/// accepted hosts are `localhost`, a loopback IPv4 address, or `[::1]` (and
/// other loopback IP literals), always with an explicit non-zero port.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(crate) struct LoopbackMinioEndpoint(String);

impl LoopbackMinioEndpoint {
    pub(crate) fn parse(value: &str) -> GuardResult<Self> {
        if value.trim() != value {
            return Err(ResourceGuardError::InvalidLoopbackEndpoint);
        }
        let authority = value
            .strip_prefix("https://")
            .ok_or(ResourceGuardError::InvalidLoopbackEndpoint)?;
        if authority.is_empty()
            || authority.contains(['/', '?', '#', '@'])
            || authority.chars().any(char::is_whitespace)
        {
            return Err(ResourceGuardError::InvalidLoopbackEndpoint);
        }

        let (host, port) = parse_loopback_authority(authority)?;
        port.parse::<u16>()
            .ok()
            .filter(|port| *port != 0)
            .ok_or(ResourceGuardError::InvalidLoopbackEndpoint)?;
        let loopback = host == "localhost"
            || host
                .parse::<IpAddr>()
                .is_ok_and(|address| address.is_loopback());
        if !loopback {
            return Err(ResourceGuardError::InvalidLoopbackEndpoint);
        }
        Ok(Self(value.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for LoopbackMinioEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("LoopbackMinioEndpoint")
            .field(&self.0)
            .finish()
    }
}

fn parse_loopback_authority(authority: &str) -> GuardResult<(&str, &str)> {
    if let Some(ipv6) = authority.strip_prefix('[') {
        let (host, suffix) = ipv6
            .split_once(']')
            .ok_or(ResourceGuardError::InvalidLoopbackEndpoint)?;
        let port = suffix
            .strip_prefix(':')
            .ok_or(ResourceGuardError::InvalidLoopbackEndpoint)?;
        if host.is_empty() || port.is_empty() || port.contains(':') {
            return Err(ResourceGuardError::InvalidLoopbackEndpoint);
        }
        return Ok((host, port));
    }

    let (host, port) = authority
        .split_once(':')
        .ok_or(ResourceGuardError::InvalidLoopbackEndpoint)?;
    if host.is_empty() || port.is_empty() || port.contains(':') {
        return Err(ResourceGuardError::InvalidLoopbackEndpoint);
    }
    Ok((host, port))
}

/// Closed, non-secret inventory of retained resources. Environment and binary
/// references are opaque identifiers, never URLs, credentials, or file data.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub(crate) struct OldResourceInventory {
    captured_at: DateTime<Utc>,
    installation_prefixes: Vec<String>,
    database_names: Vec<String>,
    object_namespaces: Vec<String>,
    binary_release_refs: Vec<String>,
    environment_snapshot_ref: String,
}

impl OldResourceInventory {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        captured_at: DateTime<Utc>,
        mut installation_prefixes: Vec<String>,
        mut database_names: Vec<String>,
        mut object_namespaces: Vec<String>,
        mut binary_release_refs: Vec<String>,
        environment_snapshot_ref: String,
    ) -> GuardResult<Self> {
        if installation_prefixes.is_empty()
            || database_names.is_empty()
            || object_namespaces.is_empty()
            || binary_release_refs.is_empty()
            || installation_prefixes
                .iter()
                .any(|value| validate_installation_prefix(value).is_err())
            || database_names
                .iter()
                .any(|value| !is_database_identifier(value))
            || object_namespaces.iter().any(|value| {
                erpx_infrastructure::MinioObjectStore::validate_namespace_reference(value).is_err()
            })
            || binary_release_refs
                .iter()
                .any(|value| !is_safe_opaque_reference(value))
            || !is_safe_opaque_reference(&environment_snapshot_ref)
        {
            return Err(ResourceGuardError::InvalidLegacyInventory);
        }

        sort_and_require_unique(&mut installation_prefixes)?;
        sort_and_require_unique(&mut database_names)?;
        sort_and_require_unique(&mut object_namespaces)?;
        sort_and_require_unique(&mut binary_release_refs)?;
        Ok(Self {
            captured_at,
            installation_prefixes,
            database_names,
            object_namespaces,
            binary_release_refs,
            environment_snapshot_ref,
        })
    }

    pub(crate) fn captured_at(&self) -> DateTime<Utc> {
        self.captured_at
    }

    pub(crate) fn installation_prefixes(&self) -> &[String] {
        &self.installation_prefixes
    }

    pub(crate) fn database_names(&self) -> &[String] {
        &self.database_names
    }

    pub(crate) fn object_namespaces(&self) -> &[String] {
        &self.object_namespaces
    }
}

impl fmt::Debug for OldResourceInventory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OldResourceInventory")
            .field("captured_at", &self.captured_at)
            .field(
                "installation_prefix_count",
                &self.installation_prefixes.len(),
            )
            .field("database_count", &self.database_names.len())
            .field("object_namespace_count", &self.object_namespaces.len())
            .field("binary_release_count", &self.binary_release_refs.len())
            .field("environment_snapshot_ref", &"<opaque>")
            .finish()
    }
}

fn sort_and_require_unique(values: &mut [String]) -> GuardResult<()> {
    values.sort();
    if values.windows(2).any(|pair| pair[0] == pair[1]) {
        Err(ResourceGuardError::InvalidLegacyInventory)
    } else {
        Ok(())
    }
}

fn is_database_identifier(value: &str) -> bool {
    (1..=63).contains(&value.len())
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn is_safe_opaque_reference(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && !value.contains("://")
        && !value.contains(['@', '?', '#', '=', '\\', '\n', '\r'])
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        })
}

/// Validated bridge from `fresh_state::FreshResourcePlan` fields to object
/// storage. It intentionally accepts plain strings to avoid coupling the two
/// cutover modules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FreshNamespacePlan {
    endpoint: LoopbackMinioEndpoint,
    installation_prefix: String,
    namespace: String,
}

impl FreshNamespacePlan {
    pub(crate) fn new(
        endpoint: &str,
        installation_prefix: &str,
        namespace: &str,
        old_resources: &OldResourceInventory,
    ) -> GuardResult<Self> {
        validate_installation_prefix(installation_prefix)?;
        erpx_infrastructure::MinioObjectStore::validate_namespace_reference(namespace)
            .map_err(|_| ResourceGuardError::InvalidObjectNamespace)?;
        let namespace_prefix = format!("{}-", installation_prefix.replace('_', "-"));
        if !namespace.starts_with(&namespace_prefix) {
            return Err(ResourceGuardError::InvalidObjectNamespace);
        }
        if old_resources
            .installation_prefixes()
            .iter()
            .any(|retained| retained == installation_prefix)
        {
            return Err(ResourceGuardError::ReusedInstallationPrefix);
        }
        if old_resources
            .object_namespaces()
            .iter()
            .any(|retained| retained == namespace)
        {
            return Err(ResourceGuardError::ReusedObjectNamespace);
        }
        Ok(Self {
            endpoint: LoopbackMinioEndpoint::parse(endpoint)?,
            installation_prefix: installation_prefix.to_owned(),
            namespace: namespace.to_owned(),
        })
    }

    pub(crate) fn endpoint(&self) -> &LoopbackMinioEndpoint {
        &self.endpoint
    }

    pub(crate) fn installation_prefix(&self) -> &str {
        &self.installation_prefix
    }

    pub(crate) fn namespace(&self) -> &str {
        &self.namespace
    }
}

fn validate_installation_prefix(value: &str) -> GuardResult<()> {
    let valid = (2..=16).contains(&value.len())
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    if valid {
        Ok(())
    } else {
        Err(ResourceGuardError::InvalidInstallationPrefix)
    }
}

/// Minimal, intentionally non-destructive namespace interface. There are no
/// copy, list, overwrite, or delete operations in the cutover boundary.
pub(crate) trait BucketNamespaceAdmin: Sync {
    fn endpoint(&self) -> &LoopbackMinioEndpoint;

    async fn create_namespace_exclusive(&self, namespace: &str) -> GuardResult<()>;

    async fn namespace_exists(&self, namespace: &str) -> GuardResult<bool>;
}

/// Production adapter over the existing signed MinIO client. Credentials stay
/// inside `MinioObjectStoreConfig`, whose Debug implementation is redacted.
#[derive(Clone)]
pub(crate) struct LoopbackMinioNamespaceAdmin {
    endpoint: LoopbackMinioEndpoint,
    store: MinioObjectStore,
}

impl LoopbackMinioNamespaceAdmin {
    pub(crate) fn new(
        endpoint: &LoopbackMinioEndpoint,
        config: MinioObjectStoreConfig,
        ca_certificate_pem: Option<&[u8]>,
    ) -> GuardResult<Self> {
        if config.endpoint != endpoint.as_str() {
            return Err(ResourceGuardError::ObjectStoreEndpointMismatch);
        }
        let store = MinioObjectStore::new_with_ca_certificate(config, ca_certificate_pem)
            .map_err(|_| ResourceGuardError::ObjectStoreConfigurationFailed)?;
        Ok(Self {
            endpoint: endpoint.clone(),
            store,
        })
    }
}

impl fmt::Debug for LoopbackMinioNamespaceAdmin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LoopbackMinioNamespaceAdmin")
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

impl BucketNamespaceAdmin for LoopbackMinioNamespaceAdmin {
    fn endpoint(&self) -> &LoopbackMinioEndpoint {
        &self.endpoint
    }

    async fn create_namespace_exclusive(&self, namespace: &str) -> GuardResult<()> {
        self.store
            .create_namespace_exclusive(namespace)
            .await
            .map_err(|_| ResourceGuardError::ObjectNamespaceOperationFailed)
    }

    async fn namespace_exists(&self, namespace: &str) -> GuardResult<bool> {
        self.store
            .namespace_exists(namespace)
            .await
            .map_err(|_| ResourceGuardError::ObjectNamespaceOperationFailed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct NamespaceVerificationReceipt {
    installation_prefix: String,
    namespace: String,
    verified_at: DateTime<Utc>,
    state: NamespaceVerificationState,
}

impl NamespaceVerificationReceipt {
    pub(crate) fn prefix(&self) -> &str {
        &self.installation_prefix
    }

    pub(crate) fn namespace(&self) -> &str {
        &self.namespace
    }

    pub(crate) fn verified_at(&self) -> DateTime<Utc> {
        self.verified_at
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NamespaceVerificationState {
    CreatedAndAccessible,
}

/// Exclusively creates a missing namespace, then performs two independent
/// signed existence checks. No receipt is issued when the namespace already
/// existed, raced with another creator, disappeared, timed out, or could not
/// be authorized.
pub(crate) async fn create_and_verify_new_namespace<A: BucketNamespaceAdmin>(
    plan: &FreshNamespacePlan,
    admin: &A,
    verified_at: DateTime<Utc>,
    operation_timeout: Duration,
) -> GuardResult<NamespaceVerificationReceipt> {
    if admin.endpoint() != plan.endpoint() {
        return Err(ResourceGuardError::ObjectStoreEndpointMismatch);
    }
    if operation_timeout.is_zero()
        || operation_timeout > Duration::from_secs(MAX_NAMESPACE_OPERATION_SECONDS)
    {
        return Err(ResourceGuardError::ObjectNamespaceOperationTimedOut);
    }
    tokio::time::timeout(
        operation_timeout,
        admin.create_namespace_exclusive(plan.namespace()),
    )
    .await
    .map_err(|_| ResourceGuardError::ObjectNamespaceOperationTimedOut)??;
    for _ in 0..2 {
        let present =
            tokio::time::timeout(operation_timeout, admin.namespace_exists(plan.namespace()))
                .await
                .map_err(|_| ResourceGuardError::ObjectNamespaceOperationTimedOut)??;
        if !present {
            return Err(ResourceGuardError::ObjectNamespaceOperationFailed);
        }
    }
    Ok(NamespaceVerificationReceipt {
        installation_prefix: plan.installation_prefix().to_owned(),
        namespace: plan.namespace().to_owned(),
        verified_at,
        state: NamespaceVerificationState::CreatedAndAccessible,
    })
}

/// Perform a signed, non-mutating preflight check that the target namespace
/// does not already exist. This is deliberately separate from the exclusive
/// create operation so `check` can fail before any database is created.
pub(crate) async fn verify_new_namespace_absent<A: BucketNamespaceAdmin>(
    plan: &FreshNamespacePlan,
    admin: &A,
    operation_timeout: Duration,
) -> GuardResult<()> {
    if admin.endpoint() != plan.endpoint() {
        return Err(ResourceGuardError::ObjectStoreEndpointMismatch);
    }
    if operation_timeout.is_zero()
        || operation_timeout > Duration::from_secs(MAX_NAMESPACE_OPERATION_SECONDS)
    {
        return Err(ResourceGuardError::ObjectNamespaceOperationTimedOut);
    }
    let present = tokio::time::timeout(operation_timeout, admin.namespace_exists(plan.namespace()))
        .await
        .map_err(|_| ResourceGuardError::ObjectNamespaceOperationTimedOut)??;
    if present {
        Err(ResourceGuardError::ReusedObjectNamespace)
    } else {
        Ok(())
    }
}

/// SHA-256 fingerprint of the exact n8n config file. Debug output is redacted;
/// serialization is the conventional lowercase hexadecimal digest.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct N8nConfigFingerprint([u8; 32]);

impl N8nConfigFingerprint {
    pub(crate) fn as_hex(&self) -> String {
        let mut encoded = String::with_capacity(64);
        for byte in self.0 {
            use fmt::Write as _;
            let _ = write!(&mut encoded, "{byte:02x}");
        }
        encoded
    }

    #[cfg(test)]
    pub(crate) fn from_digest_for_test(digest: [u8; 32]) -> Self {
        Self(digest)
    }
}

impl fmt::Debug for N8nConfigFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("N8nConfigFingerprint")
            .field(&"<sha256>")
            .finish()
    }
}

impl Serialize for N8nConfigFingerprint {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.as_hex())
    }
}

/// Resolve only `<workspace>/.runtime/n8n/.n8n/config`, reject symlinked path
/// components, and hash a bounded regular file without modifying it.
pub(crate) fn capture_n8n_config_fingerprint(
    workspace_root: &Path,
) -> GuardResult<N8nConfigFingerprint> {
    let path = checked_n8n_config_path(workspace_root)?;
    let file = File::open(path).map_err(|_| ResourceGuardError::N8nConfigReadFailed)?;
    fingerprint_reader(file)
}

pub(crate) fn verify_n8n_config_fingerprint(
    workspace_root: &Path,
    expected: &N8nConfigFingerprint,
) -> GuardResult<()> {
    let actual = capture_n8n_config_fingerprint(workspace_root)?;
    if constant_time_fingerprint_eq(expected, &actual) {
        Ok(())
    } else {
        Err(ResourceGuardError::N8nConfigChanged)
    }
}

fn checked_n8n_config_path(workspace_root: &Path) -> GuardResult<PathBuf> {
    if !workspace_root.is_absolute() {
        return Err(ResourceGuardError::N8nConfigPathInvalid);
    }
    let mut path = workspace_root.to_path_buf();
    for (index, component) in N8N_CONFIG_COMPONENTS.iter().enumerate() {
        path.push(component);
        let metadata =
            fs::symlink_metadata(&path).map_err(|_| ResourceGuardError::N8nConfigPathInvalid)?;
        if metadata.file_type().is_symlink()
            || (index + 1 == N8N_CONFIG_COMPONENTS.len() && !metadata.is_file())
            || (index + 1 != N8N_CONFIG_COMPONENTS.len() && !metadata.is_dir())
        {
            return Err(ResourceGuardError::N8nConfigPathInvalid);
        }
    }
    Ok(path)
}

fn fingerprint_reader(mut reader: impl Read) -> GuardResult<N8nConfigFingerprint> {
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let count = match reader.read(&mut buffer) {
            Ok(count) => count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(ResourceGuardError::N8nConfigReadFailed),
        };
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .ok_or(ResourceGuardError::N8nConfigTooLarge)?;
        if total > MAX_N8N_CONFIG_BYTES {
            return Err(ResourceGuardError::N8nConfigTooLarge);
        }
        hasher.update(&buffer[..count]);
    }
    Ok(N8nConfigFingerprint(hasher.finalize().into()))
}

fn constant_time_fingerprint_eq(
    expected: &N8nConfigFingerprint,
    actual: &N8nConfigFingerprint,
) -> bool {
    expected
        .0
        .iter()
        .zip(actual.0.iter())
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RetainedResourceAuthority {
    OfflineNonAuthoritative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RollbackWritePolicy {
    IsolatedNoMerge,
}

/// Serializable rollback evidence. Fixed enum values make it impossible for a
/// caller to claim that the old resources remain authoritative or that writes
/// may be merged across the old and fresh installations.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RollbackMetadata {
    recorded_at: DateTime<Utc>,
    old_resources: OldResourceInventory,
    n8n_config_sha256: N8nConfigFingerprint,
    retained_resource_authority: RetainedResourceAuthority,
    write_policy: RollbackWritePolicy,
}

impl RollbackMetadata {
    pub(crate) fn new(
        recorded_at: DateTime<Utc>,
        old_resources: OldResourceInventory,
        n8n_config_sha256: N8nConfigFingerprint,
    ) -> GuardResult<Self> {
        if recorded_at < old_resources.captured_at() {
            return Err(ResourceGuardError::InvalidRollbackMetadata);
        }
        Ok(Self {
            recorded_at,
            old_resources,
            n8n_config_sha256,
            retained_resource_authority: RetainedResourceAuthority::OfflineNonAuthoritative,
            write_policy: RollbackWritePolicy::IsolatedNoMerge,
        })
    }

    #[cfg(test)]
    pub(crate) fn old_resources(&self) -> &OldResourceInventory {
        &self.old_resources
    }
}

impl fmt::Debug for RollbackMetadata {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RollbackMetadata")
            .field("recorded_at", &self.recorded_at)
            .field("old_resources", &self.old_resources)
            .field("n8n_config_sha256", &self.n8n_config_sha256)
            .field(
                "retained_resource_authority",
                &self.retained_resource_authority,
            )
            .field("write_policy", &self.write_policy)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::Cursor,
        sync::{Arc, Mutex},
    };

    use chrono::TimeZone;

    use super::*;

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 5, hour, 0, 0)
            .single()
            .expect("fixed timestamp")
    }

    fn retained_inventory() -> OldResourceInventory {
        OldResourceInventory::new(
            at(8),
            vec!["legacy".into()],
            vec!["legacy_control_read".into(), "legacy_control_write".into()],
            vec!["legacy-tenant-objects".into()],
            vec!["release:v1".into()],
            "environment/legacy-v1".into(),
        )
        .expect("valid retained inventory")
    }

    #[test]
    fn endpoint_accepts_only_exact_https_loopback_authorities() {
        for endpoint in [
            "https://localhost:9000",
            "https://127.0.0.1:9000",
            "https://127.42.0.1:9443",
            "https://[::1]:9000",
        ] {
            assert!(LoopbackMinioEndpoint::parse(endpoint).is_ok(), "{endpoint}");
        }
        for endpoint in [
            "http://127.0.0.1:9000",
            "https://minio.example.test:9000",
            "https://0.0.0.0:9000",
            "https://localhost",
            "https://localhost:0",
            "https://localhost:9000/",
            "https://user@localhost:9000",
            " https://localhost:9000",
        ] {
            assert!(
                LoopbackMinioEndpoint::parse(endpoint).is_err(),
                "{endpoint}"
            );
        }
    }

    #[test]
    fn fresh_namespace_must_be_prefix_owned_and_absent_from_inventory() {
        let retained = retained_inventory();
        assert!(
            FreshNamespacePlan::new(
                "https://127.0.0.1:9000",
                "next_v2",
                "next-v2-tenant-objects",
                &retained,
            )
            .is_ok()
        );
        assert!(
            FreshNamespacePlan::new(
                "https://127.0.0.1:9000",
                "legacy",
                "legacy-v2-tenant-objects",
                &retained,
            )
            .is_err()
        );
        assert!(
            FreshNamespacePlan::new(
                "https://127.0.0.1:9000",
                "next_v2",
                "legacy-tenant-objects",
                &retained,
            )
            .is_err()
        );
    }

    #[derive(Clone)]
    struct FakeNamespaceAdmin {
        endpoint: LoopbackMinioEndpoint,
        calls: Arc<Mutex<Vec<String>>>,
        fail_on_call: Option<usize>,
        preexisting: bool,
    }

    impl BucketNamespaceAdmin for FakeNamespaceAdmin {
        fn endpoint(&self) -> &LoopbackMinioEndpoint {
            &self.endpoint
        }

        async fn create_namespace_exclusive(&self, namespace: &str) -> GuardResult<()> {
            let mut calls = self.calls.lock().expect("calls mutex");
            calls.push(format!("create:{namespace}"));
            if self.preexisting || self.fail_on_call == Some(calls.len()) {
                Err(ResourceGuardError::ObjectNamespaceOperationFailed)
            } else {
                Ok(())
            }
        }

        async fn namespace_exists(&self, namespace: &str) -> GuardResult<bool> {
            let mut calls = self.calls.lock().expect("calls mutex");
            calls.push(format!("exists:{namespace}"));
            if self.fail_on_call == Some(calls.len()) {
                Err(ResourceGuardError::ObjectNamespaceOperationFailed)
            } else {
                Ok(true)
            }
        }
    }

    #[tokio::test]
    async fn namespace_receipt_requires_create_and_second_verification() {
        let plan = FreshNamespacePlan::new(
            "https://127.0.0.1:9000",
            "next_v2",
            "next-v2-tenant-objects",
            &retained_inventory(),
        )
        .expect("fresh plan");
        let admin = FakeNamespaceAdmin {
            endpoint: plan.endpoint().clone(),
            calls: Arc::new(Mutex::new(Vec::new())),
            fail_on_call: None,
            preexisting: false,
        };
        let receipt = create_and_verify_new_namespace(&plan, &admin, at(9), Duration::from_secs(5))
            .await
            .expect("verified namespace");
        assert_eq!(receipt.namespace(), "next-v2-tenant-objects");
        assert_eq!(receipt.verified_at(), at(9));
        assert_eq!(admin.calls.lock().expect("calls mutex").len(), 3);

        let failing = FakeNamespaceAdmin {
            endpoint: plan.endpoint().clone(),
            calls: Arc::new(Mutex::new(Vec::new())),
            fail_on_call: Some(2),
            preexisting: false,
        };
        assert!(
            create_and_verify_new_namespace(&plan, &failing, at(9), Duration::from_secs(5),)
                .await
                .is_err()
        );

        let preexisting = FakeNamespaceAdmin {
            endpoint: plan.endpoint().clone(),
            calls: Arc::new(Mutex::new(Vec::new())),
            fail_on_call: None,
            preexisting: true,
        };
        assert!(
            create_and_verify_new_namespace(&plan, &preexisting, at(9), Duration::from_secs(5),)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn namespace_preflight_is_signed_and_non_mutating() {
        let plan = FreshNamespacePlan::new(
            "https://127.0.0.1:9000",
            "next_v2",
            "next-v2-tenant-objects",
            &retained_inventory(),
        )
        .expect("fresh plan");
        // The fake existence implementation always returns true; use a
        // dedicated adapter below to prove the absent path without allowing a
        // create call to hide in the preflight.
        #[derive(Clone)]
        struct AbsentNamespaceAdmin {
            endpoint: LoopbackMinioEndpoint,
            calls: Arc<Mutex<Vec<String>>>,
        }
        impl BucketNamespaceAdmin for AbsentNamespaceAdmin {
            fn endpoint(&self) -> &LoopbackMinioEndpoint {
                &self.endpoint
            }

            async fn create_namespace_exclusive(&self, _namespace: &str) -> GuardResult<()> {
                panic!("preflight must not create a namespace")
            }

            async fn namespace_exists(&self, namespace: &str) -> GuardResult<bool> {
                self.calls
                    .lock()
                    .expect("calls mutex")
                    .push(format!("exists:{namespace}"));
                Ok(false)
            }
        }
        let absent = AbsentNamespaceAdmin {
            endpoint: plan.endpoint().clone(),
            calls: Arc::new(Mutex::new(Vec::new())),
        };
        verify_new_namespace_absent(&plan, &absent, Duration::from_secs(5))
            .await
            .expect("namespace is absent");
        assert_eq!(
            absent.calls.lock().expect("calls mutex").as_slice(),
            ["exists:next-v2-tenant-objects"]
        );

        assert!(
            verify_new_namespace_absent(&plan, &absent, Duration::ZERO)
                .await
                .is_err()
        );
        assert!(
            verify_new_namespace_absent(&plan, &absent, Duration::from_secs(61))
                .await
                .is_err()
        );
        let present = FakeNamespaceAdmin {
            endpoint: plan.endpoint().clone(),
            calls: Arc::new(Mutex::new(Vec::new())),
            fail_on_call: None,
            preexisting: false,
        };
        assert_eq!(
            verify_new_namespace_absent(&plan, &present, Duration::from_secs(5)).await,
            Err(ResourceGuardError::ReusedObjectNamespace)
        );
    }

    #[test]
    fn sha256_fingerprint_matches_known_vector_and_debug_is_redacted() {
        let fingerprint = fingerprint_reader(Cursor::new(b"abc")).expect("fingerprint");
        assert_eq!(
            fingerprint.as_hex(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(!format!("{fingerprint:?}").contains(&fingerprint.as_hex()));
        assert!(constant_time_fingerprint_eq(&fingerprint, &fingerprint));
        let other = fingerprint_reader(Cursor::new(b"abd")).expect("other fingerprint");
        assert!(!constant_time_fingerprint_eq(&fingerprint, &other));
    }

    #[test]
    fn rollback_metadata_is_fixed_to_offline_no_merge_policy() {
        let inventory = retained_inventory();
        let fingerprint = fingerprint_reader(Cursor::new(b"n8n-config")).expect("fingerprint");
        let metadata =
            RollbackMetadata::new(at(9), inventory.clone(), fingerprint).expect("metadata");
        assert_eq!(metadata.old_resources(), &inventory);
        let encoded = serde_json::to_value(&metadata).expect("serialized metadata");
        assert_eq!(
            encoded["retained_resource_authority"],
            "offline_non_authoritative"
        );
        assert_eq!(encoded["write_policy"], "isolated_no_merge");
        assert!(
            RollbackMetadata::new(
                at(7),
                inventory,
                fingerprint_reader(Cursor::new(b"n8n-config")).expect("fingerprint")
            )
            .is_err()
        );
    }

    #[test]
    fn inventory_debug_does_not_expose_opaque_rollback_references() {
        let inventory = retained_inventory();
        let debug = format!("{inventory:?}");
        assert!(!debug.contains("release:v1"));
        assert!(!debug.contains("environment/legacy-v1"));
        assert!(debug.contains("binary_release_count"));
    }
}
