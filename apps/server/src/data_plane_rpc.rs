//! Console-facing data-plane control RPCs and the separate mTLS agent broker.

use std::{
    collections::HashMap, fs, net::SocketAddr, os::unix::fs::PermissionsExt, path::PathBuf,
    pin::Pin, sync::Arc, time::Duration as StdDuration,
};

use chrono::{DateTime, Duration, Utc};
use erpx_domain::{
    access::Scope,
    corporate_identity::{
        BUSINESS_UNIT_BRAND_MANAGE, BUSINESS_UNIT_BRAND_PUBLISH, BUSINESS_UNIT_BRAND_READ,
        BrandAssetKind, BrandAssetManifest, BrandAssetReference, BrandCategory, BrandDensity,
        BrandElevation, BrandFontPack, BrandLifecycle, BrandMotion, BrandNavigation, BrandPalette,
        BrandRadius, BrandScope, BrandTokenSet, CorporateIdentityProfile, HexColor,
        PLATFORM_BRAND_LOCK, PLATFORM_BRAND_MANAGE, PLATFORM_BRAND_PUBLISH, PLATFORM_BRAND_READ,
        ResolvedCorporateIdentity, TENANT_BRAND_LOCK, TENANT_BRAND_MANAGE, TENANT_BRAND_PUBLISH,
        TENANT_BRAND_READ,
    },
    data_plane::{
        AgentCapability, BUSINESS_UNIT_CONFIGURATION_MANAGE, BUSINESS_UNIT_CONFIGURATION_READ,
        BusinessUnitConfiguration, BusinessUnitDirectoryRoleMapping, BusinessUnitId,
        ConfigurationLayer, ConfigurationLifecycle, DataPlaneAgentBinding, DataPlaneAgentState,
        DesiredConfiguration, ObservedDataPlaneState, RootAccessApproval, RootAccessMode,
        RootAccessRequest, SecretReference, TENANT_AGENT_ENROLL, TENANT_INFRASTRUCTURE_APPLY,
        TENANT_INFRASTRUCTURE_APPROVE, TENANT_INFRASTRUCTURE_PROPOSE, TENANT_INFRASTRUCTURE_READ,
        TENANT_ROOT_ACCESS_APPROVE, TENANT_ROOT_ACCESS_REQUEST, TENANT_ROOT_ACCESS_REVOKE,
        TenantDeploymentMode, TenantInfrastructureProfile,
    },
};
use erpx_infrastructure::{
    ApprovedDesiredConfiguration, CorporateIdentityHistoryEntry, CorporateIdentityRollback,
    DataPlaneEnrollmentConsumption, DataPlaneEnrollmentIssue, PersistenceError, RootAccessGrant,
    RootAccessQueueRecord, SqlxCorporateIdentityStore, SqlxDataPlaneControlStore,
};
use erpx_proto::{
    common::v1::{CommandContext, QueryContext, TenantScope as ProtoTenantScope, tenant_scope},
    platform::{
        BrandAssetManifestMessage, BrandAssetReferenceMessage, BrandPaletteMessage,
        BrandScopeMessage, BrandTokenSetMessage, BusinessUnitConfigurationMessage,
        BusinessUnitDirectoryRoleMappingMessage,
        ConfigurationLifecycle as ProtoConfigurationLifecycle, ControlPlaneEnvelope,
        CorporateIdentityHistoryEntryMessage, CorporateIdentityProfileMessage,
        DataPlaneAgentEnvelope, DataPlaneAgentState as ProtoDataPlaneAgentState,
        DecideDesiredConfigurationRequest, DecideDesiredConfigurationResponse,
        DecideRootAccessRequest, DecideRootAccessResponse, EnrollDataPlaneAgentRequest,
        EnrollDataPlaneAgentResponse, GetBusinessUnitConfigurationRequest,
        GetBusinessUnitConfigurationResponse, GetCorporateIdentityDraftRequest,
        GetCorporateIdentityDraftResponse, GetNavigationManifestRequest,
        GetNavigationManifestResponse, GetResolvedCorporateIdentityRequest,
        GetResolvedCorporateIdentityResponse, GetTenantInfrastructureRequest,
        GetTenantInfrastructureResponse, IssueDataPlaneEnrollmentRequest,
        IssueDataPlaneEnrollmentResponse, ListCorporateIdentityHistoryRequest,
        ListCorporateIdentityHistoryResponse, ListRootAccessRequestsRequest,
        ListRootAccessRequestsResponse, NavigationManifestGroup, NavigationManifestItem,
        ProposeDesiredConfigurationRequest, ProposeDesiredConfigurationResponse,
        PublishCorporateIdentityDraftRequest, PublishCorporateIdentityDraftResponse,
        ReportDataPlaneObservationRequest, ReportDataPlaneObservationResponse,
        RequestRootAccessRequest, RequestRootAccessResponse, ResolvedCorporateIdentityMessage,
        RevokeRootAccessRequest, RevokeRootAccessResponse, RollbackCorporateIdentityRequest,
        RollbackCorporateIdentityResponse, RootAccessCapabilityMessage, RootAccessQueueEntry,
        RootAccessRequestMessage, RootAccessRevocationMessage,
        SaveBusinessUnitConfigurationRequest, SaveBusinessUnitConfigurationResponse,
        SaveCorporateIdentityDraftRequest, SaveCorporateIdentityDraftResponse,
        SaveTenantInfrastructureProfileRequest, SaveTenantInfrastructureProfileResponse,
        TenantDeploymentMode as ProtoDeploymentMode, TenantInfrastructureReadModel,
        data_plane_agent_envelope,
        data_plane_agent_service_server::{DataPlaneAgentService, DataPlaneAgentServiceServer},
        data_plane_control_service_server::{
            DataPlaneControlService, DataPlaneControlServiceServer,
        },
    },
};
use prost::Message;
use rand_core::{OsRng, RngCore};
use ring::signature::Ed25519KeyPair;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::sync::{RwLock, mpsc};
use tokio_stream::{Stream, wrappers::ReceiverStream};
use tonic::transport::{Certificate, Identity, ServerTlsConfig};
use tonic::{Request, Response, Status, Streaming};
use uuid::Uuid;

use crate::{
    AppState, access_scope, command_domain, query_context, require_actor_match,
    validate_authenticated_device_ref,
};

const MAX_AGENT_MESSAGE_BYTES: usize = 1024 * 1024;
const MAX_CONFIGURATION_JSON_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone)]
struct AgentConnection {
    tenant_id: String,
    sender: mpsc::Sender<Result<ControlPlaneEnvelope, Status>>,
}

#[derive(Debug, Clone)]
pub(crate) struct DataPlaneBroker {
    store: SqlxDataPlaneControlStore,
    connections: Arc<RwLock<HashMap<Uuid, AgentConnection>>>,
    signer: Option<Arc<DataPlaneSigningKey>>,
}

impl DataPlaneBroker {
    pub(crate) fn new(
        store: SqlxDataPlaneControlStore,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            store,
            connections: Arc::new(RwLock::new(HashMap::new())),
            signer: DataPlaneSigningKey::from_env()?.map(Arc::new),
        })
    }

    #[allow(dead_code)]
    pub(crate) async fn deliver(
        &self,
        agent_id: Uuid,
        envelope: ControlPlaneEnvelope,
    ) -> Result<(), Status> {
        let sender = self
            .connections
            .read()
            .await
            .get(&agent_id)
            .map(|connection| connection.sender.clone())
            .ok_or_else(|| Status::unavailable("customer data plane unavailable"))?;
        sender
            .send(Ok(envelope))
            .await
            .map_err(|_| Status::unavailable("customer data plane unavailable"))
    }

    async fn deliver_desired(&self, approved: ApprovedDesiredConfiguration) -> Result<(), Status> {
        let signer = self.signer.as_ref().ok_or_else(|| {
            Status::failed_precondition("data-plane signing key is not configured")
        })?;
        let agent_id = approved
            .desired
            .target_agent_id
            .ok_or_else(|| Status::failed_precondition("configuration has no target agent"))?;
        let mut message = approved_configuration_message(approved, signer.key_id())?;
        signer.sign_desired(&mut message)?;
        self.deliver(
            agent_id,
            ControlPlaneEnvelope {
                payload: Some(
                    erpx_proto::platform::control_plane_envelope::Payload::DesiredConfiguration(
                        message,
                    ),
                ),
            },
        )
        .await
    }
}

struct DataPlaneSigningKey {
    key_id: String,
    key_pair: Ed25519KeyPair,
}

impl std::fmt::Debug for DataPlaneSigningKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DataPlaneSigningKey")
            .field("key_id", &self.key_id)
            .field("public_key", &"<pinned>")
            .finish()
    }
}

impl DataPlaneSigningKey {
    fn from_env() -> Result<Option<Self>, Box<dyn std::error::Error>> {
        let key_id = std::env::var("ERPX_DATA_PLANE_SIGNING_KEY_ID").ok();
        let key_file = std::env::var_os("ERPX_DATA_PLANE_SIGNING_KEY_FILE").map(PathBuf::from);
        match (key_id, key_file) {
            (None, None) => Ok(None),
            (Some(key_id), Some(key_file)) => {
                validate_reference(&key_id, "data-plane signing key ID", 128)?;
                let pkcs8 = read_tls_file(key_file, true)?;
                let key_pair = Ed25519KeyPair::from_pkcs8(&pkcs8)
                    .map_err(|_| "data-plane Ed25519 PKCS#8 key is invalid")?;
                Ok(Some(Self { key_id, key_pair }))
            }
            _ => Err("data-plane signing key configuration is incomplete".into()),
        }
    }

    fn key_id(&self) -> &str {
        &self.key_id
    }

    fn sign_desired(
        &self,
        message: &mut erpx_proto::platform::DesiredConfigurationMessage,
    ) -> Result<(), Status> {
        if !message.control_plane_signature.is_empty() {
            return Err(Status::invalid_argument(
                "desired message is already signed",
            ));
        }
        message.control_plane_signature = self
            .key_pair
            .sign(&message.encode_to_vec())
            .as_ref()
            .to_vec();
        Ok(())
    }

    fn sign_root_capability(
        &self,
        message: &mut RootAccessCapabilityMessage,
    ) -> Result<(), Status> {
        if !message.control_plane_signature.is_empty() {
            return Err(Status::invalid_argument(
                "Root capability is already signed",
            ));
        }
        message.control_plane_signature = self
            .key_pair
            .sign(&message.encode_to_vec())
            .as_ref()
            .to_vec();
        Ok(())
    }

    fn sign_root_revocation(
        &self,
        message: &mut RootAccessRevocationMessage,
    ) -> Result<(), Status> {
        if !message.control_plane_signature.is_empty() {
            return Err(Status::invalid_argument(
                "Root revocation is already signed",
            ));
        }
        message.control_plane_signature = self
            .key_pair
            .sign(&message.encode_to_vec())
            .as_ref()
            .to_vec();
        Ok(())
    }
}

#[derive(Clone)]
pub(crate) struct DataPlaneControlRpcImpl {
    pub(crate) state: AppState,
    pub(crate) broker: Arc<DataPlaneBroker>,
}

