//! Customer-hosted ERPX data-plane agent security boundary.
//!
//! The agent resolves secrets inside the customer network and accepts only a
//! closed set of typed operations. It does not expose SQL, shell, database
//! credentials, directory bind credentials, or tenant-to-tenant routing.

use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::OpenOptionsExt,
    os::unix::fs::PermissionsExt,
    path::{Component, Path, PathBuf},
};

use chrono::{DateTime, Utc};
use erpx_domain::{
    access::Scope,
    contracts::CONTRACT_VERSION,
    data_plane::{AgentCapability, BusinessUnitId, SecretReference},
};
use erpx_proto::{
    common::v2::{TenantScope as ProtoTenantScope, tenant_scope},
    erpx::platform_administration::v2::{
        DesiredConfigurationMessage, RootAccessCapabilityMessage, RootAccessRevocationMessage,
        TypedDataPlaneCommand,
    },
};
use prost::Message;
use ring::signature::{ED25519, UnparsedPublicKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use thiserror::Error;
use uuid::Uuid;
use zeroize::Zeroizing;

const MAX_LOCAL_SECRET_BYTES: u64 = 256 * 1024;
const MAX_DESIRED_STATE_BYTES: usize = 64 * 1024;
const MAX_RECENT_COMMAND_IDS: usize = 4_096;
pub const REQUIRED_SCHEMA_GENERATION: &str = "2";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalReadinessObservation {
    pub observed_version: u64,
    pub write_schema_version: String,
    pub read_schema_version: String,
    pub write_healthy: bool,
    pub read_healthy: bool,
    pub storage_healthy: bool,
    pub directory_ready: bool,
    pub projections_healthy: bool,
    pub backup_verified: bool,
    pub failure_category: Option<String>,
}

impl LocalReadinessObservation {
    pub fn validate(&self) -> Result<(), AgentError> {
        for value in [&self.write_schema_version, &self.read_schema_version] {
            if value.trim().is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
                return Err(AgentError::InvalidLocalReadiness);
            }
        }
        if let Some(category) = self.failure_category.as_deref() {
            let lower = category.to_ascii_lowercase();
            if category.trim().is_empty()
                || category.len() > 255
                || category.chars().any(char::is_control)
                || ["password", "secret", "token", "credential"]
                    .iter()
                    .any(|marker| lower.contains(marker))
            {
                return Err(AgentError::InvalidLocalReadiness);
            }
        }
        if self.write_healthy
            && self.read_healthy
            && (self.write_schema_version != REQUIRED_SCHEMA_GENERATION
                || self.read_schema_version != REQUIRED_SCHEMA_GENERATION)
        {
            return Err(AgentError::UnsupportedSchemaGeneration);
        }
        Ok(())
    }

    pub fn is_healthy(&self) -> bool {
        self.write_healthy
            && self.read_healthy
            && self.write_schema_version == REQUIRED_SCHEMA_GENERATION
            && self.read_schema_version == REQUIRED_SCHEMA_GENERATION
            && self.storage_healthy
            && self.directory_ready
            && self.projections_healthy
            && self.backup_verified
            && self.failure_category.is_none()
    }

    pub fn unavailable(observed_version: u64) -> Self {
        Self {
            observed_version,
            write_schema_version: "unknown".into(),
            read_schema_version: "unknown".into(),
            write_healthy: false,
            read_healthy: false,
            storage_healthy: false,
            directory_ready: false,
            projections_healthy: false,
            backup_verified: false,
            failure_category: Some("local_readiness_unavailable".into()),
        }
    }
}

/// Filesystem handoff to customer-owned local workers. ERPX can enqueue only
/// validated desired packages and closed typed jobs; the worker publishes a
/// redacted readiness observation after it has verified local side effects.
#[derive(Debug, Clone)]
pub struct LocalDataPlaneRuntime {
    root: PathBuf,
}

impl LocalDataPlaneRuntime {
    pub fn new(root: PathBuf) -> Result<Self, AgentError> {
        validate_private_directory(&root)?;
        for child in ["desired", "jobs"] {
            let path = root.join(child);
            if !path.exists() {
                fs::create_dir(&path)?;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
            }
            validate_private_directory(&path)?;
        }
        Ok(Self { root })
    }

