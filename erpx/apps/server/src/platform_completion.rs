//! Server-owned API adapter for the G3--G6 platform-control completion seam.
//!
//! Existing RPC implementations remain the registration/composition owner.
//! They should construct these requests only after the shared authenticated
//! session/role evaluator has returned a [`PlatformAuthorizationProof`]. This
//! adapter never accepts a bearer token, resolves a role, or exposes a
//! database/storage/credential reference.

#![allow(dead_code)]

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use erpx_domain::access::Scope;
use erpx_domain::platform_completion::{
    BackupControlCommand, ConnectorControlCommand, PlatformAuthorizationProof,
    PlatformReconciliationReport, ReconciliationCommand, ReplacementActivationProof,
    ReplacementReadCommand, TenantBinding, TenantLifecycleCommand,
};
use erpx_infrastructure::{
    PlatformBackupJobReceipt, PlatformCommandReceipt, PlatformConnectorReadModel,
    PlatformControlStore, PlatformPersistenceError, PlatformReplacementReadReceipt,
    PlatformTenantLifecycleReceipt,
};
use serde::{Deserialize, Serialize};
use tonic::{Response, Status};

#[derive(Debug, Clone)]
pub struct PlatformCompletionApi {
    store: Arc<PlatformControlStore>,
}

impl PlatformCompletionApi {
    pub fn new(store: Arc<PlatformControlStore>) -> Self {
        Self { store }
    }

    pub async fn transition_lifecycle(
        &self,
        request: PlatformLifecycleApiRequest,
    ) -> Result<Response<PlatformTenantLifecycleReceipt>, Status> {
        let receipt = self
            .store
            .transition_lifecycle(&request.command, &request.proof)
            .await
            .map_err(status_for_platform_error)?;
        Ok(Response::new(receipt))
    }

    pub async fn reconcile_tenant(
        &self,
        request: PlatformReconciliationApiRequest,
    ) -> Result<Response<PlatformReconciliationReport>, Status> {
        let report = self
            .store
            .reconcile_tenant(&request.command, &request.proof)
            .await
            .map_err(status_for_platform_error)?;
        Ok(Response::new(report))
    }

    pub async fn schedule_backup(
        &self,
        request: PlatformBackupApiRequest,
    ) -> Result<Response<PlatformBackupJobReceipt>, Status> {
        let receipt = self
            .store
            .schedule_backup(&request.command, &request.proof)
            .await
            .map_err(status_for_platform_error)?;
        Ok(Response::new(receipt))
    }

    pub async fn start_replacement_read(
        &self,
        request: PlatformReplacementReadApiRequest,
    ) -> Result<Response<PlatformReplacementReadReceipt>, Status> {
        let receipt = self
            .store
            .start_replacement_read(&request.command, &request.proof)
            .await
            .map_err(status_for_platform_error)?;
        Ok(Response::new(receipt))
    }

    pub async fn mark_replacement_ready(
        &self,
        request: PlatformReplacementReadApiRequest,
    ) -> Result<Response<PlatformReplacementReadReceipt>, Status> {
        let receipt = self
            .store
            .mark_replacement_ready(&request.command, &request.proof)
            .await
            .map_err(status_for_platform_error)?;
        Ok(Response::new(receipt))
    }

    pub async fn fail_replacement_read(
        &self,
        request: PlatformReplacementFailureApiRequest,
    ) -> Result<Response<PlatformReplacementReadReceipt>, Status> {
        let receipt = self
            .store
            .fail_replacement_read(&request.command, &request.proof, &request.reason_code)
            .await
            .map_err(status_for_platform_error)?;
        Ok(Response::new(receipt))
    }

    pub async fn activate_replacement_read(
        &self,
        request: PlatformReplacementActivationApiRequest,
    ) -> Result<Response<PlatformReplacementReadReceipt>, Status> {
        let receipt = self
            .store
            .activate_replacement_read(&request.command, &request.proof, &request.activation)
            .await
            .map_err(status_for_platform_error)?;
        Ok(Response::new(receipt))
    }

