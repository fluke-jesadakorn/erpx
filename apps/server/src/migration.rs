//! G13 migration status and execution-work API.
//!
//! Status reads and migration lifecycle commands are proof-bound in the
//! infrastructure adapter. Production command ownership remains Legacy until
//! an explicitly approved, rollback-rehearsed cutover; this API exposes no
//! command-owner or cutover mutation (approval material is never accepted).

use std::collections::BTreeSet;

use chrono::{Duration, Utc};
use erpx_application::{AuditEvent, AuditSink};
use erpx_infrastructure::{G13_EXECUTE_PERMISSION, G13AuthorizationProof};
use erpx_migration::{
    MigrationPhase,
    g13::{G13StartRunRequest, G13TransitionRequest, phase_from_wire},
};
use erpx_proto::erpx::platform_administration::v1::{
    AdvanceMigrationPhaseRequest, AdvanceMigrationPhaseResponse, BeginMigrationRunRequest,
    BeginMigrationRunResponse, ListMigrationStatusesRequest, ListMigrationStatusesResponse,
    MigrationStatus, QueueMigrationWorkRequest, QueueMigrationWorkResponse,
};
use sha2::{Digest, Sha256};
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::AppState;

pub(crate) const MIGRATION_STATUS_READ_PERMISSION: &str = "platform.migration.read";

#[derive(Clone)]
pub(crate) struct MigrationRpcImpl {
    pub(crate) state: AppState,
}

fn migration_run_fields(
    run: erpx_migration::g13::G13MigrationRunDto,
) -> (String, String, String, String, u64, String, String) {
    (
        run.run.run_id.to_string(),
        run.run.cohort,
        erpx_migration::g13::phase_to_wire(run.run.phase).to_owned(),
        erpx_migration::g13::owner_to_wire(run.command_owner).to_owned(),
        run.aggregate_version,
        run.created_at.to_rfc3339(),
        run.updated_at.to_rfc3339(),
    )
}

fn migration_proof(principal_ref: &str) -> G13AuthorizationProof {
    G13AuthorizationProof {
        actor_ref: principal_ref.to_owned(),
        permissions: BTreeSet::from([G13_EXECUTE_PERMISSION.to_owned()]),
        mfa_verified: true,
        break_glass: false,
        expires_at: Utc::now() + Duration::minutes(5),
        cutover_approval_ref: None,
    }
}

