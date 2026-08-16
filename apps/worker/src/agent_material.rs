//! Encrypted Agent prompt/output material boundary.
//!
//! This module intentionally defines a consumer-owned KMS port instead of
//! embedding a local encryption key. Production composition cannot construct
//! this adapter until a real envelope-key manager is injected.

use std::{fmt, time::Duration as StdDuration};

use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use erpx_agent_orchestration::{
    AgentMaterialRetentionPort, Assignment, ClaimedAssignment, PortError,
};
use erpx_infrastructure::{MinioObjectStore, ObjectStorageError};
use erpx_kernel::Clock;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::ProtectedAgentMaterialPort;

const REFERENCE_PREFIX: &str = "agent-material:v1:";
const MAX_REFERENCE_BYTES: usize = 128;
const MAX_PLAINTEXT_BYTES: usize = 1024 * 1024;
const MAX_ENVELOPE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_STORAGE_TIMEOUT: StdDuration = StdDuration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEnvelopeCiphertext {
    pub key_version: String,
    pub ciphertext: Vec<u8>,
}

/// Real implementations must provide authenticated encryption and bind the
/// exact AAD. The worker never receives or stores a plaintext data key.
#[async_trait]
pub trait AgentEnvelopeKeyManagerPort: Send + Sync {
    async fn encrypt(
        &self,
        key_ref: &str,
        aad: &[u8],
        plaintext: Zeroizing<Vec<u8>>,
    ) -> Result<AgentEnvelopeCiphertext, PortError>;

    async fn decrypt(
        &self,
        key_ref: &str,
        key_version: &str,
        aad: &[u8],
        ciphertext: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, PortError>;
}

#[async_trait]
pub trait AgentEncryptedObjectPort: Send + Sync {
    async fn put_envelope(
        &self,
        namespace: &str,
        object_key: &str,
        envelope: &[u8],
        timeout: StdDuration,
    ) -> Result<(), PortError>;

    async fn get_envelope(
        &self,
        namespace: &str,
        object_key: &str,
        max_bytes: u64,
        timeout: StdDuration,
    ) -> Result<Vec<u8>, PortError>;
}

#[async_trait]
impl AgentEncryptedObjectPort for MinioObjectStore {
    async fn put_envelope(
        &self,
        namespace: &str,
        object_key: &str,
        envelope: &[u8],
        timeout: StdDuration,
    ) -> Result<(), PortError> {
        self.put_object_bounded(namespace, object_key, envelope, MAX_ENVELOPE_BYTES, timeout)
            .await
            .map_err(object_error)
    }

    async fn get_envelope(
        &self,
        namespace: &str,
        object_key: &str,
        max_bytes: u64,
        timeout: StdDuration,
    ) -> Result<Vec<u8>, PortError> {
        self.get_object_bounded(namespace, object_key, max_bytes, timeout)
            .await
            .map_err(object_error)
    }
}

fn object_error(error: ObjectStorageError) -> PortError {
    match error {
        ObjectStorageError::ProbeTimeout
        | ObjectStorageError::Url(_)
        | ObjectStorageError::HttpStatus { .. } => PortError::Persistence,
        _ => PortError::ProtectedMaterialUnavailable,
    }
}

#[derive(Clone)]
pub struct AgentProtectedMaterialConfig {
    tenant_id: Uuid,
    namespace: String,
    key_ref: String,
    max_plaintext_bytes: usize,
    storage_timeout: StdDuration,
}

impl fmt::Debug for AgentProtectedMaterialConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentProtectedMaterialConfig")
            .field("tenant_id", &self.tenant_id)
            .field("namespace", &"[OPAQUE]")
            .field("key_ref", &"[REDACTED]")
            .field("max_plaintext_bytes", &self.max_plaintext_bytes)
            .field("storage_timeout", &self.storage_timeout)
            .finish()
    }
}

impl AgentProtectedMaterialConfig {
    pub fn try_new(
        tenant_id: Uuid,
        namespace: String,
        key_ref: String,
        max_plaintext_bytes: usize,
        storage_timeout: StdDuration,
    ) -> Result<Self, PortError> {
        if tenant_id.is_nil()
            || MinioObjectStore::validate_namespace_reference(&namespace).is_err()
            || !safe_ref(&key_ref, 512)
            || max_plaintext_bytes == 0
            || max_plaintext_bytes > MAX_PLAINTEXT_BYTES
            || storage_timeout.is_zero()
            || storage_timeout > MAX_STORAGE_TIMEOUT
        {
            return Err(PortError::ProtectedMaterialUnavailable);
        }
        Ok(Self {
            tenant_id,
            namespace,
            key_ref,
            max_plaintext_bytes,
            storage_timeout,
        })
    }

