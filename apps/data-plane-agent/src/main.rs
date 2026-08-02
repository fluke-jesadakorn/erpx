//! Outbound-only mTLS process for a customer-hosted ERPX data plane.

use std::{env, fs, os::unix::fs::PermissionsExt, path::PathBuf, time::Duration};

use chrono::Utc;
use erpx_data_plane_agent::{
    AgentError, AgentIdentity, AgentMessageGuard, ControlPlaneSignatureVerifier,
    LocalDataPlaneRuntime, LocalReadinessObservation, parse_agent_capability,
};
use erpx_domain::{contracts::CONTRACT_VERSION, data_plane::AgentCapability};
use erpx_proto::{
    common::v1::{ActorAttribution, ActorKind, AuditContext},
    platform::{
        ControlPlaneEnvelope, DataPlaneAgentEnvelope, DataPlaneCommandResult,
        DataPlaneObservationMessage, EnrollDataPlaneAgentRequest, control_plane_envelope,
        data_plane_agent_envelope, data_plane_agent_service_client::DataPlaneAgentServiceClient,
    },
};
use sha2::Digest;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

#[derive(Debug)]
struct AgentProcessConfig {
    endpoint: String,
    server_name: String,
    ca_certificate: PathBuf,
    client_certificate: PathBuf,
    client_key: PathBuf,
    signing_key_id: String,
    signing_public_key: PathBuf,
    identity: AgentIdentity,
    agent_version: String,
    heartbeat_seconds: u64,
    runtime_directory: PathBuf,
}