#[tonic::async_trait]
impl DataPlaneControlService for DataPlaneControlRpcImpl {
    async fn get_tenant_infrastructure(
        &self,
        request: Request<GetTenantInfrastructureRequest>,
    ) -> Result<Response<GetTenantInfrastructureResponse>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let boundary = query_context(&request, request.get_ref().context.as_ref())?;
        require_actor_match(
            &claims.principal_ref,
            &boundary.actor,
            "query actor must match the authenticated principal",
        )?;
        let scope = proto_query_scope(request.get_ref().context.as_ref())?;
        let permission = if matches!(scope, Scope::Platform) {
            erpx_domain::data_plane::PLATFORM_INFRASTRUCTURE_READ
        } else {
            TENANT_INFRASTRUCTURE_READ
        };
        self.state
            .auth
            .authorize_role(&self.state.store, &claims.principal_ref, &scope, permission)
            .await?;
        let tenant_id = validate_reference(&request.get_ref().tenant_id, "tenant_id", 128)?;
        let summary = SqlxDataPlaneControlStore::new(self.state.store.clone())
            .infrastructure_summary(tenant_id, Utc::now())
            .await
            .map_err(persistence_status)?;
        Ok(Response::new(GetTenantInfrastructureResponse {
            infrastructure: Some(TenantInfrastructureReadModel {
                tenant_id: summary.tenant_id,
                root_holding_id: summary.root_holding_id.to_string(),
                deployment_mode: proto_deployment_mode(summary.deployment_mode) as i32,
                profile_version: summary.profile_version,
                agent_id: summary
                    .agent_id
                    .map_or_else(String::new, |id| id.to_string()),
                agent_state: summary
                    .agent_state
                    .map_or(ProtoDataPlaneAgentState::Unspecified, proto_agent_state)
                    as i32,
                agent_version: summary.agent_version.unwrap_or_default(),
                observed_version: summary.observed_version.unwrap_or_default(),
                desired_version: summary.desired_version.unwrap_or_default(),
                last_heartbeat_at: summary
                    .last_heartbeat_at
                    .map_or_else(String::new, |at| at.to_rfc3339()),
                observed_at: summary
                    .observed_at
                    .map_or_else(String::new, |at| at.to_rfc3339()),
                configuration_drifted: summary.configuration_drifted,
                write_healthy: summary.write_healthy.unwrap_or(false),
                read_healthy: summary.read_healthy.unwrap_or(false),
                storage_healthy: summary.storage_healthy.unwrap_or(false),
                directory_ready: summary.directory_ready.unwrap_or(false),
                projections_healthy: summary.projections_healthy.unwrap_or(false),
                backup_verified: summary.backup_verified.unwrap_or(false),
                root_access_active: summary.root_access_active,
                customer_managed_secrets: matches!(
                    summary.deployment_mode,
                    TenantDeploymentMode::CustomerDataPlane
                ),
            }),
            correlation_id: boundary.correlation_id.to_string(),
        }))
    }

    async fn save_tenant_infrastructure_profile(
        &self,
        request: Request<SaveTenantInfrastructureProfileRequest>,
    ) -> Result<Response<SaveTenantInfrastructureProfileResponse>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let command = command_domain(&request, request.get_ref().context.as_ref())?;
        require_actor_match(
            &claims.principal_ref,
            &command.actor.principal_ref,
            "command actor must match the authenticated principal",
        )?;
        let scope = access_scope(&command.tenant_scope);
        let profile = request
            .get_ref()
            .profile
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("infrastructure profile is required"))?;
        let deployment_mode = domain_deployment_mode(profile.deployment_mode)?;
        if matches!(scope, Scope::Platform)
            && matches!(deployment_mode, TenantDeploymentMode::CustomerDataPlane)
        {
            return Err(Status::failed_precondition(
                "ERPX Root must propose customer-hosted changes for customer approval",
            ));
        }
        let permission = if matches!(scope, Scope::Platform) {
            erpx_domain::data_plane::PLATFORM_INFRASTRUCTURE_MANAGE
        } else {
            TENANT_INFRASTRUCTURE_APPLY
        };
        self.state
            .auth
            .authorize_role(&self.state.store, &claims.principal_ref, &scope, permission)
            .await?;
        self.state
            .auth
            .authorize_mfa(&claims, command_mfa(request.get_ref().context.as_ref()))
            .await?;
        let profile_version = profile.profile_version;
        let domain_profile = TenantInfrastructureProfile {
            tenant_id: validate_reference(&profile.tenant_id, "tenant_id", 128)?.to_owned(),
            holding_id: parse_uuid(&profile.root_holding_id, "root_holding_id")?,
            deployment_mode,
            write_database_ref: SecretReference::new(profile.write_database_ref.clone())
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
            read_database_ref: SecretReference::new(profile.read_database_ref.clone())
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
            storage_ref: SecretReference::new(profile.storage_ref.clone())
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
            directory_credential_ref: if profile.directory_credential_ref.is_empty() {
                None
            } else {
                Some(
                    SecretReference::new(profile.directory_credential_ref.clone())
                        .map_err(|error| Status::invalid_argument(error.to_string()))?,
                )
            },
            backup_profile_ref: SecretReference::new(profile.backup_profile_ref.clone())
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
            version: profile_version,
        };
        SqlxDataPlaneControlStore::new(self.state.store.clone())
            .save_infrastructure_profile(
                &domain_profile,
                request.get_ref().expected_profile_version,
                &claims.principal_ref,
                &request.get_ref().reason,
                command.correlation_id,
                Utc::now(),
            )
            .await
            .map_err(persistence_status)?;
        Ok(Response::new(SaveTenantInfrastructureProfileResponse {
            profile_version,
            correlation_id: command.correlation_id.to_string(),
            awaiting_customer_approval: false,
        }))
    }

    async fn get_business_unit_configuration(
        &self,
        request: Request<GetBusinessUnitConfigurationRequest>,
    ) -> Result<Response<GetBusinessUnitConfigurationResponse>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let boundary = query_context(&request, request.get_ref().context.as_ref())?;
        require_actor_match(
            &claims.principal_ref,
            &boundary.actor,
            "query actor must match the authenticated principal",
        )?;
        let scope = proto_query_scope(request.get_ref().context.as_ref())?;
        self.state
            .auth
            .authorize_role(
                &self.state.store,
                &claims.principal_ref,
                &scope,
                BUSINESS_UNIT_CONFIGURATION_READ,
            )
            .await?;
        let tenant_id = validate_reference(&request.get_ref().tenant_id, "tenant_id", 128)?;
        let business_unit = BusinessUnitId::parse(&request.get_ref().business_unit)
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let store = SqlxDataPlaneControlStore::new(self.state.store.clone());
        let summary = store
            .infrastructure_summary(tenant_id, Utc::now())
            .await
            .map_err(persistence_status)?;
        if !scope.contains(&Scope::Holding {
            holding_id: summary.root_holding_id,
        }) {
            return Err(Status::permission_denied(
                "business-unit configuration is outside the authorized holding",
            ));
        }
        let configuration = store
            .business_unit_configuration(tenant_id, business_unit)
            .await
            .map_err(persistence_status)?;
        Ok(Response::new(GetBusinessUnitConfigurationResponse {
            exists: configuration.is_some(),
            configuration: configuration
                .as_ref()
                .map(business_unit_configuration_message)
                .transpose()?,
            correlation_id: boundary.correlation_id.to_string(),
        }))
    }

    async fn save_business_unit_configuration(
        &self,
        request: Request<SaveBusinessUnitConfigurationRequest>,
    ) -> Result<Response<SaveBusinessUnitConfigurationResponse>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let command = command_domain(&request, request.get_ref().context.as_ref())?;
        require_actor_match(
            &claims.principal_ref,
            &command.actor.principal_ref,
            "command actor must match the authenticated principal",
        )?;
        let scope = access_scope(&command.tenant_scope);
        if matches!(scope, Scope::Platform) {
            return Err(Status::permission_denied(
                "ERPX Root must use a customer-approved desired configuration",
            ));
        }
        self.state
            .auth
            .authorize_role(
                &self.state.store,
                &claims.principal_ref,
                &scope,
                BUSINESS_UNIT_CONFIGURATION_MANAGE,
            )
            .await?;
        self.state
            .auth
            .authorize_mfa(&claims, command_mfa(request.get_ref().context.as_ref()))
            .await?;
        let input =
            request.get_ref().configuration.as_ref().ok_or_else(|| {
                Status::invalid_argument("business-unit configuration is required")
            })?;
        let organization_scope = proto_scope(
            input
                .organization_scope
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("organization_scope is required"))?,
        )?;
        if !scope.contains(&organization_scope) {
            return Err(Status::permission_denied(
                "business-unit configuration exceeds the delegated scope",
            ));
        }
        let configuration_holding_id = parse_uuid(&input.root_holding_id, "root_holding_id")?;
        if holding_id(&organization_scope)? != configuration_holding_id {
            return Err(Status::permission_denied(
                "business-unit configuration holding does not match its scope",
            ));
        }
        let business_unit = BusinessUnitId::parse(&input.business_unit)
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let at = Utc::now();
        let configuration = BusinessUnitConfiguration {
            tenant_id: validate_reference(&input.tenant_id, "tenant_id", 128)?.to_owned(),
            holding_id: configuration_holding_id,
            business_unit,
            configuration_version: input.configuration_version,
            active: input.active,
            organization_scope,
            directory_role_mappings: input
                .directory_role_mappings
                .iter()
                .map(|mapping| {
                    business_unit_role_mapping(mapping, business_unit, configuration_holding_id)
                })
                .collect::<Result<Vec<_>, _>>()?,
            integration_references: input.integration_references.clone(),
            retention_policy: bounded_json_object(
                &input.retention_policy_json,
                "retention_policy_json",
            )?,
            projection_policy: bounded_json_object(
                &input.projection_policy_json,
                "projection_policy_json",
            )?,
            workflow_policy: bounded_json_object(
                &input.workflow_policy_json,
                "workflow_policy_json",
            )?,
            updated_by: claims.principal_ref,
            updated_at: at,
        };
        configuration
            .validate()
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        SqlxDataPlaneControlStore::new(self.state.store.clone())
            .save_business_unit_configuration(
                &configuration,
                request.get_ref().expected_configuration_version,
                command.correlation_id,
            )
            .await
            .map_err(persistence_status)?;
        Ok(Response::new(SaveBusinessUnitConfigurationResponse {
            configuration_version: configuration.configuration_version,
            correlation_id: command.correlation_id.to_string(),
        }))
    }

    async fn issue_data_plane_enrollment(
        &self,
        request: Request<IssueDataPlaneEnrollmentRequest>,
    ) -> Result<Response<IssueDataPlaneEnrollmentResponse>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let command = command_domain(&request, request.get_ref().context.as_ref())?;
        require_actor_match(
            &claims.principal_ref,
            &command.actor.principal_ref,
            "command actor must match the authenticated principal",
        )?;
        let scope = access_scope(&command.tenant_scope);
        if matches!(scope, Scope::Platform) {
            return Err(Status::permission_denied(
                "customer approval is required before issuing an agent enrollment",
            ));
        }
        self.state
            .auth
            .authorize_role(
                &self.state.store,
                &claims.principal_ref,
                &scope,
                TENANT_AGENT_ENROLL,
            )
            .await?;
        self.state
            .auth
            .authorize_mfa(&claims, command_mfa(request.get_ref().context.as_ref()))
            .await?;
        let lifetime = request.get_ref().lifetime_seconds;
        if !(60..=1_800).contains(&lifetime) {
            return Err(Status::invalid_argument(
                "agent enrollment lifetime must be between 60 and 1800 seconds",
            ));
        }
        let manifest_hash = decode_hash(
            &request.get_ref().signed_manifest_digest,
            "signed_manifest_digest",
        )?;
        let mut token = [0_u8; 32];
        OsRng.fill_bytes(&mut token);
        let token_hash: [u8; 32] = Sha256::digest(token).into();
        let enrollment_id = Uuid::new_v4();
        let at = Utc::now();
        let expires_at = at + Duration::seconds(i64::from(lifetime));
        self.broker
            .store
            .issue_agent_enrollment(&DataPlaneEnrollmentIssue {
                enrollment_id,
                tenant_id: validate_reference(&request.get_ref().tenant_id, "tenant_id", 128)?
                    .to_owned(),
                root_holding_id: parse_uuid(&request.get_ref().root_holding_id, "root_holding_id")?,
                token_hash,
                manifest_hash,
                issued_by: claims.principal_ref,
                issued_at: at,
                expires_at,
                correlation_id: command.correlation_id,
            })
            .await
            .map_err(persistence_status)?;
        Ok(Response::new(IssueDataPlaneEnrollmentResponse {
            enrollment_id: enrollment_id.to_string(),
            one_time_bootstrap_token: token.to_vec(),
            expires_at: expires_at.to_rfc3339(),
            correlation_id: command.correlation_id.to_string(),
        }))
    }

    async fn propose_desired_configuration(
        &self,
        request: Request<ProposeDesiredConfigurationRequest>,
    ) -> Result<Response<ProposeDesiredConfigurationResponse>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let command = command_domain(&request, request.get_ref().context.as_ref())?;
        require_actor_match(
            &claims.principal_ref,
            &command.actor.principal_ref,
            "command actor must match the authenticated principal",
        )?;
        let scope = access_scope(&command.tenant_scope);
        self.state
            .auth
            .authorize_role(
                &self.state.store,
                &claims.principal_ref,
                &scope,
                TENANT_INFRASTRUCTURE_PROPOSE,
            )
            .await?;
        let input = request
            .get_ref()
            .desired
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("desired configuration is required"))?;
        let at = Utc::now();
        let desired_state: Value = serde_json::from_slice(&input.desired_state_json)
            .map_err(|_| Status::invalid_argument("desired_state_json is invalid"))?;
        let desired = DesiredConfiguration {
            id: parse_uuid(&input.configuration_id, "configuration_id")?,
            tenant_id: validate_reference(&input.tenant_id, "tenant_id", 128)?.to_owned(),
            holding_id: parse_uuid(&input.root_holding_id, "root_holding_id")?,
            deployment_mode: domain_deployment_mode(input.deployment_mode)?,
            target_agent_id: optional_uuid(&input.target_agent_id, "target_agent_id")?,
            layer: parse_configuration_layer(&input.configuration_layer, &input.business_unit)?,
            expected_observed_version: input.expected_observed_version,
            desired_version: input.desired_version,
            lifecycle: parse_configuration_lifecycle(input.lifecycle)?,
            configuration_hash: slice_hash(&input.configuration_hash, "configuration_hash")?,
            secret_references: input
                .secret_references
                .iter()
                .map(|value| {
                    SecretReference::new(value.clone())
                        .map_err(|error| Status::invalid_argument(error.to_string()))
                })
                .collect::<Result<Vec<_>, _>>()?,
            required_capabilities: input
                .required_capabilities
                .iter()
                .map(|value| parse_agent_capability(value))
                .collect::<Result<Vec<_>, _>>()?,
            requester: claims.principal_ref,
            reason: input.reason.clone(),
            expires_at: parse_time(&input.expires_at, "expires_at")?,
        };
        self.broker
            .store
            .propose_configuration(&desired, &desired_state, command.correlation_id, at)
            .await
            .map_err(persistence_status)?;
        Ok(Response::new(ProposeDesiredConfigurationResponse {
            configuration_id: desired.id.to_string(),
            desired_version: desired.desired_version,
            lifecycle: proto_configuration_lifecycle(desired.lifecycle) as i32,
            correlation_id: command.correlation_id.to_string(),
        }))
    }

    async fn decide_desired_configuration(
        &self,
        request: Request<DecideDesiredConfigurationRequest>,
    ) -> Result<Response<DecideDesiredConfigurationResponse>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let command = command_domain(&request, request.get_ref().context.as_ref())?;
        require_actor_match(
            &claims.principal_ref,
            &command.actor.principal_ref,
            "command actor must match the authenticated principal",
        )?;
        let scope = access_scope(&command.tenant_scope);
        if matches!(scope, Scope::Platform) {
            return Err(Status::permission_denied(
                "customer approval must use a customer holding scope",
            ));
        }
        self.state
            .auth
            .authorize_role(
                &self.state.store,
                &claims.principal_ref,
                &scope,
                TENANT_INFRASTRUCTURE_APPROVE,
            )
            .await?;
        self.state
            .auth
            .authorize_mfa(&claims, command_mfa(request.get_ref().context.as_ref()))
            .await?;
        let configuration_id = parse_uuid(&request.get_ref().configuration_id, "configuration_id")?;
        let at = Utc::now();
        let lifecycle = if request.get_ref().approve {
            if self.broker.signer.is_none() {
                return Err(Status::failed_precondition(
                    "data-plane signing key is not configured",
                ));
            }
            self.broker
                .store
                .approve_configuration(
                    configuration_id,
                    &claims.principal_ref,
                    at,
                    at,
                    command.correlation_id,
                )
                .await
                .map_err(persistence_status)?;
            let approved = self
                .broker
                .store
                .approved_configuration(configuration_id, at)
                .await
                .map_err(persistence_status)?;
            if let Err(status) = self.broker.deliver_desired(approved).await {
                if status.code() == tonic::Code::Unavailable {
                    tracing::info!(
                        %configuration_id,
                        "approved configuration remains pending until the customer agent reconnects"
                    );
                } else {
                    return Err(status);
                }
            }
            ConfigurationLifecycle::Approved
        } else {
            self.broker
                .store
                .reject_configuration(
                    configuration_id,
                    &claims.principal_ref,
                    &request.get_ref().reason,
                    at,
                    command.correlation_id,
                )
                .await
                .map_err(persistence_status)?;
            ConfigurationLifecycle::Rejected
        };
        Ok(Response::new(DecideDesiredConfigurationResponse {
            configuration_id: configuration_id.to_string(),
            lifecycle: proto_configuration_lifecycle(lifecycle) as i32,
            correlation_id: command.correlation_id.to_string(),
        }))
    }

    async fn request_root_access(
        &self,
        request: Request<RequestRootAccessRequest>,
    ) -> Result<Response<RequestRootAccessResponse>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let command = command_domain(&request, request.get_ref().context.as_ref())?;
        require_actor_match(
            &claims.principal_ref,
            &command.actor.principal_ref,
            "command actor must match the authenticated principal",
        )?;
        let command_scope = access_scope(&command.tenant_scope);
        self.state
            .auth
            .authorize_role(
                &self.state.store,
                &claims.principal_ref,
                &command_scope,
                TENANT_ROOT_ACCESS_REQUEST,
            )
            .await?;
        let input = request
            .get_ref()
            .access_request
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("Root access request is required"))?;
        let active_device_id = self
            .state
            .auth
            .identity
            .active_session_device_id(claims.session_id, Utc::now())
            .await
            .map_err(|_| Status::unauthenticated("authenticated session device unavailable"))?;
        validate_authenticated_device_ref(active_device_id, &input.device_ref)?;
        let tenant_scope = proto_scope(
            input
                .tenant_scope
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("tenant_scope is required"))?,
        )?;
        let holding_id = holding_id(&tenant_scope)?;
        let domain_request = RootAccessRequest {
            id: parse_uuid(&input.request_id, "request_id")?,
            requester: claims.principal_ref,
            device_ref: validate_reference(&input.device_ref, "device_ref", 255)?.to_owned(),
            tenant_id: validate_reference(&input.tenant_id, "tenant_id", 128)?.to_owned(),
            agent_id: parse_uuid(&input.agent_id, "agent_id")?,
            tenant_scope,
            mode: parse_root_access_mode(input.mode)?,
            allowed_actions: input.allowed_actions.clone(),
            business_units: input
                .business_units
                .iter()
                .map(|unit| {
                    BusinessUnitId::parse(unit)
                        .map_err(|error| Status::invalid_argument(error.to_string()))
                })
                .collect::<Result<Vec<_>, _>>()?,
            reason: input.reason.clone(),
            ticket_reference: input.ticket_reference.clone(),
            requested_at: parse_time(&input.requested_at, "requested_at")?,
            requested_until: parse_time(&input.requested_until, "requested_until")?,
        };
        self.broker
            .store
            .request_root_access(&domain_request, holding_id, command.correlation_id)
            .await
            .map_err(persistence_status)?;
        Ok(Response::new(RequestRootAccessResponse {
            request_id: domain_request.id.to_string(),
            state: "awaiting_customer_approval".into(),
            correlation_id: command.correlation_id.to_string(),
        }))
    }

    async fn decide_root_access(
        &self,
        request: Request<DecideRootAccessRequest>,
    ) -> Result<Response<DecideRootAccessResponse>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let command = command_domain(&request, request.get_ref().context.as_ref())?;
        require_actor_match(
            &claims.principal_ref,
            &command.actor.principal_ref,
            "command actor must match the authenticated principal",
        )?;
        let scope = access_scope(&command.tenant_scope);
        if matches!(scope, Scope::Platform) {
            return Err(Status::permission_denied(
                "Root access approval must use a customer holding scope",
            ));
        }
        self.state
            .auth
            .authorize_role(
                &self.state.store,
                &claims.principal_ref,
                &scope,
                TENANT_ROOT_ACCESS_APPROVE,
            )
            .await?;
        self.state
            .auth
            .authorize_mfa(&claims, command_mfa(request.get_ref().context.as_ref()))
            .await?;
        let request_id = parse_uuid(&request.get_ref().request_id, "request_id")?;
        let at = Utc::now();
        if request.get_ref().approve {
            let signer = self.broker.signer.as_ref().ok_or_else(|| {
                Status::failed_precondition("data-plane signing key is not configured")
            })?;
            let mut bearer = [0_u8; 32];
            OsRng.fill_bytes(&mut bearer);
            let token_hash: [u8; 32] = Sha256::digest(bearer).into();
            let capability = self
                .broker
                .store
                .approve_root_access(
                    &RootAccessGrant {
                        approval: RootAccessApproval {
                            request_id,
                            approver: claims.principal_ref.clone(),
                            approved_at: at,
                            fresh_mfa: true,
                        },
                        capability_id: Uuid::new_v4(),
                        capability_token_hash: token_hash,
                    },
                    command.correlation_id,
                )
                .await
                .map_err(persistence_status)?;
            let mut capability_message = RootAccessCapabilityMessage {
                contract_version: u32::from(erpx_domain::contracts::CONTRACT_VERSION),
                capability_id: capability.id.to_string(),
                request_id: capability.request_id.to_string(),
                one_time_capability_token: bearer.to_vec(),
                root_principal_ref: capability.root_principal.clone(),
                device_ref: capability.device_ref.clone(),
                tenant_id: capability.tenant_id.clone(),
                agent_id: capability.agent_id.to_string(),
                tenant_scope: Some(scope_to_proto(&capability.scope)),
                allowed_actions: capability.allowed_actions.clone(),
                business_units: capability
                    .business_units
                    .iter()
                    .map(|unit| unit.as_str().to_owned())
                    .collect(),
                issued_at: capability.issued_at.to_rfc3339(),
                expires_at: capability.expires_at.to_rfc3339(),
                correlation_id: command.correlation_id.to_string(),
                signing_key_id: signer.key_id().to_owned(),
                control_plane_signature: Vec::new(),
            };
            signer.sign_root_capability(&mut capability_message)?;
            let delivery = self
                .broker
                .deliver(
                    capability.agent_id,
                    ControlPlaneEnvelope {
                        payload: Some(
                            erpx_proto::platform::control_plane_envelope::Payload::RootAccessCapability(
                                capability_message,
                            ),
                        ),
                    },
                )
                .await;
            bearer.fill(0);
            if let Err(status) = delivery {
                self.broker
                    .store
                    .revoke_root_access(
                        request_id,
                        &claims.principal_ref,
                        "Root capability delivery failed; access remained closed",
                        command.correlation_id,
                        Utc::now(),
                    )
                    .await
                    .map_err(persistence_status)?;
                return Err(status);
            }
            Ok(Response::new(DecideRootAccessResponse {
                request_id: request_id.to_string(),
                state: "active".into(),
                expires_at: capability.expires_at.to_rfc3339(),
                correlation_id: command.correlation_id.to_string(),
            }))
        } else {
            self.broker
                .store
                .reject_root_access(
                    request_id,
                    &claims.principal_ref,
                    &request.get_ref().reason,
                    command.correlation_id,
                    at,
                )
                .await
                .map_err(persistence_status)?;
            Ok(Response::new(DecideRootAccessResponse {
                request_id: request_id.to_string(),
                state: "rejected".into(),
                expires_at: String::new(),
                correlation_id: command.correlation_id.to_string(),
            }))
        }
    }

    async fn revoke_root_access(
        &self,
        request: Request<RevokeRootAccessRequest>,
    ) -> Result<Response<RevokeRootAccessResponse>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let command = command_domain(&request, request.get_ref().context.as_ref())?;
        require_actor_match(
            &claims.principal_ref,
            &command.actor.principal_ref,
            "command actor must match the authenticated principal",
        )?;
        let scope = access_scope(&command.tenant_scope);
        self.state
            .auth
            .authorize_role(
                &self.state.store,
                &claims.principal_ref,
                &scope,
                TENANT_ROOT_ACCESS_REVOKE,
            )
            .await?;
        self.state
            .auth
            .authorize_mfa(&claims, command_mfa(request.get_ref().context.as_ref()))
            .await?;
        let request_id = parse_uuid(&request.get_ref().request_id, "request_id")?;
        let capability = self
            .broker
            .store
            .root_access_capability(request_id)
            .await
            .map_err(persistence_status)?;
        let revoked_at = Utc::now();
        self.broker
            .store
            .revoke_root_access(
                request_id,
                &claims.principal_ref,
                &request.get_ref().reason,
                command.correlation_id,
                revoked_at,
            )
            .await
            .map_err(persistence_status)?;
        if let Some(signer) = &self.broker.signer {
            let mut revocation = RootAccessRevocationMessage {
                contract_version: u32::from(erpx_domain::contracts::CONTRACT_VERSION),
                capability_id: capability.id.to_string(),
                tenant_id: capability.tenant_id,
                agent_id: capability.agent_id.to_string(),
                revoked_at: revoked_at.to_rfc3339(),
                correlation_id: command.correlation_id.to_string(),
                signing_key_id: signer.key_id().to_owned(),
                control_plane_signature: Vec::new(),
            };
            signer.sign_root_revocation(&mut revocation)?;
            let _ = self
                .broker
                .deliver(
                    capability.agent_id,
                    ControlPlaneEnvelope {
                        payload: Some(
                            erpx_proto::platform::control_plane_envelope::Payload::RootAccessRevocation(
                                revocation,
                            ),
                        ),
                    },
                )
                .await;
        }
        Ok(Response::new(RevokeRootAccessResponse {
            revoked: true,
            correlation_id: command.correlation_id.to_string(),
        }))
    }

    async fn list_root_access_requests(
        &self,
        request: Request<ListRootAccessRequestsRequest>,
    ) -> Result<Response<ListRootAccessRequestsResponse>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let boundary = query_context(&request, request.get_ref().context.as_ref())?;
        require_actor_match(
            &claims.principal_ref,
            &boundary.actor,
            "query actor must match the authenticated principal",
        )?;
        let scope = proto_query_scope(request.get_ref().context.as_ref())?;
        let permission = if matches!(scope, Scope::Platform) {
            TENANT_ROOT_ACCESS_REQUEST
        } else {
            TENANT_ROOT_ACCESS_APPROVE
        };
        self.state
            .auth
            .authorize_role(&self.state.store, &claims.principal_ref, &scope, permission)
            .await?;
        let tenant_id = validate_reference(&request.get_ref().tenant_id, "tenant_id", 128)?;
        let store = SqlxDataPlaneControlStore::new(self.state.store.clone());
        let summary = store
            .infrastructure_summary(tenant_id, Utc::now())
            .await
            .map_err(persistence_status)?;
        if !scope.contains(&Scope::Holding {
            holding_id: summary.root_holding_id,
        }) {
            return Err(Status::permission_denied(
                "Root access queue is outside the authorized holding",
            ));
        }
        let page_size = match request.get_ref().page_size {
            0 => 50,
            value if value <= 100 => value,
            _ => {
                return Err(Status::invalid_argument(
                    "Root access queue page size must be at most 100",
                ));
            }
        };
        let entries = store
            .list_root_access_requests(tenant_id, request.get_ref().include_closed, page_size)
            .await
            .map_err(persistence_status)?;
        Ok(Response::new(ListRootAccessRequestsResponse {
            requests: entries
                .iter()
                .map(root_access_queue_entry)
                .collect::<Result<Vec<_>, _>>()?,
            correlation_id: boundary.correlation_id.to_string(),
        }))
    }

    async fn get_resolved_corporate_identity(
        &self,
        request: Request<GetResolvedCorporateIdentityRequest>,
    ) -> Result<Response<GetResolvedCorporateIdentityResponse>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let boundary = query_context(&request, request.get_ref().context.as_ref())?;
        require_actor_match(
            &claims.principal_ref,
            &boundary.actor,
            "query actor must match the authenticated principal",
        )?;
        let access = proto_query_scope(request.get_ref().context.as_ref())?;
        let target = parse_brand_scope(
            request
                .get_ref()
                .scope
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("brand scope is required"))?,
        )?;
        ensure_brand_scope_access(&access, &target)?;
        let permission = brand_permission(&access, &target, BrandOperation::Read);
        self.state
            .auth
            .authorize_role(
                &self.state.store,
                &claims.principal_ref,
                &access,
                permission,
            )
            .await?;
        let resolved = SqlxCorporateIdentityStore::new(self.state.store.clone())
            .resolved(
                target.tenant_id(),
                target.holding_id(),
                target.business_unit(),
            )
            .await
            .map_err(persistence_status)?;
        Ok(Response::new(GetResolvedCorporateIdentityResponse {
            identity: Some(resolved_identity_message(&resolved)),
            correlation_id: boundary.correlation_id.to_string(),
        }))
    }

    async fn get_corporate_identity_draft(
        &self,
        request: Request<GetCorporateIdentityDraftRequest>,
    ) -> Result<Response<GetCorporateIdentityDraftResponse>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let boundary = query_context(&request, request.get_ref().context.as_ref())?;
        require_actor_match(
            &claims.principal_ref,
            &boundary.actor,
            "query actor must match the authenticated principal",
        )?;
        let access = proto_query_scope(request.get_ref().context.as_ref())?;
        let target = parse_brand_scope(
            request
                .get_ref()
                .scope
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("brand scope is required"))?,
        )?;
        ensure_brand_scope_access(&access, &target)?;
        let permission = brand_permission(&access, &target, BrandOperation::Read);
        self.state
            .auth
            .authorize_role(
                &self.state.store,
                &claims.principal_ref,
                &access,
                permission,
            )
            .await?;
        let draft = SqlxCorporateIdentityStore::new(self.state.store.clone())
            .draft(&target)
            .await
            .map_err(persistence_status)?;
        Ok(Response::new(GetCorporateIdentityDraftResponse {
            exists: draft.is_some(),
            draft: draft.as_ref().map(corporate_identity_profile_message),
            correlation_id: boundary.correlation_id.to_string(),
        }))
    }

    async fn save_corporate_identity_draft(
        &self,
        request: Request<SaveCorporateIdentityDraftRequest>,
    ) -> Result<Response<SaveCorporateIdentityDraftResponse>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let command = command_domain(&request, request.get_ref().context.as_ref())?;
        require_actor_match(
            &claims.principal_ref,
            &command.actor.principal_ref,
            "command actor must match the authenticated principal",
        )?;
        let access = access_scope(&command.tenant_scope);
        let input = request
            .get_ref()
            .draft
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("corporate identity draft is required"))?;
        let mut draft = parse_corporate_identity_profile(input)?;
        draft.updated_by = claims.principal_ref.clone();
        ensure_brand_mutation_access(&access, &draft.scope)?;
        self.state
            .auth
            .authorize_role(
                &self.state.store,
                &claims.principal_ref,
                &access,
                brand_permission(&access, &draft.scope, BrandOperation::Manage),
            )
            .await?;
        if !draft.locked_categories.is_empty() {
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims.principal_ref,
                    &access,
                    brand_permission(&access, &draft.scope, BrandOperation::Lock),
                )
                .await?;
        }
        let saved = SqlxCorporateIdentityStore::new(self.state.store.clone())
            .save_draft(
                &draft,
                request.get_ref().expected_version,
                &request.get_ref().reason,
                command.correlation_id,
                Utc::now(),
            )
            .await
            .map_err(persistence_status)?;
        Ok(Response::new(SaveCorporateIdentityDraftResponse {
            draft: Some(corporate_identity_profile_message(&saved)),
            correlation_id: command.correlation_id.to_string(),
        }))
    }

    async fn publish_corporate_identity_draft(
        &self,
        request: Request<PublishCorporateIdentityDraftRequest>,
    ) -> Result<Response<PublishCorporateIdentityDraftResponse>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let command = command_domain(&request, request.get_ref().context.as_ref())?;
        require_actor_match(
            &claims.principal_ref,
            &command.actor.principal_ref,
            "command actor must match the authenticated principal",
        )?;
        let access = access_scope(&command.tenant_scope);
        let profile_id = parse_uuid(&request.get_ref().profile_id, "profile_id")?;
        let store = SqlxCorporateIdentityStore::new(self.state.store.clone());
        let target = store
            .profile_scope(profile_id)
            .await
            .map_err(persistence_status)?;
        ensure_brand_mutation_access(&access, &target)?;
        self.state
            .auth
            .authorize_role(
                &self.state.store,
                &claims.principal_ref,
                &access,
                brand_permission(&access, &target, BrandOperation::Publish),
            )
            .await?;
        self.state
            .auth
            .authorize_mfa(&claims, command_mfa(request.get_ref().context.as_ref()))
            .await?;
        let published = store
            .publish_draft(
                profile_id,
                request.get_ref().expected_draft_version,
                &claims.principal_ref,
                &request.get_ref().reason,
                command.correlation_id,
                Utc::now(),
            )
            .await
            .map_err(persistence_status)?;
        Ok(Response::new(PublishCorporateIdentityDraftResponse {
            published: Some(corporate_identity_profile_message(&published)),
            correlation_id: command.correlation_id.to_string(),
        }))
    }

    async fn rollback_corporate_identity(
        &self,
        request: Request<RollbackCorporateIdentityRequest>,
    ) -> Result<Response<RollbackCorporateIdentityResponse>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let command = command_domain(&request, request.get_ref().context.as_ref())?;
        require_actor_match(
            &claims.principal_ref,
            &command.actor.principal_ref,
            "command actor must match the authenticated principal",
        )?;
        let access = access_scope(&command.tenant_scope);
        let profile_id = parse_uuid(&request.get_ref().profile_id, "profile_id")?;
        let store = SqlxCorporateIdentityStore::new(self.state.store.clone());
        let target = store
            .profile_scope(profile_id)
            .await
            .map_err(persistence_status)?;
        ensure_brand_mutation_access(&access, &target)?;
        self.state
            .auth
            .authorize_role(
                &self.state.store,
                &claims.principal_ref,
                &access,
                brand_permission(&access, &target, BrandOperation::Publish),
            )
            .await?;
        self.state
            .auth
            .authorize_mfa(&claims, command_mfa(request.get_ref().context.as_ref()))
            .await?;
        let published = store
            .rollback(&CorporateIdentityRollback {
                profile_id,
                source_version: request.get_ref().source_version,
                expected_published_version: request.get_ref().expected_published_version,
                actor_ref: claims.principal_ref.clone(),
                reason: request.get_ref().reason.clone(),
                correlation_id: command.correlation_id,
                at: Utc::now(),
            })
            .await
            .map_err(persistence_status)?;
        Ok(Response::new(RollbackCorporateIdentityResponse {
            published: Some(corporate_identity_profile_message(&published)),
            correlation_id: command.correlation_id.to_string(),
        }))
    }

    async fn list_corporate_identity_history(
        &self,
        request: Request<ListCorporateIdentityHistoryRequest>,
    ) -> Result<Response<ListCorporateIdentityHistoryResponse>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let boundary = query_context(&request, request.get_ref().context.as_ref())?;
        require_actor_match(
            &claims.principal_ref,
            &boundary.actor,
            "query actor must match the authenticated principal",
        )?;
        let access = proto_query_scope(request.get_ref().context.as_ref())?;
        let target = parse_brand_scope(
            request
                .get_ref()
                .scope
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("brand scope is required"))?,
        )?;
        ensure_brand_scope_access(&access, &target)?;
        self.state
            .auth
            .authorize_role(
                &self.state.store,
                &claims.principal_ref,
                &access,
                brand_permission(&access, &target, BrandOperation::Read),
            )
            .await?;
        let entries = SqlxCorporateIdentityStore::new(self.state.store.clone())
            .history(&target, request.get_ref().page_size)
            .await
            .map_err(persistence_status)?;
        Ok(Response::new(ListCorporateIdentityHistoryResponse {
            entries: entries.iter().map(history_entry_message).collect(),
            correlation_id: boundary.correlation_id.to_string(),
        }))
    }

    async fn get_navigation_manifest(
        &self,
        request: Request<GetNavigationManifestRequest>,
    ) -> Result<Response<GetNavigationManifestResponse>, Status> {
        let claims = self.state.auth.authorize(&request).await?;
        let boundary = query_context(&request, request.get_ref().context.as_ref())?;
        require_actor_match(
            &claims.principal_ref,
            &boundary.actor,
            "query actor must match the authenticated principal",
        )?;
        let scope = proto_query_scope(request.get_ref().context.as_ref())?;
        let groups = navigation_manifest_groups(&self.state, &claims.principal_ref, &scope).await;
        let audience = if matches!(scope, Scope::Platform) {
            "platform"
        } else {
            "tenant"
        };
        let home_route = if matches!(scope, Scope::Platform) {
            "/platform"
        } else {
            "/workspaces/operations"
        };
        Ok(Response::new(GetNavigationManifestResponse {
            audience: audience.into(),
            home_route: home_route.into(),
            groups,
            correlation_id: boundary.correlation_id.to_string(),
        }))
    }
}