    pub fn stage_desired(
        &self,
        desired: &DesiredConfigurationMessage,
        validated_state: &Value,
    ) -> Result<(), AgentError> {
        let configuration_id = parse_uuid(&desired.configuration_id)?;
        let value = serde_json::json!({
            "contract_version": desired.contract_version,
            "configuration_id": configuration_id,
            "tenant_id": desired.tenant_id,
            "root_holding_id": desired.root_holding_id,
            "target_agent_id": desired.target_agent_id,
            "configuration_layer": desired.configuration_layer,
            "business_unit": desired.business_unit,
            "expected_observed_version": desired.expected_observed_version,
            "desired_version": desired.desired_version,
            "desired_state": validated_state,
            "secret_references": desired.secret_references,
            "required_capabilities": desired.required_capabilities,
            "requester_ref": desired.requester_ref,
            "reason": desired.reason,
            "expires_at": desired.expires_at,
            "correlation_id": desired.correlation_id,
            "signing_key_id": desired.signing_key_id,
        });
        let bytes = serde_json::to_vec(&value).map_err(|_| AgentError::InvalidDesiredState)?;
        write_private(
            &self.root.join("desired"),
            &format!("{configuration_id}.json"),
            &bytes,
            true,
        )
    }

    pub fn stage_command(
        &self,
        command: &TypedDataPlaneCommand,
        operation: TypedAgentOperation,
    ) -> Result<(), AgentError> {
        let command_id = parse_uuid(&command.command_id)?;
        let payload = validate_desired_json(&command.payload_json)?;
        let tenant_scope = proto_scope(
            command
                .tenant_scope
                .as_ref()
                .ok_or(AgentError::BindingMismatch)?,
        )?;
        let audit = command.audit.as_ref().map(|audit| {
            serde_json::json!({
                "purpose": audit.purpose,
                "customer_visible": audit.customer_visible,
                "module": audit.module,
                "correlation_id": audit.correlation_id,
            })
        });
        let value = serde_json::json!({
            "contract_version": command.contract_version,
            "command_id": command_id,
            "tenant_id": command.tenant_id,
            "agent_id": command.agent_id,
            "operation": operation.as_str(),
            "payload": payload,
            "correlation_id": command.correlation_id,
            "tenant_scope": tenant_scope,
            "audit": audit,
            "expires_at": command.expires_at,
            "device_ref": command.device_ref,
        });
        let bytes = serde_json::to_vec(&value).map_err(|_| AgentError::InvalidDesiredState)?;
        write_private(
            &self.root.join("jobs"),
            &format!("{command_id}.json"),
            &bytes,
            false,
        )
    }

    pub fn readiness(&self) -> Result<Option<LocalReadinessObservation>, AgentError> {
        let path = self.root.join("observed-state.json");
        if !path.exists() {
            return Ok(None);
        }
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.file_type().is_file()
            || metadata.file_type().is_symlink()
            || metadata.permissions().mode() & 0o777 != 0o600
            || metadata.len() == 0
            || metadata.len() > MAX_DESIRED_STATE_BYTES as u64
        {
            return Err(AgentError::InvalidLocalReadiness);
        }
        let observation: LocalReadinessObservation = serde_json::from_slice(&fs::read(path)?)
            .map_err(|_| AgentError::InvalidLocalReadiness)?;
        observation.validate()?;
        Ok(Some(observation))
    }
}

fn validate_private_directory(path: &Path) -> Result<(), AgentError> {
    if !path.is_absolute() {
        return Err(AgentError::Configuration(
            "local runtime directory must be absolute",
        ));
    }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o777 != 0o700
    {
        return Err(AgentError::Configuration(
            "local runtime directory must be a private regular directory",
        ));
    }
    Ok(())
}