impl MigrationRpcImpl {
    pub(crate) async fn list_migration_statuses(
        &self,
        request: Request<ListMigrationStatusesRequest>,
    ) -> Result<Response<ListMigrationStatusesResponse>, Status> {
        let correlation_id = crate::boundary::request_correlation(&request);
        let result = async {
            let claims = self.state.auth.authorize(&request).await?;
            let proto_context = request
                .get_ref()
                .context
                .as_ref()
                .cloned()
                .ok_or_else(|| Status::failed_precondition("query context is required"))?;
            let context = crate::boundary::query(&request, Some(&proto_context))?;
            let scope = crate::query_access_scope(&proto_context)?;
            crate::require_platform_scope(&scope)?;
            self.state
                .auth
                .authorize_role(
                    &self.state.store,
                    &claims,
                    &scope,
                    MIGRATION_STATUS_READ_PERMISSION,
                )
                .await?;

            let requested = request.get_ref().page_size;
            let page_size = if requested == 0 {
                50
            } else {
                requested.min(100)
            };
            let statuses = self
                .state
                .migration
                .list_read_model(page_size)
                .await
                .map_err(|_| Status::unavailable("migration status read unavailable"))?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: None,
                    action: "platform.migration.status.list".into(),
                    purpose: proto_context
                        .audit
                        .as_ref()
                        .map(|audit| audit.purpose.clone())
                        .unwrap_or_else(|| "G13 migration status read".into()),
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("migration audit persistence unavailable"))?;

            Ok(Response::new(ListMigrationStatusesResponse {
                statuses: statuses
                    .into_iter()
                    .map(|status| MigrationStatus {
                        run_id: status.run_id.to_string(),
                        cohort: status.cohort,
                        phase: status.phase,
                        command_owner: status.command_owner,
                        version: status.version,
                        retention_until: status
                            .retention_until
                            .map(|value| value.to_rfc3339())
                            .unwrap_or_default(),
                        transition_count: status.transition_count,
                        work_pending: status.work_pending,
                        work_failed: status.work_failed,
                        reconciliation_state: status.reconciliation_state,
                        rollback_state: status.rollback_state,
                        updated_at: status.updated_at.to_rfc3339(),
                        source_snapshot_hash: status.source_snapshot_hash,
                        source_missing_table_count: status.source_missing_table_count,
                        source_entity_kind_count: status.source_entity_kind_count,
                        source_total_rows: status.source_total_rows,
                        source_captured_at: status
                            .source_captured_at
                            .map(|value| value.to_rfc3339())
                            .unwrap_or_default(),
                    })
                    .collect(),
                correlation_id: context.correlation_id.to_string(),
            }))
        }
        .await;
        result.map_err(|status| crate::errors::enrich(status, Some(correlation_id)))
    }

    pub(crate) async fn queue_migration_work(
        &self,
        request: Request<QueueMigrationWorkRequest>,
    ) -> Result<Response<QueueMigrationWorkResponse>, Status> {
        let correlation_id = crate::boundary::request_correlation(&request);
        let result = async {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let context = crate::command_domain(&request, input.context.as_ref())?;
            let scope = crate::access_scope(&context.tenant_scope);
            crate::require_platform_scope(&scope)?;
            crate::require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor does not match authenticated principal",
            )?;
            self.state
                .auth
                .authorize_role(&self.state.store, &claims, &scope, G13_EXECUTE_PERMISSION)
                .await?;
            self.state
                .auth
                .authorize_mfa(&claims, &input.mfa_code)
                .await?;

            let run_id = input
                .run_id
                .parse::<Uuid>()
                .map_err(|_| Status::invalid_argument("run_id is invalid"))?;
            if input.work_kind.trim().is_empty()
                || input.scope_key.trim().is_empty()
                || input.source_fence.trim().is_empty()
                || input.max_attempts == 0
                || input.max_attempts > 25
            {
                return Err(Status::invalid_argument(
                    "migration work fields are invalid",
                ));
            }
            let request_material = serde_json::json!({
                "run_id": run_id,
                "work_kind": input.work_kind.trim(),
                "scope_key": input.scope_key.trim(),
                "source_fence": input.source_fence.trim(),
                "max_attempts": input.max_attempts,
            });
            let encoded = serde_json::to_vec(&request_material)
                .map_err(|_| Status::internal("migration request hashing failed"))?;
            let mut digest = Sha256::new();
            digest.update(encoded);
            let request_hash = hex::encode(digest.finalize());
            let work_id = Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                format!("g13-queue-work:{}", context.idempotency_key).as_bytes(),
            );
            let proof = G13AuthorizationProof {
                actor_ref: claims.principal_ref.clone(),
                permissions: BTreeSet::from([G13_EXECUTE_PERMISSION.to_owned()]),
                mfa_verified: true,
                break_glass: false,
                expires_at: Utc::now() + Duration::minutes(5),
                cutover_approval_ref: None,
            };
            let receipt = self
                .state
                .migration
                .enqueue_work_command(
                    run_id,
                    work_id,
                    input.work_kind.trim(),
                    input.scope_key.trim(),
                    input.source_fence.trim(),
                    &request_hash,
                    &context.idempotency_key,
                    input.max_attempts as u16,
                    &context.actor.principal_ref,
                    context.correlation_id,
                    &proof,
                )
                .await
                .map_err(|_| Status::failed_precondition("migration work could not be queued"))?;
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: None,
                    action: "platform.migration.work.queue".into(),
                    purpose: context.audit.purpose,
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("migration audit persistence unavailable"))?;
            Ok(Response::new(QueueMigrationWorkResponse {
                work_id: receipt.work_id.to_string(),
                run_id: receipt.run_id.to_string(),
                work_kind: receipt.work_kind,
                scope_key: receipt.scope_key,
                state: if receipt.queued {
                    "pending".into()
                } else {
                    "already_pending".into()
                },
                queued: receipt.queued,
                correlation_id: context.correlation_id.to_string(),
            }))
        }
        .await;
        result.map_err(|status| crate::errors::enrich(status, Some(correlation_id)))
    }

    pub(crate) async fn begin_migration_run(
        &self,
        request: Request<BeginMigrationRunRequest>,
    ) -> Result<Response<BeginMigrationRunResponse>, Status> {
        let correlation_id = crate::boundary::request_correlation(&request);
        let result = async {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let context = crate::command_domain(&request, input.context.as_ref())?;
            let scope = crate::access_scope(&context.tenant_scope);
            crate::require_platform_scope(&scope)?;
            crate::require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor does not match authenticated principal",
            )?;
            self.state
                .auth
                .authorize_role(&self.state.store, &claims, &scope, G13_EXECUTE_PERMISSION)
                .await?;
            self.state
                .auth
                .authorize_mfa(&claims, &input.mfa_code)
                .await?;

            let run_id = input
                .run_id
                .parse::<Uuid>()
                .map_err(|_| Status::invalid_argument("run_id is invalid"))?;
            let cohort = input.cohort.trim();
            let source_contract_hash = input.source_contract_hash.trim();
            let id_namespace = input.id_namespace.trim();
            if cohort.is_empty()
                || cohort.len() > 128
                || source_contract_hash.len() != 64
                || !source_contract_hash
                    .chars()
                    .all(|character| character.is_ascii_digit() || ('a'..='f').contains(&character))
                || id_namespace.is_empty()
                || id_namespace.len() > 128
            {
                return Err(Status::invalid_argument("migration run fields are invalid"));
            }
            let now = Utc::now();
            let run = self
                .state
                .migration
                .start_run(
                    &G13StartRunRequest {
                        run_id,
                        cohort: cohort.to_owned(),
                        source_contract_hash: source_contract_hash.to_owned(),
                        id_namespace: id_namespace.to_owned(),
                        actor_ref: context.actor.principal_ref.clone(),
                        correlation_id: context.correlation_id,
                        idempotency_key: context.idempotency_key.clone(),
                        now,
                    },
                    &migration_proof(&claims.principal_ref),
                )
                .await
                .map_err(|_| Status::failed_precondition("migration run could not be started"))?;
            let (run_id, cohort, phase, command_owner, version, created_at, updated_at) =
                migration_run_fields(run);
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: None,
                    action: "platform.migration.run.begin".into(),
                    purpose: context.audit.purpose,
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("migration audit persistence unavailable"))?;
            Ok(Response::new(BeginMigrationRunResponse {
                run_id,
                cohort,
                phase,
                command_owner,
                version,
                created_at,
                updated_at,
                correlation_id: context.correlation_id.to_string(),
            }))
        }
        .await;
        result.map_err(|status| crate::errors::enrich(status, Some(correlation_id)))
    }

    pub(crate) async fn advance_migration_phase(
        &self,
        request: Request<AdvanceMigrationPhaseRequest>,
    ) -> Result<Response<AdvanceMigrationPhaseResponse>, Status> {
        let correlation_id = crate::boundary::request_correlation(&request);
        let result = async {
            let claims = self.state.auth.authorize(&request).await?;
            let input = request.get_ref();
            let context = crate::command_domain(&request, input.context.as_ref())?;
            let scope = crate::access_scope(&context.tenant_scope);
            crate::require_platform_scope(&scope)?;
            crate::require_actor_match(
                &claims.principal_ref,
                &context.actor.principal_ref,
                "command actor does not match authenticated principal",
            )?;
            self.state
                .auth
                .authorize_role(&self.state.store, &claims, &scope, G13_EXECUTE_PERMISSION)
                .await?;
            self.state
                .auth
                .authorize_mfa(&claims, &input.mfa_code)
                .await?;

            let run_id = input
                .run_id
                .parse::<Uuid>()
                .map_err(|_| Status::invalid_argument("run_id is invalid"))?;
            if input.expected_version == 0 || input.expected_phase.trim().is_empty() {
                return Err(Status::invalid_argument(
                    "expected migration version and phase are required",
                ));
            }
            let expected_phase = phase_from_wire(input.expected_phase.trim())
                .map_err(|_| Status::invalid_argument("expected migration phase is invalid"))?;
            if expected_phase >= MigrationPhase::FinalDeltaImported {
                return Err(Status::failed_precondition(
                    "migration command-owner approval remains external and is not exposed",
                ));
            }
            let run = self
                .state
                .migration
                .transition(
                    &G13TransitionRequest {
                        run_id,
                        expected_aggregate_version: input.expected_version,
                        expected_phase,
                        actor_ref: context.actor.principal_ref.clone(),
                        correlation_id: context.correlation_id,
                        idempotency_key: context.idempotency_key.clone(),
                        now: Utc::now(),
                    },
                    &migration_proof(&claims.principal_ref),
                )
                .await
                .map_err(|_| Status::failed_precondition("migration phase could not advance"))?;
            let (run_id, cohort, phase, command_owner, version, created_at, updated_at) =
                migration_run_fields(run);
            self.state
                .store
                .record(AuditEvent {
                    actor_ref: claims.principal_ref,
                    customer_account_id: None,
                    action: "platform.migration.phase.advance".into(),
                    purpose: context.audit.purpose,
                    correlation_id: context.correlation_id,
                })
                .await
                .map_err(|_| Status::unavailable("migration audit persistence unavailable"))?;
            Ok(Response::new(AdvanceMigrationPhaseResponse {
                run_id,
                cohort,
                phase,
                command_owner,
                version,
                created_at,
                updated_at,
                correlation_id: context.correlation_id.to_string(),
            }))
        }
        .await;
        result.map_err(|status| crate::errors::enrich(status, Some(correlation_id)))
    }
}