#[tonic::async_trait]
impl DataPlaneAgentService for DataPlaneBroker {
    async fn enroll(
        &self,
        request: Request<EnrollDataPlaneAgentRequest>,
    ) -> Result<Response<EnrollDataPlaneAgentResponse>, Status> {
        let fingerprint = peer_certificate_fingerprint(&request)?;
        let input = request.into_inner();
        if !input
            .certificate_fingerprint
            .eq_ignore_ascii_case(&fingerprint)
        {
            return Err(Status::permission_denied(
                "mTLS certificate does not match enrollment request",
            ));
        }
        validate_agent_envelope_common(
            input.contract_version,
            &input.request_id,
            &input.correlation_id,
        )?;
        if input.one_time_bootstrap_token.len() > 4096
            || input.signed_installation_manifest.is_empty()
            || input.signed_installation_manifest.len() > MAX_AGENT_MESSAGE_BYTES
        {
            return Err(Status::invalid_argument("enrollment proof is invalid"));
        }
        let at = Utc::now();
        let binding = DataPlaneAgentBinding {
            tenant_id: validate_reference(&input.tenant_id, "tenant_id", 128)?.to_owned(),
            holding_id: parse_uuid(&input.root_holding_id, "root_holding_id")?,
            agent_id: parse_uuid(&input.agent_id, "agent_id")?,
            certificate_fingerprint: fingerprint,
            capabilities: input
                .capabilities
                .iter()
                .map(|value| parse_agent_capability(value))
                .collect::<Result<Vec<_>, _>>()?,
            state: DataPlaneAgentState::Connected,
            observed_version: 0,
        };
        self.store
            .consume_agent_enrollment(&DataPlaneEnrollmentConsumption {
                enrollment_id: parse_uuid(&input.enrollment_id, "enrollment_id")?,
                presented_token_hash: Sha256::digest(&input.one_time_bootstrap_token).into(),
                presented_manifest_hash: Sha256::digest(&input.signed_installation_manifest).into(),
                binding: binding.clone(),
                agent_version: validate_reference(&input.agent_version, "agent_version", 128)?
                    .to_owned(),
                certificate_expires_at: parse_time(
                    &input.certificate_expires_at,
                    "certificate_expires_at",
                )?,
                enrolled_at: at,
            })
            .await
            .map_err(persistence_status)?;
        Ok(Response::new(EnrollDataPlaneAgentResponse {
            agent_id: binding.agent_id.to_string(),
            state: ProtoDataPlaneAgentState::Connected as i32,
            enrolled_at: at.to_rfc3339(),
            correlation_id: input.correlation_id,
            pinned_control_plane_signing_key_id: std::env::var("ERPX_DATA_PLANE_SIGNING_KEY_ID")
                .unwrap_or_default(),
        }))
    }

