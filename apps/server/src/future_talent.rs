//! Authorization-first target transport for Recruiting and Talent.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_infrastructure::RoutedDatabaseSide;
use erpx_integration_contracts::{
    CurrencyCode, MoneyAmount, RecordTypeId, StableRecordRef, catalog::ContextId,
};
use erpx_kernel::{AuthorityScope, CommandId, CorrelationId, RecordId};
use erpx_proto::{common, erpx::talent as wire};
use erpx_talent as talent;
use erpx_talent_sqlx as talent_sqlx;
use prost::Message;
use sha2::{Digest, Sha256};
use tonic::{Request, Response, Status};
use uuid::Uuid;

use super::future_auth::{self, FutureInvocation};
use crate::AppState;

const TALENT_WRITE: &str = "talent.write";

#[derive(Clone)]
pub(crate) struct TalentCommandRpc {
    state: AppState,
}

#[derive(Clone)]
pub(crate) struct TalentQueryRpc {
    state: AppState,
}

impl TalentCommandRpc {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

impl TalentQueryRpc {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[derive(Debug, Clone)]
struct TalentAccess {
    actor: talent::TalentActor,
    write: bool,
}

#[async_trait]
impl talent::AuthorizationPort for TalentAccess {
    async fn authorize(
        &self,
        principal: &talent::RealmPrincipal,
        authority_scope: &AuthorityScope,
        target_scope: &AuthorityScope,
        action: talent::TalentAction,
    ) -> Result<(), talent::TalentPortError> {
        let action_writes = !matches!(
            action,
            talent::TalentAction::ViewOwnTalent | talent::TalentAction::ViewRestrictedTalent
        );
        if principal != &self.actor.principal
            || authority_scope != &self.actor.authority_scope
            || action_writes != self.write
            || !authority_scope.contains(target_scope)
        {
            return Err(talent::TalentPortError::Denied);
        }
        Ok(())
    }
}

fn actor(
    invocation: &FutureInvocation,
    realm: talent::TalentRealm,
) -> Result<talent::TalentActor, Status> {
    let principal = match realm {
        talent::TalentRealm::Candidate => {
            talent::RealmPrincipal::candidate(invocation.principal_id.as_uuid())
        }
        talent::TalentRealm::Workforce => {
            talent::RealmPrincipal::workforce(invocation.principal_id.as_uuid())
        }
    }
    .map_err(|_| Status::permission_denied("talent principal is invalid"))?;
    Ok(talent::TalentActor {
        holding_id: invocation.holding_id,
        principal,
        authority_scope: invocation.scope.clone(),
    })
}

fn command_service(
    pool: sqlx::PgPool,
    actor: &talent::TalentActor,
) -> talent::TalentCommandService<TalentAccess, talent_sqlx::SqlxTalentWriteRepository> {
    talent::TalentCommandService::new(
        TalentAccess {
            actor: actor.clone(),
            write: true,
        },
        talent_sqlx::SqlxTalentWriteRepository::new(pool),
    )
}

fn query_service(
    pool: sqlx::PgPool,
    actor: &talent::TalentActor,
) -> Result<talent::TalentQueryService<TalentAccess, talent_sqlx::SqlxTalentReadRepository>, Status>
{
    Ok(talent::TalentQueryService::new(
        TalentAccess {
            actor: actor.clone(),
            write: false,
        },
        talent_sqlx::SqlxTalentReadRepository::new(pool, actor.holding_id)
            .map_err(|_| Status::unavailable("talent read model is unavailable"))?,
    ))
}

async fn command_invocation<T>(
    state: &AppState,
    request: &Request<T>,
    context: Option<&common::CommandContext>,
    realm: talent::TalentRealm,
) -> Result<(FutureInvocation, talent::TalentActor), Status> {
    let invocation = future_auth::command(state, request, context, "talent", TALENT_WRITE).await?;
    let actor = actor(&invocation, realm)?;
    Ok((invocation, actor))
}

fn identity<M: Message>(
    context: &common::CommandContext,
    request: &M,
) -> Result<talent::MutationIdentity, Status> {
    Ok(talent::MutationIdentity {
        command_id: CommandId::try_from_uuid(uuid(&context.command_id, "command_id")?)
            .map_err(|_| Status::invalid_argument("command_id is invalid"))?,
        correlation_id: CorrelationId::try_from_uuid(uuid(
            &context.correlation_id,
            "correlation_id",
        )?)
        .map_err(|_| Status::invalid_argument("correlation_id is invalid"))?,
        idempotency_key: context.idempotency_key.clone(),
        request_digest: hex::encode(Sha256::digest(request.encode_to_vec())),
    })
}

fn required_command(
    value: Option<&common::CommandContext>,
) -> Result<&common::CommandContext, Status> {
    value.ok_or_else(|| Status::invalid_argument("command context is required"))
}

fn create_expected(context: &common::CommandContext) -> Result<(), Status> {
    if context.expected_aggregate_version == 0 {
        Ok(())
    } else {
        Err(Status::failed_precondition(
            "create command cannot carry an expected aggregate version",
        ))
    }
}

fn mutation_expected(context: &common::CommandContext) -> Result<u64, Status> {
    if context.expected_aggregate_version == 0 {
        Err(Status::failed_precondition(
            "expected aggregate version is required",
        ))
    } else {
        Ok(context.expected_aggregate_version)
    }
}

fn uuid(value: &str, field: &'static str) -> Result<Uuid, Status> {
    Uuid::parse_str(value).map_err(|_| Status::invalid_argument(format!("{field} is invalid")))
}

fn timestamp(value: &str, field: &'static str) -> Result<DateTime<Utc>, Status> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| Status::invalid_argument(format!("{field} is invalid")))
}

fn decode_scope(value: Option<&common::AuthorityScope>) -> Result<AuthorityScope, Status> {
    use common::authority_scope::Scope;
    let scope = value
        .and_then(|value| value.scope.as_ref())
        .ok_or_else(|| Status::invalid_argument("talent record scope is required"))?;
    let result = match scope {
        Scope::Platform(_) => AuthorityScope::Platform,
        Scope::Holding(value) => AuthorityScope::Holding {
            holding_id: uuid(&value.holding_id, "holding_id")?,
        },
        Scope::Company(value) => AuthorityScope::Company {
            holding_id: uuid(&value.holding_id, "holding_id")?,
            company_id: uuid(&value.company_id, "company_id")?,
        },
        Scope::Branch(value) => AuthorityScope::Branch {
            holding_id: uuid(&value.holding_id, "holding_id")?,
            company_id: uuid(&value.company_id, "company_id")?,
            branch_id: uuid(&value.branch_id, "branch_id")?,
        },
        Scope::Subtree(value) => AuthorityScope::Subtree {
            holding_id: uuid(&value.holding_id, "holding_id")?,
            organization_node_id: uuid(&value.organization_node_id, "organization_node_id")?,
        },
    };
    result
        .validate()
        .map_err(|_| Status::invalid_argument("talent record scope is invalid"))?;
    if !matches!(
        result,
        AuthorityScope::Company { .. } | AuthorityScope::Branch { .. }
    ) {
        return Err(Status::invalid_argument(
            "talent record scope must be an exact company or branch",
        ));
    }
    Ok(result)
}

fn encode_scope(value: &AuthorityScope) -> common::AuthorityScope {
    use common::authority_scope::Scope;
    common::AuthorityScope {
        scope: Some(match value {
            AuthorityScope::Platform => Scope::Platform(common::PlatformScope {}),
            AuthorityScope::Holding { holding_id } => Scope::Holding(common::HoldingScope {
                holding_id: holding_id.to_string(),
            }),
            AuthorityScope::Company {
                holding_id,
                company_id,
            } => Scope::Company(common::CompanyScope {
                holding_id: holding_id.to_string(),
                company_id: company_id.to_string(),
            }),
            AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Scope::Branch(common::BranchScope {
                holding_id: holding_id.to_string(),
                company_id: company_id.to_string(),
                branch_id: branch_id.to_string(),
            }),
            AuthorityScope::Subtree {
                holding_id,
                organization_node_id,
            } => Scope::Subtree(common::SubtreeScope {
                holding_id: holding_id.to_string(),
                organization_node_id: organization_node_id.to_string(),
            }),
        }),
    }
}

fn stable_ref(
    value: &str,
    field: &'static str,
    owner: ContextId,
    record_type: &'static str,
) -> Result<StableRecordRef, Status> {
    StableRecordRef::try_new(
        RecordId::try_from_uuid(uuid(value, field)?)
            .map_err(|_| Status::invalid_argument(format!("{field} is invalid")))?,
        owner,
        RecordTypeId::try_new(record_type)
            .map_err(|_| Status::internal("talent record mapping is invalid"))?,
    )
    .map_err(|_| Status::invalid_argument(format!("{field} is invalid")))
}

fn enforce_actor(
    value: &str,
    field: &'static str,
    actor: &talent::TalentActor,
) -> Result<Uuid, Status> {
    let value = uuid(value, field)?;
    if value != actor.principal.principal_id {
        return Err(Status::permission_denied(format!(
            "{field} must match the authenticated principal"
        )));
    }
    Ok(value)
}

fn status(error: talent::TalentPortError) -> Status {
    match error {
        talent::TalentPortError::Denied
        | talent::TalentPortError::RealmMismatch
        | talent::TalentPortError::NotFound => Status::not_found("talent record unavailable"),
        talent::TalentPortError::Conflict => Status::aborted("talent version conflict"),
        talent::TalentPortError::IdempotencyConflict => {
            Status::already_exists("talent idempotency identity conflicts")
        }
        talent::TalentPortError::Stale => Status::unavailable("talent projection is not current"),
        talent::TalentPortError::Unavailable => {
            Status::unavailable("talent dependency is unavailable")
        }
        talent::TalentPortError::Invalid => Status::invalid_argument("talent request is invalid"),
        talent::TalentPortError::Internal => Status::internal("talent operation failed"),
    }
}

fn response(
    context: &common::CommandContext,
    result: talent::TalentMutationReceipt,
) -> wire::TalentMutationResponse {
    let aggregate_type = external_record_type(&result.aggregate_type).to_owned();
    wire::TalentMutationResponse {
        receipt: Some(common::CommandReceipt {
            command_id: context.command_id.clone(),
            correlation_id: context.correlation_id.clone(),
            aggregate: Some(common::AggregateVersion {
                context: "talent".into(),
                aggregate_type: aggregate_type.clone(),
                aggregate_id: result.receipt.aggregate_id.to_string(),
                version: result.receipt.aggregate_version,
            }),
            committed_events: vec![common::EventPosition {
                event_id: result.receipt.event_id.to_string(),
                global_sequence: result.receipt.global_sequence,
            }],
            committed_at: result.receipt.committed_at.to_rfc3339(),
            replayed: result.receipt.replayed,
        }),
        record_type: aggregate_type,
        record_id: result.receipt.aggregate_id.to_string(),
        lifecycle_status: result.lifecycle_status,
    }
}

fn external_record_type(value: &str) -> &'static str {
    match value {
        "talent_requisition" => "talent.requisition",
        "talent_candidate" => "talent.candidate",
        "talent_application" => "talent.application",
        "talent_offer" => "talent.offer",
        "talent_hire_handoff" => "talent.hire-handoff",
        "talent_proposal" => "talent.proposal",
        "talent_interview" => "talent.interview",
        "talent_review_cycle" => "talent.review-cycle",
        _ => unreachable!("typed Talent application returned an unsupported aggregate type"),
    }
}

fn requisition_status(value: i32) -> Result<talent::RequisitionStatus, Status> {
    match wire::RequisitionStatus::try_from(value).ok() {
        Some(wire::RequisitionStatus::Draft) => Ok(talent::RequisitionStatus::Draft),
        Some(wire::RequisitionStatus::Approved) => Ok(talent::RequisitionStatus::Approved),
        Some(wire::RequisitionStatus::Published) => Ok(talent::RequisitionStatus::Published),
        Some(wire::RequisitionStatus::OnHold) => Ok(talent::RequisitionStatus::OnHold),
        Some(wire::RequisitionStatus::Closed) => Ok(talent::RequisitionStatus::Closed),
        Some(wire::RequisitionStatus::Cancelled) => Ok(talent::RequisitionStatus::Cancelled),
        Some(wire::RequisitionStatus::Archived) => Ok(talent::RequisitionStatus::Archived),
        _ => Err(Status::invalid_argument(
            "target requisition status is invalid",
        )),
    }
}

fn proposal_kind(value: i32) -> Result<talent::ProposalKind, Status> {
    match wire::TalentProposalKind::try_from(value).ok() {
        Some(wire::TalentProposalKind::PerformanceReview) => {
            Ok(talent::ProposalKind::PerformanceReview)
        }
        Some(wire::TalentProposalKind::Calibration) => Ok(talent::ProposalKind::Calibration),
        Some(wire::TalentProposalKind::Succession) => Ok(talent::ProposalKind::Succession),
        Some(wire::TalentProposalKind::Compensation) => Ok(talent::ProposalKind::Compensation),
        _ => Err(Status::invalid_argument("talent proposal kind is invalid")),
    }
}

fn realm(value: i32) -> Result<talent::TalentRealm, Status> {
    match wire::TalentRealm::try_from(value).ok() {
        Some(wire::TalentRealm::Candidate) => Ok(talent::TalentRealm::Candidate),
        Some(wire::TalentRealm::Workforce) => Ok(talent::TalentRealm::Workforce),
        _ => Err(Status::invalid_argument("talent realm is required")),
    }
}

#[tonic::async_trait]
impl wire::talent_command_service_server::TalentCommandService for TalentCommandRpc {
    async fn create_requisition(
        &self,
        request: Request<wire::CreateRequisitionRequest>,
    ) -> Result<Response<wire::TalentMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, actor) = command_invocation(
            &self.state,
            &request,
            Some(context),
            talent::TalentRealm::Workforce,
        )
        .await?;
        create_expected(context)?;
        let scope = decode_scope(body.scope.as_ref())?;
        if !actor.authority_scope.contains(&scope) {
            return Err(Status::permission_denied(
                "requisition scope is outside authenticated authority",
            ));
        }
        let now = Utc::now();
        let requisition = talent::Requisition::draft(
            talent::RequisitionId::try_from_uuid(uuid(&body.requisition_id, "requisition_id")?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
            scope,
            body.title.clone(),
            stable_ref(
                &body.position_record_id,
                "position_record_id",
                ContextId::Organization,
                "organization.company",
            )?,
            body.position_version,
            stable_ref(
                &body.budget_record_id,
                "budget_record_id",
                ContextId::Finance,
                "finance.close-cycle",
            )?,
            body.budget_version,
            body.public_content_digest.clone(),
            body.privacy_review_version,
            enforce_actor(
                &body.proposed_by_principal_id,
                "proposed_by_principal_id",
                &actor,
            )?,
            now,
        )
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let result = command_service(pool, &actor)
            .create_requisition(actor, identity(context, body)?, requisition)
            .await
            .map_err(status)?;
        Ok(Response::new(response(context, result)))
    }