impl AgentProcessConfig {
    fn from_env() -> Result<Self, AgentError> {
        let capabilities = required_env("ERPX_AGENT_CAPABILITIES")?
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(parse_agent_capability)
            .collect::<Result<_, _>>()?;
        let heartbeat_seconds = required_env("ERPX_AGENT_HEARTBEAT_SECONDS")?
            .parse::<u64>()
            .map_err(|_| AgentError::Configuration("heartbeat interval is invalid"))?;
        if !(5..=300).contains(&heartbeat_seconds) {
            return Err(AgentError::Configuration(
                "heartbeat interval must be between 5 and 300 seconds",
            ));
        }
        let endpoint = required_env("ERPX_AGENT_CONTROL_PLANE_URL")?;
        if !endpoint.starts_with("https://") {
            return Err(AgentError::Configuration(
                "control-plane URL must use HTTPS",
            ));
        }
        let identity = AgentIdentity {
            tenant_id: required_env("ERPX_AGENT_TENANT_ID")?,
            holding_id: parse_uuid_env("ERPX_AGENT_HOLDING_ID")?,
            agent_id: parse_uuid_env("ERPX_AGENT_ID")?,
            capabilities,
        };
        identity.validate()?;
        Ok(Self {
            endpoint,
            server_name: required_env("ERPX_AGENT_SERVER_NAME")?,
            ca_certificate: absolute_path_env("ERPX_AGENT_CA_CERT_FILE")?,
            client_certificate: absolute_path_env("ERPX_AGENT_CLIENT_CERT_FILE")?,
            client_key: absolute_path_env("ERPX_AGENT_CLIENT_KEY_FILE")?,
            signing_key_id: required_env("ERPX_AGENT_SIGNING_KEY_ID")?,
            signing_public_key: absolute_path_env("ERPX_AGENT_SIGNING_PUBLIC_KEY_FILE")?,
            identity,
            agent_version: env!("CARGO_PKG_VERSION").to_owned(),
            heartbeat_seconds,
            runtime_directory: absolute_path_env("ERPX_AGENT_RUNTIME_DIR")?,
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    let config = AgentProcessConfig::from_env()?;
    if env::args().nth(1).as_deref() == Some("enroll") {
        enroll_once(&config).await?;
        return Ok(());
    }
    let public_key_hex = read_bounded_regular_file(&config.signing_public_key, false)?;
    let public_key = hex::decode(String::from_utf8(public_key_hex)?.trim())?;
    let verifier = ControlPlaneSignatureVerifier::new(config.signing_key_id.clone(), public_key)?;
    let mut guard = AgentMessageGuard::new(config.identity.clone(), verifier, 0)?;
    let runtime = LocalDataPlaneRuntime::new(config.runtime_directory.clone())?;

    let mut retry = Duration::from_secs(1);
    loop {
        match connect_once(&config, &runtime, &mut guard).await {
            Ok(()) => retry = Duration::from_secs(1),
            Err(error) => tracing::warn!(error = %error, "customer data-plane channel closed"),
        }
        tokio::time::sleep(retry).await;
        retry = (retry * 2).min(Duration::from_secs(60));
    }
}

async fn enroll_once(config: &AgentProcessConfig) -> Result<(), Box<dyn std::error::Error>> {
    let enrollment_id = required_env("ERPX_AGENT_ENROLLMENT_ID")?;
    let token_file = absolute_path_env("ERPX_AGENT_ENROLLMENT_TOKEN_FILE")?;
    let manifest_file = absolute_path_env("ERPX_AGENT_INSTALLATION_MANIFEST_FILE")?;
    let certificate_expires_at = required_env("ERPX_AGENT_CERTIFICATE_EXPIRES_AT")?;
    let token = read_bounded_regular_file(&token_file, true)?;
    let manifest = read_bounded_regular_file(&manifest_file, false)?;
    if token.is_empty() || token.len() > 4096 || manifest.is_empty() {
        return Err("agent enrollment inputs are invalid".into());
    }
    let correlation_id = Uuid::new_v4();
    let mut client = DataPlaneAgentServiceClient::new(m_tls_channel(config).await?);
    let response = client
        .enroll(EnrollDataPlaneAgentRequest {
            contract_version: u32::from(CONTRACT_VERSION),
            request_id: Uuid::new_v4().to_string(),
            correlation_id: correlation_id.to_string(),
            enrollment_id,
            one_time_bootstrap_token: token,
            signed_installation_manifest: manifest,
            agent_id: config.identity.agent_id.to_string(),
            tenant_id: config.identity.tenant_id.clone(),
            root_holding_id: config.identity.holding_id.to_string(),
            certificate_fingerprint: certificate_fingerprint(&config.client_certificate)?,
            capabilities: config
                .identity
                .capabilities
                .iter()
                .map(|capability| capability.as_str().to_owned())
                .collect(),
            agent_version: config.agent_version.clone(),
            certificate_expires_at,
            actor: Some(ActorAttribution {
                principal_ref: format!("agent:{}", config.identity.agent_id),
                kind: ActorKind::Service as i32,
                session_id: String::new(),
                represented_principal_ref: String::new(),
                support_capability_id: String::new(),
            }),
            audit: Some(AuditContext {
                purpose: "one-time customer data-plane enrollment".into(),
                customer_visible: true,
                module: "data-plane-agent".into(),
                correlation_id: correlation_id.to_string(),
            }),
        })
        .await?
        .into_inner();
    if response.correlation_id != correlation_id.to_string()
        || response.agent_id != config.identity.agent_id.to_string()
        || response.pinned_control_plane_signing_key_id != config.signing_key_id
    {
        return Err("agent enrollment response binding is invalid".into());
    }
    tracing::info!(
        agent_id = %response.agent_id,
        enrolled_at = %response.enrolled_at,
        "customer data-plane agent enrolled"
    );
    Ok(())
}

async fn connect_once(
    config: &AgentProcessConfig,
    runtime: &LocalDataPlaneRuntime,
    guard: &mut AgentMessageGuard,
) -> Result<(), Box<dyn std::error::Error>> {
    let channel = m_tls_channel(config).await?;
    let mut client = DataPlaneAgentServiceClient::new(channel);
    let (outbound, receiver) = mpsc::channel(32);
    let readiness = refresh_local_readiness(runtime, guard);
    outbound
        .send(observation_envelope(config, guard, &readiness))
        .await?;
    let mut inbound = client
        .connect(ReceiverStream::new(receiver))
        .await?
        .into_inner();
    tracing::info!(
        tenant_id = %config.identity.tenant_id,
        agent_id = %config.identity.agent_id,
        "customer data-plane outbound mTLS channel connected"
    );
    let mut heartbeat = tokio::time::interval(Duration::from_secs(config.heartbeat_seconds));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                let readiness = refresh_local_readiness(runtime, guard);
                outbound.send(observation_envelope(config, guard, &readiness)).await?;
            }
            envelope = inbound.message() => {
                let Some(envelope) = envelope? else {
                    return Err("control-plane stream ended".into());
                };
                if let Some(result) = handle_control_envelope(runtime, guard, envelope)? {
                    outbound.send(result).await?;
                }
            }
        }
    }
}