    async fn report_observation(
        &self,
        request: Request<ReportDataPlaneObservationRequest>,
    ) -> Result<Response<ReportDataPlaneObservationResponse>, Status> {
        let fingerprint = peer_certificate_fingerprint(&request)?;
        let input = request
            .get_ref()
            .observation
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("observation is required"))?;
        let observation = parse_observation(input)?;
        self.store
            .authenticate_agent_certificate(
                observation.agent_id,
                &observation.tenant_id,
                &fingerprint,
            )
            .await
            .map_err(persistence_status)?;
        let state = self
            .store
            .record_observation(&observation, &input.agent_version)
            .await
            .map_err(persistence_status)?;
        Ok(Response::new(ReportDataPlaneObservationResponse {
            state: proto_agent_state(state) as i32,
            correlation_id: observation.correlation_id.to_string(),
        }))
    }

    type ConnectStream = Pin<Box<dyn Stream<Item = Result<ControlPlaneEnvelope, Status>> + Send>>;

    async fn connect(
        &self,
        request: Request<Streaming<DataPlaneAgentEnvelope>>,
    ) -> Result<Response<Self::ConnectStream>, Status> {
        let fingerprint = peer_certificate_fingerprint(&request)?;
        let mut inbound = request.into_inner();
        let first = tokio::time::timeout(StdDuration::from_secs(10), inbound.message())
            .await
            .map_err(|_| Status::deadline_exceeded("initial agent heartbeat timed out"))??
            .ok_or_else(|| Status::unauthenticated("initial agent heartbeat is required"))?;
        let first_observation = match first.payload {
            Some(data_plane_agent_envelope::Payload::Observation(observation)) => observation,
            _ => {
                return Err(Status::unauthenticated(
                    "first agent stream message must be an observation",
                ));
            }
        };
        let observation = parse_observation(&first_observation)?;
        let binding = self
            .store
            .authenticate_agent_certificate(
                observation.agent_id,
                &observation.tenant_id,
                &fingerprint,
            )
            .await
            .map_err(persistence_status)?;
        self.store
            .record_observation(&observation, &first_observation.agent_version)
            .await
            .map_err(persistence_status)?;

        let (sender, receiver) = mpsc::channel(32);
        self.connections.write().await.insert(
            binding.agent_id,
            AgentConnection {
                tenant_id: binding.tenant_id.clone(),
                sender,
            },
        );
        let approved = self
            .store
            .next_approved_configuration(binding.agent_id, observation.observed_version, Utc::now())
            .await
            .map_err(persistence_status)?;
        if let Some(approved) = approved
            && let Err(error) = self.deliver_desired(approved).await
        {
            self.connections.write().await.remove(&binding.agent_id);
            return Err(error);
        }
        let broker = self.clone();
        tokio::spawn(async move {
            loop {
                let message = match inbound.message().await {
                    Ok(Some(message)) => message,
                    Ok(None) => break,
                    Err(error) => {
                        tracing::warn!(agent_id = %binding.agent_id, error = %error, "agent stream failed");
                        break;
                    }
                };
                match message.payload {
                    Some(data_plane_agent_envelope::Payload::Observation(input)) => {
                        match parse_observation(&input) {
                            Ok(observation)
                                if observation.agent_id == binding.agent_id
                                    && observation.tenant_id == binding.tenant_id =>
                            {
                                if let Err(error) = broker
                                    .store
                                    .record_observation(&observation, &input.agent_version)
                                    .await
                                {
                                    tracing::warn!(agent_id = %binding.agent_id, error = %error, "agent observation rejected");
                                    break;
                                }
                            }
                            _ => break,
                        }
                    }
                    Some(data_plane_agent_envelope::Payload::CommandResult(_)) => {
                        // Command results are correlated to pending typed broker
                        // commands. No raw diagnostic payload is logged here.
                    }
                    None => break,
                }
            }
            let removed = broker.connections.write().await.remove(&binding.agent_id);
            if removed
                .as_ref()
                .is_some_and(|connection| connection.tenant_id == binding.tenant_id)
            {
                let _ = broker
                    .store
                    .mark_agent_offline(binding.agent_id, Uuid::new_v4(), Utc::now())
                    .await;
            }
        });
        Ok(Response::new(Box::pin(ReceiverStream::new(receiver))))
    }
}