    async fn approve_requisition(
        &self,
        request: Request<wire::ApproveRequisitionRequest>,
    ) -> Result<Response<wire::TalentMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, actor) = command_invocation(
            &self.state,
            &request,
            Some(context),
            talent::TalentRealm::Workforce,
        )
        .await?;
        let expected = mutation_expected(context)?;
        let approver = enforce_actor(
            &body.approved_by_principal_id,
            "approved_by_principal_id",
            &actor,
        )?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let result = command_service(pool, &actor)
            .approve_requisition(
                actor,
                identity(context, body)?,
                talent::RequisitionId::try_from_uuid(uuid(&body.requisition_id, "requisition_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                expected,
                approver,
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(context, result)))
    }

    async fn transition_requisition(
        &self,
        request: Request<wire::TransitionRequisitionRequest>,
    ) -> Result<Response<wire::TalentMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, actor) = command_invocation(
            &self.state,
            &request,
            Some(context),
            talent::TalentRealm::Workforce,
        )
        .await?;
        let expected = mutation_expected(context)?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let result = command_service(pool, &actor)
            .transition_requisition(
                actor,
                identity(context, body)?,
                talent::RequisitionId::try_from_uuid(uuid(&body.requisition_id, "requisition_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                expected,
                requisition_status(body.target_status)?,
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(context, result)))
    }

    async fn create_candidate_profile(
        &self,
        request: Request<wire::CreateCandidateProfileRequest>,
    ) -> Result<Response<wire::TalentMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, actor) = command_invocation(
            &self.state,
            &request,
            Some(context),
            talent::TalentRealm::Candidate,
        )
        .await?;
        create_expected(context)?;
        let scope = decode_scope(body.scope.as_ref())?;
        if !actor.authority_scope.contains(&scope) {
            return Err(Status::permission_denied(
                "candidate scope is outside authenticated authority",
            ));
        }
        let candidate_principal_id = enforce_actor(
            &body.candidate_principal_id,
            "candidate_principal_id",
            &actor,
        )?;
        let candidate = talent::CandidateProfile::create(
            talent::CandidateId::try_from_uuid(uuid(&body.candidate_id, "candidate_id")?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
            scope,
            talent::RealmPrincipal::candidate(candidate_principal_id)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
            body.profile_digest.clone(),
            body.attachment_digests.clone(),
            body.recruiting_consent,
            body.marketing_consent,
            body.retention_policy_version,
            Utc::now(),
        )
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let result = command_service(pool, &actor)
            .create_candidate_profile(actor, identity(context, body)?, candidate)
            .await
            .map_err(status)?;
        Ok(Response::new(response(context, result)))
    }

    async fn request_candidate_erasure(
        &self,
        request: Request<wire::CandidateIdentityRequest>,
    ) -> Result<Response<wire::TalentMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, actor) = command_invocation(
            &self.state,
            &request,
            Some(context),
            talent::TalentRealm::Candidate,
        )
        .await?;
        let expected = mutation_expected(context)?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let result = command_service(pool, &actor)
            .request_candidate_erasure(
                actor,
                identity(context, body)?,
                talent::CandidateId::try_from_uuid(uuid(&body.candidate_id, "candidate_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                expected,
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(context, result)))
    }

    async fn complete_candidate_erasure(
        &self,
        request: Request<wire::CandidateIdentityRequest>,
    ) -> Result<Response<wire::TalentMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, actor) = command_invocation(
            &self.state,
            &request,
            Some(context),
            talent::TalentRealm::Workforce,
        )
        .await?;
        let expected = mutation_expected(context)?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let result = command_service(pool, &actor)
            .complete_candidate_erasure(
                actor,
                identity(context, body)?,
                talent::CandidateId::try_from_uuid(uuid(&body.candidate_id, "candidate_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                expected,
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(context, result)))
    }

    async fn set_candidate_legal_hold(
        &self,
        request: Request<wire::SetCandidateLegalHoldRequest>,
    ) -> Result<Response<wire::TalentMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, actor) = command_invocation(
            &self.state,
            &request,
            Some(context),
            talent::TalentRealm::Workforce,
        )
        .await?;
        let expected = mutation_expected(context)?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let result = command_service(pool, &actor)
            .set_candidate_legal_hold(
                actor,
                identity(context, body)?,
                talent::CandidateId::try_from_uuid(uuid(&body.candidate_id, "candidate_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                expected,
                body.legal_hold,
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(context, result)))
    }

    async fn submit_application(
        &self,
        request: Request<wire::SubmitApplicationRequest>,
    ) -> Result<Response<wire::TalentMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, actor) = command_invocation(
            &self.state,
            &request,
            Some(context),
            talent::TalentRealm::Candidate,
        )
        .await?;
        create_expected(context)?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let result = command_service(pool, &actor)
            .submit_application(
                actor,
                identity(context, body)?,
                talent::ApplicationId::try_from_uuid(uuid(&body.application_id, "application_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                talent::CandidateId::try_from_uuid(uuid(&body.candidate_id, "candidate_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                body.candidate_profile_version,
                talent::RequisitionId::try_from_uuid(uuid(&body.requisition_id, "requisition_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                body.requisition_version,
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(context, result)))
    }

    async fn add_application_interview(
        &self,
        request: Request<wire::AddApplicationInterviewRequest>,
    ) -> Result<Response<wire::TalentMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, actor) = command_invocation(
            &self.state,
            &request,
            Some(context),
            talent::TalentRealm::Workforce,
        )
        .await?;
        let expected = mutation_expected(context)?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let result = command_service(pool, &actor)
            .add_application_interview(
                actor,
                identity(context, body)?,
                talent::ApplicationId::try_from_uuid(uuid(&body.application_id, "application_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                expected,
                talent::InterviewEvidence {
                    slot_digest: body.slot_digest.clone(),
                    participant_digest: body.participant_digest.clone(),
                    outcome_digest: body.outcome_digest.clone(),
                },
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(context, result)))
    }

    async fn select_application(
        &self,
        request: Request<wire::SelectApplicationRequest>,
    ) -> Result<Response<wire::TalentMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, actor) = command_invocation(
            &self.state,
            &request,
            Some(context),
            talent::TalentRealm::Workforce,
        )
        .await?;
        let expected = mutation_expected(context)?;
        let decided_by = enforce_actor(
            &body.decided_by_principal_id,
            "decided_by_principal_id",
            &actor,
        )?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let result = command_service(pool, &actor)
            .select_application(
                actor,
                identity(context, body)?,
                talent::ApplicationId::try_from_uuid(uuid(&body.application_id, "application_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                expected,
                talent::SelectionEvidence {
                    profile_version: body.exact_profile_version,
                    requisition_version: body.exact_requisition_version,
                    scorecard_digest: body.scorecard_digest.clone(),
                    decided_by,
                },
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(context, result)))
    }

    async fn withdraw_application(
        &self,
        request: Request<wire::ApplicationIdentityRequest>,
    ) -> Result<Response<wire::TalentMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, actor) = command_invocation(
            &self.state,
            &request,
            Some(context),
            talent::TalentRealm::Candidate,
        )
        .await?;
        let expected = mutation_expected(context)?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let result = command_service(pool, &actor)
            .withdraw_application(
                actor,
                identity(context, body)?,
                talent::ApplicationId::try_from_uuid(uuid(&body.application_id, "application_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                expected,
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(context, result)))
    }

    async fn create_offer(
        &self,
        request: Request<wire::CreateOfferRequest>,
    ) -> Result<Response<wire::TalentMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, actor) = command_invocation(
            &self.state,
            &request,
            Some(context),
            talent::TalentRealm::Workforce,
        )
        .await?;
        create_expected(context)?;
        let proposed_by = enforce_actor(
            &body.proposed_by_principal_id,
            "proposed_by_principal_id",
            &actor,
        )?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let result = command_service(pool, &actor)
            .create_offer(
                actor,
                identity(context, body)?,
                talent::OfferId::try_from_uuid(uuid(&body.offer_id, "offer_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                talent::ApplicationId::try_from_uuid(uuid(&body.application_id, "application_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                body.application_version,
                body.terms_digest.clone(),
                body.terms_version,
                timestamp(&body.expires_at, "expires_at")?,
                proposed_by,
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(context, result)))
    }

    async fn approve_offer(
        &self,
        request: Request<wire::ApproveOfferRequest>,
    ) -> Result<Response<wire::TalentMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, actor) = command_invocation(
            &self.state,
            &request,
            Some(context),
            talent::TalentRealm::Workforce,
        )
        .await?;
        let expected = mutation_expected(context)?;
        let approver = enforce_actor(
            &body.approved_by_principal_id,
            "approved_by_principal_id",
            &actor,
        )?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let result = command_service(pool, &actor)
            .approve_offer(
                actor,
                identity(context, body)?,
                talent::OfferId::try_from_uuid(uuid(&body.offer_id, "offer_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                expected,
                approver,
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(context, result)))
    }

    async fn present_offer(
        &self,
        request: Request<wire::OfferIdentityRequest>,
    ) -> Result<Response<wire::TalentMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, actor) = command_invocation(
            &self.state,
            &request,
            Some(context),
            talent::TalentRealm::Workforce,
        )
        .await?;
        let expected = mutation_expected(context)?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let result = command_service(pool, &actor)
            .present_offer(
                actor,
                identity(context, body)?,
                talent::OfferId::try_from_uuid(uuid(&body.offer_id, "offer_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                expected,
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(context, result)))
    }

    async fn decide_offer(
        &self,
        request: Request<wire::DecideOfferRequest>,
    ) -> Result<Response<wire::TalentMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, actor) = command_invocation(
            &self.state,
            &request,
            Some(context),
            talent::TalentRealm::Candidate,
        )
        .await?;
        let expected = mutation_expected(context)?;
        let candidate_principal_id = enforce_actor(
            &body.candidate_principal_id,
            "candidate_principal_id",
            &actor,
        )?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let result = command_service(pool, &actor)
            .decide_offer(
                actor,
                identity(context, body)?,
                talent::OfferId::try_from_uuid(uuid(&body.offer_id, "offer_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                expected,
                candidate_principal_id,
                body.accepted,
                body.acceptance_receipt_id.clone(),
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(context, result)))
    }

    async fn request_hire_handoff(
        &self,
        request: Request<wire::RequestHireHandoffRequest>,
    ) -> Result<Response<wire::TalentMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, actor) = command_invocation(
            &self.state,
            &request,
            Some(context),
            talent::TalentRealm::Workforce,
        )
        .await?;
        create_expected(context)?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let result = command_service(pool, &actor)
            .request_hire_handoff(
                actor,
                identity(context, body)?,
                talent::HireHandoffId::try_from_uuid(uuid(&body.handoff_id, "handoff_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                talent::OfferId::try_from_uuid(uuid(&body.offer_id, "offer_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                body.offer_version,
                body.evidence_digest.clone(),
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(context, result)))
    }

    async fn reconcile_hire_handoff(
        &self,
        request: Request<wire::ReconcileHireHandoffRequest>,
    ) -> Result<Response<wire::TalentMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (_invocation, _actor) = command_invocation(
            &self.state,
            &request,
            Some(context),
            talent::TalentRealm::Workforce,
        )
        .await?;
        let _ = mutation_expected(context)?;
        let _ = talent::HireHandoffId::try_from_uuid(uuid(&body.handoff_id, "handoff_id")?)
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        Err(Status::unavailable(
            "People hire-handoff acknowledgement port is unavailable",
        ))
    }

    async fn create_proposal(
        &self,
        request: Request<wire::CreateTalentProposalRequest>,
    ) -> Result<Response<wire::TalentMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, actor) = command_invocation(
            &self.state,
            &request,
            Some(context),
            talent::TalentRealm::Workforce,
        )
        .await?;
        create_expected(context)?;
        let scope = decode_scope(body.scope.as_ref())?;
        if !actor.authority_scope.contains(&scope) {
            return Err(Status::permission_denied(
                "proposal scope is outside authenticated authority",
            ));
        }
        let kind = proposal_kind(body.kind)?;
        let amount = body
            .compensation_amount
            .as_ref()
            .map(|value| {
                Ok::<_, Status>(MoneyAmount::new(
                    value.minor_units,
                    CurrencyCode::try_new(&value.currency)
                        .map_err(|_| Status::invalid_argument("currency is invalid"))?,
                ))
            })
            .transpose()?;
        let proposal = talent::TalentProposal::draft(
            talent::ProposalId::try_from_uuid(uuid(&body.proposal_id, "proposal_id")?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
            scope,
            kind,
            stable_ref(
                &body.employee_record_id,
                "employee_record_id",
                ContextId::PeoplePayroll,
                "people.employee",
            )?,
            body.employee_version,
            body.cycle_version,
            body.evidence_digest.clone(),
            amount,
            enforce_actor(
                &body.proposed_by_principal_id,
                "proposed_by_principal_id",
                &actor,
            )?,
            Utc::now(),
        )
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let result = command_service(pool, &actor)
            .create_proposal(actor, identity(context, body)?, proposal)
            .await
            .map_err(status)?;
        Ok(Response::new(response(context, result)))
    }

    async fn submit_proposal(
        &self,
        request: Request<wire::TalentProposalIdentityRequest>,
    ) -> Result<Response<wire::TalentMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, actor) = command_invocation(
            &self.state,
            &request,
            Some(context),
            talent::TalentRealm::Workforce,
        )
        .await?;
        let expected = mutation_expected(context)?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let result = command_service(pool, &actor)
            .submit_proposal(
                actor,
                identity(context, body)?,
                talent::ProposalId::try_from_uuid(uuid(&body.proposal_id, "proposal_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                expected,
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(context, result)))
    }

    async fn approve_proposal(
        &self,
        request: Request<wire::ApproveTalentProposalRequest>,
    ) -> Result<Response<wire::TalentMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, actor) = command_invocation(
            &self.state,
            &request,
            Some(context),
            talent::TalentRealm::Workforce,
        )
        .await?;
        let expected = mutation_expected(context)?;
        let approver = enforce_actor(
            &body.approved_by_principal_id,
            "approved_by_principal_id",
            &actor,
        )?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let result = command_service(pool, &actor)
            .approve_proposal(
                actor,
                identity(context, body)?,
                talent::ProposalId::try_from_uuid(uuid(&body.proposal_id, "proposal_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                expected,
                approver,
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(context, result)))
    }

    async fn reconcile_proposal(
        &self,
        request: Request<wire::ReconcileTalentProposalRequest>,
    ) -> Result<Response<wire::TalentMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (_invocation, _actor) = command_invocation(
            &self.state,
            &request,
            Some(context),
            talent::TalentRealm::Workforce,
        )
        .await?;
        let _ = mutation_expected(context)?;
        let _ = talent::ProposalId::try_from_uuid(uuid(&body.proposal_id, "proposal_id")?)
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        Err(Status::unavailable(
            "People/Payroll proposal acknowledgement port is unavailable",
        ))
    }
}