fn handle_control_envelope(
    runtime: &LocalDataPlaneRuntime,
    guard: &mut AgentMessageGuard,
    envelope: ControlPlaneEnvelope,
) -> Result<Option<DataPlaneAgentEnvelope>, AgentError> {
    match envelope.payload.ok_or(AgentError::UnsupportedOperation)? {
        control_plane_envelope::Payload::DesiredConfiguration(desired) => {
            let validated_metadata = guard.admit_desired(&desired, Utc::now())?;
            runtime.stage_desired(&desired, &validated_metadata)?;
            tracing::info!(
                desired_version = desired.desired_version,
                "staged signed desired configuration for local reconciliation"
            );
            Ok(None)
        }
        control_plane_envelope::Payload::TypedCommand(command) => {
            let operation = guard.admit_command(&command, Utc::now())?;
            runtime.stage_command(&command, operation)?;
            tracing::info!(?operation, command_id = %command.command_id, "staged admitted typed command");
            Ok(Some(command_accepted_envelope(&command)))
        }
        control_plane_envelope::Payload::RootAccessCapability(capability) => {
            let capability_id = guard.admit_root_capability(&capability, Utc::now())?;
            tracing::info!(%capability_id, "admitted customer-approved Root capability");
            Ok(None)
        }
        control_plane_envelope::Payload::RootAccessRevocation(revocation) => {
            guard.admit_root_revocation(&revocation, Utc::now())?;
            tracing::info!(
                capability_id = %revocation.capability_id,
                "revoked Root capability"
            );
            Ok(None)
        }
    }
}

fn observation_envelope(
    config: &AgentProcessConfig,
    guard: &AgentMessageGuard,
    readiness: &LocalReadinessObservation,
) -> DataPlaneAgentEnvelope {
    let correlation_id = Uuid::new_v4();
    DataPlaneAgentEnvelope {
        payload: Some(data_plane_agent_envelope::Payload::Observation(Box::new(
            DataPlaneObservationMessage {
                contract_version: u32::from(CONTRACT_VERSION),
                request_id: Uuid::new_v4().to_string(),
                tenant_id: config.identity.tenant_id.clone(),
                agent_id: config.identity.agent_id.to_string(),
                observed_version: guard.observed_version(),
                desired_version: readiness.observed_version.max(guard.observed_version()),
                write_schema_version: readiness.write_schema_version.clone(),
                read_schema_version: readiness.read_schema_version.clone(),
                write_healthy: readiness.write_healthy,
                read_healthy: readiness.read_healthy,
                storage_healthy: readiness.storage_healthy,
                directory_ready: readiness.directory_ready,
                projections_healthy: readiness.projections_healthy,
                backup_verified: readiness.backup_verified,
                failure_category: readiness.failure_category.clone().unwrap_or_default(),
                correlation_id: correlation_id.to_string(),
                observed_at: Utc::now().to_rfc3339(),
                agent_version: config.agent_version.clone(),
                actor: Some(ActorAttribution {
                    principal_ref: format!("agent:{}", config.identity.agent_id),
                    kind: ActorKind::Service as i32,
                    session_id: String::new(),
                    represented_principal_ref: String::new(),
                    support_capability_id: String::new(),
                }),
                audit: Some(AuditContext {
                    purpose: "redacted customer data-plane heartbeat".into(),
                    customer_visible: true,
                    module: "data-plane-agent".into(),
                    correlation_id: correlation_id.to_string(),
                }),
            },
        ))),
    }
}

fn refresh_local_readiness(
    runtime: &LocalDataPlaneRuntime,
    guard: &mut AgentMessageGuard,
) -> LocalReadinessObservation {
    let current = guard.observed_version();
    match runtime.readiness() {
        Ok(Some(observation))
            if observation.observed_version == current.saturating_add(1)
                && observation.is_healthy() =>
        {
            if let Err(error) = guard.mark_desired_verified(observation.observed_version) {
                tracing::warn!(error = %error, "local readiness version was rejected");
                LocalReadinessObservation::unavailable(current)
            } else {
                observation
            }
        }
        Ok(Some(observation))
            if observation.observed_version == current
                || observation.observed_version == current.saturating_add(1) =>
        {
            observation
        }
        Ok(Some(_)) => {
            tracing::warn!("local readiness observation is out of order");
            LocalReadinessObservation::unavailable(current)
        }
        Ok(None) => LocalReadinessObservation::unavailable(current),
        Err(error) => {
            tracing::warn!(error = %error, "local readiness observation rejected");
            LocalReadinessObservation::unavailable(current)
        }
    }
}