    pub async fn save_connector(
        &self,
        request: PlatformConnectorApiRequest,
    ) -> Result<Response<PlatformCommandReceipt>, Status> {
        let receipt = self
            .store
            .save_connector(&request.command, &request.proof)
            .await
            .map_err(status_for_platform_error)?;
        Ok(Response::new(receipt))
    }

    pub async fn read_connector(
        &self,
        connector_id: uuid::Uuid,
        authorization: PlatformReadAuthorization,
    ) -> Result<Response<Option<PlatformConnectorReadModel>>, Status> {
        validate_platform_read_proof(&authorization)?;
        let model = self
            .store
            .read_connector(connector_id)
            .await
            .map_err(status_for_platform_error)?;
        Ok(Response::new(model))
    }

    /// Authorized, UI-safe connector handoff. The existing RPC must still
    /// authenticate the caller and mint the proof; this second check prevents
    /// an accidentally unguarded integration surface from exposing the raw
    /// read model.
    pub async fn read_connector_health(
        &self,
        connector_id: uuid::Uuid,
        authorization: PlatformReadAuthorization,
    ) -> Result<Response<Option<PlatformConnectorHealthResponse>>, Status> {
        validate_platform_read_proof(&authorization)?;
        let model = self
            .store
            .read_connector(connector_id)
            .await
            .map_err(status_for_platform_error)?;
        Ok(Response::new(
            model.map(PlatformConnectorHealthResponse::from),
        ))
    }

    pub async fn read_reconciliation(
        &self,
        binding: &TenantBinding,
        authorization: PlatformReadAuthorization,
    ) -> Result<Response<Option<PlatformReconciliationReport>>, Status> {
        validate_platform_read_proof(&authorization)?;
        let model = self
            .store
            .read_reconciliation(binding)
            .await
            .map_err(status_for_platform_error)?;
        Ok(Response::new(model))
    }

    pub async fn read_reconciliation_ui(
        &self,
        binding: &TenantBinding,
        authorization: PlatformReadAuthorization,
    ) -> Result<Response<Option<PlatformReconciliationUiResponse>>, Status> {
        validate_platform_read_proof(&authorization)?;
        let model = self
            .store
            .read_reconciliation(binding)
            .await
            .map_err(status_for_platform_error)?;
        let response = model
            .map(|report| PlatformReconciliationUiResponse::from_report(report, authorization.at));
        Ok(Response::new(response))
    }
}

#[derive(Debug, Clone)]
pub struct PlatformLifecycleApiRequest {
    pub command: TenantLifecycleCommand,
    pub proof: PlatformAuthorizationProof,
}

#[derive(Debug, Clone)]
pub struct PlatformReconciliationApiRequest {
    pub command: ReconciliationCommand,
    pub proof: PlatformAuthorizationProof,
}

#[derive(Debug, Clone)]
pub struct PlatformBackupApiRequest {
    pub command: BackupControlCommand,
    pub proof: PlatformAuthorizationProof,
}

#[derive(Debug, Clone)]
pub struct PlatformReplacementReadApiRequest {
    pub command: ReplacementReadCommand,
    pub proof: PlatformAuthorizationProof,
}

#[derive(Debug, Clone)]
pub struct PlatformReplacementFailureApiRequest {
    pub command: ReplacementReadCommand,
    pub proof: PlatformAuthorizationProof,
    pub reason_code: String,
}

#[derive(Debug, Clone)]
pub struct PlatformReplacementActivationApiRequest {
    pub command: ReplacementReadCommand,
    pub proof: PlatformAuthorizationProof,
    pub activation: ReplacementActivationProof,
}

#[derive(Debug, Clone)]
pub struct PlatformConnectorApiRequest {
    pub command: ConnectorControlCommand,
    pub proof: PlatformAuthorizationProof,
}

#[derive(Debug, Clone)]
pub struct PlatformReadAuthorization {
    pub actor_ref: String,
    pub proof: PlatformAuthorizationProof,
    pub at: DateTime<Utc>,
}