fn write_private(
    directory: &Path,
    name: &str,
    bytes: &[u8],
    accept_identical: bool,
) -> Result<(), AgentError> {
    if bytes.is_empty() || bytes.len() > MAX_DESIRED_STATE_BYTES {
        return Err(AgentError::InvalidDesiredState);
    }
    let temporary = directory.join(format!(".{name}.{}.tmp", Uuid::new_v4()));
    let final_path = directory.join(name);
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    if let Err(error) = fs::hard_link(&temporary, &final_path) {
        let _ = fs::remove_file(&temporary);
        return if error.kind() == std::io::ErrorKind::AlreadyExists && accept_identical {
            let metadata = fs::symlink_metadata(&final_path)?;
            if metadata.file_type().is_file()
                && !metadata.file_type().is_symlink()
                && metadata.permissions().mode() & 0o777 == 0o600
                && metadata.len() == bytes.len() as u64
                && fs::read(final_path)? == bytes
            {
                Ok(())
            } else {
                Err(AgentError::ReplayOrOutOfOrder)
            }
        } else if error.kind() == std::io::ErrorKind::AlreadyExists {
            Err(AgentError::ReplayOrOutOfOrder)
        } else {
            Err(AgentError::Io(error))
        };
    }
    fs::remove_file(temporary)?;
    Ok(())
}

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("invalid agent configuration: {0}")]
    Configuration(&'static str),
    #[error("local secret reference is invalid")]
    InvalidSecretReference,
    #[error("local secret file is not a private regular file")]
    UnsafeSecretFile,
    #[error("control-plane signature is invalid")]
    InvalidSignature,
    #[error("control-plane signing key is not pinned")]
    SigningKeyMismatch,
    #[error("message contract version is unsupported")]
    UnsupportedContract,
    #[error("message tenant or agent binding is invalid")]
    BindingMismatch,
    #[error("message is expired")]
    Expired,
    #[error("message is replayed or out of order")]
    ReplayOrOutOfOrder,
    #[error("requested agent capability is not enrolled")]
    MissingCapability,
    #[error("typed operation is unsupported")]
    UnsupportedOperation,
    #[error("desired state contains credential material")]
    SensitiveDesiredState,
    #[error("invalid desired state")]
    InvalidDesiredState,
    #[error("local worker readiness evidence is invalid")]
    InvalidLocalReadiness,
    #[error("local worker schema generation is unsupported")]
    UnsupportedSchemaGeneration,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct AgentIdentity {
    pub tenant_id: String,
    pub holding_id: Uuid,
    pub agent_id: Uuid,
    pub capabilities: BTreeSet<AgentCapability>,
}

impl AgentIdentity {
    pub fn validate(&self) -> Result<(), AgentError> {
        if self.tenant_id.trim().is_empty()
            || self.tenant_id.len() > 128
            || self.holding_id.is_nil()
            || self.agent_id.is_nil()
            || self.capabilities.is_empty()
        {
            return Err(AgentError::Configuration("agent identity is incomplete"));
        }
        Ok(())
    }
}

/// Maps `customer-secret:` references to private files beneath one local root.
/// No secret bytes are sent back to ERPX.
#[derive(Debug, Clone)]
pub struct LocalSecretResolver {
    root: PathBuf,
}

impl LocalSecretResolver {
    pub fn new(root: PathBuf) -> Result<Self, AgentError> {
        if !root.is_absolute() {
            return Err(AgentError::Configuration(
                "local secret root must be absolute",
            ));
        }
        let metadata = fs::symlink_metadata(&root)?;
        if !metadata.file_type().is_dir()
            || metadata.file_type().is_symlink()
            || metadata.permissions().mode() & 0o777 != 0o700
        {
            return Err(AgentError::Configuration(
                "local secret root must be a private 0700 directory",
            ));
        }
        Ok(Self { root })
    }

    pub fn resolve(&self, reference: &SecretReference) -> Result<Zeroizing<Vec<u8>>, AgentError> {
        let relative = reference
            .as_str()
            .strip_prefix("customer-secret:")
            .ok_or(AgentError::InvalidSecretReference)?;
        let relative = Path::new(relative);
        if relative.is_absolute()
            || relative.components().count() == 0
            || relative.components().any(|component| match component {
                Component::Normal(value) => {
                    let value = value.to_string_lossy();
                    value.is_empty()
                        || value.len() > 128
                        || !value
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
                }
                _ => true,
            })
        {
            return Err(AgentError::InvalidSecretReference);
        }
        let target = self.root.join(relative);
        if !target.starts_with(&self.root) {
            return Err(AgentError::InvalidSecretReference);
        }
        let metadata = fs::symlink_metadata(&target)?;
        if !metadata.file_type().is_file()
            || metadata.file_type().is_symlink()
            || metadata.permissions().mode() & 0o777 != 0o600
            || metadata.len() == 0
            || metadata.len() > MAX_LOCAL_SECRET_BYTES
        {
            return Err(AgentError::UnsafeSecretFile);
        }
        Ok(Zeroizing::new(fs::read(target)?))
    }
}

#[derive(Debug, Clone)]
pub struct ControlPlaneSignatureVerifier {
    key_id: String,
    public_key: Vec<u8>,
}

impl ControlPlaneSignatureVerifier {
    /// `public_key` is the pinned raw 32-byte Ed25519 public key.
    pub fn new(key_id: String, public_key: Vec<u8>) -> Result<Self, AgentError> {
        if key_id.trim().is_empty() || key_id.len() > 128 || public_key.len() != 32 {
            return Err(AgentError::Configuration(
                "pinned Ed25519 signing key is invalid",
            ));
        }
        Ok(Self { key_id, public_key })
    }

    pub fn verify_desired(&self, desired: &DesiredConfigurationMessage) -> Result<(), AgentError> {
        if desired.signing_key_id != self.key_id {
            return Err(AgentError::SigningKeyMismatch);
        }
        let mut unsigned = desired.clone();
        let signature = std::mem::take(&mut unsigned.control_plane_signature);
        verify_ed25519(&self.public_key, &unsigned.encode_to_vec(), &signature)
    }

    pub fn verify_command(&self, command: &TypedDataPlaneCommand) -> Result<(), AgentError> {
        // Commands use the same pinned key. Its identifier is bound during mTLS
        // enrollment and therefore is not repeated in every command envelope.
        let mut unsigned = command.clone();
        let signature = std::mem::take(&mut unsigned.control_plane_signature);
        verify_ed25519(&self.public_key, &unsigned.encode_to_vec(), &signature)
    }

    pub fn verify_root_capability(
        &self,
        capability: &RootAccessCapabilityMessage,
    ) -> Result<(), AgentError> {
        if capability.signing_key_id != self.key_id {
            return Err(AgentError::SigningKeyMismatch);
        }
        let mut unsigned = capability.clone();
        let signature = std::mem::take(&mut unsigned.control_plane_signature);
        verify_ed25519(&self.public_key, &unsigned.encode_to_vec(), &signature)
    }

    pub fn verify_root_revocation(
        &self,
        revocation: &RootAccessRevocationMessage,
    ) -> Result<(), AgentError> {
        if revocation.signing_key_id != self.key_id {
            return Err(AgentError::SigningKeyMismatch);
        }
        let mut unsigned = revocation.clone();
        let signature = std::mem::take(&mut unsigned.control_plane_signature);
        verify_ed25519(&self.public_key, &unsigned.encode_to_vec(), &signature)
    }
}

fn verify_ed25519(public_key: &[u8], message: &[u8], signature: &[u8]) -> Result<(), AgentError> {
    UnparsedPublicKey::new(&ED25519, public_key)
        .verify(message, signature)
        .map_err(|_| AgentError::InvalidSignature)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypedAgentOperation {
    HealthDiagnostic,
    ApplyConfiguration,
    RunMigration,
    RunBackup,
    VerifyRestore,
    SynchronizeDirectory,
    RebuildProjection,
    TypedTenantQuery,
    TypedTenantCommand,
}

impl TypedAgentOperation {
    pub fn parse(value: &str) -> Result<Self, AgentError> {
        match value {
            "health.diagnostic" => Ok(Self::HealthDiagnostic),
            "configuration.apply" => Ok(Self::ApplyConfiguration),
            "migration.run_signed" => Ok(Self::RunMigration),
            "backup.run" => Ok(Self::RunBackup),
            "restore.verify" => Ok(Self::VerifyRestore),
            "directory.synchronize" => Ok(Self::SynchronizeDirectory),
            "projection.rebuild" => Ok(Self::RebuildProjection),
            "tenant.query.typed" => Ok(Self::TypedTenantQuery),
            "tenant.command.typed" => Ok(Self::TypedTenantCommand),
            // Generic SQL and shell strings necessarily land here.
            _ => Err(AgentError::UnsupportedOperation),
        }
    }

    pub const fn capability(self) -> AgentCapability {
        match self {
            Self::HealthDiagnostic => AgentCapability::HealthTelemetry,
            Self::ApplyConfiguration => AgentCapability::ConfigurationReconciliation,
            Self::RunMigration => AgentCapability::Migration,
            Self::RunBackup => AgentCapability::Backup,
            Self::VerifyRestore => AgentCapability::Restore,
            Self::SynchronizeDirectory => AgentCapability::DirectorySync,
            Self::RebuildProjection => AgentCapability::ProjectionWorker,
            Self::TypedTenantQuery => AgentCapability::TypedTenantQueries,
            Self::TypedTenantCommand => AgentCapability::TypedTenantCommands,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HealthDiagnostic => "health.diagnostic",
            Self::ApplyConfiguration => "configuration.apply",
            Self::RunMigration => "migration.run_signed",
            Self::RunBackup => "backup.run",
            Self::VerifyRestore => "restore.verify",
            Self::SynchronizeDirectory => "directory.synchronize",
            Self::RebuildProjection => "projection.rebuild",
            Self::TypedTenantQuery => "tenant.query.typed",
            Self::TypedTenantCommand => "tenant.command.typed",
        }
    }
}

#[derive(Debug)]
pub struct AgentMessageGuard {
    identity: AgentIdentity,
    verifier: ControlPlaneSignatureVerifier,
    observed_version: u64,
    recent_command_ids: VecDeque<Uuid>,
    recent_command_set: BTreeSet<Uuid>,
    root_capabilities: HashMap<Uuid, ActiveRootCapability>,
}

#[derive(Debug, Clone)]
struct ActiveRootCapability {
    token_hash: [u8; 32],
    root_principal: String,
    device_ref: String,
    scope: Scope,
    allowed_actions: BTreeSet<String>,
    business_units: BTreeSet<BusinessUnitId>,
    issued_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

impl AgentMessageGuard {
    pub fn new(
        identity: AgentIdentity,
        verifier: ControlPlaneSignatureVerifier,
        observed_version: u64,
    ) -> Result<Self, AgentError> {
        identity.validate()?;
        Ok(Self {
            identity,
            verifier,
            observed_version,
            recent_command_ids: VecDeque::new(),
            recent_command_set: BTreeSet::new(),
            root_capabilities: HashMap::new(),
        })
    }

    pub fn observed_version(&self) -> u64 {
        self.observed_version
    }

    /// Verify signature, tenant binding, expiry, capabilities, and strict
    /// desired-version ordering before local reconciliation begins.
    pub fn admit_desired(
        &self,
        desired: &DesiredConfigurationMessage,
        at: DateTime<Utc>,
    ) -> Result<Value, AgentError> {
        self.verifier.verify_desired(desired)?;
        validate_contract(desired.contract_version)?;
        if desired.tenant_id != self.identity.tenant_id
            || desired.target_agent_id != self.identity.agent_id.to_string()
            || desired.root_holding_id != self.identity.holding_id.to_string()
        {
            return Err(AgentError::BindingMismatch);
        }
        validate_expiry(&desired.expires_at, at)?;
        if desired.expected_observed_version != self.observed_version
            || desired.desired_version != self.observed_version.saturating_add(1)
        {
            return Err(AgentError::ReplayOrOutOfOrder);
        }
        for capability in &desired.required_capabilities {
            let capability = parse_agent_capability(capability)?;
            if !self.identity.capabilities.contains(&capability) {
                return Err(AgentError::MissingCapability);
            }
        }
        validate_desired_json(&desired.desired_state_json)
    }

    /// Call only after the local applier verifies the resulting database,
    /// storage, directory, projection, and backup state.
    pub fn mark_desired_verified(&mut self, desired_version: u64) -> Result<(), AgentError> {
        if desired_version != self.observed_version.saturating_add(1) {
            return Err(AgentError::ReplayOrOutOfOrder);
        }
        self.observed_version = desired_version;
        Ok(())
    }

    pub fn admit_command(
        &mut self,
        command: &TypedDataPlaneCommand,
        at: DateTime<Utc>,
    ) -> Result<TypedAgentOperation, AgentError> {
        self.verifier.verify_command(command)?;
        validate_contract(command.contract_version)?;
        if command.tenant_id != self.identity.tenant_id
            || command.agent_id != self.identity.agent_id.to_string()
        {
            return Err(AgentError::BindingMismatch);
        }
        validate_expiry(&command.expires_at, at)?;
        let command_id =
            Uuid::parse_str(&command.command_id).map_err(|_| AgentError::ReplayOrOutOfOrder)?;
        if command_id.is_nil() || self.recent_command_set.contains(&command_id) {
            return Err(AgentError::ReplayOrOutOfOrder);
        }
        let operation = TypedAgentOperation::parse(&command.command_type)?;
        if !self.identity.capabilities.contains(&operation.capability()) {
            return Err(AgentError::MissingCapability);
        }
        if !matches!(
            operation,
            TypedAgentOperation::HealthDiagnostic | TypedAgentOperation::ApplyConfiguration
        ) || !command.root_access_capability_id.is_empty()
        {
            self.authorize_root_command(command, at)?;
        }
        self.recent_command_set.insert(command_id);
        self.recent_command_ids.push_back(command_id);
        if self.recent_command_ids.len() > MAX_RECENT_COMMAND_IDS
            && let Some(expired) = self.recent_command_ids.pop_front()
        {
            self.recent_command_set.remove(&expired);
        }
        Ok(operation)
    }

    pub fn admit_root_capability(
        &mut self,
        capability: &RootAccessCapabilityMessage,
        at: DateTime<Utc>,
    ) -> Result<Uuid, AgentError> {
        self.verifier.verify_root_capability(capability)?;
        validate_contract(capability.contract_version)?;
        if capability.tenant_id != self.identity.tenant_id
            || capability.agent_id != self.identity.agent_id.to_string()
            || capability.one_time_capability_token.is_empty()
            || capability.one_time_capability_token.len() > 128
        {
            return Err(AgentError::BindingMismatch);
        }
        let capability_id = parse_uuid(&capability.capability_id)?;
        let _request_id = parse_uuid(&capability.request_id)?;
        let _correlation_id = parse_uuid(&capability.correlation_id)?;
        if self.root_capabilities.contains_key(&capability_id) {
            return Err(AgentError::ReplayOrOutOfOrder);
        }
        let issued_at = parse_time(&capability.issued_at)?;
        let expires_at = parse_time(&capability.expires_at)?;
        if at < issued_at || at >= expires_at || expires_at - issued_at > chrono::Duration::hours(8)
        {
            return Err(AgentError::Expired);
        }
        let scope = proto_scope(
            capability
                .tenant_scope
                .as_ref()
                .ok_or(AgentError::BindingMismatch)?,
        )?;
        if scope_holding_id(&scope) != self.identity.holding_id
            || capability.allowed_actions.is_empty()
            || capability.allowed_actions.len() > 64
        {
            return Err(AgentError::BindingMismatch);
        }
        let allowed_actions = capability
            .allowed_actions
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        if allowed_actions.len() != capability.allowed_actions.len() {
            return Err(AgentError::ReplayOrOutOfOrder);
        }
        let business_units = capability
            .business_units
            .iter()
            .map(|unit| parse_business_unit(unit))
            .collect::<Result<BTreeSet<_>, _>>()?;
        if business_units.len() != capability.business_units.len() {
            return Err(AgentError::ReplayOrOutOfOrder);
        }
        self.root_capabilities.insert(
            capability_id,
            ActiveRootCapability {
                token_hash: Sha256::digest(&capability.one_time_capability_token).into(),
                root_principal: capability.root_principal_ref.clone(),
                device_ref: capability.device_ref.clone(),
                scope,
                allowed_actions,
                business_units,
                issued_at,
                expires_at,
            },
        );
        Ok(capability_id)
    }

    pub fn admit_root_revocation(
        &mut self,
        revocation: &RootAccessRevocationMessage,
        at: DateTime<Utc>,
    ) -> Result<(), AgentError> {
        self.verifier.verify_root_revocation(revocation)?;
        validate_contract(revocation.contract_version)?;
        if revocation.tenant_id != self.identity.tenant_id
            || revocation.agent_id != self.identity.agent_id.to_string()
            || parse_time(&revocation.revoked_at)? > at
        {
            return Err(AgentError::BindingMismatch);
        }
        let capability_id = parse_uuid(&revocation.capability_id)?;
        self.root_capabilities.remove(&capability_id);
        Ok(())
    }

    fn authorize_root_command(
        &self,
        command: &TypedDataPlaneCommand,
        at: DateTime<Utc>,
    ) -> Result<(), AgentError> {
        let capability_id = parse_uuid(&command.root_access_capability_id)?;
        let capability = self
            .root_capabilities
            .get(&capability_id)
            .ok_or(AgentError::BindingMismatch)?;
        let presented_hash: [u8; 32] = Sha256::digest(&command.root_access_capability_token).into();
        if at < capability.issued_at
            || at >= capability.expires_at
            || command
                .actor
                .as_ref()
                .is_none_or(|actor| actor.principal_ref != capability.root_principal)
            || command.device_ref != capability.device_ref
            || !capability.allowed_actions.contains(&command.command_type)
            || !bool::from(presented_hash.ct_eq(&capability.token_hash))
        {
            return Err(AgentError::BindingMismatch);
        }
        let requested_scope = proto_scope(
            command
                .tenant_scope
                .as_ref()
                .ok_or(AgentError::BindingMismatch)?,
        )?;
        if !capability.scope.contains(&requested_scope) {
            return Err(AgentError::BindingMismatch);
        }
        if !capability.business_units.is_empty() {
            let unit = command
                .audit
                .as_ref()
                .and_then(|audit| BusinessUnitId::parse(&audit.module).ok())
                .ok_or(AgentError::BindingMismatch)?;
            if !capability.business_units.contains(&unit) {
                return Err(AgentError::BindingMismatch);
            }
        }
        Ok(())
    }
}

fn validate_contract(version: u32) -> Result<(), AgentError> {
    if version != u32::from(CONTRACT_VERSION) {
        return Err(AgentError::UnsupportedContract);
    }
    Ok(())
}

fn validate_expiry(value: &str, at: DateTime<Utc>) -> Result<(), AgentError> {
    let expires = parse_time(value)?;
    if at >= expires {
        return Err(AgentError::Expired);
    }
    Ok(())
}

fn parse_time(value: &str) -> Result<DateTime<Utc>, AgentError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| AgentError::Expired)
}

fn proto_scope(scope: &ProtoTenantScope) -> Result<Scope, AgentError> {
    match scope.scope.as_ref().ok_or(AgentError::BindingMismatch)? {
        tenant_scope::Scope::Platform(_) => Err(AgentError::BindingMismatch),
        tenant_scope::Scope::Holding(scope) => Ok(Scope::Holding {
            holding_id: parse_uuid(&scope.holding_id)?,
        }),
        tenant_scope::Scope::Company(scope) => Ok(Scope::Company {
            holding_id: parse_uuid(&scope.holding_id)?,
            company_id: parse_uuid(&scope.company_id)?,
        }),
        tenant_scope::Scope::Branch(scope) => Ok(Scope::Branch {
            holding_id: parse_uuid(&scope.holding_id)?,
            company_id: parse_uuid(&scope.company_id)?,
            branch_id: parse_uuid(&scope.branch_id)?,
        }),
        tenant_scope::Scope::Subtree(scope) => Ok(Scope::Subtree {
            holding_id: parse_uuid(&scope.holding_id)?,
            company_ids: scope
                .company_ids
                .iter()
                .map(|id| parse_uuid(id))
                .collect::<Result<Vec<_>, _>>()?,
            branch_ids: scope
                .branch_ids
                .iter()
                .map(|id| parse_uuid(id))
                .collect::<Result<Vec<_>, _>>()?,
        }),
    }
}

fn parse_uuid(value: &str) -> Result<Uuid, AgentError> {
    let value = Uuid::parse_str(value).map_err(|_| AgentError::BindingMismatch)?;
    if value.is_nil() {
        return Err(AgentError::BindingMismatch);
    }
    Ok(value)
}

fn scope_holding_id(scope: &Scope) -> Uuid {
    match scope {
        Scope::Platform => Uuid::nil(),
        Scope::Holding { holding_id }
        | Scope::Company { holding_id, .. }
        | Scope::Branch { holding_id, .. }
        | Scope::Subtree { holding_id, .. } => *holding_id,
    }
}

fn validate_desired_json(bytes: &[u8]) -> Result<Value, AgentError> {
    if bytes.is_empty() || bytes.len() > MAX_DESIRED_STATE_BYTES {
        return Err(AgentError::InvalidDesiredState);
    }
    let value: Value =
        serde_json::from_slice(bytes).map_err(|_| AgentError::InvalidDesiredState)?;
    if !value.is_object() {
        return Err(AgentError::InvalidDesiredState);
    }
    inspect_desired_json(&value)?;
    Ok(value)
}

fn inspect_desired_json(value: &Value) -> Result<(), AgentError> {
    match value {
        Value::Object(values) => {
            for (key, value) in values {
                let key = key.to_ascii_lowercase().replace(['-', ' '], "_");
                if [
                    "password",
                    "passwd",
                    "secret",
                    "credential",
                    "private_key",
                    "connection_string",
                    "token",
                ]
                .iter()
                .any(|sensitive| key.contains(sensitive))
                {
                    return Err(AgentError::SensitiveDesiredState);
                }
                inspect_desired_json(value)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                inspect_desired_json(value)?;
            }
        }
        Value::String(value) => {
            let lower = value.to_ascii_lowercase();
            if lower.contains("-----begin")
                || lower.starts_with("bearer ")
                || lower.contains("://") && lower.contains('@')
            {
                return Err(AgentError::SensitiveDesiredState);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
    Ok(())
}

pub fn parse_agent_capability(value: &str) -> Result<AgentCapability, AgentError> {
    match value {
        "typed_tenant_queries" => Ok(AgentCapability::TypedTenantQueries),
        "typed_tenant_commands" => Ok(AgentCapability::TypedTenantCommands),
        "directory_sync" => Ok(AgentCapability::DirectorySync),
        "projection_worker" => Ok(AgentCapability::ProjectionWorker),
        "backup" => Ok(AgentCapability::Backup),
        "restore" => Ok(AgentCapability::Restore),
        "migration" => Ok(AgentCapability::Migration),
        "configuration_reconciliation" => Ok(AgentCapability::ConfigurationReconciliation),
        "health_telemetry" => Ok(AgentCapability::HealthTelemetry),
        _ => Err(AgentError::Configuration(
            "unknown enrolled agent capability",
        )),
    }
}

pub fn parse_business_unit(value: &str) -> Result<BusinessUnitId, AgentError> {
    BusinessUnitId::parse(value).map_err(|_| AgentError::BindingMismatch)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_runtime() -> (PathBuf, LocalDataPlaneRuntime) {
        let root = std::env::temp_dir().join(format!("erpx-agent-runtime-{}", Uuid::new_v4()));
        fs::create_dir(&root).expect("create runtime root");
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
            .expect("protect runtime root");
        let runtime = LocalDataPlaneRuntime::new(root.clone()).expect("valid runtime");
        (root, runtime)
    }

    #[test]
    fn generic_shell_and_sql_operations_are_unrepresentable() {
        assert!(TypedAgentOperation::parse("shell.execute").is_err());
        assert!(TypedAgentOperation::parse("sql.execute").is_err());
        assert!(TypedAgentOperation::parse("database.proxy").is_err());
        assert_eq!(
            TypedAgentOperation::parse("backup.run").expect("typed operation"),
            TypedAgentOperation::RunBackup
        );
    }

    #[test]
    fn desired_json_rejects_credential_material() {
        assert!(validate_desired_json(br#"{"password":"no"}"#).is_err());
        assert!(validate_desired_json(br#"{"database":"postgres://u:p@db/tenant"}"#).is_err());
        assert!(validate_desired_json(br#"{"projection":{"lag":30}}"#).is_ok());
    }

    #[test]
    fn canonical_business_unit_parser_has_exact_catalog() {
        for unit in BusinessUnitId::ALL {
            assert_eq!(parse_business_unit(unit.as_str()).expect("unit"), unit);
        }
        assert!(parse_business_unit("support").is_err());
    }

    #[test]
    fn local_runtime_stages_only_redacted_typed_jobs() {
        let (root, runtime) = test_runtime();
        let command_id = Uuid::new_v4();
        let command = TypedDataPlaneCommand {
            contract_version: u32::from(CONTRACT_VERSION),
            command_id: command_id.to_string(),
            tenant_id: "tenant-1".into(),
            agent_id: Uuid::new_v4().to_string(),
            command_type: "backup.run".into(),
            payload_json: br#"{"backup_profile_ref":"customer-secret:erpx/backup"}"#.to_vec(),
            correlation_id: Uuid::new_v4().to_string(),
            actor: None,
            tenant_scope: Some(ProtoTenantScope {
                scope: Some(tenant_scope::Scope::Holding(
                    erpx_proto::common::v2::HoldingScope {
                        holding_id: Uuid::new_v4().to_string(),
                    },
                )),
            }),
            audit: None,
            root_access_capability_id: Uuid::new_v4().to_string(),
            control_plane_signature: vec![7; 64],
            expires_at: Utc::now().to_rfc3339(),
            root_access_capability_token: b"must-never-reach-disk".to_vec(),
            device_ref: "device-1".into(),
        };
        runtime
            .stage_command(&command, TypedAgentOperation::RunBackup)
            .expect("stage typed job");
        let bytes = fs::read(root.join("jobs").join(format!("{command_id}.json")))
            .expect("read staged job");
        let text = String::from_utf8(bytes).expect("JSON");
        assert!(text.contains("backup.run"));
        assert!(!text.contains("must-never-reach-disk"));
        assert!(!text.contains("control_plane_signature"));
        fs::remove_dir_all(root).expect("remove isolated test runtime");
    }

    #[test]
    fn local_readiness_requires_private_redacted_evidence() {
        let (root, runtime) = test_runtime();
        let observation = LocalReadinessObservation {
            observed_version: 1,
            write_schema_version: REQUIRED_SCHEMA_GENERATION.into(),
            read_schema_version: REQUIRED_SCHEMA_GENERATION.into(),
            write_healthy: true,
            read_healthy: true,
            storage_healthy: true,
            directory_ready: true,
            projections_healthy: true,
            backup_verified: true,
            failure_category: None,
        };
        let state_path = root.join("observed-state.json");
        let mut state = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&state_path)
            .expect("create readiness evidence");
        state
            .write_all(&serde_json::to_vec(&observation).unwrap())
            .expect("write readiness");
        state.sync_all().expect("sync readiness");
        assert_eq!(runtime.readiness().unwrap(), Some(observation));
        fs::remove_dir_all(root).expect("remove isolated test runtime");
    }

    #[test]
    fn healthy_readiness_fails_closed_on_another_schema_generation() {
        let observation = LocalReadinessObservation {
            observed_version: 1,
            write_schema_version: "1".into(),
            read_schema_version: "1".into(),
            write_healthy: true,
            read_healthy: true,
            storage_healthy: true,
            directory_ready: true,
            projections_healthy: true,
            backup_verified: true,
            failure_category: None,
        };
        assert_eq!(
            observation.validate().unwrap_err().to_string(),
            "local worker schema generation is unsupported"
        );
        assert!(!observation.is_healthy());
    }
}