    pub const fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }
}

pub struct GovernedAgentProtectedMaterial<O, K, R, C> {
    objects: O,
    keys: K,
    retention: R,
    clock: C,
    config: AgentProtectedMaterialConfig,
}

impl<O, K, R, C> GovernedAgentProtectedMaterial<O, K, R, C> {
    pub fn new(
        objects: O,
        keys: K,
        retention: R,
        clock: C,
        config: AgentProtectedMaterialConfig,
    ) -> Self {
        Self {
            objects,
            keys,
            retention,
            clock,
            config,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MaterialKind {
    Prompt,
    Output,
}

#[derive(Debug, Serialize)]
struct MaterialAad<'a> {
    version: u8,
    tenant_id: Uuid,
    assignment_id: Uuid,
    material_id: Uuid,
    kind: MaterialKind,
    plaintext_digest: &'a str,
    retention_policy_version: u64,
    created_at: chrono::DateTime<chrono::Utc>,
    expires_at: chrono::DateTime<chrono::Utc>,
    legal_hold: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredMaterialEnvelope {
    version: u8,
    tenant_id: Uuid,
    assignment_id: Uuid,
    material_id: Uuid,
    kind: MaterialKind,
    plaintext_digest: String,
    retention_policy_version: u64,
    created_at: chrono::DateTime<chrono::Utc>,
    expires_at: chrono::DateTime<chrono::Utc>,
    legal_hold: bool,
    key_version: String,
    ciphertext_base64: String,
}

impl<O, K, R, C> GovernedAgentProtectedMaterial<O, K, R, C>
where
    O: AgentEncryptedObjectPort,
    K: AgentEnvelopeKeyManagerPort,
    R: AgentMaterialRetentionPort,
    C: Clock,
{
    pub async fn protect_prompt(
        &self,
        assignment: &Assignment,
        material: &str,
    ) -> Result<String, PortError> {
        self.protect(assignment, material, MaterialKind::Prompt)
            .await
    }

    async fn protect(
        &self,
        assignment: &Assignment,
        material: &str,
        kind: MaterialKind,
    ) -> Result<String, PortError> {
        if material.is_empty() || material.len() > self.config.max_plaintext_bytes {
            return Err(PortError::ProtectedMaterialUnavailable);
        }
        let at = self.clock.now();
        let retention = self.retention.load_retention(assignment, at).await?;
        if retention.policy_version == 0 || retention.expires_at <= at {
            return Err(PortError::ProtectedMaterialUnavailable);
        }
        let digest = format!("{:x}", Sha256::digest(material.as_bytes()));
        let material_id = material_id(
            self.config.tenant_id,
            assignment.id().as_uuid(),
            kind,
            &digest,
        );
        let aad = aad_bytes(MaterialAad {
            version: 1,
            tenant_id: self.config.tenant_id,
            assignment_id: assignment.id().as_uuid(),
            material_id,
            kind,
            plaintext_digest: &digest,
            retention_policy_version: retention.policy_version,
            created_at: at,
            expires_at: retention.expires_at,
            legal_hold: retention.legal_hold,
        })?;
        let encrypted = self
            .keys
            .encrypt(
                &self.config.key_ref,
                &aad,
                Zeroizing::new(material.as_bytes().to_vec()),
            )
            .await?;
        if !safe_ref(&encrypted.key_version, 128)
            || encrypted.ciphertext.is_empty()
            || encrypted.ciphertext.len() > self.config.max_plaintext_bytes.saturating_add(65_536)
        {
            return Err(PortError::ProtectedMaterialUnavailable);
        }
        let envelope = StoredMaterialEnvelope {
            version: 1,
            tenant_id: self.config.tenant_id,
            assignment_id: assignment.id().as_uuid(),
            material_id,
            kind,
            plaintext_digest: digest,
            retention_policy_version: retention.policy_version,
            created_at: at,
            expires_at: retention.expires_at,
            legal_hold: retention.legal_hold,
            key_version: encrypted.key_version,
            ciphertext_base64: BASE64.encode(encrypted.ciphertext),
        };
        let bytes =
            serde_json::to_vec(&envelope).map_err(|_| PortError::ProtectedMaterialUnavailable)?;
        if bytes.len() as u64 > MAX_ENVELOPE_BYTES {
            return Err(PortError::ProtectedMaterialUnavailable);
        }
        self.objects
            .put_envelope(
                &self.config.namespace,
                &object_key(
                    self.config.tenant_id,
                    assignment.id().as_uuid(),
                    material_id,
                ),
                &bytes,
                self.config.storage_timeout,
            )
            .await?;
        Ok(format!("{REFERENCE_PREFIX}{material_id}"))
    }

    async fn resolve(
        &self,
        assignment: &Assignment,
        material_ref: &str,
        expected_kind: MaterialKind,
    ) -> Result<String, PortError> {
        let material_id = parse_material_ref(material_ref)?;
        let bytes = self
            .objects
            .get_envelope(
                &self.config.namespace,
                &object_key(
                    self.config.tenant_id,
                    assignment.id().as_uuid(),
                    material_id,
                ),
                MAX_ENVELOPE_BYTES,
                self.config.storage_timeout,
            )
            .await?;
        let envelope: StoredMaterialEnvelope =
            serde_json::from_slice(&bytes).map_err(|_| PortError::ProtectedMaterialUnavailable)?;
        let at = self.clock.now();
        let current = self.retention.load_retention(assignment, at).await?;
        if envelope.version != 1
            || envelope.tenant_id != self.config.tenant_id
            || envelope.assignment_id != assignment.id().as_uuid()
            || envelope.material_id != material_id
            || envelope.kind != expected_kind
            || envelope.retention_policy_version > current.policy_version
            || envelope.expires_at > current.expires_at
            || envelope.expires_at <= at
            || !valid_digest(&envelope.plaintext_digest)
            || !safe_ref(&envelope.key_version, 128)
        {
            return Err(PortError::ProtectedMaterialUnavailable);
        }
        let aad = aad_bytes(MaterialAad {
            version: envelope.version,
            tenant_id: envelope.tenant_id,
            assignment_id: envelope.assignment_id,
            material_id: envelope.material_id,
            kind: envelope.kind,
            plaintext_digest: &envelope.plaintext_digest,
            retention_policy_version: envelope.retention_policy_version,
            created_at: envelope.created_at,
            expires_at: envelope.expires_at,
            legal_hold: envelope.legal_hold,
        })?;
        let ciphertext = BASE64
            .decode(envelope.ciphertext_base64.as_bytes())
            .map_err(|_| PortError::ProtectedMaterialUnavailable)?;
        let plaintext = self
            .keys
            .decrypt(
                &self.config.key_ref,
                &envelope.key_version,
                &aad,
                &ciphertext,
            )
            .await?;
        if plaintext.is_empty()
            || plaintext.len() > self.config.max_plaintext_bytes
            || format!("{:x}", Sha256::digest(plaintext.as_slice()))
                != envelope.plaintext_digest.to_ascii_lowercase()
        {
            return Err(PortError::ProtectedMaterialUnavailable);
        }
        String::from_utf8(plaintext.to_vec()).map_err(|_| PortError::ProtectedMaterialUnavailable)
    }
}

#[async_trait]
impl<O, K, R, C> ProtectedAgentMaterialPort for GovernedAgentProtectedMaterial<O, K, R, C>
where
    O: AgentEncryptedObjectPort,
    K: AgentEnvelopeKeyManagerPort,
    R: AgentMaterialRetentionPort,
    C: Clock,
{
    async fn resolve_prompt(&self, claim: &ClaimedAssignment) -> Result<String, PortError> {
        self.resolve(&claim.assignment, &claim.prompt_ref, MaterialKind::Prompt)
            .await
    }

    async fn protect_output(
        &self,
        claim: &ClaimedAssignment,
        material: &str,
    ) -> Result<String, PortError> {
        self.protect(&claim.assignment, material, MaterialKind::Output)
            .await
    }
}

fn material_id(tenant_id: Uuid, assignment_id: Uuid, kind: MaterialKind, digest: &str) -> Uuid {
    let mut value = Sha256::new();
    value.update(b"erpx.agent.material.v1");
    value.update(tenant_id.as_bytes());
    value.update(assignment_id.as_bytes());
    value.update([kind as u8]);
    value.update(digest.as_bytes());
    let value = value.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&value[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn parse_material_ref(value: &str) -> Result<Uuid, PortError> {
    if value.len() > MAX_REFERENCE_BYTES {
        return Err(PortError::ProtectedMaterialUnavailable);
    }
    value
        .strip_prefix(REFERENCE_PREFIX)
        .and_then(|value| Uuid::parse_str(value).ok())
        .filter(|value| !value.is_nil())
        .ok_or(PortError::ProtectedMaterialUnavailable)
}

fn object_key(tenant_id: Uuid, assignment_id: Uuid, material_id: Uuid) -> String {
    format!("agent-material/{tenant_id}/{assignment_id}/{material_id}.json")
}

fn aad_bytes(aad: MaterialAad<'_>) -> Result<Vec<u8>, PortError> {
    serde_json::to_vec(&aad).map_err(|_| PortError::ProtectedMaterialUnavailable)
}

fn safe_ref(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_are_opaque_bounded_and_do_not_embed_storage_paths() {
        let id = Uuid::from_u128(9);
        let reference = format!("{REFERENCE_PREFIX}{id}");
        assert_eq!(parse_material_ref(&reference).unwrap(), id);
        assert!(!reference.contains("agent-material/"));
        assert!(parse_material_ref("https://object.example/prompt").is_err());
    }

    #[test]
    fn config_debug_redacts_namespace_and_key_reference() {
        let config = AgentProtectedMaterialConfig::try_new(
            Uuid::from_u128(1),
            "tenant-agent-material".into(),
            "kms:key:tenant-1".into(),
            1024,
            StdDuration::from_secs(2),
        )
        .expect("config");
        let debug = format!("{config:?}");
        assert!(debug.contains("REDACTED"));
        assert!(!debug.contains("kms:key:tenant-1"));
        assert!(!debug.contains("tenant-agent-material"));
    }

    #[test]
    fn material_identity_is_tenant_assignment_kind_and_digest_bound() {
        let tenant = Uuid::from_u128(1);
        let assignment = Uuid::from_u128(2);
        let prompt = material_id(tenant, assignment, MaterialKind::Prompt, &"a".repeat(64));
        assert_eq!(
            prompt,
            material_id(tenant, assignment, MaterialKind::Prompt, &"a".repeat(64))
        );
        assert_ne!(
            prompt,
            material_id(tenant, assignment, MaterialKind::Output, &"a".repeat(64))
        );
        assert_ne!(
            prompt,
            material_id(
                Uuid::from_u128(3),
                assignment,
                MaterialKind::Prompt,
                &"a".repeat(64)
            )
        );
    }

    #[test]
    fn production_source_has_no_local_key_or_plaintext_persistence_fallback() {
        let source = include_str!("agent_material.rs");
        assert!(!source.contains(concat!("AES", "_KEY")));
        assert!(!source.contains(concat!("from_", "env")));
        assert!(source.contains("AgentEnvelopeKeyManagerPort"));
        assert!(source.contains("Zeroizing"));
        assert!(source.contains("plaintext_digest"));
    }

    #[tokio::test]
    #[ignore = "requires an explicitly configured disposable HTTPS MinIO namespace"]
    async fn minio_envelope_boundary_round_trips_bounded_ciphertext_and_cleans_up() {
        let endpoint = std::env::var("ERPX_TEST_AGENT_MINIO_ENDPOINT")
            .expect("ERPX_TEST_AGENT_MINIO_ENDPOINT is required");
        let access_key = std::env::var("ERPX_TEST_AGENT_MINIO_ACCESS_KEY")
            .expect("ERPX_TEST_AGENT_MINIO_ACCESS_KEY is required");
        let secret_key = std::env::var("ERPX_TEST_AGENT_MINIO_SECRET_KEY")
            .expect("ERPX_TEST_AGENT_MINIO_SECRET_KEY is required");
        let region = std::env::var("ERPX_TEST_AGENT_MINIO_REGION")
            .expect("ERPX_TEST_AGENT_MINIO_REGION is required");
        let namespace = std::env::var("ERPX_TEST_AGENT_MINIO_NAMESPACE")
            .expect("ERPX_TEST_AGENT_MINIO_NAMESPACE is required");
        let objects = MinioObjectStore::new(erpx_infrastructure::MinioObjectStoreConfig {
            endpoint,
            access_key,
            secret_key,
            region,
        })
        .expect("governed MinIO configuration");
        objects
            .ensure_namespace(&namespace)
            .await
            .expect("disposable namespace");
        let object_key = format!("agent-material/live-test/{}.json", Uuid::new_v4());
        let envelope = br#"{"version":1,"ciphertext_base64":"AQID","key_version":"test-v1"}"#;
        objects
            .put_envelope(&namespace, &object_key, envelope, StdDuration::from_secs(5))
            .await
            .expect("bounded upload");
        let restored = objects
            .get_envelope(&namespace, &object_key, 1024, StdDuration::from_secs(5))
            .await
            .expect("bounded download");
        assert_eq!(restored, envelope);
        objects
            .delete_object_bounded(&namespace, &object_key, StdDuration::from_secs(5))
            .await
            .expect("cleanup disposable object");
    }
}
