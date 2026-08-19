use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    time::Duration,
};

use chrono::{DateTime, Utc};
use erpx_domain::legal::SignedDownloadAuthorization;
use hmac::{Hmac, Mac};
use reqwest::{Certificate, Client, StatusCode, Url};
use sha2::{Digest, Sha256};
use thiserror::Error;

type HmacSha256 = Hmac<Sha256>;

const MAX_LEGAL_DOWNLOAD_BYTES: u64 = 64 * 1024 * 1024;
const MAX_LEGAL_DOWNLOAD_SECONDS: i64 = 7 * 24 * 60 * 60;
const MAX_OBJECT_INVENTORY_ENTRIES: u32 = 1_000;
const MAX_OBJECT_INVENTORY_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone)]
pub struct MinioObjectStoreConfig {
    pub endpoint: String,
    pub access_key: String,
    pub secret_key: String,
    pub region: String,
}

impl fmt::Debug for MinioObjectStoreConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MinioObjectStoreConfig")
            .field("endpoint", &self.endpoint)
            .field("access_key", &"REDACTED")
            .field("secret_key", &"REDACTED")
            .field("region", &self.region)
            .finish()
    }
}

#[derive(Debug, Error)]
pub enum ObjectStorageError {
    #[error("invalid object storage configuration: {0}")]
    InvalidConfiguration(String),
    #[error("invalid object storage reference: {0}")]
    InvalidReference(String),
    #[error("object storage URL error: {0}")]
    Url(#[from] reqwest::Error),
    #[error("object storage request failed with HTTP status {status}")]
    HttpStatus { status: u16 },
    #[error("object storage namespace already exists")]
    NamespaceAlreadyExists,
    #[error("object storage readiness probe timed out")]
    ProbeTimeout,
    #[error("object exceeds the configured size bound")]
    ObjectTooLarge,
    #[error("object inventory exceeds the configured entry bound")]
    InventoryTooLarge,
    #[error("object storage returned an invalid inventory response")]
    InvalidResponse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedObjectUrl {
    pub url: String,
    pub method: &'static str,
    pub expires_at: DateTime<Utc>,
}

/// A bounded, provider-neutral object inventory row. Object keys are retained
/// only inside the trusted storage boundary and are redacted from Debug.
#[derive(Clone, PartialEq, Eq)]
pub struct ObjectInventoryEntry {
    pub key: String,
    pub size_bytes: u64,
}

impl fmt::Debug for ObjectInventoryEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ObjectInventoryEntry")
            .field("key", &"<opaque>")
            .field("size_bytes", &self.size_bytes)
            .finish()
    }
}

#[derive(Clone)]
pub struct MinioObjectStore {
    config: MinioObjectStoreConfig,
    client: Client,
}

