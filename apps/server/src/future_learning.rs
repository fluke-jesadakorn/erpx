//! Native target transport for the Learning bounded context.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use erpx_infrastructure::RoutedDatabaseSide;
use erpx_integration_contracts::{
    RecordTypeId, RecordVersionRef, StableRecordRef, catalog::ContextId,
};
use erpx_kernel::{CommandId, CorrelationId, PrincipalId, RecordId, TenantScope};
use erpx_learning as learning;
use erpx_learning_sqlx as learning_sqlx;
use erpx_proto::{
    common::v2 as common,
    erpx::learning::v2::{self as wire},
};
use prost::Message;
use sha2::{Digest, Sha256};
use tonic::{Request, Response, Status};
use uuid::Uuid;

use super::future_auth::{self, FutureInvocation};
use crate::AppState;

const PRODUCT: &str = "learning";
const WRITE_PERMISSION: &str = "learning.write";
const READ_PERMISSION: &str = "learning.read";

#[derive(Clone)]
pub(crate) struct LearningCommandRpc {
    state: AppState,
}

#[derive(Clone)]
pub(crate) struct LearningQueryRpc {
    state: AppState,
}

impl LearningCommandRpc {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

impl LearningQueryRpc {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[derive(Debug, Clone)]
struct LearningAccess {
    actor: learning::ActorContext,
    write: bool,
}

#[async_trait]
impl learning::AuthorizationPort for LearningAccess {
    async fn authorize(
        &self,
        actor: &learning::ActorContext,
        target_scope: &TenantScope,
        action: learning::LearningAction,
    ) -> Result<learning::AuthorizedLearningAccess, learning::PortError> {
        let writes = !matches!(action, learning::LearningAction::View);
        if actor != &self.actor
            || writes != self.write
            || !self.actor.authority_scope.contains(target_scope)
        {
            return Err(learning::PortError::Denied);
        }
        Ok(learning::AuthorizedLearningAccess {
            actor: actor.clone(),
            target_scope: target_scope.clone(),
            action,
        })
    }
}

fn actor(invocation: &FutureInvocation) -> learning::ActorContext {
    learning::ActorContext {
        tenant_id: invocation.tenant_id,
        principal_id: invocation.principal_id,
        authority_scope: invocation.scope.clone(),
    }
}

fn invalid(field: &'static str) -> Status {
    Status::invalid_argument(format!("{field} is invalid"))
}

fn uuid(value: &str, field: &'static str) -> Result<Uuid, Status> {
    Uuid::parse_str(value).map_err(|_| invalid(field))
}

fn principal(value: &str, field: &'static str) -> Result<PrincipalId, Status> {
    PrincipalId::try_from_uuid(uuid(value, field)?).map_err(|_| invalid(field))
}

fn expected_version(context: &common::CommandContext) -> Result<Option<u64>, Status> {
    if context.expected_aggregate_version == 0 {
        Err(Status::failed_precondition(
            "expected_aggregate_version is required for an update",
        ))
    } else {
        Ok(Some(context.expected_aggregate_version))
    }
}

fn require_create(context: &common::CommandContext) -> Result<(), Status> {
    if context.expected_aggregate_version == 0 {
        Ok(())
    } else {
        Err(Status::failed_precondition(
            "a create command cannot carry an expected version",
        ))
    }
}

fn identity<M: Message>(
    context: &common::CommandContext,
    request: &M,
) -> Result<learning::MutationIdentity, Status> {
    Ok(learning::MutationIdentity {
        command_id: CommandId::try_from_uuid(uuid(&context.command_id, "command_id")?)
            .map_err(|_| invalid("command_id"))?,
        idempotency_key: context.idempotency_key.clone(),
        request_digest: hex::encode(Sha256::digest(request.encode_to_vec())),
        correlation_id: CorrelationId::try_from_uuid(uuid(
            &context.correlation_id,
            "correlation_id",
        )?)
        .map_err(|_| invalid("correlation_id"))?,
    })
}

fn command_context(
    context: Option<&common::CommandContext>,
) -> Result<&common::CommandContext, Status> {
    context.ok_or_else(|| Status::invalid_argument("command context is required"))
}

fn query_context(context: Option<&common::QueryContext>) -> Result<&common::QueryContext, Status> {
    context.ok_or_else(|| Status::invalid_argument("query context is required"))
}

fn decode_scope(value: Option<&common::TenantScope>) -> Result<TenantScope, Status> {
    use common::tenant_scope::Scope;
    let scope = value
        .and_then(|value| value.scope.as_ref())
        .ok_or_else(|| Status::invalid_argument("record scope is required"))?;
    let decoded = match scope {
        Scope::Platform(_) => TenantScope::Platform,
        Scope::Holding(value) => TenantScope::Holding {
            holding_id: uuid(&value.holding_id, "holding_id")?,
        },
        Scope::Company(value) => TenantScope::Company {
            holding_id: uuid(&value.holding_id, "holding_id")?,
            company_id: uuid(&value.company_id, "company_id")?,
        },
        Scope::Branch(value) => TenantScope::Branch {
            holding_id: uuid(&value.holding_id, "holding_id")?,
            company_id: uuid(&value.company_id, "company_id")?,
            branch_id: uuid(&value.branch_id, "branch_id")?,
        },
        Scope::Subtree(value) => TenantScope::Subtree {
            holding_id: uuid(&value.holding_id, "holding_id")?,
            company_ids: value
                .company_ids
                .iter()
                .map(|value| uuid(value, "company_id"))
                .collect::<Result<_, _>>()?,
            branch_ids: value
                .branch_ids
                .iter()
                .map(|value| uuid(value, "branch_id"))
                .collect::<Result<_, _>>()?,
        },
    };
    decoded.validate().map_err(|_| invalid("record scope"))?;
    Ok(decoded)
}

fn ensure_scope(
    invocation: &FutureInvocation,
    presented: Option<&common::TenantScope>,
) -> Result<(), Status> {
    if decode_scope(presented)? == invocation.scope {
        Ok(())
    } else {
        Err(Status::permission_denied(
            "record scope does not match authenticated authority",
        ))
    }
}

fn encode_scope(scope: &TenantScope) -> common::TenantScope {
    use common::tenant_scope::Scope;
    let scope = match scope {
        TenantScope::Platform => Scope::Platform(common::PlatformScope {}),
        TenantScope::Holding { holding_id } => Scope::Holding(common::HoldingScope {
            holding_id: holding_id.to_string(),
        }),
        TenantScope::Company {
            holding_id,
            company_id,
        } => Scope::Company(common::CompanyScope {
            holding_id: holding_id.to_string(),
            company_id: company_id.to_string(),
        }),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Scope::Branch(common::BranchScope {
            holding_id: holding_id.to_string(),
            company_id: company_id.to_string(),
            branch_id: branch_id.to_string(),
        }),
        TenantScope::Subtree {
            holding_id,
            company_ids,
            branch_ids,
        } => Scope::Subtree(common::SubtreeScope {
            holding_id: holding_id.to_string(),
            company_ids: company_ids.iter().map(ToString::to_string).collect(),
            branch_ids: branch_ids.iter().map(ToString::to_string).collect(),
        }),
    };
    common::TenantScope { scope: Some(scope) }
}

fn stable_reference(
    record_id: &str,
    owner_context: &str,
    record_type: &str,
) -> Result<StableRecordRef, Status> {
    let owner = ContextId::ALL
        .into_iter()
        .find(|context| context.slug() == owner_context)
        .ok_or_else(|| invalid("owner_context"))?;
    StableRecordRef::try_new(
        RecordId::try_from_uuid(uuid(record_id, "record_id")?).map_err(|_| invalid("record_id"))?,
        owner,
        RecordTypeId::try_new(record_type).map_err(|_| invalid("record_type"))?,
    )
    .map_err(|_| invalid("record reference"))
}

fn version_reference(value: Option<wire::RecordVersionRef>) -> Result<RecordVersionRef, Status> {
    let value = value.ok_or_else(|| Status::invalid_argument("record reference is required"))?;
    RecordVersionRef::try_new(
        stable_reference(&value.record_id, &value.owner_context, &value.record_type)?,
        value.version,
    )
    .map_err(|_| invalid("record version reference"))
}

fn optional_reference(value: Option<wire::RecordRef>) -> Result<Option<StableRecordRef>, Status> {
    value
        .map(|value| stable_reference(&value.record_id, &value.owner_context, &value.record_type))
        .transpose()
}

fn parse_time(value: &str, field: &'static str) -> Result<DateTime<Utc>, Status> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| invalid(field))
}

fn parse_date(value: &str, field: &'static str) -> Result<NaiveDate, Status> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| invalid(field))
}

fn learning_status(error: learning::LearningError) -> Status {
    match error {
        learning::LearningError::Neutral => Status::not_found("learning item unavailable"),
        learning::LearningError::Conflict => Status::aborted("learning version conflict"),
        learning::LearningError::Invalid(message) => Status::invalid_argument(message),
        learning::LearningError::Unavailable => {
            Status::unavailable("learning dependency unavailable")
        }
        learning::LearningError::Internal => Status::internal("learning operation failed"),
    }
}

fn receipt(
    context: &common::CommandContext,
    value: learning::CommitReceipt,
    record_type: &'static str,
    record_id: Uuid,
    lifecycle_status: &'static str,
) -> wire::LearningMutationResponse {
    wire::LearningMutationResponse {
        receipt: Some(common::CommandReceipt {
            command_id: context.command_id.clone(),
            correlation_id: context.correlation_id.clone(),
            aggregate: Some(common::AggregateVersion {
                context: PRODUCT.into(),
                aggregate_type: record_type.into(),
                aggregate_id: record_id.to_string(),
                version: value.version,
            }),
            committed_events: vec![common::EventPosition {
                event_id: value.event_id.to_string(),
                global_sequence: value.global_sequence,
            }],
            committed_at: value.committed_at.to_rfc3339(),
            replayed: value.replayed,
        }),
        record_type: record_type.into(),
        record_id: record_id.to_string(),
        lifecycle_status: lifecycle_status.into(),
    }
}

async fn invocation_for_command<T>(
    state: &AppState,
    request: &Request<T>,
    context: &common::CommandContext,
    scope: Option<&common::TenantScope>,
) -> Result<FutureInvocation, Status> {
    let invocation =
        future_auth::command(state, request, Some(context), PRODUCT, WRITE_PERMISSION).await?;
    ensure_scope(&invocation, scope)?;
    Ok(invocation)
}

async fn command_service(
    state: &AppState,
    invocation: &FutureInvocation,
) -> Result<
    learning::LearningCommandService<LearningAccess, learning_sqlx::SqlxLearningWriteRepository>,
    Status,
> {
    let owner_actor = actor(invocation);
    let pool = future_auth::pool(state, invocation, RoutedDatabaseSide::Write).await?;
    Ok(learning::LearningCommandService::new(
        LearningAccess {
            actor: owner_actor,
            write: true,
        },
        learning_sqlx::SqlxLearningWriteRepository::new(pool),
    ))
}

#[tonic::async_trait]
impl wire::learning_command_service_server::LearningCommandService for LearningCommandRpc {
    async fn publish_course(
        &self,
        request: Request<wire::PublishCourseRequest>,
    ) -> Result<Response<wire::LearningMutationResponse>, Status> {
        let body = request.get_ref();
        let context = command_context(body.context.as_ref())?;
        require_create(context)?;
        let invocation =
            invocation_for_command(&self.state, &request, context, body.scope.as_ref()).await?;
        let course_id = learning::CourseId::try_from_uuid(uuid(&body.course_id, "course_id")?)
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let passing_basis_points = u16::try_from(body.passing_basis_points)
            .map_err(|_| invalid("passing_basis_points"))?;
        let course = learning::Course::published(
            course_id,
            invocation.scope.clone(),
            body.title.clone(),
            passing_basis_points,
            body.required_attendance_minutes,
            optional_reference(body.provider.clone())?,
            Utc::now(),
        )
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let owner_actor = actor(&invocation);
        let result = command_service(&self.state, &invocation)
            .await?
            .publish_course(&owner_actor, identity(context, request.get_ref())?, course)
            .await
            .map_err(learning_status)?;
        Ok(Response::new(receipt(
            context,
            result,
            "course",
            course_id.as_uuid(),
            "published",
        )))
    }