#[derive(Debug, Clone, Copy)]
enum TalentRecordRoute {
    Requisition,
    Candidate,
    Application,
    Offer,
    HireHandoff,
    Proposal,
    Interview,
    ReviewCycle,
}

impl TalentRecordRoute {
    const fn record_type(self) -> &'static str {
        match self {
            Self::Requisition => "talent.requisition",
            Self::Candidate => "talent.candidate",
            Self::Application => "talent.application",
            Self::Offer => "talent.offer",
            Self::HireHandoff => "talent.hire-handoff",
            Self::Proposal => "talent.proposal",
            Self::Interview => "talent.interview",
            Self::ReviewCycle => "talent.review-cycle",
        }
    }

    const fn permission(self, caller_realm: talent::TalentRealm) -> &'static str {
        match self {
            Self::Candidate | Self::Application | Self::Offer | Self::Interview => {
                match caller_realm {
                    talent::TalentRealm::Candidate => "talent.candidate.self.read",
                    talent::TalentRealm::Workforce => "talent.candidate.manage.read",
                }
            }
            Self::Requisition | Self::HireHandoff | Self::Proposal | Self::ReviewCycle => {
                "talent.workforce.read"
            }
        }
    }
}

impl TalentQueryRpc {
    async fn get(
        &self,
        request: Request<wire::GetTalentRecordRequest>,
        route: TalentRecordRoute,
    ) -> Result<Response<wire::GetTalentRecordResponse>, Status> {
        let body = request.get_ref();
        let caller_realm = realm(body.realm)?;
        let invocation = future_auth::query(
            &self.state,
            &request,
            body.context.as_ref(),
            "talent",
            route.permission(caller_realm),
        )
        .await?;
        let actor = actor(&invocation, caller_realm)?;
        let id = uuid(&body.record_id, "record_id")?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Read).await?;
        let service = query_service(pool, &actor)?;
        let projection = match route {
            TalentRecordRoute::Requisition => {
                service
                    .get_requisition(
                        &actor,
                        talent::RequisitionId::try_from_uuid(id)
                            .map_err(|error| Status::invalid_argument(error.to_string()))?,
                    )
                    .await
            }
            TalentRecordRoute::Candidate => {
                service
                    .get_candidate_profile(
                        &actor,
                        talent::CandidateId::try_from_uuid(id)
                            .map_err(|error| Status::invalid_argument(error.to_string()))?,
                    )
                    .await
            }
            TalentRecordRoute::Application => {
                service
                    .get_application(
                        &actor,
                        talent::ApplicationId::try_from_uuid(id)
                            .map_err(|error| Status::invalid_argument(error.to_string()))?,
                    )
                    .await
            }
            TalentRecordRoute::Offer => {
                service
                    .get_offer(
                        &actor,
                        talent::OfferId::try_from_uuid(id)
                            .map_err(|error| Status::invalid_argument(error.to_string()))?,
                    )
                    .await
            }
            TalentRecordRoute::HireHandoff => {
                service
                    .get_hire_handoff(
                        &actor,
                        talent::HireHandoffId::try_from_uuid(id)
                            .map_err(|error| Status::invalid_argument(error.to_string()))?,
                    )
                    .await
            }
            TalentRecordRoute::Proposal => {
                service
                    .get_proposal(
                        &actor,
                        talent::ProposalId::try_from_uuid(id)
                            .map_err(|error| Status::invalid_argument(error.to_string()))?,
                    )
                    .await
            }
            TalentRecordRoute::Interview => service.get_interview(&actor, id).await,
            TalentRecordRoute::ReviewCycle => service.get_review_cycle(&actor, id).await,
        }
        .map_err(status)?;
        let lag_millis = Utc::now()
            .signed_duration_since(projection.projected_at)
            .to_std()
            .ok()
            .and_then(|duration| u64::try_from(duration.as_millis()).ok());
        Ok(Response::new(wire::GetTalentRecordResponse {
            record: Some(wire::TalentRecordProjection {
                record_id: projection.aggregate_id.to_string(),
                source_version: projection.aggregate_version,
                scope: Some(encode_scope(&projection.scope)),
                authority_realm: match projection.authority_realm {
                    talent::TalentRealm::Workforce => wire::TalentRealm::Workforce as i32,
                    talent::TalentRealm::Candidate => wire::TalentRealm::Candidate as i32,
                },
                record_type: route.record_type().into(),
                lifecycle_status: projection.lifecycle_status,
                candidate_principal_id: projection
                    .candidate_principal_id
                    .map(|value| value.to_string()),
                requisition_id: None,
                application_id: None,
                offer_id: None,
                employee_record_id: None,
                source_position: projection.source_position,
                projected_at: projection.projected_at.to_rfc3339(),
            }),
            projection: Some(common::ProjectionMetadata {
                checkpoint: None,
                freshness: common::ProjectionFreshnessState::Current as i32,
                lag_millis,
            }),
        }))
    }
}