pub(crate) fn control_service(
    state: AppState,
    broker: Arc<DataPlaneBroker>,
) -> DataPlaneControlServiceServer<DataPlaneControlRpcImpl> {
    DataPlaneControlServiceServer::new(DataPlaneControlRpcImpl { state, broker })
}

/// Start the dedicated agent listener only when all mTLS settings are present.
/// It is separate from the browser/gRPC-Web listener so client certificates are
/// mandatory for agents without being required from console browsers.
pub(crate) async fn spawn_agent_broker(
    broker: Arc<DataPlaneBroker>,
) -> Result<(), Box<dyn std::error::Error>> {
    let settings = [
        "ERPX_DATA_PLANE_GRPC_ADDR",
        "ERPX_DATA_PLANE_SERVER_CERT_FILE",
        "ERPX_DATA_PLANE_SERVER_KEY_FILE",
        "ERPX_DATA_PLANE_CLIENT_CA_FILE",
    ];
    let configured = settings.iter().map(std::env::var_os).collect::<Vec<_>>();
    if configured.iter().all(Option::is_none) {
        tracing::info!("customer data-plane mTLS broker is not configured");
        return Ok(());
    }
    if configured.iter().any(Option::is_none) {
        return Err("customer data-plane mTLS broker configuration is incomplete".into());
    }
    let address: SocketAddr = configured[0]
        .as_ref()
        .and_then(|value| value.to_str())
        .ok_or("data-plane broker address is invalid")?
        .parse()?;
    let certificate = read_tls_file(PathBuf::from(configured[1].clone().unwrap()), false)?;
    let key = read_tls_file(PathBuf::from(configured[2].clone().unwrap()), true)?;
    let client_ca = read_tls_file(PathBuf::from(configured[3].clone().unwrap()), false)?;
    let tls = ServerTlsConfig::new()
        .identity(Identity::from_pem(certificate, key))
        .client_ca_root(Certificate::from_pem(client_ca));
    let server = tonic::transport::Server::builder().tls_config(tls)?;
    tracing::info!(%address, "ERPX customer data-plane mTLS broker listening");
    tokio::spawn(async move {
        let mut server = server;
        if let Err(error) = server
            .add_service(DataPlaneAgentServiceServer::new((*broker).clone()))
            .serve(address)
            .await
        {
            tracing::error!(error = %error, "customer data-plane broker stopped");
        }
    });
    Ok(())
}

fn read_tls_file(path: PathBuf, private: bool) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if !path.is_absolute() {
        return Err("data-plane TLS paths must be absolute".into());
    }
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() == 0
        || metadata.len() > 1024 * 1024
        || private && metadata.permissions().mode() & 0o777 != 0o600
    {
        return Err("data-plane TLS file is unsafe".into());
    }
    Ok(fs::read(path)?)
}

fn peer_certificate_fingerprint<T>(request: &Request<T>) -> Result<String, Status> {
    let certificates = request
        .peer_certs()
        .ok_or_else(|| Status::unauthenticated("mTLS client certificate is required"))?;
    let certificate = certificates
        .first()
        .ok_or_else(|| Status::unauthenticated("mTLS client certificate is required"))?;
    Ok(hex::encode(Sha256::digest(certificate.as_ref())))
}

fn parse_observation(
    input: &erpx_proto::platform::DataPlaneObservationMessage,
) -> Result<ObservedDataPlaneState, Status> {
    validate_agent_envelope_common(
        input.contract_version,
        &input.request_id,
        &input.correlation_id,
    )?;
    Ok(ObservedDataPlaneState {
        tenant_id: validate_reference(&input.tenant_id, "tenant_id", 128)?.to_owned(),
        agent_id: parse_uuid(&input.agent_id, "agent_id")?,
        observed_version: input.observed_version,
        desired_version: input.desired_version,
        write_schema_version: input.write_schema_version.clone(),
        read_schema_version: input.read_schema_version.clone(),
        write_healthy: input.write_healthy,
        read_healthy: input.read_healthy,
        storage_healthy: input.storage_healthy,
        directory_ready: input.directory_ready,
        projections_healthy: input.projections_healthy,
        backup_verified: input.backup_verified,
        failure_category: (!input.failure_category.is_empty())
            .then(|| input.failure_category.clone()),
        correlation_id: parse_uuid(&input.correlation_id, "correlation_id")?,
        observed_at: parse_time(&input.observed_at, "observed_at")?,
    })
}

fn approved_configuration_message(
    approved: ApprovedDesiredConfiguration,
    signing_key_id: &str,
) -> Result<erpx_proto::platform::DesiredConfigurationMessage, Status> {
    let desired = approved.desired;
    let (configuration_layer, business_unit) = match desired.layer {
        ConfigurationLayer::Platform => ("platform".to_owned(), String::new()),
        ConfigurationLayer::Tenant => ("tenant".to_owned(), String::new()),
        ConfigurationLayer::BusinessUnit(unit) => {
            ("business_unit".to_owned(), unit.as_str().to_owned())
        }
    };
    Ok(erpx_proto::platform::DesiredConfigurationMessage {
        contract_version: u32::from(erpx_domain::contracts::CONTRACT_VERSION),
        configuration_id: desired.id.to_string(),
        tenant_id: desired.tenant_id,
        root_holding_id: desired.holding_id.to_string(),
        target_agent_id: desired
            .target_agent_id
            .map_or_else(String::new, |id| id.to_string()),
        deployment_mode: proto_deployment_mode(desired.deployment_mode) as i32,
        configuration_layer,
        business_unit,
        expected_observed_version: desired.expected_observed_version,
        desired_version: desired.desired_version,
        lifecycle: ProtoConfigurationLifecycle::Approved as i32,
        configuration_hash: desired.configuration_hash.to_vec(),
        desired_state_json: serde_json::to_vec(&approved.desired_state)
            .map_err(|_| Status::internal("desired configuration serialization failed"))?,
        secret_references: desired
            .secret_references
            .iter()
            .map(|reference| reference.as_str().to_owned())
            .collect(),
        required_capabilities: desired
            .required_capabilities
            .iter()
            .map(|capability| capability.as_str().to_owned())
            .collect(),
        requester_ref: desired.requester,
        reason: desired.reason,
        expires_at: desired.expires_at.to_rfc3339(),
        correlation_id: approved.correlation_id.to_string(),
        control_plane_signature: Vec::new(),
        signing_key_id: signing_key_id.to_owned(),
    })
}