fn command_accepted_envelope(
    command: &erpx_proto::platform::TypedDataPlaneCommand,
) -> DataPlaneAgentEnvelope {
    DataPlaneAgentEnvelope {
        payload: Some(data_plane_agent_envelope::Payload::CommandResult(
            DataPlaneCommandResult {
                contract_version: u32::from(CONTRACT_VERSION),
                command_id: command.command_id.clone(),
                tenant_id: command.tenant_id.clone(),
                agent_id: command.agent_id.clone(),
                result_type: "accepted".into(),
                redacted_payload_json: b"{}".to_vec(),
                correlation_id: command.correlation_id.clone(),
                completed_at: Utc::now().to_rfc3339(),
                agent_signature: Vec::new(),
            },
        )),
    }
}

async fn m_tls_channel(config: &AgentProcessConfig) -> Result<Channel, Box<dyn std::error::Error>> {
    let ca = read_bounded_regular_file(&config.ca_certificate, false)?;
    let certificate = read_bounded_regular_file(&config.client_certificate, false)?;
    let key = read_bounded_regular_file(&config.client_key, true)?;
    let tls = ClientTlsConfig::new()
        .ca_certificate(Certificate::from_pem(ca))
        .identity(Identity::from_pem(certificate, key))
        .domain_name(config.server_name.clone());
    Ok(Endpoint::from_shared(config.endpoint.clone())?
        .tls_config(tls)?
        .connect()
        .await?)
}

fn read_bounded_regular_file(path: &PathBuf, private: bool) -> Result<Vec<u8>, AgentError> {
    if !path.is_absolute() {
        return Err(AgentError::Configuration("TLS/key path must be absolute"));
    }
    let metadata = fs::symlink_metadata(path)?;
    let mode = metadata.permissions().mode() & 0o777;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() == 0
        || metadata.len() > 1024 * 1024
        || private && mode != 0o600
    {
        return Err(AgentError::Configuration(
            "TLS/key file is not a bounded regular file",
        ));
    }
    Ok(fs::read(path)?)
}

fn certificate_fingerprint(path: &PathBuf) -> Result<String, AgentError> {
    // Enrollment binds the exact DER certificate observed by the server. The
    // configured client certificate is PEM, so parse the first PEM body into
    // DER for the same SHA-256 representation.
    let pem = read_bounded_regular_file(path, false)?;
    let text = std::str::from_utf8(&pem)
        .map_err(|_| AgentError::Configuration("client certificate PEM is invalid"))?;
    let body = text
        .split("-----BEGIN CERTIFICATE-----")
        .nth(1)
        .and_then(|value| value.split("-----END CERTIFICATE-----").next())
        .ok_or(AgentError::Configuration(
            "client certificate PEM is invalid",
        ))?;
    let encoded = body.lines().map(str::trim).collect::<String>();
    let der = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
        .map_err(|_| AgentError::Configuration("client certificate PEM is invalid"))?;
    Ok(hex::encode(sha2::Sha256::digest(der)))
}

fn required_env(key: &'static str) -> Result<String, AgentError> {
    env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty() && value.len() <= 2048)
        .ok_or(AgentError::Configuration(key))
}

fn absolute_path_env(key: &'static str) -> Result<PathBuf, AgentError> {
    let value = PathBuf::from(required_env(key)?);
    if !value.is_absolute() {
        return Err(AgentError::Configuration(key));
    }
    Ok(value)
}

fn parse_uuid_env(key: &'static str) -> Result<Uuid, AgentError> {
    let value = Uuid::parse_str(&required_env(key)?).map_err(|_| AgentError::Configuration(key))?;
    if value.is_nil() {
        return Err(AgentError::Configuration(key));
    }
    Ok(value)
}

#[allow(dead_code)]
fn capability_keys(values: &[AgentCapability]) -> Vec<String> {
    values
        .iter()
        .map(|capability| capability.as_str().to_owned())
        .collect()
}