#[tonic::async_trait]
impl wire::talent_query_service_server::TalentQueryService for TalentQueryRpc {
    async fn get_requisition(
        &self,
        request: Request<wire::GetTalentRecordRequest>,
    ) -> Result<Response<wire::GetTalentRecordResponse>, Status> {
        self.get(request, TalentRecordRoute::Requisition).await
    }

    async fn get_candidate_profile(
        &self,
        request: Request<wire::GetTalentRecordRequest>,
    ) -> Result<Response<wire::GetTalentRecordResponse>, Status> {
        self.get(request, TalentRecordRoute::Candidate).await
    }

    async fn get_application(
        &self,
        request: Request<wire::GetTalentRecordRequest>,
    ) -> Result<Response<wire::GetTalentRecordResponse>, Status> {
        self.get(request, TalentRecordRoute::Application).await
    }

    async fn get_offer(
        &self,
        request: Request<wire::GetTalentRecordRequest>,
    ) -> Result<Response<wire::GetTalentRecordResponse>, Status> {
        self.get(request, TalentRecordRoute::Offer).await
    }

    async fn get_hire_handoff(
        &self,
        request: Request<wire::GetTalentRecordRequest>,
    ) -> Result<Response<wire::GetTalentRecordResponse>, Status> {
        self.get(request, TalentRecordRoute::HireHandoff).await
    }

