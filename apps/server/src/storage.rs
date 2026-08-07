use std::{
    env,
    fmt::{self, Display},
    fs,
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
};

use chrono::{DateTime, Utc};
use erpx_infrastructure::{
    MinioObjectStore, MinioObjectStoreConfig, ObjectStorageError, PersistenceError,
    SignedObjectUrl, TenantDatabaseProvisioner, TenantStorageHealth,
};
use serde::Serialize;

const ENDPOINT_ENV: &str = "ERPX_MINIO_ENDPOINT";
const ACCESS_KEY_ENV: &str = "ERPX_MINIO_ACCESS_KEY";
const SECRET_KEY_ENV: &str = "ERPX_MINIO_SECRET_KEY";
const REGION_ENV: &str = "ERPX_MINIO_REGION";
const REQUIRED_ENV: &str = "ERPX_MINIO_REQUIRED";
const CA_CERTIFICATE_PATH_ENV: &str = "ERPX_MINIO_CA_CERTIFICATE_PATH";
const MAX_CA_CERTIFICATE_BYTES: u64 = 128 * 1024;

type NamespaceFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), ObjectStorageError>> + Send + 'a>>;

const MAX_UPLOAD_BYTES: u64 = 64 * 1024 * 1024;
const MAX_DOWNLOAD_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PRESIGN_EXPIRY_SECONDS: u32 = 3_600;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TenantObjectUploadRequest {
    pub(crate) tenant_id: String,
    pub(crate) storage_namespace: String,
    pub(crate) object_key: String,
    pub(crate) expires_seconds: u32,
    pub(crate) content_type: String,
    pub(crate) content_length: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TenantObjectDownloadRequest {
    pub(crate) tenant_id: String,
    pub(crate) storage_namespace: String,
    pub(crate) object_key: String,
    pub(crate) expires_seconds: u32,
    pub(crate) content_type: Option<String>,
    pub(crate) max_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TenantSignedObjectUrl {
    pub(crate) url: String,
    pub(crate) method: &'static str,
    pub(crate) expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ObjectAuthorizationError {
    Denied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ObjectAccessError {
    InvalidRequest(&'static str),
    Unauthorized,
    StorageUnavailable,
    PresignFailed,
}

/// The server-side object-store seam. Implementations never cross an RPC or
/// HTTP boundary, and callers receive only the namespace operation result.
pub(crate) trait ObjectStoreBoundary: Send + Sync {
    fn ensure_namespace<'a>(&'a self, namespace: &'a str) -> NamespaceFuture<'a>;

    fn probe(&self) -> NamespaceFuture<'_>;

    fn presign_upload(
        &self,
        namespace: &str,
        object_key: &str,
        expires_seconds: u32,
        at: DateTime<Utc>,
    ) -> Result<SignedObjectUrl, ObjectStorageError>;

    fn presign_download(
        &self,
        namespace: &str,
        object_key: &str,
        expires_seconds: u32,
        at: DateTime<Utc>,
    ) -> Result<SignedObjectUrl, ObjectStorageError>;
}

impl ObjectStoreBoundary for MinioObjectStore {
    fn ensure_namespace<'a>(&'a self, namespace: &'a str) -> NamespaceFuture<'a> {
        Box::pin(MinioObjectStore::ensure_namespace(self, namespace))
    }

    fn probe(&self) -> NamespaceFuture<'_> {
        Box::pin(MinioObjectStore::probe(self))
    }

    fn presign_upload(
        &self,
        namespace: &str,
        object_key: &str,
        expires_seconds: u32,
        at: DateTime<Utc>,
    ) -> Result<SignedObjectUrl, ObjectStorageError> {
        MinioObjectStore::presign_upload(self, namespace, object_key, expires_seconds, at)
    }

    fn presign_download(
        &self,
        namespace: &str,
        object_key: &str,
        expires_seconds: u32,
        at: DateTime<Utc>,
    ) -> Result<SignedObjectUrl, ObjectStorageError> {
        MinioObjectStore::presign_download(self, namespace, object_key, expires_seconds, at)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StorageConfigurationError {
    Incomplete,
    Invalid,
    Required,
    InvalidValue(&'static str),
}

impl Display for StorageConfigurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Incomplete => formatter.write_str(
                "ERPX_MINIO_ENDPOINT, ERPX_MINIO_ACCESS_KEY, ERPX_MINIO_SECRET_KEY, and ERPX_MINIO_REGION must be provided together",
            ),
            Self::Invalid => formatter.write_str("ERPX_MINIO object-store configuration is invalid"),
            Self::Required => formatter.write_str(
                "ERPX_MINIO_REQUIRED=true requires a complete ERPX_MINIO object-store configuration",
            ),
            Self::InvalidValue(name) => write!(formatter, "{name} contains an invalid value"),
        }
    }
}

impl std::error::Error for StorageConfigurationError {}

#[derive(Debug, Clone, Default)]
struct StorageConfigValues {
    endpoint: Option<String>,
    access_key: Option<String>,
    secret_key: Option<String>,
    region: Option<String>,
    ca_certificate_path: Option<String>,
    required: bool,
}

impl StorageConfigValues {
    fn from_env() -> Result<Self, StorageConfigurationError> {
        Ok(Self {
            endpoint: read_env(ENDPOINT_ENV)?,
            access_key: read_env(ACCESS_KEY_ENV)?,
            secret_key: read_env(SECRET_KEY_ENV)?,
            region: read_env(REGION_ENV)?,
            ca_certificate_path: read_env(CA_CERTIFICATE_PATH_ENV)?,
            required: match read_env(REQUIRED_ENV)?.as_deref() {
                None | Some("false") => false,
                Some("true") => true,
                Some(_) => return Err(StorageConfigurationError::InvalidValue(REQUIRED_ENV)),
            },
        })
    }

    fn is_empty(&self) -> bool {
        self.endpoint.is_none()
            && self.access_key.is_none()
            && self.secret_key.is_none()
            && self.region.is_none()
            && self.ca_certificate_path.is_none()
    }

    fn into_store(self) -> Result<Option<Arc<dyn ObjectStoreBoundary>>, StorageConfigurationError> {
        if self.is_empty() {
            if self.required {
                return Err(StorageConfigurationError::Required);
            }
            return Ok(None);
        }
        let (Some(endpoint), Some(access_key), Some(secret_key), Some(region)) =
            (self.endpoint, self.access_key, self.secret_key, self.region)
        else {
            return Err(StorageConfigurationError::Incomplete);
        };
        let ca_certificate = self
            .ca_certificate_path
            .map(|path| {
                let metadata =
                    fs::metadata(&path).map_err(|_| StorageConfigurationError::Invalid)?;
                if !metadata.is_file()
                    || metadata.len() == 0
                    || metadata.len() > MAX_CA_CERTIFICATE_BYTES
                {
                    return Err(StorageConfigurationError::Invalid);
                }
                fs::read(path).map_err(|_| StorageConfigurationError::Invalid)
            })
            .transpose()?;
        let store = MinioObjectStore::new_with_ca_certificate(
            MinioObjectStoreConfig {
                endpoint,
                access_key,
                secret_key,
                region,
            },
            ca_certificate.as_deref(),
        )
        .map_err(|_| StorageConfigurationError::Invalid)?;
        Ok(Some(Arc::new(store)))
    }
}

fn read_env(name: &'static str) -> Result<Option<String>, StorageConfigurationError> {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => Ok(Some(value)),
        Ok(_) | Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => Err(StorageConfigurationError::InvalidValue(name)),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct StorageHealthSignal {
    configured: bool,
    required: bool,
    status: &'static str,
    live_connectivity: &'static str,
    namespace_verification: &'static str,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StorageProbeState {
    Disabled = 0,
    Unprobed = 1,
    Ready = 2,
    Failed = 3,
}

impl StorageProbeState {
    fn from_raw(value: u8) -> Self {
        match value {
            value if value == Self::Disabled as u8 => Self::Disabled,
            value if value == Self::Ready as u8 => Self::Ready,
            value if value == Self::Failed as u8 => Self::Failed,
            _ => Self::Unprobed,
        }
    }
}

#[derive(Clone)]
pub(crate) struct StorageComposition {
    object_store: Option<Arc<dyn ObjectStoreBoundary>>,
    required: bool,
    probe_state: Arc<AtomicU8>,
}

impl fmt::Debug for StorageComposition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StorageComposition")
            .field("health", &self.health_signal())
            .finish()
    }
}

impl StorageComposition {
    pub(crate) fn from_env() -> Result<Self, StorageConfigurationError> {
        Self::from_values(StorageConfigValues::from_env()?)
    }

    fn from_values(values: StorageConfigValues) -> Result<Self, StorageConfigurationError> {
        let required = values.required;
        let object_store = values.into_store()?;
        let initial_state = if object_store.is_some() {
            StorageProbeState::Unprobed
        } else {
            StorageProbeState::Disabled
        };
        Ok(Self {
            object_store,
            required,
            probe_state: Arc::new(AtomicU8::new(initial_state as u8)),
        })
    }

    #[cfg(test)]
    fn from_boundary(object_store: Arc<dyn ObjectStoreBoundary>) -> Self {
        Self::from_boundary_with_required(object_store, false)
    }

    #[cfg(test)]
    fn from_boundary_with_required(
        object_store: Arc<dyn ObjectStoreBoundary>,
        required: bool,
    ) -> Self {
        Self {
            object_store: Some(object_store),
            required,
            probe_state: Arc::new(AtomicU8::new(StorageProbeState::Unprobed as u8)),
        }
    }

    pub(crate) fn health_signal(&self) -> StorageHealthSignal {
        let state = StorageProbeState::from_raw(self.probe_state.load(Ordering::Acquire));
        match state {
            StorageProbeState::Disabled => StorageHealthSignal {
                configured: false,
                required: self.required,
                status: "disabled",
                live_connectivity: "not_configured",
                namespace_verification: "per_tenant",
            },
            StorageProbeState::Unprobed => StorageHealthSignal {
                configured: true,
                required: self.required,
                status: "unprobed",
                live_connectivity: "not_probed",
                namespace_verification: "per_tenant",
            },
            StorageProbeState::Ready => StorageHealthSignal {
                configured: true,
                required: self.required,
                status: "ready",
                live_connectivity: "verified",
                namespace_verification: "per_tenant",
            },
            StorageProbeState::Failed => StorageHealthSignal {
                configured: true,
                required: self.required,
                status: "unavailable",
                live_connectivity: "failed",
                namespace_verification: "per_tenant",
            },
        }
    }

    /// Return only categorical integration health for the authenticated
    /// Platform Operations read model. Endpoint, bucket, and credentials never
    /// cross this boundary.
    pub(crate) fn integration_health(&self) -> (&'static str, &'static str) {
        let signal = self.health_signal();
        // The durable integration observation contract has a deliberately
        // smaller state vocabulary than the detailed storage diagnostic. Map
        // disabled/unprobed into that vocabulary before persistence instead of
        // letting a healthy RPC fail a database CHECK constraint.
        let state = match signal.status {
            "disabled" => "not_configured",
            "unprobed" => "not_probed",
            value => value,
        };
        (state, signal.live_connectivity)
    }

    /// A disabled optional store does not block server readiness. A configured
    /// store is not ready until a live probe succeeds, including when storage
    /// is marked required.
    pub(crate) fn configuration_ready(&self) -> bool {
        match StorageProbeState::from_raw(self.probe_state.load(Ordering::Acquire)) {
            StorageProbeState::Disabled => !self.required,
            StorageProbeState::Ready => true,
            StorageProbeState::Unprobed | StorageProbeState::Failed => false,
        }
    }

    /// Probe the configured store without mutating object namespaces. The
    /// result is retained for readiness and health reporting; failures are
    /// intentionally represented only as an unavailable state.
    pub(crate) async fn refresh_readiness(&self) -> bool {
        let Some(object_store) = self.object_store.as_ref() else {
            self.probe_state
                .store(StorageProbeState::Disabled as u8, Ordering::Release);
            return !self.required;
        };
        match object_store.probe().await {
            Ok(()) => {
                self.probe_state
                    .store(StorageProbeState::Ready as u8, Ordering::Release);
                true
            }
            Err(_) => {
                self.probe_state
                    .store(StorageProbeState::Failed as u8, Ordering::Release);
                false
            }
        }
    }

    pub(crate) async fn ensure_namespace(&self, namespace: &str) -> Result<(), ObjectStorageError> {
        let object_store = self.object_store.as_ref().ok_or_else(|| {
            ObjectStorageError::InvalidConfiguration("object storage is not configured".into())
        })?;
        object_store.ensure_namespace(namespace).await
    }

    /// Validate and authorize a tenant upload before returning a bounded
    /// presigned URL. The authorization callback must validate the tenant and
    /// registered namespace; this composition layer never accepts credentials.
    pub(crate) async fn presign_upload_request<F, Fut>(
        &self,
        request: TenantObjectUploadRequest,
        authorize: F,
    ) -> Result<TenantSignedObjectUrl, ObjectAccessError>
    where
        F: FnOnce(&TenantObjectUploadRequest) -> Fut,
        Fut: Future<Output = Result<(), ObjectAuthorizationError>>,
    {
        validate_upload_request(&request)?;
        authorize(&request)
            .await
            .map_err(|_| ObjectAccessError::Unauthorized)?;
        let object_store = self
            .object_store
            .as_ref()
            .ok_or(ObjectAccessError::StorageUnavailable)?;
        let signed = object_store
            .presign_upload(
                &request.storage_namespace,
                &request.object_key,
                request.expires_seconds,
                Utc::now(),
            )
            .map_err(|_| ObjectAccessError::PresignFailed)?;
        Ok(TenantSignedObjectUrl::from(signed))
    }

    /// Validate and authorize a tenant download before returning a bounded
    /// presigned URL. Optional content metadata is policy input for the
    /// authoritative caller; the object-store URL itself carries no secrets.
    pub(crate) async fn presign_download_request<F, Fut>(
        &self,
        request: TenantObjectDownloadRequest,
        authorize: F,
    ) -> Result<TenantSignedObjectUrl, ObjectAccessError>
    where
        F: FnOnce(&TenantObjectDownloadRequest) -> Fut,
        Fut: Future<Output = Result<(), ObjectAuthorizationError>>,
    {
        validate_download_request(&request)?;
        authorize(&request)
            .await
            .map_err(|_| ObjectAccessError::Unauthorized)?;
        let object_store = self
            .object_store
            .as_ref()
            .ok_or(ObjectAccessError::StorageUnavailable)?;
        let signed = object_store
            .presign_download(
                &request.storage_namespace,
                &request.object_key,
                request.expires_seconds,
                Utc::now(),
            )
            .map_err(|_| ObjectAccessError::PresignFailed)?;
        Ok(TenantSignedObjectUrl::from(signed))
    }

    /// Registration seam for the existing provisioning gate. The server owns
    /// the object-store boundary; provisioning owns state and activation rules.
    #[allow(dead_code)]
    pub(crate) async fn provision_tenant_storage(
        &self,
        provisioner: &TenantDatabaseProvisioner,
        tenant_id: &str,
    ) -> Result<TenantStorageHealth, PersistenceError> {
        if self.object_store.is_none() {
            return Err(PersistenceError::Configuration(
                "object storage is not configured",
            ));
        }
        let composition = self.clone();
        provisioner
            .provision_storage_namespace(tenant_id, move |namespace| async move {
                composition.ensure_namespace(&namespace).await
            })
            .await
    }
}

impl From<SignedObjectUrl> for TenantSignedObjectUrl {
    fn from(value: SignedObjectUrl) -> Self {
        Self {
            url: value.url,
            method: value.method,
            expires_at: value.expires_at,
        }
    }
}

fn validate_upload_request(request: &TenantObjectUploadRequest) -> Result<(), ObjectAccessError> {
    validate_object_request(
        &request.tenant_id,
        &request.storage_namespace,
        &request.object_key,
        request.expires_seconds,
    )?;
    validate_content_type(&request.content_type)?;
    if request.content_length > MAX_UPLOAD_BYTES {
        return Err(ObjectAccessError::InvalidRequest(
            "upload exceeds the configured content-size limit",
        ));
    }
    Ok(())
}

fn validate_download_request(
    request: &TenantObjectDownloadRequest,
) -> Result<(), ObjectAccessError> {
    validate_object_request(
        &request.tenant_id,
        &request.storage_namespace,
        &request.object_key,
        request.expires_seconds,
    )?;
    if let Some(content_type) = &request.content_type {
        validate_content_type(content_type)?;
    }
    if request
        .max_bytes
        .is_some_and(|max_bytes| max_bytes == 0 || max_bytes > MAX_DOWNLOAD_BYTES)
    {
        return Err(ObjectAccessError::InvalidRequest(
            "download exceeds the configured content-size limit",
        ));
    }
    Ok(())
}

fn validate_object_request(
    tenant_id: &str,
    storage_namespace: &str,
    object_key: &str,
    expires_seconds: u32,
) -> Result<(), ObjectAccessError> {
    validate_opaque_reference(tenant_id, "tenant_id")?;
    validate_namespace(storage_namespace)?;
    if object_key.is_empty()
        || object_key.len() > 1024
        || object_key.chars().any(char::is_control)
        || object_key.split('/').any(|segment| segment == "..")
    {
        return Err(ObjectAccessError::InvalidRequest(
            "object key is empty, too long, unsafe, or contains parent traversal",
        ));
    }
    if !(1..=MAX_PRESIGN_EXPIRY_SECONDS).contains(&expires_seconds) {
        return Err(ObjectAccessError::InvalidRequest(
            "presigned URL expiry is outside the server policy",
        ));
    }
    Ok(())
}

fn validate_opaque_reference(value: &str, field: &'static str) -> Result<(), ObjectAccessError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':' | ' '))
    {
        return Err(ObjectAccessError::InvalidRequest(field));
    }
    Ok(())
}

fn validate_namespace(value: &str) -> Result<(), ObjectAccessError> {
    if !(3..=63).contains(&value.len())
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || !value.as_bytes()[0].is_ascii_alphanumeric()
        || !value.as_bytes()[value.len() - 1].is_ascii_alphanumeric()
    {
        return Err(ObjectAccessError::InvalidRequest("storage_namespace"));
    }
    Ok(())
}

fn validate_content_type(value: &str) -> Result<(), ObjectAccessError> {
    if value.trim() != value
        || value.is_empty()
        || value.len() > 255
        || value.chars().any(char::is_control)
    {
        return Err(ObjectAccessError::InvalidRequest("content_type"));
    }
    let media_type = value.split(';').next().unwrap_or_default().trim();
    let Some((major, minor)) = media_type.split_once('/') else {
        return Err(ObjectAccessError::InvalidRequest("content_type"));
    };
    if major.is_empty()
        || minor.is_empty()
        || major.chars().chain(minor.chars()).any(|character| {
            !(character.is_ascii_alphanumeric()
                || matches!(
                    character,
                    '!' | '#' | '$' | '&' | '^' | '_' | '.' | '+' | '-'
                ))
        })
    {
        return Err(ObjectAccessError::InvalidRequest("content_type"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    };

    #[derive(Debug)]
    struct FakeObjectStore {
        namespaces: Mutex<Vec<String>>,
        probe_succeeds: Arc<AtomicBool>,
    }

    impl Default for FakeObjectStore {
        fn default() -> Self {
            Self {
                namespaces: Mutex::default(),
                probe_succeeds: Arc::new(AtomicBool::new(true)),
            }
        }
    }

    impl ObjectStoreBoundary for FakeObjectStore {
        fn ensure_namespace<'a>(&'a self, namespace: &'a str) -> NamespaceFuture<'a> {
            Box::pin(async move {
                self.namespaces
                    .lock()
                    .expect("fake lock")
                    .push(namespace.to_owned());
                Ok(())
            })
        }

        fn probe(&self) -> NamespaceFuture<'_> {
            let succeeds = self.probe_succeeds.load(Ordering::Relaxed);
            Box::pin(async move {
                if succeeds {
                    Ok(())
                } else {
                    Err(ObjectStorageError::HttpStatus { status: 503 })
                }
            })
        }

        fn presign_upload(
            &self,
            namespace: &str,
            object_key: &str,
            expires_seconds: u32,
            at: DateTime<Utc>,
        ) -> Result<SignedObjectUrl, ObjectStorageError> {
            Ok(SignedObjectUrl {
                url: format!(
                    "https://objects.test/{namespace}/{object_key}?expires={expires_seconds}"
                ),
                method: "PUT",
                expires_at: at + chrono::Duration::seconds(i64::from(expires_seconds)),
            })
        }

        fn presign_download(
            &self,
            namespace: &str,
            object_key: &str,
            expires_seconds: u32,
            at: DateTime<Utc>,
        ) -> Result<SignedObjectUrl, ObjectStorageError> {
            Ok(SignedObjectUrl {
                url: format!(
                    "https://objects.test/{namespace}/{object_key}?expires={expires_seconds}"
                ),
                method: "GET",
                expires_at: at + chrono::Duration::seconds(i64::from(expires_seconds)),
            })
        }
    }

    fn upload_request() -> TenantObjectUploadRequest {
        TenantObjectUploadRequest {
            tenant_id: "tenant-a".into(),
            storage_namespace: "tenant-a-storage".into(),
            object_key: "documents/report.pdf".into(),
            expires_seconds: 300,
            content_type: "application/pdf".into(),
            content_length: 1024,
        }
    }

    fn download_request() -> TenantObjectDownloadRequest {
        TenantObjectDownloadRequest {
            tenant_id: "tenant-a".into(),
            storage_namespace: "tenant-a-storage".into(),
            object_key: "documents/report.pdf".into(),
            expires_seconds: 300,
            content_type: Some("application/pdf".into()),
            max_bytes: Some(1024),
        }
    }

    #[test]
    fn absent_configuration_is_optional_and_redacted() {
        let composition = StorageComposition::from_values(StorageConfigValues::default())
            .expect("disabled storage should be valid");
        assert_eq!(
            composition.health_signal(),
            StorageHealthSignal {
                configured: false,
                required: false,
                status: "disabled",
                live_connectivity: "not_configured",
                namespace_verification: "per_tenant",
            }
        );
        assert!(composition.configuration_ready());
        assert_eq!(composition.integration_health().0, "not_configured");
        let debug = format!("{composition:?}");
        assert!(!debug.contains("access"));
        assert!(!debug.contains("secret"));
    }

    #[test]
    fn partial_configuration_fails_closed_without_echoing_values() {
        let error = StorageComposition::from_values(StorageConfigValues {
            endpoint: Some("https://127.0.0.1:9000".into()),
            access_key: Some("access-secret".into()),
            ..StorageConfigValues::default()
        })
        .expect_err("partial storage configuration must fail");
        assert_eq!(error, StorageConfigurationError::Incomplete);
        let debug = format!("{error:?}");
        assert!(!debug.contains("access-secret"));
    }

    #[test]
    fn configured_health_does_not_claim_live_connectivity() {
        let composition = StorageComposition::from_values(StorageConfigValues {
            endpoint: Some("https://127.0.0.1:9000".into()),
            access_key: Some("access-secret".into()),
            secret_key: Some("private-secret".into()),
            region: Some("us-east-1".into()),
            required: false,
            ca_certificate_path: None,
        })
        .expect("valid storage configuration");
        assert_eq!(composition.health_signal().status, "unprobed");
        assert!(!composition.health_signal().required);
        assert_eq!(composition.health_signal().live_connectivity, "not_probed");
        assert_eq!(composition.integration_health().0, "not_probed");
        assert!(!composition.configuration_ready());
        let debug = format!("{composition:?}");
        assert!(!debug.contains("access-secret"));
        assert!(!debug.contains("private-secret"));
    }

    #[test]
    fn required_configuration_fails_closed_when_absent() {
        let error = StorageComposition::from_values(StorageConfigValues {
            required: true,
            ..StorageConfigValues::default()
        })
        .expect_err("required storage must be configured");
        assert_eq!(error, StorageConfigurationError::Required);
    }

    #[test]
    fn invalid_configuration_fails_without_echoing_credentials() {
        let error = StorageComposition::from_values(StorageConfigValues {
            endpoint: Some("not-an-endpoint".into()),
            access_key: Some("access-secret".into()),
            secret_key: Some("private-secret".into()),
            region: Some("us-east-1".into()),
            required: false,
            ca_certificate_path: None,
        })
        .expect_err("invalid endpoint must fail startup validation");
        assert_eq!(error, StorageConfigurationError::Invalid);
        let debug = format!("{error:?}");
        assert!(!debug.contains("access-secret"));
        assert!(!debug.contains("private-secret"));
    }

    #[tokio::test]
    async fn injected_boundary_is_testable_without_live_credentials() {
        let fake = Arc::new(FakeObjectStore::default());
        let composition = StorageComposition::from_boundary(fake.clone());
        composition
            .ensure_namespace("tenant-storage-test")
            .await
            .expect("fake namespace operation");
        assert_eq!(
            fake.namespaces.lock().expect("fake lock").as_slice(),
            ["tenant-storage-test"]
        );
    }

    #[test]
    fn required_configured_storage_is_not_ready_before_probe() {
        let composition = StorageComposition::from_values(StorageConfigValues {
            endpoint: Some("https://127.0.0.1:9000".into()),
            access_key: Some("access-secret".into()),
            secret_key: Some("private-secret".into()),
            region: Some("us-east-1".into()),
            required: true,
            ca_certificate_path: None,
        })
        .expect("valid storage configuration");
        assert_eq!(composition.health_signal().status, "unprobed");
        assert!(!composition.configuration_ready());
    }

    #[tokio::test]
    async fn injected_probe_transitions_failure_to_ready_without_credentials() {
        let probe_succeeds = Arc::new(AtomicBool::new(false));
        let fake = Arc::new(FakeObjectStore {
            namespaces: Mutex::default(),
            probe_succeeds: probe_succeeds.clone(),
        });
        let composition = StorageComposition::from_boundary_with_required(fake, true);

        assert!(!composition.refresh_readiness().await);
        assert_eq!(composition.health_signal().status, "unavailable");
        assert!(!composition.configuration_ready());

        probe_succeeds.store(true, Ordering::Relaxed);
        assert!(composition.refresh_readiness().await);
        assert_eq!(composition.health_signal().status, "ready");
        assert!(composition.configuration_ready());
    }

    #[tokio::test]
    async fn object_access_rejects_traversal_and_cross_tenant_authorization() {
        let composition = StorageComposition::from_boundary(Arc::new(FakeObjectStore::default()));
        let mut traversal = download_request();
        traversal.object_key = "documents/../secret".into();
        assert_eq!(
            composition
                .presign_download_request(traversal, |_| async { Ok(()) })
                .await
                .expect_err("traversal must fail"),
            ObjectAccessError::InvalidRequest(
                "object key is empty, too long, unsafe, or contains parent traversal"
            )
        );

        let mut cross_tenant = download_request();
        cross_tenant.tenant_id = "tenant-b".into();
        let result = composition
            .presign_download_request(cross_tenant, |request| {
                let allowed = request.tenant_id == "tenant-a"
                    && request.storage_namespace == "tenant-a-storage";
                async move {
                    if allowed {
                        Ok(())
                    } else {
                        Err(ObjectAuthorizationError::Denied)
                    }
                }
            })
            .await;
        assert_eq!(result, Err(ObjectAccessError::Unauthorized));
    }

    #[tokio::test]
    async fn object_access_enforces_expiry_content_and_size_policy() {
        let composition = StorageComposition::from_boundary(Arc::new(FakeObjectStore::default()));
        let mut upload = upload_request();
        upload.expires_seconds = MAX_PRESIGN_EXPIRY_SECONDS + 1;
        assert!(matches!(
            composition
                .presign_upload_request(upload, |_| async { Ok(()) })
                .await,
            Err(ObjectAccessError::InvalidRequest(
                "presigned URL expiry is outside the server policy"
            ))
        ));

        let mut upload = upload_request();
        upload.content_length = MAX_UPLOAD_BYTES + 1;
        assert!(matches!(
            composition
                .presign_upload_request(upload, |_| async { Ok(()) })
                .await,
            Err(ObjectAccessError::InvalidRequest(
                "upload exceeds the configured content-size limit"
            ))
        ));

        let mut download = download_request();
        download.content_type = Some("not-a-media-type".into());
        assert!(matches!(
            composition
                .presign_download_request(download, |_| async { Ok(()) })
                .await,
            Err(ObjectAccessError::InvalidRequest("content_type"))
        ));
    }

    #[tokio::test]
    async fn authorized_upload_and_download_return_only_bounded_signed_urls() {
        let composition = StorageComposition::from_boundary(Arc::new(FakeObjectStore::default()));
        let upload = composition
            .presign_upload_request(upload_request(), |_| async { Ok(()) })
            .await
            .expect("authorized upload");
        assert_eq!(upload.method, "PUT");
        assert!(upload.url.contains("tenant-a-storage/documents/report.pdf"));
        assert!(!upload.url.contains("secret"));

        let download = composition
            .presign_download_request(download_request(), |_| async { Ok(()) })
            .await
            .expect("authorized download");
        assert_eq!(download.method, "GET");
        assert!(
            download
                .url
                .contains("tenant-a-storage/documents/report.pdf")
        );
        assert!(!download.url.contains("secret"));
    }
}