impl fmt::Debug for MinioObjectStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MinioObjectStore")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl MinioObjectStore {
    pub fn new(config: MinioObjectStoreConfig) -> Result<Self, ObjectStorageError> {
        Self::new_with_ca_certificate(config, None)
    }

    /// Construct an HTTPS-only object store with an optional trusted internal
    /// CA. The CA is public trust material, never an access credential; the
    /// endpoint is rejected unless it passes the operational HTTPS
    /// validation before a client can be constructed.
    pub fn new_with_ca_certificate(
        config: MinioObjectStoreConfig,
        ca_certificate_pem: Option<&[u8]>,
    ) -> Result<Self, ObjectStorageError> {
        let endpoint = Url::parse(config.endpoint.trim_end_matches('/'))
            .map_err(|_| ObjectStorageError::InvalidConfiguration("endpoint is invalid".into()))?;
        if endpoint.scheme() != "https"
            || endpoint.host_str().is_none()
            || config.access_key.trim().is_empty()
            || config.secret_key.trim().is_empty()
            || config.region.trim().is_empty()
        {
            let message = if endpoint.scheme() != "https" {
                "object storage requires an HTTPS endpoint"
            } else {
                "endpoint, credentials, and region are required"
            };
            return Err(ObjectStorageError::InvalidConfiguration(message.into()));
        }
        if !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(ObjectStorageError::InvalidConfiguration(
                "endpoint must not contain credentials or query parameters".into(),
            ));
        }
        let client = if let Some(ca_certificate_pem) = ca_certificate_pem {
            let certificate = Certificate::from_pem(ca_certificate_pem).map_err(|_| {
                ObjectStorageError::InvalidConfiguration("CA certificate is invalid".into())
            })?;
            Client::builder()
                .add_root_certificate(certificate)
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|_| {
                    ObjectStorageError::InvalidConfiguration("HTTP client is invalid".into())
                })?
        } else {
            Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|_| {
                    ObjectStorageError::InvalidConfiguration("HTTP client is invalid".into())
                })?
        };
        Ok(Self { config, client })
    }

    /// Create a bucket/namespace if it does not exist. An already-owned
    /// bucket response is treated as idempotent success.
    pub async fn ensure_namespace(&self, namespace: &str) -> Result<(), ObjectStorageError> {
        if self.namespace_exists(namespace).await? {
            return Ok(());
        }
        match self.create_namespace_exclusive(namespace).await {
            Ok(()) | Err(ObjectStorageError::NamespaceAlreadyExists) => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// Return whether an exact namespace already exists. Authorization and
    /// transport failures are never collapsed into `false`.
    pub async fn namespace_exists(&self, namespace: &str) -> Result<bool, ObjectStorageError> {
        validate_namespace(namespace)?;
        let now = Utc::now();
        let head = self
            .client
            .head(self.presigned_url("HEAD", namespace, None, 900, now)?.url)
            .send()
            .await?;
        if head.status().is_success() {
            return Ok(true);
        }
        if head.status() != StatusCode::NOT_FOUND {
            return Err(ObjectStorageError::HttpStatus {
                status: head.status().as_u16(),
            });
        }
        Ok(false)
    }

    /// Create one namespace and fail when it existed before this operation or
    /// was concurrently allocated. Fresh-state bootstrap uses this instead of
    /// the idempotent ensure path so an old namespace cannot be adopted.
    pub async fn create_namespace_exclusive(
        &self,
        namespace: &str,
    ) -> Result<(), ObjectStorageError> {
        validate_namespace(namespace)?;
        if self.namespace_exists(namespace).await? {
            return Err(ObjectStorageError::NamespaceAlreadyExists);
        }
        let now = Utc::now();
        let put = self
            .client
            .put(self.presigned_url("PUT", namespace, None, 900, now)?.url)
            .send()
            .await?;
        if put.status().is_success() {
            return Ok(());
        }
        if put.status() == StatusCode::CONFLICT {
            return Err(ObjectStorageError::NamespaceAlreadyExists);
        }
        Err(ObjectStorageError::HttpStatus {
            status: put.status().as_u16(),
        })
    }

    /// Probe the configured endpoint without creating or deleting a bucket.
    /// A missing probe bucket still proves that the endpoint answered a
    /// signed request; authorization failures and transport/server failures
    /// remain readiness failures.
    pub async fn probe(&self) -> Result<(), ObjectStorageError> {
        let response = tokio::time::timeout(
            Duration::from_secs(5),
            self.client
                .head(
                    self.presigned_url("HEAD", "erpx-readiness-probe", None, 15, Utc::now())?
                        .url,
                )
                .send(),
        )
        .await
        .map_err(|_| ObjectStorageError::ProbeTimeout)??;
        if response.status().is_success() || response.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        Err(ObjectStorageError::HttpStatus {
            status: response.status().as_u16(),
        })
    }

    /// Upload one bounded object through a short-lived presigned request.
    /// Callers remain responsible for holding authorization; this method only
    /// validates the storage-safe namespace/key and transfer bounds.
    pub async fn put_object_bounded(
        &self,
        namespace: &str,
        object_key: &str,
        body: &[u8],
        max_bytes: u64,
        timeout: Duration,
    ) -> Result<(), ObjectStorageError> {
        validate_namespace(namespace)?;
        validate_object_key(object_key)?;
        if max_bytes == 0 {
            return Err(ObjectStorageError::ObjectTooLarge);
        }
        let body_size =
            u64::try_from(body.len()).map_err(|_| ObjectStorageError::ObjectTooLarge)?;
        if body_size > max_bytes {
            return Err(ObjectStorageError::ObjectTooLarge);
        }
        let signed = self.presign_upload(namespace, object_key, 900, Utc::now())?;
        let response = tokio::time::timeout(
            timeout,
            self.client.put(signed.url).body(body.to_vec()).send(),
        )
        .await
        .map_err(|_| ObjectStorageError::ProbeTimeout)??;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(ObjectStorageError::HttpStatus {
                status: response.status().as_u16(),
            })
        }
    }

    /// Download one bounded object. The content length and every streamed
    /// chunk are checked before the body is returned to the caller.
    pub async fn get_object_bounded(
        &self,
        namespace: &str,
        object_key: &str,
        max_bytes: u64,
        timeout: Duration,
    ) -> Result<Vec<u8>, ObjectStorageError> {
        validate_namespace(namespace)?;
        validate_object_key(object_key)?;
        if max_bytes == 0 {
            return Err(ObjectStorageError::ObjectTooLarge);
        }
        let signed = self.presign_download(namespace, object_key, 900, Utc::now())?;
        let response = tokio::time::timeout(timeout, self.client.get(signed.url).send())
            .await
            .map_err(|_| ObjectStorageError::ProbeTimeout)??;
        if !response.status().is_success() {
            return Err(ObjectStorageError::HttpStatus {
                status: response.status().as_u16(),
            });
        }
        let declared_length = response.content_length();
        if declared_length.is_some_and(|size| size > max_bytes) {
            return Err(ObjectStorageError::ObjectTooLarge);
        }
        let mut body = Vec::new();
        let mut response = response;
        loop {
            let chunk = tokio::time::timeout(timeout, response.chunk())
                .await
                .map_err(|_| ObjectStorageError::ProbeTimeout)??;
            let Some(chunk) = chunk else { break };
            if u64::try_from(body.len().saturating_add(chunk.len()))
                .map_err(|_| ObjectStorageError::ObjectTooLarge)?
                > max_bytes
            {
                return Err(ObjectStorageError::ObjectTooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        validate_download_content_length(declared_length, body.len(), max_bytes)?;
        Ok(body)
    }

    /// Delete one holding-bound object through a short-lived presigned request.
    /// Missing objects are idempotent success so a retried retention sweep
    /// cannot recreate a deletion side effect.
    pub async fn delete_object_bounded(
        &self,
        namespace: &str,
        object_key: &str,
        timeout: Duration,
    ) -> Result<(), ObjectStorageError> {
        validate_namespace(namespace)?;
        validate_object_key(object_key)?;
        let signed = self.presigned_url("DELETE", namespace, Some(object_key), 900, Utc::now())?;
        let response = tokio::time::timeout(timeout, self.client.delete(signed.url).send())
            .await
            .map_err(|_| ObjectStorageError::ProbeTimeout)??;
        if response.status().is_success() || response.status() == StatusCode::NOT_FOUND {
            Ok(())
        } else {
            Err(ObjectStorageError::HttpStatus {
                status: response.status().as_u16(),
            })
        }
    }

    /// Read a bounded S3-compatible object inventory for one holding-bound
    /// prefix. A truncated response is rejected so callers never mistake a
    /// partial provider page for a complete reconciliation.
    pub async fn list_objects_bounded(
        &self,
        namespace: &str,
        prefix: &str,
        max_entries: u32,
        timeout: Duration,
    ) -> Result<Vec<ObjectInventoryEntry>, ObjectStorageError> {
        validate_namespace(namespace)?;
        if max_entries == 0 || max_entries > MAX_OBJECT_INVENTORY_ENTRIES {
            return Err(ObjectStorageError::InventoryTooLarge);
        }
        if !prefix.is_empty() {
            validate_object_key(prefix)?;
        }
        let mut provider_query = BTreeMap::new();
        provider_query.insert("list-type".into(), "2".into());
        provider_query.insert("max-keys".into(), max_entries.to_string());
        provider_query.insert("prefix".into(), prefix.into());
        let signed =
            self.presigned_url_with_query("GET", namespace, None, 900, Utc::now(), provider_query)?;
        let response = tokio::time::timeout(timeout, self.client.get(signed.url).send())
            .await
            .map_err(|_| ObjectStorageError::ProbeTimeout)??;
        if !response.status().is_success() {
            return Err(ObjectStorageError::HttpStatus {
                status: response.status().as_u16(),
            });
        }
        let body = read_bounded_inventory_response(response, timeout).await?;
        parse_inventory_response(&body, prefix, max_entries)
    }

    /// Validate a bucket name before constructing an adapter configuration.
    pub fn validate_namespace_reference(namespace: &str) -> Result<(), ObjectStorageError> {
        validate_namespace(namespace)
    }

    pub fn presign_upload(
        &self,
        namespace: &str,
        object_key: &str,
        expires_seconds: u32,
        at: DateTime<Utc>,
    ) -> Result<SignedObjectUrl, ObjectStorageError> {
        self.presigned_url("PUT", namespace, Some(object_key), expires_seconds, at)
    }

    pub fn presign_download(
        &self,
        namespace: &str,
        object_key: &str,
        expires_seconds: u32,
        at: DateTime<Utc>,
    ) -> Result<SignedObjectUrl, ObjectStorageError> {
        self.presigned_url("GET", namespace, Some(object_key), expires_seconds, at)
    }

    /// Convert an already-authorized, short-lived Legal download grant into a
    /// MinIO URL. The server remains responsible for identity, holding scope,
    /// RBAC, retention, and access-history recording; this adapter only
    /// enforces the grant's storage-safe bounds and never receives bytes or
    /// credentials from the caller.
    pub fn presign_legal_download(
        &self,
        namespace: &str,
        authorization: &SignedDownloadAuthorization,
        now: DateTime<Utc>,
    ) -> Result<SignedObjectUrl, ObjectStorageError> {
        if authorization.validate_at(now).is_err()
            || authorization.max_bytes > MAX_LEGAL_DOWNLOAD_BYTES
        {
            return Err(ObjectStorageError::InvalidReference(
                "legal download authorization is expired or invalid".into(),
            ));
        }
        let lifetime = authorization
            .expires_at
            .signed_duration_since(now)
            .num_seconds();
        if !(1..=MAX_LEGAL_DOWNLOAD_SECONDS).contains(&lifetime) {
            return Err(ObjectStorageError::InvalidReference(
                "legal download authorization lifetime is outside the storage bound".into(),
            ));
        }
        self.presign_download(
            namespace,
            &authorization.object_ref,
            u32::try_from(lifetime).map_err(|_| {
                ObjectStorageError::InvalidReference(
                    "legal download authorization lifetime is invalid".into(),
                )
            })?,
            now,
        )
    }

    fn presigned_url(
        &self,
        method: &'static str,
        namespace: &str,
        object_key: Option<&str>,
        expires_seconds: u32,
        at: DateTime<Utc>,
    ) -> Result<SignedObjectUrl, ObjectStorageError> {
        self.presigned_url_with_query(
            method,
            namespace,
            object_key,
            expires_seconds,
            at,
            BTreeMap::new(),
        )
    }

    fn presigned_url_with_query(
        &self,
        method: &'static str,
        namespace: &str,
        object_key: Option<&str>,
        expires_seconds: u32,
        at: DateTime<Utc>,
        provider_query: BTreeMap<String, String>,
    ) -> Result<SignedObjectUrl, ObjectStorageError> {
        let endpoint = validate_operational_endpoint(&self.config.endpoint)?;
        validate_namespace(namespace)?;
        if !(1..=604_800).contains(&expires_seconds) {
            return Err(ObjectStorageError::InvalidReference(
                "presigned URL lifetime must be between one second and seven days".into(),
            ));
        }
        if let Some(key) = object_key {
            validate_object_key(key)?;
        }
        let url = object_url(endpoint.as_str(), namespace, object_key)?;
        let host = url_host(&url)?;
        let short_date = at.format("%Y%m%d").to_string();
        let amz_date = at.format("%Y%m%dT%H%M%SZ").to_string();
        let credential = format!(
            "{}/{}/{}/s3/aws4_request",
            self.config.access_key, short_date, self.config.region
        );
        let mut query = provider_query;
        query.insert("X-Amz-Algorithm".into(), "AWS4-HMAC-SHA256".into());
        query.insert("X-Amz-Credential".into(), credential);
        query.insert("X-Amz-Date".into(), amz_date.clone());
        query.insert("X-Amz-Expires".into(), expires_seconds.to_string());
        query.insert("X-Amz-SignedHeaders".into(), "host".into());
        let initial_query = canonical_query(&query);
        let canonical_request = format!(
            "{method}\n{}\n{initial_query}\nhost:{host}\n\nhost\nUNSIGNED-PAYLOAD",
            url.path()
        );
        let scope = format!("{short_date}/{}/s3/aws4_request", self.config.region);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
            sha256_hex(canonical_request.as_bytes())
        );
        let signing_key = signing_key(&self.config.secret_key, &short_date, &self.config.region);
        query.insert(
            "X-Amz-Signature".into(),
            hex::encode(hmac_bytes(&signing_key, &string_to_sign)),
        );
        let mut signed_url = url;
        signed_url.set_query(Some(&canonical_query(&query)));
        Ok(SignedObjectUrl {
            url: signed_url.to_string(),
            method,
            expires_at: at + chrono::Duration::seconds(i64::from(expires_seconds)),
        })
    }
}

fn object_url(
    endpoint: &str,
    namespace: &str,
    object_key: Option<&str>,
) -> Result<Url, ObjectStorageError> {
    let mut value = endpoint.trim_end_matches('/').to_owned();
    value.push('/');
    value.push_str(&percent_encode(namespace));
    if let Some(key) = object_key {
        for segment in key.split('/') {
            value.push('/');
            value.push_str(&percent_encode(segment));
        }
    }
    Url::parse(&value)
        .map_err(|_| ObjectStorageError::InvalidConfiguration("object URL is invalid".into()))
}

fn validate_operational_endpoint(endpoint: &str) -> Result<Url, ObjectStorageError> {
    let endpoint = Url::parse(endpoint.trim_end_matches('/'))
        .map_err(|_| ObjectStorageError::InvalidConfiguration("endpoint is invalid".into()))?;
    if endpoint.scheme() != "https" {
        return Err(ObjectStorageError::InvalidConfiguration(
            "object storage requires an HTTPS endpoint".into(),
        ));
    }
    if endpoint.host_str().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(ObjectStorageError::InvalidConfiguration(
            "endpoint is invalid".into(),
        ));
    }
    Ok(endpoint)
}

fn url_host(url: &Url) -> Result<String, ObjectStorageError> {
    let host = url.host_str().ok_or_else(|| {
        ObjectStorageError::InvalidConfiguration("endpoint host is missing".into())
    })?;
    Ok(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.into(),
    })
}