    async fn get_proposal(
        &self,
        request: Request<wire::GetTalentRecordRequest>,
    ) -> Result<Response<wire::GetTalentRecordResponse>, Status> {
        self.get(request, TalentRecordRoute::Proposal).await
    }

    async fn get_record(
        &self,
        request: Request<wire::GetTalentRecordRequest>,
    ) -> Result<Response<wire::GetTalentRecordResponse>, Status> {
        let route = match realm(request.get_ref().realm)? {
            talent::TalentRealm::Candidate => TalentRecordRoute::Interview,
            talent::TalentRealm::Workforce => TalentRecordRoute::ReviewCycle,
        };
        self.get(request, route).await
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_query_method_uses_a_closed_talent_route() {
        let source = include_str!("future_talent.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source");
        for route in [
            "TalentRecordRoute::Requisition",
            "TalentRecordRoute::Candidate",
            "TalentRecordRoute::Application",
            "TalentRecordRoute::Offer",
            "TalentRecordRoute::HireHandoff",
            "TalentRecordRoute::Proposal",
            "TalentRecordRoute::Interview",
            "TalentRecordRoute::ReviewCycle",
        ] {
            assert!(production.contains(route));
        }
        assert!(!production.contains("aggregate_type: body"));
    }

    #[test]
    fn only_owner_acknowledgements_are_explicitly_unavailable() {
        let source = include_str!("future_talent.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source");
        assert_eq!(
            production
                .matches("acknowledgement port is unavailable")
                .count(),
            2
        );
        assert!(production.contains("People hire-handoff acknowledgement port is unavailable"));
        assert!(production.contains("People/Payroll proposal acknowledgement port is unavailable"));
        assert!(!production.contains("typed use case is activating"));
    }
}