/// Safe categorical response used by platform integration surfaces. It is
/// intentionally not a provider configuration DTO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformConnectorHealthResponse {
    pub connector_id: uuid::Uuid,
    pub state: String,
    pub capability_count: u32,
    pub credential_bound: bool,
    pub checked_at: String,
}

impl From<PlatformConnectorReadModel> for PlatformConnectorHealthResponse {
    fn from(model: PlatformConnectorReadModel) -> Self {
        Self {
            connector_id: model.connector_id,
            state: model.state,
            capability_count: u32::try_from(model.capabilities.len()).unwrap_or(u32::MAX),
            credential_bound: model.credential_bound,
            checked_at: model.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformReconciliationUiResponse {
    pub tenant_id: String,
    pub holding_ref: String,
    pub state: String,
    pub observed_at: String,
    pub stale: bool,
    pub connector_count: u32,
    pub drift_codes: Vec<String>,
}

impl PlatformReconciliationUiResponse {
    fn from_report(report: PlatformReconciliationReport, at: DateTime<Utc>) -> Self {
        let stale = report.observed_at < at - Duration::minutes(5);
        Self {
            tenant_id: report.binding.tenant_id,
            holding_ref: report.binding.holding_ref,
            state: report.state.as_str().into(),
            observed_at: report.observed_at.to_rfc3339(),
            stale,
            connector_count: report.connector_count,
            drift_codes: report.drift_codes.into_iter().collect(),
        }
    }
}

fn status_for_platform_error(error: PlatformPersistenceError) -> Status {
    match error {
        PlatformPersistenceError::Invalid(_) => {
            Status::invalid_argument("platform request is invalid")
        }
        PlatformPersistenceError::Conflict => {
            Status::failed_precondition("platform state changed; retry with fresh state")
        }
        PlatformPersistenceError::NotFound => Status::not_found("platform record not found"),
        PlatformPersistenceError::Unavailable => {
            Status::unavailable("platform control is unavailable")
        }
        PlatformPersistenceError::ReadModelNotRegistered => {
            Status::unavailable("platform read model is unavailable")
        }
    }
}

fn validate_platform_read_proof(authorization: &PlatformReadAuthorization) -> Result<(), Status> {
    authorization
        .proof
        .validate(
            &authorization.actor_ref,
            &Scope::Platform,
            "platform.tenant.read",
            false,
            authorization.at,
        )
        .map_err(|_| Status::permission_denied("platform read authorization is invalid"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_error_mapping_redacts_database_details() {
        let status = status_for_platform_error(PlatformPersistenceError::Invalid(
            "postgres://secret should never escape".into(),
        ));
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
        assert!(!status.message().contains("postgres://"));
        assert!(!status.message().contains("secret"));
    }

    #[test]
    fn replacement_api_types_do_not_define_candidate_or_credential_response_fields() {
        let receipt = PlatformCommandReceipt {
            aggregate_id: uuid::Uuid::new_v4(),
            aggregate_version: 1,
            event_id: uuid::Uuid::new_v4(),
            replayed: false,
        };
        let encoded = serde_json::to_string(&receipt).expect("receipt JSON");
        assert!(!encoded.contains("candidate"));
        assert!(!encoded.contains("credential"));
    }

    #[test]
    fn ui_handoff_contains_only_categorical_connector_health() {
        let model = PlatformConnectorReadModel {
            connector_id: uuid::Uuid::new_v4(),
            kind: "active_directory".into(),
            scope: "platform".into(),
            tenant_id: None,
            holding_ref: None,
            provider_key: "ad-primary".into(),
            credential_bound: true,
            configuration_digest: "00".repeat(32),
            capabilities: ["change_feed".into()].into_iter().collect(),
            state: "active".into(),
            version: 2,
            updated_at: Utc::now(),
        };
        let response = PlatformConnectorHealthResponse::from(model);
        let encoded = serde_json::to_string(&response).expect("health JSON");
        assert!(encoded.contains("active"));
        assert!(!encoded.contains("ad-primary"));
        assert!(!encoded.contains("00".repeat(32).as_str()));
    }
}