    async fn retire_course(
        &self,
        request: Request<wire::RetireCourseRequest>,
    ) -> Result<Response<wire::LearningMutationResponse>, Status> {
        let body = request.get_ref();
        let context = command_context(body.context.as_ref())?;
        let invocation =
            invocation_for_command(&self.state, &request, context, body.course_scope.as_ref())
                .await?;
        let course_id = learning::CourseId::try_from_uuid(uuid(&body.course_id, "course_id")?)
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let owner_actor = actor(&invocation);
        let result = command_service(&self.state, &invocation)
            .await?
            .retire_course(
                &owner_actor,
                identity(context, request.get_ref())?,
                course_id,
                &invocation.scope,
                expected_version(context)?,
                Utc::now(),
            )
            .await
            .map_err(learning_status)?;
        Ok(Response::new(receipt(
            context,
            result,
            "course",
            course_id.as_uuid(),
            "retired",
        )))
    }

    async fn assign_course(
        &self,
        request: Request<wire::AssignCourseRequest>,
    ) -> Result<Response<wire::LearningMutationResponse>, Status> {
        let body = request.get_ref();
        let context = command_context(body.context.as_ref())?;
        require_create(context)?;
        let invocation =
            invocation_for_command(&self.state, &request, context, body.scope.as_ref()).await?;
        let enrollment_id =
            learning::EnrollmentId::try_from_uuid(uuid(&body.enrollment_id, "enrollment_id")?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let owner_actor = actor(&invocation);
        let result = command_service(&self.state, &invocation)
            .await?
            .assign_course(
                &owner_actor,
                identity(context, request.get_ref())?,
                &invocation.scope,
                enrollment_id,
                learning::AssignmentId::try_from_uuid(uuid(&body.assignment_id, "assignment_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                version_reference(body.learner.clone())?,
                principal(&body.learner_principal_id, "learner_principal_id")?,
                learning::CourseId::try_from_uuid(uuid(&body.course_id, "course_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                body.course_version,
                parse_date(&body.due_on, "due_on")?,
                Utc::now(),
            )
            .await
            .map_err(learning_status)?;
        Ok(Response::new(receipt(
            context,
            result,
            "enrollment",
            enrollment_id.as_uuid(),
            "assigned",
        )))
    }

    async fn record_attendance(
        &self,
        request: Request<wire::RecordAttendanceRequest>,
    ) -> Result<Response<wire::LearningMutationResponse>, Status> {
        let body = request.get_ref();
        let context = command_context(body.context.as_ref())?;
        let invocation = invocation_for_command(
            &self.state,
            &request,
            context,
            body.enrollment_scope.as_ref(),
        )
        .await?;
        let enrollment_id =
            learning::EnrollmentId::try_from_uuid(uuid(&body.enrollment_id, "enrollment_id")?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let recorded_by = principal(&body.recorded_by_principal_id, "recorded_by_principal_id")?;
        let fact = learning::AttendanceFact {
            id: learning::AttendanceFactId::try_from_uuid(uuid(
                &body.attendance_fact_id,
                "attendance_fact_id",
            )?)
            .map_err(|error| Status::invalid_argument(error.to_string()))?,
            session_id: learning::LearningSessionId::try_from_uuid(uuid(
                &body.session_id,
                "session_id",
            )?)
            .map_err(|error| Status::invalid_argument(error.to_string()))?,
            attended_minutes: body.attended_minutes,
            recorded_by,
            evidence: version_reference(body.evidence.clone())?,
            at: parse_time(&body.occurred_at, "occurred_at")?,
        };
        let owner_actor = actor(&invocation);
        let authenticated_principal = invocation.principal_id;
        let result = command_service(&self.state, &invocation)
            .await?
            .mutate_enrollment(
                &owner_actor,
                identity(context, request.get_ref())?,
                enrollment_id,
                &invocation.scope,
                expected_version(context)?,
                learning::LearningAction::RecordAttendance,
                "learning.attendance_recorded",
                move |enrollment, _| {
                    enrollment
                        .record_attendance(authenticated_principal, fact)
                        .map(|_| ())
                },
            )
            .await
            .map_err(learning_status)?;
        Ok(Response::new(receipt(
            context,
            result,
            "enrollment",
            enrollment_id.as_uuid(),
            "in_progress",
        )))
    }

    async fn record_provider_result(
        &self,
        request: Request<wire::RecordProviderResultRequest>,
    ) -> Result<Response<wire::LearningMutationResponse>, Status> {
        let body = request.get_ref();
        let context = command_context(body.context.as_ref())?;
        let invocation = invocation_for_command(
            &self.state,
            &request,
            context,
            body.enrollment_scope.as_ref(),
        )
        .await?;
        let enrollment_id =
            learning::EnrollmentId::try_from_uuid(uuid(&body.enrollment_id, "enrollment_id")?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let result_id = learning::ProviderResultId::try_from_uuid(uuid(
            &body.provider_result_id,
            "provider_result_id",
        )?)
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let score_basis_points =
            u16::try_from(body.score_basis_points).map_err(|_| invalid("score_basis_points"))?;
        let provider_result = learning::ProviderResult {
            id: result_id,
            provider_attempt_id: body.provider_attempt_id.clone(),
            score_basis_points,
            passed: body.passed,
            evidence: version_reference(body.evidence.clone())?,
            received_at: parse_time(&body.received_at, "received_at")?,
        };
        let owner_actor = actor(&invocation);
        let result = command_service(&self.state, &invocation)
            .await?
            .mutate_enrollment(
                &owner_actor,
                identity(context, request.get_ref())?,
                enrollment_id,
                &invocation.scope,
                expected_version(context)?,
                learning::LearningAction::RecordProviderResult,
                "learning.provider_result_recorded",
                move |enrollment, _| {
                    enrollment
                        .record_provider_result(provider_result)
                        .map(|_| ())
                },
            )
            .await
            .map_err(learning_status)?;
        Ok(Response::new(receipt(
            context,
            result,
            "enrollment",
            enrollment_id.as_uuid(),
            "in_progress",
        )))
    }

    async fn waive_enrollment(
        &self,
        request: Request<wire::WaiveEnrollmentRequest>,
    ) -> Result<Response<wire::LearningMutationResponse>, Status> {
        let body = request.get_ref();
        let context = command_context(body.context.as_ref())?;
        let invocation = invocation_for_command(
            &self.state,
            &request,
            context,
            body.enrollment_scope.as_ref(),
        )
        .await?;
        let enrollment_id =
            learning::EnrollmentId::try_from_uuid(uuid(&body.enrollment_id, "enrollment_id")?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let authorized_by = principal(
            &body.authorized_by_principal_id,
            "authorized_by_principal_id",
        )?;
        let waiver = learning::Waiver {
            id: learning::WaiverId::try_from_uuid(uuid(&body.waiver_id, "waiver_id")?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
            authorized_by,
            reason: body.reason.clone(),
            evidence: version_reference(body.evidence.clone())?,
            granted_at: parse_time(&body.granted_at, "granted_at")?,
        };
        let owner_actor = actor(&invocation);
        let authenticated_principal = invocation.principal_id;
        let result = command_service(&self.state, &invocation)
            .await?
            .mutate_enrollment(
                &owner_actor,
                identity(context, request.get_ref())?,
                enrollment_id,
                &invocation.scope,
                expected_version(context)?,
                learning::LearningAction::Waive,
                "learning.enrollment_waived",
                move |enrollment, _| enrollment.waive(authenticated_principal, waiver),
            )
            .await
            .map_err(learning_status)?;
        Ok(Response::new(receipt(
            context,
            result,
            "enrollment",
            enrollment_id.as_uuid(),
            "waived",
        )))
    }

    async fn complete_enrollment(
        &self,
        request: Request<wire::CompleteEnrollmentRequest>,
    ) -> Result<Response<wire::LearningMutationResponse>, Status> {
        let body = request.get_ref();
        let context = command_context(body.context.as_ref())?;
        let invocation = invocation_for_command(
            &self.state,
            &request,
            context,
            body.enrollment_scope.as_ref(),
        )
        .await?;
        let enrollment_id =
            learning::EnrollmentId::try_from_uuid(uuid(&body.enrollment_id, "enrollment_id")?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let owner_actor = actor(&invocation);
        let result = command_service(&self.state, &invocation)
            .await?
            .complete(
                &owner_actor,
                identity(context, request.get_ref())?,
                enrollment_id,
                &invocation.scope,
                expected_version(context)?,
                learning::CourseId::try_from_uuid(uuid(&body.course_id, "course_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                body.course_version,
                learning::CompletionId::try_from_uuid(uuid(&body.completion_id, "completion_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                Utc::now(),
            )
            .await
            .map_err(learning_status)?;
        Ok(Response::new(receipt(
            context,
            result,
            "enrollment",
            enrollment_id.as_uuid(),
            "completed",
        )))
    }
}

fn enrollment_status(value: learning::EnrollmentStatus) -> i32 {
    match value {
        learning::EnrollmentStatus::Assigned => wire::LearningEnrollmentStatus::Assigned as i32,
        learning::EnrollmentStatus::InProgress => wire::LearningEnrollmentStatus::InProgress as i32,
        learning::EnrollmentStatus::Completed => wire::LearningEnrollmentStatus::Completed as i32,
        learning::EnrollmentStatus::Waived => wire::LearningEnrollmentStatus::Waived as i32,
        learning::EnrollmentStatus::Cancelled => wire::LearningEnrollmentStatus::Cancelled as i32,
    }
}

fn record_status(value: learning::LearningRecordStatus) -> &'static str {
    match value {
        learning::LearningRecordStatus::Proposed => "proposed",
        learning::LearningRecordStatus::Assigned => "assigned",
        learning::LearningRecordStatus::InProgress => "in_progress",
        learning::LearningRecordStatus::Completed => "completed",
        learning::LearningRecordStatus::Waived => "waived",
        learning::LearningRecordStatus::Cancelled => "cancelled",
    }
}

fn projection_metadata(projected_at: DateTime<Utc>) -> common::ProjectionMetadata {
    let lag = Utc::now().signed_duration_since(projected_at);
    common::ProjectionMetadata {
        // The Learning projection currently persists a globally ordered
        // position but not the source event UUID. Omitting the checkpoint is
        // truthful; the response itself carries source_position.
        checkpoint: None,
        freshness: common::ProjectionFreshnessState::Current as i32,
        lag_millis: Some(u64::try_from(lag.num_milliseconds().max(0)).unwrap_or(u64::MAX)),
    }
}

#[tonic::async_trait]
impl wire::learning_query_service_server::LearningQueryService for LearningQueryRpc {
    async fn get_enrollment(
        &self,
        request: Request<wire::GetLearningEnrollmentRequest>,
    ) -> Result<Response<wire::GetLearningEnrollmentResponse>, Status> {
        let body = request.get_ref();
        let context = query_context(body.context.as_ref())?;
        let invocation = future_auth::query(
            &self.state,
            &request,
            Some(context),
            PRODUCT,
            READ_PERMISSION,
        )
        .await?;
        ensure_scope(&invocation, body.enrollment_scope.as_ref())?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Read).await?;
        let owner_actor = actor(&invocation);
        let read = learning_sqlx::SqlxLearningReadRepository::new(pool, invocation.tenant_id)
            .map_err(|_| Status::unavailable("learning read model is unavailable"))?;
        let projection = learning::LearningQueryService::new(
            LearningAccess {
                actor: owner_actor.clone(),
                write: false,
            },
            read,
        )
        .get_enrollment(
            &owner_actor,
            &invocation.scope,
            learning::EnrollmentId::try_from_uuid(uuid(&body.enrollment_id, "enrollment_id")?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
        )
        .await
        .map_err(learning_status)?;
        let enrollment = &projection.enrollment;
        Ok(Response::new(wire::GetLearningEnrollmentResponse {
            enrollment: Some(wire::LearningEnrollmentProjection {
                enrollment_id: enrollment.id().as_uuid().to_string(),
                source_version: enrollment.version(),
                scope: Some(encode_scope(enrollment.scope())),
                assignment_id: enrollment.assignment_id().as_uuid().to_string(),
                learner_record_id: enrollment.learner().record().id().to_string(),
                learner_principal_id: enrollment.learner_principal().to_string(),
                course_id: enrollment.course().course_id.as_uuid().to_string(),
                course_version: enrollment.course().version,
                due_on: enrollment.due_on().to_string(),
                status: enrollment_status(enrollment.status()),
                completion_id: enrollment
                    .completion()
                    .map(|completion| completion.id.as_uuid().to_string()),
                source_position: projection.source_position,
                projected_at: projection.projected_at.to_rfc3339(),
            }),
            projection: Some(projection_metadata(projection.projected_at)),
        }))
    }

    async fn get_record(
        &self,
        request: Request<wire::GetLearningRecordRequest>,
    ) -> Result<Response<wire::GetLearningRecordResponse>, Status> {
        let body = request.get_ref();
        let context = query_context(body.context.as_ref())?;
        let invocation = future_auth::query(
            &self.state,
            &request,
            Some(context),
            PRODUCT,
            READ_PERMISSION,
        )
        .await?;
        ensure_scope(&invocation, body.record_scope.as_ref())?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Read).await?;
        let owner_actor = actor(&invocation);
        let read = learning_sqlx::SqlxLearningReadRepository::new(pool, invocation.tenant_id)
            .map_err(|_| Status::unavailable("learning read model is unavailable"))?;
        let projection = learning::LearningQueryService::new(
            LearningAccess {
                actor: owner_actor.clone(),
                write: false,
            },
            read,
        )
        .get_record(
            &owner_actor,
            &invocation.scope,
            uuid(&body.record_id, "record_id")?,
        )
        .await
        .map_err(learning_status)?;
        let record = &projection.record;
        Ok(Response::new(wire::GetLearningRecordResponse {
            record: Some(wire::LearningRecordProjection {
                record_id: record.id().to_string(),
                source_version: record.version(),
                scope: Some(encode_scope(record.scope())),
                record_kind: record.kind().token().into(),
                subject_record_id: record.subject().id().to_string(),
                lifecycle_status: record_status(record.status()).into(),
                source_position: projection.source_position,
                projected_at: projection.projected_at.to_rfc3339(),
            }),
            projection: Some(projection_metadata(projection.projected_at)),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_update_requires_an_exact_version() {
        let context = common::CommandContext::default();
        assert_eq!(
            expected_version(&context).unwrap_err().code(),
            tonic::Code::FailedPrecondition
        );
        let context = common::CommandContext {
            expected_aggregate_version: 7,
            ..Default::default()
        };
        assert_eq!(expected_version(&context).unwrap(), Some(7));
        assert_eq!(
            require_create(&context).unwrap_err().code(),
            tonic::Code::FailedPrecondition
        );
    }
}