fn validate_agent_envelope_common(
    contract_version: u32,
    request_id: &str,
    correlation_id: &str,
) -> Result<(), Status> {
    if contract_version != u32::from(erpx_domain::contracts::CONTRACT_VERSION) {
        return Err(Status::failed_precondition(
            "agent contract version is unsupported",
        ));
    }
    parse_uuid(request_id, "request_id")?;
    parse_uuid(correlation_id, "correlation_id")?;
    Ok(())
}

#[derive(Clone, Copy)]
enum BrandOperation {
    Read,
    Manage,
    Publish,
    Lock,
}

fn brand_permission(
    access: &Scope,
    target: &BrandScope,
    operation: BrandOperation,
) -> &'static str {
    if matches!(access, Scope::Platform) {
        return match operation {
            BrandOperation::Read => PLATFORM_BRAND_READ,
            BrandOperation::Manage => PLATFORM_BRAND_MANAGE,
            BrandOperation::Publish => PLATFORM_BRAND_PUBLISH,
            BrandOperation::Lock => PLATFORM_BRAND_LOCK,
        };
    }
    if matches!(target, BrandScope::BusinessUnit { .. }) {
        return match operation {
            BrandOperation::Read => BUSINESS_UNIT_BRAND_READ,
            BrandOperation::Manage | BrandOperation::Lock => BUSINESS_UNIT_BRAND_MANAGE,
            BrandOperation::Publish => BUSINESS_UNIT_BRAND_PUBLISH,
        };
    }
    match operation {
        BrandOperation::Read => TENANT_BRAND_READ,
        BrandOperation::Manage => TENANT_BRAND_MANAGE,
        BrandOperation::Publish => TENANT_BRAND_PUBLISH,
        BrandOperation::Lock => TENANT_BRAND_LOCK,
    }
}

fn ensure_brand_scope_access(access: &Scope, target: &BrandScope) -> Result<(), Status> {
    let target_scope = match target {
        BrandScope::Platform => Scope::Platform,
        BrandScope::Holding { holding_id, .. } | BrandScope::BusinessUnit { holding_id, .. } => {
            Scope::Holding {
                holding_id: *holding_id,
            }
        }
    };
    if access.contains(&target_scope) {
        Ok(())
    } else {
        Err(Status::permission_denied(
            "corporate identity is outside the authorized scope",
        ))
    }
}

fn ensure_brand_mutation_access(access: &Scope, target: &BrandScope) -> Result<(), Status> {
    ensure_brand_scope_access(access, target)?;
    if matches!(access, Scope::Platform) && !matches!(target, BrandScope::Platform) {
        return Err(Status::permission_denied(
            "ERPX Root must create a customer-visible proposal; it cannot publish customer branding directly",
        ));
    }
    Ok(())
}

fn parse_brand_scope(input: &BrandScopeMessage) -> Result<BrandScope, Status> {
    let scope = match input.scope_kind.as_str() {
        "platform"
            if input.tenant_id.is_empty()
                && input.root_holding_id.is_empty()
                && input.business_unit.is_empty() =>
        {
            BrandScope::Platform
        }
        "holding" if input.business_unit.is_empty() => BrandScope::Holding {
            tenant_id: validate_reference(&input.tenant_id, "tenant_id", 128)?.to_owned(),
            holding_id: parse_uuid(&input.root_holding_id, "root_holding_id")?,
        },
        "business_unit" => BrandScope::BusinessUnit {
            tenant_id: validate_reference(&input.tenant_id, "tenant_id", 128)?.to_owned(),
            holding_id: parse_uuid(&input.root_holding_id, "root_holding_id")?,
            business_unit: BusinessUnitId::parse(&input.business_unit)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
        },
        _ => {
            return Err(Status::invalid_argument(
                "corporate identity scope is invalid",
            ));
        }
    };
    scope
        .validate()
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
    Ok(scope)
}

fn parse_corporate_identity_profile(
    input: &CorporateIdentityProfileMessage,
) -> Result<CorporateIdentityProfile, Status> {
    let lifecycle = match input.lifecycle.as_str() {
        "draft" => BrandLifecycle::Draft,
        _ => {
            return Err(Status::invalid_argument(
                "client corporate identity profile must be a draft",
            ));
        }
    };
    let profile = CorporateIdentityProfile {
        profile_id: parse_uuid(&input.profile_id, "profile_id")?,
        scope: parse_brand_scope(
            input
                .scope
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("brand scope is required"))?,
        )?,
        version: input.version,
        parent_published_version: (input.parent_published_version > 0)
            .then_some(input.parent_published_version),
        lifecycle,
        display_name: input.display_name.clone(),
        short_name: input.short_name.clone(),
        tokens: parse_brand_tokens(
            input
                .tokens
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("brand tokens are required"))?,
        )?,
        assets: parse_brand_assets(input.assets.as_ref())?,
        overridden_categories: parse_brand_categories(&input.overridden_categories)?,
        locked_categories: parse_brand_categories(&input.locked_categories)?,
        updated_by: input.updated_by.clone(),
    };
    profile
        .validate(false)
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
    Ok(profile)
}

fn parse_brand_tokens(input: &BrandTokenSetMessage) -> Result<BrandTokenSet, Status> {
    let tokens = BrandTokenSet {
        light: parse_brand_palette(
            input
                .light
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("light brand palette is required"))?,
        )?,
        dark: parse_brand_palette(
            input
                .dark
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("dark brand palette is required"))?,
        )?,
        font_pack: BrandFontPack::parse(&input.font_pack)
            .map_err(|error| Status::invalid_argument(error.to_string()))?,
        heading_scale_percent: u16::try_from(input.heading_scale_percent)
            .map_err(|_| Status::invalid_argument("heading scale is invalid"))?,
        body_scale_percent: u16::try_from(input.body_scale_percent)
            .map_err(|_| Status::invalid_argument("body scale is invalid"))?,
        line_height_percent: u16::try_from(input.line_height_percent)
            .map_err(|_| Status::invalid_argument("line height is invalid"))?,
        density: match input.density.as_str() {
            "compact" => BrandDensity::Compact,
            "comfortable" => BrandDensity::Comfortable,
            "spacious" => BrandDensity::Spacious,
            _ => return Err(Status::invalid_argument("brand density is invalid")),
        },
        radius: match input.radius.as_str() {
            "square" => BrandRadius::Square,
            "soft" => BrandRadius::Soft,
            "rounded" => BrandRadius::Rounded,
            _ => return Err(Status::invalid_argument("brand radius is invalid")),
        },
        elevation: match input.elevation.as_str() {
            "flat" => BrandElevation::Flat,
            "subtle" => BrandElevation::Subtle,
            "elevated" => BrandElevation::Elevated,
            _ => return Err(Status::invalid_argument("brand elevation is invalid")),
        },
        navigation: match input.navigation.as_str() {
            "solid" => BrandNavigation::Solid,
            "tinted" => BrandNavigation::Tinted,
            _ => return Err(Status::invalid_argument("brand navigation is invalid")),
        },
        navigation_expanded_by_default: input.navigation_expanded_by_default,
        motion: match input.motion.as_str() {
            "none" => BrandMotion::None,
            "subtle" => BrandMotion::Subtle,
            "standard" => BrandMotion::Standard,
            _ => return Err(Status::invalid_argument("brand motion is invalid")),
        },
    };
    tokens
        .validate_structure()
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
    Ok(tokens)
}

fn parse_brand_palette(input: &BrandPaletteMessage) -> Result<BrandPalette, Status> {
    let color = |value: &str| {
        HexColor::new(value.to_owned()).map_err(|error| Status::invalid_argument(error.to_string()))
    };
    Ok(BrandPalette {
        primary: color(&input.primary)?,
        on_primary: color(&input.on_primary)?,
        accent: color(&input.accent)?,
        on_accent: color(&input.on_accent)?,
        canvas: color(&input.canvas)?,
        surface: color(&input.surface)?,
        surface_raised: color(&input.surface_raised)?,
        border: color(&input.border)?,
        text: color(&input.text)?,
        text_muted: color(&input.text_muted)?,
        link: color(&input.link)?,
        focus: color(&input.focus)?,
    })
}

fn parse_brand_assets(
    input: Option<&BrandAssetManifestMessage>,
) -> Result<BrandAssetManifest, Status> {
    let Some(input) = input else {
        return Ok(BrandAssetManifest::default());
    };
    Ok(BrandAssetManifest {
        logo_mark: input
            .logo_mark
            .as_ref()
            .map(parse_brand_asset)
            .transpose()?,
        wordmark: input.wordmark.as_ref().map(parse_brand_asset).transpose()?,
        favicon: input.favicon.as_ref().map(parse_brand_asset).transpose()?,
        login_artwork: input
            .login_artwork
            .as_ref()
            .map(parse_brand_asset)
            .transpose()?,
    })
}

fn parse_brand_asset(input: &BrandAssetReferenceMessage) -> Result<BrandAssetReference, Status> {
    let kind = match input.kind.as_str() {
        "logo_mark" => BrandAssetKind::LogoMark,
        "wordmark" => BrandAssetKind::Wordmark,
        "favicon" => BrandAssetKind::Favicon,
        "login_artwork" => BrandAssetKind::LoginArtwork,
        _ => {
            return Err(Status::invalid_argument(
                "corporate identity asset kind is invalid",
            ));
        }
    };
    Ok(BrandAssetReference {
        kind,
        object_key: input.object_key.clone(),
        sha256: input.sha256.clone(),
        media_type: input.media_type.clone(),
        byte_length: input.byte_length,
        width: (input.width > 0).then_some(input.width),
        height: (input.height > 0).then_some(input.height),
    })
}

fn parse_brand_categories(
    values: &[String],
) -> Result<std::collections::BTreeSet<BrandCategory>, Status> {
    let parsed = values
        .iter()
        .map(|value| {
            BrandCategory::parse(value).map_err(|error| Status::invalid_argument(error.to_string()))
        })
        .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
    if parsed.len() != values.len() {
        return Err(Status::invalid_argument(
            "corporate identity categories contain duplicates",
        ));
    }
    Ok(parsed)
}

fn corporate_identity_profile_message(
    profile: &CorporateIdentityProfile,
) -> CorporateIdentityProfileMessage {
    CorporateIdentityProfileMessage {
        profile_id: profile.profile_id.to_string(),
        scope: Some(brand_scope_message(&profile.scope)),
        version: profile.version,
        parent_published_version: profile.parent_published_version.unwrap_or_default(),
        lifecycle: profile.lifecycle.as_str().to_owned(),
        display_name: profile.display_name.clone(),
        short_name: profile.short_name.clone(),
        tokens: Some(brand_tokens_message(&profile.tokens)),
        assets: Some(brand_assets_message(&profile.assets)),
        overridden_categories: profile
            .overridden_categories
            .iter()
            .map(|category| category.as_str().to_owned())
            .collect(),
        locked_categories: profile
            .locked_categories
            .iter()
            .map(|category| category.as_str().to_owned())
            .collect(),
        updated_by: profile.updated_by.clone(),
    }
}

fn resolved_identity_message(
    identity: &ResolvedCorporateIdentity,
) -> ResolvedCorporateIdentityMessage {
    ResolvedCorporateIdentityMessage {
        scope: Some(brand_scope_message(&identity.scope)),
        published_version: identity.published_version,
        display_name: identity.display_name.clone(),
        short_name: identity.short_name.clone(),
        tokens: Some(brand_tokens_message(&identity.tokens)),
        assets: Some(brand_assets_message(&identity.assets)),
        inherited_categories: identity
            .inherited_categories
            .iter()
            .map(|category| category.as_str().to_owned())
            .collect(),
        locked_categories: identity
            .locked_categories
            .iter()
            .map(|category| category.as_str().to_owned())
            .collect(),
        digest: identity.digest.clone(),
    }
}

fn brand_scope_message(scope: &BrandScope) -> BrandScopeMessage {
    match scope {
        BrandScope::Platform => BrandScopeMessage {
            scope_kind: "platform".into(),
            ..BrandScopeMessage::default()
        },
        BrandScope::Holding {
            tenant_id,
            holding_id,
        } => BrandScopeMessage {
            scope_kind: "holding".into(),
            tenant_id: tenant_id.clone(),
            root_holding_id: holding_id.to_string(),
            business_unit: String::new(),
        },
        BrandScope::BusinessUnit {
            tenant_id,
            holding_id,
            business_unit,
        } => BrandScopeMessage {
            scope_kind: "business_unit".into(),
            tenant_id: tenant_id.clone(),
            root_holding_id: holding_id.to_string(),
            business_unit: business_unit.as_str().to_owned(),
        },
    }
}