fn canonical_query(query: &BTreeMap<String, String>) -> String {
    query
        .iter()
        .map(|(key, value)| format!("{}={}", percent_encode(key), percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn signing_key(secret: &str, date: &str, region: &str) -> [u8; 32] {
    let first = hmac_bytes(format!("AWS4{secret}").as_bytes(), date);
    let second = hmac_bytes(&first, region);
    let third = hmac_bytes(&second, "s3");
    hmac_bytes(&third, "aws4_request")
}

fn hmac_bytes(key: &[u8], value: &str) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts every key length");
    mac.update(value.as_bytes());
    let mut bytes = [0_u8; 32];
    bytes.copy_from_slice(&mac.finalize().into_bytes());
    bytes
}

fn sha256_hex(value: &[u8]) -> String {
    hex::encode(Sha256::digest(value))
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }
    encoded
}

fn parse_inventory_response(
    body: &str,
    prefix: &str,
    max_entries: u32,
) -> Result<Vec<ObjectInventoryEntry>, ObjectStorageError> {
    if !body.contains("<ListBucketResult") {
        return Err(ObjectStorageError::InvalidResponse);
    }
    match xml_tag_value(body, "IsTruncated").map(str::trim) {
        Some("false") => {}
        Some("true") => return Err(ObjectStorageError::InventoryTooLarge),
        _ => return Err(ObjectStorageError::InvalidResponse),
    }
    let mut entries = Vec::new();
    let mut keys = BTreeSet::new();
    for contents in body.split("<Contents>").skip(1) {
        let key = xml_tag_value(contents, "Key")
            .and_then(xml_unescape)
            .ok_or(ObjectStorageError::InvalidResponse)?;
        let size_bytes = xml_tag_value(contents, "Size")
            .ok_or(ObjectStorageError::InvalidResponse)?
            .parse::<u64>()
            .map_err(|_| ObjectStorageError::InvalidResponse)?;
        if !key.starts_with(prefix) {
            return Err(ObjectStorageError::InvalidResponse);
        }
        validate_object_key(&key)?;
        if !keys.insert(key.clone()) {
            return Err(ObjectStorageError::InvalidResponse);
        }
        entries.push(ObjectInventoryEntry { key, size_bytes });
        if entries.len() > usize::try_from(max_entries).unwrap_or(usize::MAX) {
            return Err(ObjectStorageError::InventoryTooLarge);
        }
    }
    Ok(entries)
}

async fn read_bounded_inventory_response(
    mut response: reqwest::Response,
    timeout: Duration,
) -> Result<String, ObjectStorageError> {
    let mut body = Vec::new();
    loop {
        let chunk = tokio::time::timeout(timeout, response.chunk())
            .await
            .map_err(|_| ObjectStorageError::ProbeTimeout)??;
        let Some(chunk) = chunk else { break };
        let new_len = body
            .len()
            .checked_add(chunk.len())
            .ok_or(ObjectStorageError::ObjectTooLarge)?;
        if new_len > MAX_OBJECT_INVENTORY_RESPONSE_BYTES {
            return Err(ObjectStorageError::ObjectTooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    String::from_utf8(body).map_err(|_| ObjectStorageError::InvalidResponse)
}

fn validate_download_content_length(
    declared_length: Option<u64>,
    actual_length: usize,
    max_bytes: u64,
) -> Result<(), ObjectStorageError> {
    let actual_length =
        u64::try_from(actual_length).map_err(|_| ObjectStorageError::ObjectTooLarge)?;
    if actual_length > max_bytes {
        return Err(ObjectStorageError::ObjectTooLarge);
    }
    if declared_length.is_some_and(|declared| declared != actual_length) {
        return Err(ObjectStorageError::InvalidResponse);
    }
    Ok(())
}

fn xml_tag_value<'a>(value: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = value.find(&open)? + open.len();
    let end = value[start..].find(&close)? + start;
    Some(&value[start..end])
}

fn xml_unescape(value: &str) -> Option<String> {
    let mut result = String::with_capacity(value.len());
    let mut remainder = value;
    while let Some((before, after)) = remainder.split_once('&') {
        result.push_str(before);
        let (entity, tail) = after.split_once(';')?;
        match entity {
            "amp" => result.push('&'),
            "lt" => result.push('<'),
            "gt" => result.push('>'),
            "quot" => result.push('"'),
            "apos" => result.push('\''),
            _ => return None,
        }
        remainder = tail;
    }
    result.push_str(remainder);
    Some(result)
}

fn validate_namespace(value: &str) -> Result<(), ObjectStorageError> {
    if !(3..=63).contains(&value.len())
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || !value.as_bytes()[0].is_ascii_alphanumeric()
        || !value.as_bytes()[value.len() - 1].is_ascii_alphanumeric()
    {
        return Err(ObjectStorageError::InvalidReference(
            "namespace must be a lowercase S3-compatible bucket name".into(),
        ));
    }
    Ok(())
}

fn validate_object_key(value: &str) -> Result<(), ObjectStorageError> {
    if value.is_empty()
        || value.len() > 1024
        || value.chars().any(char::is_control)
        || value.split('/').any(|segment| segment == "..")
    {
        return Err(ObjectStorageError::InvalidReference(
            "object key is empty, too long, unsafe, or contains a parent traversal".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use erpx_domain::{
        contracts::{ActorAttribution, ActorKind, AuthorityScope, CommandContext},
        legal::{LegalScope, SignedDownloadAuthorization},
    };
    use uuid::Uuid;

    fn store() -> MinioObjectStore {
        MinioObjectStore::new(MinioObjectStoreConfig {
            endpoint: "https://127.0.0.1:9000".into(),
            access_key: "minio-test".into(),
            secret_key: "minio-secret".into(),
            region: "us-east-1".into(),
        })
        .expect("store")
    }

    #[test]
    fn presigned_urls_are_bounded_and_bind_method_namespace_and_object() {
        let at = Utc
            .with_ymd_and_hms(2026, 7, 24, 12, 0, 0)
            .single()
            .expect("fixed timestamp");
        let put = store()
            .presign_upload("holding-abc", "documents/a b.txt", 300, at)
            .expect("put URL");
        assert_eq!(put.method, "PUT");
        assert_eq!(put.expires_at, at + chrono::Duration::seconds(300));
        assert!(put.url.contains("X-Amz-Signature="));
        assert!(put.url.contains("/holding-abc/documents/a%20b.txt"));
        let get = store()
            .presign_download("holding-abc", "documents/a b.txt", 300, at)
            .expect("get URL");
        assert_ne!(put.url, get.url);
        assert!(
            store()
                .presign_upload("holding-abc", "../secret", 300, at)
                .is_err()
        );
        assert!(
            store()
                .presign_upload("Holding-abc", "safe", 300, at)
                .is_err()
        );
        assert!(
            store()
                .presign_upload("holding-abc", "safe", 604_801, at)
                .is_err()
        );
    }

    #[test]
    fn configuration_debug_redacts_credentials() {
        let debug = format!(
            "{:?}",
            MinioObjectStoreConfig {
                endpoint: "https://127.0.0.1:9000".into(),
                access_key: "access-secret".into(),
                secret_key: "private-secret".into(),
                region: "us-east-1".into(),
            }
        );
        assert!(!debug.contains("access-secret"));
        assert!(!debug.contains("private-secret"));
        assert!(debug.contains("REDACTED"));
    }

    #[test]
    fn signed_urls_never_contain_the_secret_key() {
        let at = Utc
            .with_ymd_and_hms(2026, 7, 24, 12, 0, 0)
            .single()
            .expect("fixed timestamp");
        let upload = store()
            .presign_upload("holding-abc", "documents/report.pdf", 300, at)
            .expect("upload URL");
        let download = store()
            .presign_download("holding-abc", "documents/report.pdf", 300, at)
            .expect("download URL");
        assert!(!upload.url.contains("minio-secret"));
        assert!(!download.url.contains("minio-secret"));
        assert!(upload.url.contains("X-Amz-Credential=minio-test"));
    }

    #[test]
    fn inventory_parser_rejects_partial_pages_and_preserves_bounded_rows() {
        let body = r#"
            <ListBucketResult>
              <Contents><Key>holdings/holding-a/backup-chains/chain-a/artifacts/a.dump</Key><Size>42</Size></Contents>
              <Contents><Key>holdings/holding-a/backup-chains/chain-a/artifacts/b.dump</Key><Size>84</Size></Contents>
              <IsTruncated>false</IsTruncated>
            </ListBucketResult>
        "#;
        let entries = parse_inventory_response(
            body,
            "holdings/holding-a/backup-chains/chain-a/artifacts/",
            2,
        )
        .expect("inventory");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].size_bytes, 42);
        assert!(
            parse_inventory_response(
                &body.replace("false", "true"),
                "holdings/holding-a/backup-chains/chain-a/artifacts/",
                2,
            )
            .is_err()
        );
        assert!(
            parse_inventory_response(
                &body.replace(
                    "holdings/holding-a/backup-chains/chain-a/artifacts/b.dump",
                    "holdings/holding-a/backup-chains/chain-a/artifacts/a.dump",
                ),
                "holdings/holding-a/backup-chains/chain-a/artifacts/",
                2,
            )
            .is_err()
        );
        assert!(matches!(
            parse_inventory_response(
                &body.replace("<IsTruncated>false</IsTruncated>", ""),
                "holdings/holding-a/backup-chains/chain-a/artifacts/",
                2,
            ),
            Err(ObjectStorageError::InvalidResponse)
        ));
        assert!(matches!(
            parse_inventory_response(
                "<NotAnInventory><IsTruncated>false</IsTruncated></NotAnInventory>",
                "holdings/holding-a/backup-chains/chain-a/artifacts/",
                2,
            ),
            Err(ObjectStorageError::InvalidResponse)
        ));
    }

    #[test]
    fn bounded_download_reconciles_declared_and_received_lengths() {
        assert!(validate_download_content_length(Some(4), 4, 8).is_ok());
        assert!(matches!(
            validate_download_content_length(Some(4), 3, 8),
            Err(ObjectStorageError::InvalidResponse)
        ));
        assert!(matches!(
            validate_download_content_length(Some(9), 9, 8),
            Err(ObjectStorageError::ObjectTooLarge)
        ));
        assert!(validate_download_content_length(None, 4, 8).is_ok());
    }

    #[tokio::test]
    async fn bounded_transfers_reject_zero_byte_budget_before_network_io() {
        assert!(matches!(
            store()
                .put_object_bounded(
                    "holding-abc",
                    "artifact.bin",
                    b"payload",
                    0,
                    Duration::from_secs(1),
                )
                .await,
            Err(ObjectStorageError::ObjectTooLarge)
        ));
        assert!(matches!(
            store()
                .get_object_bounded("holding-abc", "artifact.bin", 0, Duration::from_secs(1),)
                .await,
            Err(ObjectStorageError::ObjectTooLarge)
        ));
    }

    fn legal_authorization(
        expires_at: DateTime<Utc>,
        max_bytes: u64,
    ) -> SignedDownloadAuthorization {
        let holding_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let scope = LegalScope::new(holding_id, company_id, None).expect("scope");
        let context = CommandContext::try_new(
            "legal-download-test",
            ActorAttribution {
                principal_ref: "principal:legal-reader".into(),
                kind: ActorKind::HoldingUser,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            AuthorityScope::Company {
                holding_id,
                company_id,
            },
            "download legal document",
        )
        .expect("context");
        SignedDownloadAuthorization::new(
            Uuid::new_v4(),
            1,
            "legal/documents/contract.pdf",
            scope,
            "contract review",
            expires_at,
            max_bytes,
            &context,
        )
        .expect("authorization")
    }

    #[test]
    fn legal_download_presign_enforces_expiry_size_and_opaque_key() {
        let now = Utc::now() + chrono::Duration::minutes(1);
        let authorization = legal_authorization(now + chrono::Duration::minutes(10), 1024);
        let signed = store()
            .presign_legal_download("holding-abc", &authorization, now)
            .expect("signed legal download");
        assert_eq!(signed.method, "GET");
        assert!(
            signed
                .url
                .contains("/holding-abc/legal/documents/contract.pdf")
        );

        let expired = legal_authorization(now, 1024);
        assert!(
            store()
                .presign_legal_download("holding-abc", &expired, now)
                .is_err()
        );
        let oversized = legal_authorization(
            now + chrono::Duration::minutes(10),
            MAX_LEGAL_DOWNLOAD_BYTES + 1,
        );
        assert!(
            store()
                .presign_legal_download("holding-abc", &oversized, now)
                .is_err()
        );
        let traversal = SignedDownloadAuthorization {
            object_ref: "legal/../secret".into(),
            ..authorization.clone()
        };
        assert!(
            store()
                .presign_legal_download("holding-abc", &traversal, now)
                .is_err()
        );

        let invalid_actor = SignedDownloadAuthorization {
            actor_ref: String::new(),
            ..authorization.clone()
        };
        let error = store()
            .presign_legal_download("holding-abc", &invalid_actor, now)
            .expect_err("presign must revalidate actor metadata");
        assert_eq!(
            error.to_string(),
            "invalid object storage reference: legal download authorization is expired or invalid"
        );

        let invalid_scope = SignedDownloadAuthorization {
            scope: LegalScope {
                holding_id: Uuid::nil(),
                ..invalid_actor.scope.clone()
            },
            ..invalid_actor
        };
        assert!(
            store()
                .presign_legal_download("holding-abc", &invalid_scope, now)
                .is_err()
        );
    }
}