fn brand_tokens_message(tokens: &BrandTokenSet) -> BrandTokenSetMessage {
    BrandTokenSetMessage {
        light: Some(brand_palette_message(&tokens.light)),
        dark: Some(brand_palette_message(&tokens.dark)),
        font_pack: tokens.font_pack.as_str().to_owned(),
        heading_scale_percent: u32::from(tokens.heading_scale_percent),
        body_scale_percent: u32::from(tokens.body_scale_percent),
        line_height_percent: u32::from(tokens.line_height_percent),
        density: tokens.density.as_str().to_owned(),
        radius: tokens.radius.as_str().to_owned(),
        elevation: tokens.elevation.as_str().to_owned(),
        navigation: tokens.navigation.as_str().to_owned(),
        navigation_expanded_by_default: tokens.navigation_expanded_by_default,
        motion: tokens.motion.as_str().to_owned(),
    }
}

fn brand_palette_message(palette: &BrandPalette) -> BrandPaletteMessage {
    BrandPaletteMessage {
        primary: palette.primary.as_str().to_owned(),
        on_primary: palette.on_primary.as_str().to_owned(),
        accent: palette.accent.as_str().to_owned(),
        on_accent: palette.on_accent.as_str().to_owned(),
        canvas: palette.canvas.as_str().to_owned(),
        surface: palette.surface.as_str().to_owned(),
        surface_raised: palette.surface_raised.as_str().to_owned(),
        border: palette.border.as_str().to_owned(),
        text: palette.text.as_str().to_owned(),
        text_muted: palette.text_muted.as_str().to_owned(),
        link: palette.link.as_str().to_owned(),
        focus: palette.focus.as_str().to_owned(),
    }
}

fn brand_assets_message(assets: &BrandAssetManifest) -> BrandAssetManifestMessage {
    BrandAssetManifestMessage {
        logo_mark: assets.logo_mark.as_ref().map(brand_asset_message),
        wordmark: assets.wordmark.as_ref().map(brand_asset_message),
        favicon: assets.favicon.as_ref().map(brand_asset_message),
        login_artwork: assets.login_artwork.as_ref().map(brand_asset_message),
    }
}

fn brand_asset_message(asset: &BrandAssetReference) -> BrandAssetReferenceMessage {
    let kind = match asset.kind {
        BrandAssetKind::LogoMark => "logo_mark",
        BrandAssetKind::Wordmark => "wordmark",
        BrandAssetKind::Favicon => "favicon",
        BrandAssetKind::LoginArtwork => "login_artwork",
    };
    BrandAssetReferenceMessage {
        kind: kind.into(),
        object_key: asset.object_key.clone(),
        sha256: asset.sha256.clone(),
        media_type: asset.media_type.clone(),
        byte_length: asset.byte_length,
        width: asset.width.unwrap_or_default(),
        height: asset.height.unwrap_or_default(),
    }
}

fn history_entry_message(
    entry: &CorporateIdentityHistoryEntry,
) -> CorporateIdentityHistoryEntryMessage {
    CorporateIdentityHistoryEntryMessage {
        profile: Some(corporate_identity_profile_message(&entry.profile)),
        reason: entry.reason.clone(),
        correlation_id: entry.correlation_id.to_string(),
        created_at: entry.created_at.to_rfc3339(),
        published_at: entry
            .published_at
            .map_or_else(String::new, |value| value.to_rfc3339()),
        archived_at: entry
            .archived_at
            .map_or_else(String::new, |value| value.to_rfc3339()),
    }
}

async fn navigation_manifest_groups(
    state: &AppState,
    principal_ref: &str,
    scope: &Scope,
) -> Vec<NavigationManifestGroup> {
    struct Candidate {
        route_id: &'static str,
        label: &'static str,
        path: &'static str,
        icon: &'static str,
        permission: String,
    }
    async fn allowed(
        state: &AppState,
        principal_ref: &str,
        scope: &Scope,
        permission: &str,
    ) -> bool {
        state
            .auth
            .authorize_role(&state.store, principal_ref, scope, permission)
            .await
            .is_ok()
    }
    async fn group(
        state: &AppState,
        principal_ref: &str,
        scope: &Scope,
        group_id: &str,
        label: &str,
        candidates: Vec<Candidate>,
    ) -> Option<NavigationManifestGroup> {
        let mut items = Vec::new();
        for candidate in candidates {
            if allowed(state, principal_ref, scope, &candidate.permission).await {
                items.push(NavigationManifestItem {
                    route_id: candidate.route_id.into(),
                    label: candidate.label.into(),
                    path: candidate.path.into(),
                    icon: candidate.icon.into(),
                    badge_count: 0,
                });
            }
        }
        (!items.is_empty()).then(|| NavigationManifestGroup {
            group_id: group_id.into(),
            label: label.into(),
            items,
        })
    }

    let mut groups = Vec::new();
    if matches!(scope, Scope::Platform)
        && let Some(value) = group(
            state,
            principal_ref,
            scope,
            "customer_operations",
            "Customer Operations",
            vec![
                Candidate {
                    route_id: "customer_360",
                    label: "Customer 360",
                    path: "/platform/customers",
                    icon: "customers",
                    permission: "platform.customer.read".into(),
                },
                Candidate {
                    route_id: "directories",
                    label: "Directories",
                    path: "/platform/directories",
                    icon: "directory",
                    permission: "platform.directory.read".into(),
                },
                Candidate {
                    route_id: "support",
                    label: "Support",
                    path: "/platform/support",
                    icon: "support",
                    permission: "support.read".into(),
                },
                Candidate {
                    route_id: "billing",
                    label: "Billing",
                    path: "/platform/billing",
                    icon: "billing",
                    permission: "commercial.billing.read".into(),
                },
            ],
        )
        .await
    {
        groups.push(value);
    }
    let mut modules = Vec::new();
    for unit in BusinessUnitId::ALL {
        let path = match unit {
            BusinessUnitId::MarketingCrm => "/crm/leads",
            BusinessUnitId::Operations => "/workspaces/operations",
            BusinessUnitId::Finance => "/workspaces/finance",
            BusinessUnitId::Sales => "/workspaces/sales",
            BusinessUnitId::Procurement => "/workspaces/procurement",
            BusinessUnitId::Inventory => "/workspaces/inventory",
            BusinessUnitId::Assets => "/workspaces/assets",
            BusinessUnitId::Maintenance => "/workspaces/maintenance",
            BusinessUnitId::Quality => "/workspaces/quality",
            BusinessUnitId::HrPayroll => "/workspaces/hr-payroll",
            BusinessUnitId::PointOfSale => "/workspaces/point-of-sale",
            BusinessUnitId::Legal => "/workspaces/legal",
        };
        modules.push(Candidate {
            route_id: unit.as_str(),
            label: unit.display_name(),
            path,
            icon: "module",
            permission: format!("{}.read", unit.as_str()),
        });
    }
    if let Some(value) = group(
        state,
        principal_ref,
        scope,
        "business_modules",
        "Business Modules",
        modules,
    )
    .await
    {
        groups.push(value);
    }
    if matches!(scope, Scope::Platform)
        && let Some(value) = group(
            state,
            principal_ref,
            scope,
            "platform_administration",
            "Platform Administration",
            vec![
                Candidate {
                    route_id: "tenants",
                    label: "Tenants",
                    path: "/platform/tenants",
                    icon: "tenants",
                    permission: "platform.tenant.read".into(),
                },
                Candidate {
                    route_id: "projections",
                    label: "Projections",
                    path: "/platform/projections",
                    icon: "projections",
                    permission: "platform.tenant.read".into(),
                },
                Candidate {
                    route_id: "backups",
                    label: "Backups",
                    path: "/platform/backups",
                    icon: "backups",
                    permission: "platform.tenant.backup".into(),
                },
                Candidate {
                    route_id: "integrations",
                    label: "Integrations",
                    path: "/platform/integrations",
                    icon: "integrations",
                    permission: "platform.tenant.read".into(),
                },
                Candidate {
                    route_id: "audit",
                    label: "Audit",
                    path: "/platform/audit",
                    icon: "audit",
                    permission: "platform.tenant.read".into(),
                },
                Candidate {
                    route_id: "role_review",
                    label: "Role Review",
                    path: "/platform/role-review",
                    icon: "roles",
                    permission: "role.assignment.review".into(),
                },
                Candidate {
                    route_id: "settings",
                    label: "Settings",
                    path: "/platform/settings",
                    icon: "settings",
                    permission: PLATFORM_BRAND_READ.into(),
                },
            ],
        )
        .await
    {
        groups.push(value);
    }
    groups
}

fn persistence_status(error: PersistenceError) -> Status {
    match error {
        PersistenceError::NotFound { .. } => Status::not_found("data-plane record was not found"),
        PersistenceError::OptimisticConcurrency { .. }
        | PersistenceError::IdempotencyConflict { .. } => {
            Status::aborted("data-plane state changed; refresh and retry")
        }
        PersistenceError::InvalidData(message) => Status::failed_precondition(message),
        PersistenceError::Configuration(message) => Status::failed_precondition(message),
        _ => Status::unavailable("data-plane control store is unavailable"),
    }
}

fn parse_uuid(value: &str, field: &str) -> Result<Uuid, Status> {
    let value = Uuid::parse_str(value)
        .map_err(|_| Status::invalid_argument(format!("{field} is invalid")))?;
    if value.is_nil() {
        return Err(Status::invalid_argument(format!("{field} is invalid")));
    }
    Ok(value)
}

fn optional_uuid(value: &str, field: &str) -> Result<Option<Uuid>, Status> {
    if value.is_empty() {
        Ok(None)
    } else {
        parse_uuid(value, field).map(Some)
    }
}

fn parse_time(value: &str, field: &str) -> Result<DateTime<Utc>, Status> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| Status::invalid_argument(format!("{field} is invalid")))
}

fn validate_reference<'a>(value: &'a str, field: &str, max: usize) -> Result<&'a str, Status> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(Status::invalid_argument(format!("{field} is invalid")));
    }
    Ok(value)
}

fn decode_hash(value: &str, field: &str) -> Result<[u8; 32], Status> {
    let bytes =
        hex::decode(value).map_err(|_| Status::invalid_argument(format!("{field} is invalid")))?;
    slice_hash(&bytes, field)
}

fn slice_hash(value: &[u8], field: &str) -> Result<[u8; 32], Status> {
    value
        .try_into()
        .map_err(|_| Status::invalid_argument(format!("{field} must be SHA-256")))
}

fn domain_deployment_mode(value: i32) -> Result<TenantDeploymentMode, Status> {
    match ProtoDeploymentMode::try_from(value) {
        Ok(ProtoDeploymentMode::ErpxManaged) => Ok(TenantDeploymentMode::ErpxManaged),
        Ok(ProtoDeploymentMode::CustomerDataPlane) => Ok(TenantDeploymentMode::CustomerDataPlane),
        _ => Err(Status::invalid_argument("deployment_mode is invalid")),
    }
}

fn proto_deployment_mode(value: TenantDeploymentMode) -> ProtoDeploymentMode {
    match value {
        TenantDeploymentMode::ErpxManaged => ProtoDeploymentMode::ErpxManaged,
        TenantDeploymentMode::CustomerDataPlane => ProtoDeploymentMode::CustomerDataPlane,
    }
}

fn parse_configuration_lifecycle(value: i32) -> Result<ConfigurationLifecycle, Status> {
    match ProtoConfigurationLifecycle::try_from(value) {
        Ok(ProtoConfigurationLifecycle::Draft) => Ok(ConfigurationLifecycle::Draft),
        Ok(ProtoConfigurationLifecycle::AwaitingApproval) => {
            Ok(ConfigurationLifecycle::AwaitingApproval)
        }
        _ => Err(Status::invalid_argument(
            "new configuration lifecycle must be draft or awaiting approval",
        )),
    }
}

fn proto_configuration_lifecycle(value: ConfigurationLifecycle) -> ProtoConfigurationLifecycle {
    match value {
        ConfigurationLifecycle::Draft => ProtoConfigurationLifecycle::Draft,
        ConfigurationLifecycle::AwaitingApproval => ProtoConfigurationLifecycle::AwaitingApproval,
        ConfigurationLifecycle::Approved => ProtoConfigurationLifecycle::Approved,
        ConfigurationLifecycle::Applying => ProtoConfigurationLifecycle::Applying,
        ConfigurationLifecycle::Verified => ProtoConfigurationLifecycle::Verified,
        ConfigurationLifecycle::Active => ProtoConfigurationLifecycle::Active,
        ConfigurationLifecycle::Rejected => ProtoConfigurationLifecycle::Rejected,
        ConfigurationLifecycle::Drifted => ProtoConfigurationLifecycle::Drifted,
        ConfigurationLifecycle::Failed => ProtoConfigurationLifecycle::Failed,
        ConfigurationLifecycle::Suspended => ProtoConfigurationLifecycle::Suspended,
    }
}

fn parse_configuration_layer(value: &str, unit: &str) -> Result<ConfigurationLayer, Status> {
    match value {
        "platform" if unit.is_empty() => Ok(ConfigurationLayer::Platform),
        "tenant" if unit.is_empty() => Ok(ConfigurationLayer::Tenant),
        "business_unit" => BusinessUnitId::parse(unit)
            .map(ConfigurationLayer::BusinessUnit)
            .map_err(|error| Status::invalid_argument(error.to_string())),
        _ => Err(Status::invalid_argument("configuration_layer is invalid")),
    }
}

fn parse_agent_capability(value: &str) -> Result<AgentCapability, Status> {
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
        _ => Err(Status::invalid_argument("agent capability is invalid")),
    }
}

fn parse_root_access_mode(value: i32) -> Result<RootAccessMode, Status> {
    use erpx_proto::platform::RootAccessMode as ProtoMode;
    match ProtoMode::try_from(value) {
        Ok(ProtoMode::HealthDiagnostic) => Ok(RootAccessMode::HealthDiagnostic),
        Ok(ProtoMode::InfrastructureMaintenance) => Ok(RootAccessMode::InfrastructureMaintenance),
        Ok(ProtoMode::BusinessDiagnostic) => Ok(RootAccessMode::BusinessDiagnostic),
        Ok(ProtoMode::BusinessMutation) => Ok(RootAccessMode::BusinessMutation),
        Ok(ProtoMode::EmergencyRecovery) => Ok(RootAccessMode::EmergencyRecovery),
        _ => Err(Status::invalid_argument("Root access mode is invalid")),
    }
}

fn proto_agent_state(value: DataPlaneAgentState) -> ProtoDataPlaneAgentState {
    match value {
        DataPlaneAgentState::Unenrolled => ProtoDataPlaneAgentState::Unenrolled,
        DataPlaneAgentState::Enrolling => ProtoDataPlaneAgentState::Enrolling,
        DataPlaneAgentState::Connected => ProtoDataPlaneAgentState::Connected,
        DataPlaneAgentState::Healthy => ProtoDataPlaneAgentState::Healthy,
        DataPlaneAgentState::Drifted => ProtoDataPlaneAgentState::Drifted,
        DataPlaneAgentState::Degraded => ProtoDataPlaneAgentState::Degraded,
        DataPlaneAgentState::Offline => ProtoDataPlaneAgentState::Offline,
        DataPlaneAgentState::Suspended => ProtoDataPlaneAgentState::Suspended,
        DataPlaneAgentState::UpgradeRequired => ProtoDataPlaneAgentState::UpgradeRequired,
    }
}

fn command_mfa(context: Option<&CommandContext>) -> &str {
    context.map_or("", |context| context.mfa_code.as_str())
}

fn proto_query_scope(context: Option<&QueryContext>) -> Result<Scope, Status> {
    let context = context.ok_or_else(|| Status::invalid_argument("query context is required"))?;
    proto_scope(
        context
            .tenant_scope
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("tenant_scope is required"))?,
    )
}

fn proto_scope(scope: &ProtoTenantScope) -> Result<Scope, Status> {
    match scope
        .scope
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("tenant_scope is required"))?
    {
        tenant_scope::Scope::Platform(_) => Ok(Scope::Platform),
        tenant_scope::Scope::Holding(scope) => Ok(Scope::Holding {
            holding_id: parse_uuid(&scope.holding_id, "holding_id")?,
        }),
        tenant_scope::Scope::Company(scope) => Ok(Scope::Company {
            holding_id: parse_uuid(&scope.holding_id, "holding_id")?,
            company_id: parse_uuid(&scope.company_id, "company_id")?,
        }),
        tenant_scope::Scope::Branch(scope) => Ok(Scope::Branch {
            holding_id: parse_uuid(&scope.holding_id, "holding_id")?,
            company_id: parse_uuid(&scope.company_id, "company_id")?,
            branch_id: parse_uuid(&scope.branch_id, "branch_id")?,
        }),
        tenant_scope::Scope::Subtree(scope) => Ok(Scope::Subtree {
            holding_id: parse_uuid(&scope.holding_id, "holding_id")?,
            company_ids: scope
                .company_ids
                .iter()
                .map(|id| parse_uuid(id, "company_id"))
                .collect::<Result<Vec<_>, _>>()?,
            branch_ids: scope
                .branch_ids
                .iter()
                .map(|id| parse_uuid(id, "branch_id"))
                .collect::<Result<Vec<_>, _>>()?,
        }),
    }
}

fn scope_to_proto(scope: &Scope) -> ProtoTenantScope {
    let scope = match scope {
        Scope::Platform => tenant_scope::Scope::Platform(erpx_proto::common::v1::PlatformScope {}),
        Scope::Holding { holding_id } => {
            tenant_scope::Scope::Holding(erpx_proto::common::v1::HoldingScope {
                holding_id: holding_id.to_string(),
            })
        }
        Scope::Company {
            holding_id,
            company_id,
        } => tenant_scope::Scope::Company(erpx_proto::common::v1::CompanyScope {
            holding_id: holding_id.to_string(),
            company_id: company_id.to_string(),
        }),
        Scope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => tenant_scope::Scope::Branch(erpx_proto::common::v1::BranchScope {
            holding_id: holding_id.to_string(),
            company_id: company_id.to_string(),
            branch_id: branch_id.to_string(),
        }),
        Scope::Subtree {
            holding_id,
            company_ids,
            branch_ids,
        } => tenant_scope::Scope::Subtree(erpx_proto::common::v1::SubtreeScope {
            holding_id: holding_id.to_string(),
            company_ids: company_ids.iter().map(Uuid::to_string).collect(),
            branch_ids: branch_ids.iter().map(Uuid::to_string).collect(),
        }),
    };
    ProtoTenantScope { scope: Some(scope) }
}

fn holding_id(scope: &Scope) -> Result<Uuid, Status> {
    match scope {
        Scope::Platform => Err(Status::invalid_argument(
            "Root access must use a customer tenant scope",
        )),
        Scope::Holding { holding_id }
        | Scope::Company { holding_id, .. }
        | Scope::Branch { holding_id, .. }
        | Scope::Subtree { holding_id, .. } => Ok(*holding_id),
    }
}

fn bounded_json_object(bytes: &[u8], field: &str) -> Result<Value, Status> {
    if bytes.is_empty() || bytes.len() > MAX_CONFIGURATION_JSON_BYTES {
        return Err(Status::invalid_argument(format!(
            "{field} must be a bounded JSON object"
        )));
    }
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| Status::invalid_argument(format!("{field} is invalid")))?;
    if !value.is_object() {
        return Err(Status::invalid_argument(format!(
            "{field} must be a JSON object"
        )));
    }
    Ok(value)
}

fn business_unit_role_mapping(
    mapping: &BusinessUnitDirectoryRoleMappingMessage,
    _business_unit: BusinessUnitId,
    holding_id: Uuid,
) -> Result<BusinessUnitDirectoryRoleMapping, Status> {
    let target_scope = proto_scope(
        mapping
            .target_scope
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("mapping target_scope is required"))?,
    )?;
    if super_holding_id(&target_scope) != holding_id {
        return Err(Status::permission_denied(
            "directory role mapping cannot cross the holding boundary",
        ));
    }
    Ok(BusinessUnitDirectoryRoleMapping {
        group_ref: mapping.group_ref.clone(),
        role_key: mapping.role_key.clone(),
        target_scope,
        requires_mfa: mapping.requires_mfa,
        assignment_duration_hours: mapping.assignment_duration_hours,
        approval_limit_minor: mapping.approval_limit_minor,
        active: mapping.active,
    })
}

fn super_holding_id(scope: &Scope) -> Uuid {
    match scope {
        Scope::Platform => Uuid::nil(),
        Scope::Holding { holding_id }
        | Scope::Company { holding_id, .. }
        | Scope::Branch { holding_id, .. }
        | Scope::Subtree { holding_id, .. } => *holding_id,
    }
}

fn business_unit_configuration_message(
    configuration: &BusinessUnitConfiguration,
) -> Result<BusinessUnitConfigurationMessage, Status> {
    Ok(BusinessUnitConfigurationMessage {
        tenant_id: configuration.tenant_id.clone(),
        root_holding_id: configuration.holding_id.to_string(),
        business_unit: configuration.business_unit.as_str().to_owned(),
        configuration_version: configuration.configuration_version,
        active: configuration.active,
        organization_scope: Some(scope_to_proto(&configuration.organization_scope)),
        directory_role_mappings: configuration
            .directory_role_mappings
            .iter()
            .map(|mapping| BusinessUnitDirectoryRoleMappingMessage {
                group_ref: mapping.group_ref.clone(),
                role_key: mapping.role_key.clone(),
                target_scope: Some(scope_to_proto(&mapping.target_scope)),
                requires_mfa: mapping.requires_mfa,
                assignment_duration_hours: mapping.assignment_duration_hours,
                approval_limit_minor: mapping.approval_limit_minor,
                active: mapping.active,
            })
            .collect(),
        integration_references: configuration.integration_references.clone(),
        retention_policy_json: serde_json::to_vec(&configuration.retention_policy)
            .map_err(|_| Status::internal("retention policy serialization failed"))?,
        projection_policy_json: serde_json::to_vec(&configuration.projection_policy)
            .map_err(|_| Status::internal("projection policy serialization failed"))?,
        workflow_policy_json: serde_json::to_vec(&configuration.workflow_policy)
            .map_err(|_| Status::internal("workflow policy serialization failed"))?,
        updated_by: configuration.updated_by.clone(),
        updated_at: configuration.updated_at.to_rfc3339(),
    })
}

fn root_access_queue_entry(record: &RootAccessQueueRecord) -> Result<RootAccessQueueEntry, Status> {
    let request = &record.request;
    Ok(RootAccessQueueEntry {
        access_request: Some(RootAccessRequestMessage {
            request_id: request.id.to_string(),
            requester_ref: request.requester.clone(),
            device_ref: request.device_ref.clone(),
            tenant_id: request.tenant_id.clone(),
            agent_id: request.agent_id.to_string(),
            tenant_scope: Some(scope_to_proto(&request.tenant_scope)),
            mode: proto_root_access_mode(request.mode) as i32,
            allowed_actions: request.allowed_actions.clone(),
            business_units: request
                .business_units
                .iter()
                .map(|unit| unit.as_str().to_owned())
                .collect(),
            reason: request.reason.clone(),
            ticket_reference: request.ticket_reference.clone(),
            requested_at: request.requested_at.to_rfc3339(),
            requested_until: request.requested_until.to_rfc3339(),
        }),
        state: record.state.clone(),
        customer_approver_ref: record.customer_approver_ref.clone().unwrap_or_default(),
        customer_approved_at: record
            .customer_approved_at
            .map_or_else(String::new, |at| at.to_rfc3339()),
        rejected_at: record
            .rejected_at
            .map_or_else(String::new, |at| at.to_rfc3339()),
        revoked_at: record
            .revoked_at
            .map_or_else(String::new, |at| at.to_rfc3339()),
        revocation_reason: record.revocation_reason.clone().unwrap_or_default(),
    })
}

fn proto_root_access_mode(value: RootAccessMode) -> erpx_proto::platform::RootAccessMode {
    use erpx_proto::platform::RootAccessMode as ProtoMode;
    match value {
        RootAccessMode::HealthDiagnostic => ProtoMode::HealthDiagnostic,
        RootAccessMode::InfrastructureMaintenance => ProtoMode::InfrastructureMaintenance,
        RootAccessMode::BusinessDiagnostic => ProtoMode::BusinessDiagnostic,
        RootAccessMode::BusinessMutation => ProtoMode::BusinessMutation,
        RootAccessMode::EmergencyRecovery => ProtoMode::EmergencyRecovery,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_access_modes_are_closed() {
        assert!(parse_root_access_mode(0).is_err());
        assert_eq!(
            parse_root_access_mode(erpx_proto::platform::RootAccessMode::EmergencyRecovery as i32)
                .expect("mode"),
            RootAccessMode::EmergencyRecovery
        );
    }

    #[test]
    fn hashes_require_exact_sha256_width() {
        assert!(slice_hash(&[0; 31], "hash").is_err());
        assert_eq!(slice_hash(&[7; 32], "hash").expect("hash"), [7; 32]);
    }
}
