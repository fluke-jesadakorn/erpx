//! Authorization-first target transport for the Benefits bounded context.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use erpx_benefits as benefits;
use erpx_benefits_sqlx as benefits_sqlx;
use erpx_infrastructure::RoutedDatabaseSide;
use erpx_integration_contracts::{
    CurrencyCode, MoneyAmount, RecordTypeId, RecordVersionRef, StableRecordRef, catalog::ContextId,
};
use erpx_kernel::{CommandId, CorrelationId, RecordId};
use erpx_proto::{common::v1 as common, erpx::benefits::v1 as wire};
use prost::Message;
use sha2::{Digest, Sha256};
use tonic::{Request, Response, Status};
use uuid::Uuid;

use super::future_auth::{self, FutureInvocation};
use crate::AppState;

const BENEFITS_WRITE: &str = "benefits.write";
const BENEFITS_READ: &str = "benefits.read";

#[derive(Clone)]
pub(crate) struct BenefitsCommandRpc {
    state: AppState,
}

#[derive(Clone)]
pub(crate) struct BenefitsQueryRpc {
    state: AppState,
}

impl BenefitsCommandRpc {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

impl BenefitsQueryRpc {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[derive(Debug, Clone)]
struct BenefitsAccess {
    actor: benefits::ActorContext,
    write: bool,
}

#[async_trait]
impl benefits::AuthorizationPort for BenefitsAccess {
    async fn authorize(
        &self,
        actor: &benefits::ActorContext,
        target: &erpx_kernel::TenantScope,
        action: benefits::BenefitsAction,
    ) -> Result<benefits::AuthorizedBenefitsAccess, benefits::PortError> {
        let action_writes = !matches!(action, benefits::BenefitsAction::View);
        if actor != &self.actor
            || action_writes != self.write
            || !actor.authority_scope.contains(target)
        {
            return Err(benefits::PortError::Denied);
        }
        Ok(benefits::AuthorizedBenefitsAccess {
            actor: actor.clone(),
            target_scope: target.clone(),
            action,
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct PayrollHandoffUnavailable;

#[async_trait]
impl benefits::PayrollHandoffPort for PayrollHandoffUnavailable {
    async fn release(
        &self,
        _authority: &benefits::AuthorizedBenefitsAccess,
        _handoff: &benefits::BenefitsPayrollHandoff,
    ) -> Result<benefits::PayrollHandoffReceipt, benefits::PortError> {
        Err(benefits::PortError::Unavailable)
    }
}

fn actor(invocation: &FutureInvocation) -> benefits::ActorContext {
    benefits::ActorContext {
        tenant_id: invocation.tenant_id,
        principal_id: invocation.principal_id,
        authority_scope: invocation.scope.clone(),
    }
}

async fn command_invocation<T>(
    state: &AppState,
    request: &Request<T>,
    context: Option<&common::CommandContext>,
    target_scope: Option<&common::TenantScope>,
) -> Result<(FutureInvocation, erpx_kernel::TenantScope), Status> {
    let invocation =
        future_auth::command(state, request, context, "benefits", BENEFITS_WRITE).await?;
    let target = decode_scope(target_scope)?;
    if !invocation.scope.contains(&target) {
        return Err(Status::permission_denied(
            "benefits record scope is outside authenticated authority",
        ));
    }
    Ok((invocation, target))
}

async fn query_invocation<T>(
    state: &AppState,
    request: &Request<T>,
    context: Option<&common::QueryContext>,
    target_scope: Option<&common::TenantScope>,
) -> Result<(FutureInvocation, erpx_kernel::TenantScope), Status> {
    let invocation = future_auth::query(state, request, context, "benefits", BENEFITS_READ).await?;
    let target = decode_scope(target_scope)?;
    if !invocation.scope.contains(&target) {
        return Err(Status::permission_denied(
            "benefits record scope is outside authenticated authority",
        ));
    }
    Ok((invocation, target))
}

fn identity<M: Message>(
    context: &common::CommandContext,
    request: &M,
) -> Result<benefits::MutationIdentity, Status> {
    Ok(benefits::MutationIdentity {
        command_id: CommandId::try_from_uuid(uuid(&context.command_id, "command_id")?)
            .map_err(|_| Status::invalid_argument("command_id is invalid"))?,
        idempotency_key: context.idempotency_key.clone(),
        request_digest: hex::encode(Sha256::digest(request.encode_to_vec())),
        correlation_id: CorrelationId::try_from_uuid(uuid(
            &context.correlation_id,
            "correlation_id",
        )?)
        .map_err(|_| Status::invalid_argument("correlation_id is invalid"))?,
    })
}

fn decode_scope(value: Option<&common::TenantScope>) -> Result<erpx_kernel::TenantScope, Status> {
    use common::tenant_scope::Scope;
    let scope = value
        .and_then(|value| value.scope.as_ref())
        .ok_or_else(|| Status::invalid_argument("record scope is required"))?;
    let result = match scope {
        Scope::Platform(_) => erpx_kernel::TenantScope::Platform,
        Scope::Holding(value) => erpx_kernel::TenantScope::Holding {
            holding_id: uuid(&value.holding_id, "holding_id")?,
        },
        Scope::Company(value) => erpx_kernel::TenantScope::Company {
            holding_id: uuid(&value.holding_id, "holding_id")?,
            company_id: uuid(&value.company_id, "company_id")?,
        },
        Scope::Branch(value) => erpx_kernel::TenantScope::Branch {
            holding_id: uuid(&value.holding_id, "holding_id")?,
            company_id: uuid(&value.company_id, "company_id")?,
            branch_id: uuid(&value.branch_id, "branch_id")?,
        },
        Scope::Subtree(value) => erpx_kernel::TenantScope::Subtree {
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
    result
        .validate()
        .map_err(|_| Status::invalid_argument("record scope is invalid"))?;
    Ok(result)
}

fn encode_scope(value: &erpx_kernel::TenantScope) -> common::TenantScope {
    use common::tenant_scope::Scope;
    common::TenantScope {
        scope: Some(match value {
            erpx_kernel::TenantScope::Platform => Scope::Platform(common::PlatformScope {}),
            erpx_kernel::TenantScope::Holding { holding_id } => {
                Scope::Holding(common::HoldingScope {
                    holding_id: holding_id.to_string(),
                })
            }
            erpx_kernel::TenantScope::Company {
                holding_id,
                company_id,
            } => Scope::Company(common::CompanyScope {
                holding_id: holding_id.to_string(),
                company_id: company_id.to_string(),
            }),
            erpx_kernel::TenantScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Scope::Branch(common::BranchScope {
                holding_id: holding_id.to_string(),
                company_id: company_id.to_string(),
                branch_id: branch_id.to_string(),
            }),
            erpx_kernel::TenantScope::Subtree {
                holding_id,
                company_ids,
                branch_ids,
            } => Scope::Subtree(common::SubtreeScope {
                holding_id: holding_id.to_string(),
                company_ids: company_ids.iter().map(ToString::to_string).collect(),
                branch_ids: branch_ids.iter().map(ToString::to_string).collect(),
            }),
        }),
    }
}

fn version_ref(value: Option<wire::RecordVersionRef>) -> Result<RecordVersionRef, Status> {
    let value = value.ok_or_else(|| Status::invalid_argument("record version is required"))?;
    let owner = ContextId::ALL
        .into_iter()
        .find(|candidate| candidate.slug() == value.owner_context)
        .ok_or_else(|| Status::invalid_argument("record owner context is invalid"))?;
    let record_type = RecordTypeId::try_new(value.record_type)
        .map_err(|_| Status::invalid_argument("record type is invalid"))?;
    let record = StableRecordRef::try_new(
        RecordId::try_from_uuid(uuid(&value.record_id, "record_id")?)
            .map_err(|_| Status::invalid_argument("record_id is invalid"))?,
        owner,
        record_type,
    )
    .map_err(|_| Status::invalid_argument("record reference is invalid"))?;
    RecordVersionRef::try_new(record, value.version)
        .map_err(|_| Status::invalid_argument("record version is invalid"))
}

fn money(value: Option<wire::MoneyAmount>) -> Result<MoneyAmount, Status> {
    let value = value.ok_or_else(|| Status::invalid_argument("money amount is required"))?;
    let currency = CurrencyCode::try_new(&value.currency)
        .map_err(|_| Status::invalid_argument("currency is invalid"))?;
    Ok(MoneyAmount::new(value.minor_units, currency))
}

fn date(value: &str, field: &'static str) -> Result<NaiveDate, Status> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| Status::invalid_argument(format!("{field} is invalid")))
}

fn timestamp(value: &str, field: &'static str) -> Result<DateTime<Utc>, Status> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| Status::invalid_argument(format!("{field} is invalid")))
}

fn uuid(value: &str, field: &'static str) -> Result<Uuid, Status> {
    Uuid::parse_str(value).map_err(|_| Status::invalid_argument(format!("{field} is invalid")))
}

fn expected(context: &common::CommandContext) -> Option<u64> {
    (context.expected_aggregate_version > 0).then_some(context.expected_aggregate_version)
}

fn response(
    context: &common::CommandContext,
    receipt: benefits::CommitReceipt,
    record_type: &str,
    lifecycle_status: impl Into<String>,
) -> wire::BenefitsMutationResponse {
    let record_id = receipt
        .aggregate_key
        .rsplit_once(':')
        .map_or(receipt.aggregate_key.as_str(), |(_, id)| id)
        .to_owned();
    wire::BenefitsMutationResponse {
        receipt: Some(common::CommandReceipt {
            command_id: context.command_id.clone(),
            correlation_id: context.correlation_id.clone(),
            aggregate: Some(common::AggregateVersion {
                context: "benefits".into(),
                aggregate_type: record_type.into(),
                aggregate_id: record_id.clone(),
                version: receipt.version,
            }),
            committed_events: vec![common::EventPosition {
                event_id: receipt.event_id.to_string(),
                global_sequence: receipt.global_sequence,
            }],
            committed_at: receipt.committed_at.to_rfc3339(),
            replayed: receipt.replayed,
        }),
        record_type: record_type.into(),
        record_id,
        lifecycle_status: lifecycle_status.into(),
    }
}

fn status(error: benefits::BenefitsError) -> Status {
    match error {
        benefits::BenefitsError::Neutral => Status::not_found("item unavailable"),
        benefits::BenefitsError::Conflict => Status::aborted("version conflict"),
        benefits::BenefitsError::Invalid(message) => Status::invalid_argument(message),
        benefits::BenefitsError::HandoffRejected(message) => Status::failed_precondition(message),
        benefits::BenefitsError::Unavailable => {
            Status::unavailable("benefits dependency unavailable")
        }
        benefits::BenefitsError::Internal => Status::internal("benefits operation failed"),
    }
}

fn lifecycle(value: impl serde::Serialize) -> Result<String, Status> {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .ok_or_else(|| Status::internal("benefits lifecycle encoding failed"))
}

#[tonic::async_trait]
impl wire::benefits_command_service_server::BenefitsCommandService for BenefitsCommandRpc {
    async fn publish_plan(
        &self,
        request: Request<wire::PublishBenefitPlanRequest>,
    ) -> Result<Response<wire::BenefitsMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, target) =
            command_invocation(&self.state, &request, Some(context), body.scope.as_ref()).await?;
        if context.expected_aggregate_version != 0 {
            return Err(Status::failed_precondition(
                "publish plan cannot carry an expected version",
            ));
        }
        let currency = CurrencyCode::try_new(&body.currency)
            .map_err(|_| Status::invalid_argument("currency is invalid"))?;
        let contribution = benefits::ContributionRule {
            employee_basis_points: u16::try_from(body.employee_contribution_basis_points)
                .map_err(|_| Status::invalid_argument("employee basis points are invalid"))?,
            employer_basis_points: u16::try_from(body.employer_contribution_basis_points)
                .map_err(|_| Status::invalid_argument("employer basis points are invalid"))?,
            employee_flat: money(body.employee_flat_contribution.clone())?,
            employer_flat: money(body.employer_flat_contribution.clone())?,
            employee_cap: body
                .employee_contribution_cap
                .clone()
                .map(|value| money(Some(value)))
                .transpose()?,
        };
        let now = Utc::now();
        let plan = benefits::BenefitPlan::draft(
            benefits::BenefitPlanId::try_from_uuid(uuid(&body.plan_id, "plan_id")?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
            target,
            body.name.clone(),
            currency,
            body.eligibility_class.clone(),
            body.minimum_tenure_days,
            contribution,
            now,
        )
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let actor = actor(&invocation);
        let service = benefits::BenefitsCommandService::new(
            BenefitsAccess {
                actor: actor.clone(),
                write: true,
            },
            benefits_sqlx::SqlxBenefitsWriteRepository::new(pool),
            PayrollHandoffUnavailable,
        );
        let receipt = service
            .publish_plan(&actor, identity(context, body)?, plan, now)
            .await
            .map_err(status)?;
        Ok(Response::new(response(
            context,
            receipt,
            "benefits.plan",
            "published",
        )))
    }

    async fn open_enrollment(
        &self,
        request: Request<wire::OpenBenefitEnrollmentRequest>,
    ) -> Result<Response<wire::BenefitsMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, target) =
            command_invocation(&self.state, &request, Some(context), body.scope.as_ref()).await?;
        if context.expected_aggregate_version != 0 {
            return Err(Status::failed_precondition(
                "open enrollment cannot carry an expected version",
            ));
        }
        let eligibility = body
            .eligibility
            .clone()
            .ok_or_else(|| Status::invalid_argument("eligibility is required"))?;
        let decision = benefits::EligibilityDecision {
            employee: version_ref(eligibility.employee)?,
            employment_class: eligibility.employment_class,
            tenure_days: eligibility.tenure_days,
            eligible: eligibility.eligible,
            source: version_ref(eligibility.source)?,
            evaluated_at: timestamp(&eligibility.evaluated_at, "evaluated_at")?,
        };
        let life_event = match (
            body.life_event_id.as_deref(),
            body.life_event_kind.as_deref(),
            body.life_event_occurred_on.as_deref(),
            body.life_event_evidence.clone(),
        ) {
            (None, None, None, None) => None,
            (Some(id), Some(kind), Some(occurred), Some(evidence)) => Some(benefits::LifeEvent {
                id: benefits::LifeEventId::try_from_uuid(uuid(id, "life_event_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                kind: match kind {
                    "marriage" => benefits::LifeEventKind::Marriage,
                    "birth_or_adoption" => benefits::LifeEventKind::BirthOrAdoption,
                    "loss_of_coverage" => benefits::LifeEventKind::LossOfCoverage,
                    "divorce" => benefits::LifeEventKind::Divorce,
                    "employment_change" => benefits::LifeEventKind::EmploymentChange,
                    _ => return Err(Status::invalid_argument("life_event_kind is invalid")),
                },
                occurred_on: date(occurred, "life_event_occurred_on")?,
                evidence: version_ref(Some(evidence))?,
            }),
            _ => {
                return Err(Status::invalid_argument(
                    "life event fields must be complete",
                ));
            }
        };
        let window = benefits::EnrollmentWindow {
            id: benefits::EnrollmentWindowId::try_from_uuid(uuid(
                &body.enrollment_window_id,
                "enrollment_window_id",
            )?)
            .map_err(|error| Status::invalid_argument(error.to_string()))?,
            plan: benefits::BenefitPlanRef {
                plan_id: benefits::BenefitPlanId::try_from_uuid(uuid(&body.plan_id, "plan_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                version: body.plan_version,
            },
            opens_on: date(&body.window_opens_on, "window_opens_on")?,
            closes_on: date(&body.window_closes_on, "window_closes_on")?,
            coverage_starts_on: date(&body.coverage_starts_on, "coverage_starts_on")?,
        };
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let actor = actor(&invocation);
        let service = benefits::BenefitsCommandService::new(
            BenefitsAccess {
                actor: actor.clone(),
                write: true,
            },
            benefits_sqlx::SqlxBenefitsWriteRepository::new(pool),
            PayrollHandoffUnavailable,
        );
        let receipt = service
            .open_enrollment(
                &actor,
                identity(context, body)?,
                benefits::BenefitEnrollmentId::try_from_uuid(uuid(
                    &body.enrollment_id,
                    "enrollment_id",
                )?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
                &target,
                window.plan.plan_id,
                body.plan_version,
                window,
                version_ref(body.employee.clone())?,
                decision,
                life_event,
                date(&body.enrollment_date, "enrollment_date")?,
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(
            context,
            receipt,
            "benefits.election",
            "draft",
        )))
    }

    async fn add_dependent(
        &self,
        request: Request<wire::AddBenefitDependentRequest>,
    ) -> Result<Response<wire::BenefitsMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, target) = command_invocation(
            &self.state,
            &request,
            Some(context),
            body.enrollment_scope.as_ref(),
        )
        .await?;
        let dependent = benefits::Dependent {
            id: benefits::DependentId::try_from_uuid(uuid(&body.dependent_id, "dependent_id")?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
            person_reference: body.person_reference.clone(),
            relationship: body.relationship.clone(),
            eligibility_evidence: version_ref(body.eligibility_evidence.clone())?,
        };
        self.mutate(
            &request,
            context,
            invocation,
            target,
            &body.enrollment_id,
            benefits::BenefitsAction::ChangeElection,
            "benefits.dependent_added",
            move |enrollment, _| enrollment.add_dependent(dependent, Utc::now()),
        )
        .await
    }

    async fn elect_coverage(
        &self,
        request: Request<wire::ElectBenefitCoverageRequest>,
    ) -> Result<Response<wire::BenefitsMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, target) = command_invocation(
            &self.state,
            &request,
            Some(context),
            body.enrollment_scope.as_ref(),
        )
        .await?;
        let plan_id = benefits::BenefitPlanId::try_from_uuid(uuid(&body.plan_id, "plan_id")?)
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let plan_version = body.plan_version;
        let election_id =
            benefits::ElectionId::try_from_uuid(uuid(&body.election_id, "election_id")?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let tier = match wire::CoverageTier::try_from(body.coverage_tier) {
            Ok(wire::CoverageTier::Employee) => benefits::CoverageTier::Employee,
            Ok(wire::CoverageTier::EmployeeSpouse) => benefits::CoverageTier::EmployeeSpouse,
            Ok(wire::CoverageTier::EmployeeChildren) => benefits::CoverageTier::EmployeeChildren,
            Ok(wire::CoverageTier::Family) => benefits::CoverageTier::Family,
            _ => return Err(Status::invalid_argument("coverage_tier is invalid")),
        };
        let dependent_ids = body
            .dependent_ids
            .iter()
            .map(|value| {
                benefits::DependentId::try_from_uuid(uuid(value, "dependent_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let base_pay = money(body.base_pay.clone())?;
        let base_pay_source = version_ref(body.base_pay_source.clone())?;
        let effective_on = date(&body.effective_on, "effective_on")?;
        self.mutate(
            &request,
            context,
            invocation,
            target,
            &body.enrollment_id,
            benefits::BenefitsAction::ChangeElection,
            "benefits.coverage_elected",
            move |enrollment, plan| {
                let plan = plan.ok_or(benefits::BenefitsDomainError::InvalidPlan)?;
                if plan.id() != plan_id || plan.version() != plan_version {
                    return Err(benefits::BenefitsDomainError::InvalidPlan);
                }
                enrollment.elect(
                    plan,
                    election_id,
                    tier,
                    dependent_ids,
                    base_pay,
                    base_pay_source,
                    effective_on,
                    Utc::now(),
                )
            },
        )
        .await
    }

    async fn submit_provider(
        &self,
        request: Request<wire::EnrollmentIdentityRequest>,
    ) -> Result<Response<wire::BenefitsMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, target) = command_invocation(
            &self.state,
            &request,
            Some(context),
            body.enrollment_scope.as_ref(),
        )
        .await?;
        self.mutate(
            &request,
            context,
            invocation,
            target,
            &body.enrollment_id,
            benefits::BenefitsAction::SubmitProvider,
            "benefits.provider_submitted",
            |enrollment, _| enrollment.submit_provider(Utc::now()),
        )
        .await
    }

    async fn acknowledge_provider(
        &self,
        request: Request<wire::AcknowledgeBenefitProviderRequest>,
    ) -> Result<Response<wire::BenefitsMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, target) = command_invocation(
            &self.state,
            &request,
            Some(context),
            body.enrollment_scope.as_ref(),
        )
        .await?;
        let acknowledgement = benefits::ProviderAcknowledgement {
            id: benefits::ProviderAcknowledgementId::try_from_uuid(uuid(
                &body.acknowledgement_id,
                "acknowledgement_id",
            )?)
            .map_err(|error| Status::invalid_argument(error.to_string()))?,
            request_key: body.request_key.clone(),
            accepted: body.accepted,
            provider_reference: body.provider_reference.clone(),
            reason: body.rejection_reason.clone(),
            received_at: timestamp(&body.received_at, "received_at")?,
        };
        self.mutate(
            &request,
            context,
            invocation,
            target,
            &body.enrollment_id,
            benefits::BenefitsAction::RecordProviderAcknowledgement,
            "benefits.provider_acknowledged",
            move |enrollment, _| enrollment.acknowledge_provider(acknowledgement).map(|_| ()),
        )
        .await
    }

    async fn release_payroll(
        &self,
        request: Request<wire::ReleaseBenefitsPayrollRequest>,
    ) -> Result<Response<wire::BenefitsMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, target) = command_invocation(
            &self.state,
            &request,
            Some(context),
            body.enrollment_scope.as_ref(),
        )
        .await?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let actor = actor(&invocation);
        let service = benefits::BenefitsCommandService::new(
            BenefitsAccess {
                actor: actor.clone(),
                write: true,
            },
            benefits_sqlx::SqlxBenefitsWriteRepository::new(pool),
            PayrollHandoffUnavailable,
        );
        let receipt = service
            .release_payroll(
                &actor,
                identity(context, body)?,
                benefits::BenefitEnrollmentId::try_from_uuid(uuid(
                    &body.enrollment_id,
                    "enrollment_id",
                )?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
                &target,
                benefits::PayrollHandoffId::try_from_uuid(uuid(&body.handoff_id, "handoff_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(
            context,
            receipt,
            "benefits.election",
            "payroll_handed_off",
        )))
    }
}

impl BenefitsCommandRpc {
    #[allow(clippy::too_many_arguments)]
    async fn mutate<T, F>(
        &self,
        request: &Request<T>,
        context: &common::CommandContext,
        invocation: FutureInvocation,
        target: erpx_kernel::TenantScope,
        enrollment_id: &str,
        action: benefits::BenefitsAction,
        event_type: &'static str,
        change: F,
    ) -> Result<Response<wire::BenefitsMutationResponse>, Status>
    where
        T: Message,
        F: FnOnce(
            &mut benefits::BenefitEnrollment,
            Option<&benefits::BenefitPlan>,
        ) -> Result<(), benefits::BenefitsDomainError>,
    {
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let actor = actor(&invocation);
        let service = benefits::BenefitsCommandService::new(
            BenefitsAccess {
                actor: actor.clone(),
                write: true,
            },
            benefits_sqlx::SqlxBenefitsWriteRepository::new(pool),
            PayrollHandoffUnavailable,
        );
        let receipt = service
            .mutate_enrollment(
                &actor,
                identity(context, request.get_ref())?,
                benefits::BenefitEnrollmentId::try_from_uuid(uuid(enrollment_id, "enrollment_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                &target,
                expected(context),
                action,
                event_type,
                change,
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(
            context,
            receipt,
            "benefits.election",
            "updated",
        )))
    }
}

#[tonic::async_trait]
impl wire::benefits_query_service_server::BenefitsQueryService for BenefitsQueryRpc {
    async fn get_enrollment(
        &self,
        request: Request<wire::GetBenefitEnrollmentRequest>,
    ) -> Result<Response<wire::GetBenefitEnrollmentResponse>, Status> {
        let body = request.get_ref();
        let (invocation, target) = query_invocation(
            &self.state,
            &request,
            body.context.as_ref(),
            body.enrollment_scope.as_ref(),
        )
        .await?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Read).await?;
        let actor = actor(&invocation);
        let service = benefits::BenefitsQueryService::new(
            BenefitsAccess {
                actor: actor.clone(),
                write: false,
            },
            benefits_sqlx::SqlxBenefitsReadRepository::new(pool, invocation.tenant_id)
                .map_err(|_| Status::unavailable("benefits read model is unavailable"))?,
        );
        let result = service
            .get_enrollment(
                &actor,
                &target,
                benefits::BenefitEnrollmentId::try_from_uuid(uuid(
                    &body.enrollment_id,
                    "enrollment_id",
                )?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
            )
            .await
            .map_err(status)?;
        let enrollment = &result.enrollment;
        Ok(Response::new(wire::GetBenefitEnrollmentResponse {
            enrollment: Some(wire::BenefitEnrollmentProjection {
                enrollment_id: enrollment.id().as_uuid().to_string(),
                source_version: enrollment.version(),
                scope: Some(encode_scope(enrollment.scope())),
                plan_id: enrollment.plan().plan_id.as_uuid().to_string(),
                plan_version: enrollment.plan().version,
                employee_record_id: enrollment.employee().record().id().as_uuid().to_string(),
                status: encode_enrollment_status(enrollment.status()),
                election_id: enrollment
                    .election()
                    .map(|value| value.id.as_uuid().to_string()),
                payroll_handoff_id: enrollment
                    .payroll_handoff_id()
                    .map(|value| value.as_uuid().to_string()),
                source_position: result.source_position,
                projected_at: result.projected_at.to_rfc3339(),
            }),
            projection: Some(current_projection(&result)),
        }))
    }

    async fn get_record(
        &self,
        request: Request<wire::GetBenefitsRecordRequest>,
    ) -> Result<Response<wire::GetBenefitsRecordResponse>, Status> {
        let body = request.get_ref();
        let (invocation, target) = query_invocation(
            &self.state,
            &request,
            body.context.as_ref(),
            body.record_scope.as_ref(),
        )
        .await?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Read).await?;
        let actor = actor(&invocation);
        let service = benefits::BenefitsQueryService::new(
            BenefitsAccess {
                actor: actor.clone(),
                write: false,
            },
            benefits_sqlx::SqlxBenefitsReadRepository::new(pool, invocation.tenant_id)
                .map_err(|_| Status::unavailable("benefits read model is unavailable"))?,
        );
        let result = service
            .get_record(&actor, &target, uuid(&body.record_id, "record_id")?)
            .await
            .map_err(status)?;
        let record = &result.record;
        Ok(Response::new(wire::GetBenefitsRecordResponse {
            record: Some(wire::BenefitsRecordProjection {
                record_id: record.id().to_string(),
                source_version: record.version(),
                scope: Some(encode_scope(record.scope())),
                record_kind: record.kind().token().into(),
                subject_record_id: record.subject().id().as_uuid().to_string(),
                lifecycle_status: lifecycle(record.status())?,
                source_position: result.source_position,
                projected_at: result.projected_at.to_rfc3339(),
            }),
            projection: Some(current_projection(&result)),
        }))
    }
}

fn required_command(
    value: Option<&common::CommandContext>,
) -> Result<&common::CommandContext, Status> {
    value.ok_or_else(|| Status::invalid_argument("command context is required"))
}

fn current_projection<T>(value: &T) -> common::ProjectionMetadata
where
    T: BenefitsProjection,
{
    common::ProjectionMetadata {
        checkpoint: None,
        freshness: common::ProjectionFreshnessState::Current as i32,
        lag_millis: Utc::now()
            .signed_duration_since(value.projected_at())
            .to_std()
            .ok()
            .and_then(|duration| u64::try_from(duration.as_millis()).ok()),
    }
}

trait BenefitsProjection {
    fn projected_at(&self) -> DateTime<Utc>;
}

impl BenefitsProjection for benefits::BenefitEnrollmentProjection {
    fn projected_at(&self) -> DateTime<Utc> {
        self.projected_at
    }
}

impl BenefitsProjection for benefits::BenefitsRecordProjection {
    fn projected_at(&self) -> DateTime<Utc> {
        self.projected_at
    }
}

fn encode_enrollment_status(value: benefits::BenefitEnrollmentStatus) -> i32 {
    (match value {
        benefits::BenefitEnrollmentStatus::Draft => wire::BenefitEnrollmentStatus::Draft,
        benefits::BenefitEnrollmentStatus::Elected => wire::BenefitEnrollmentStatus::Elected,
        benefits::BenefitEnrollmentStatus::PendingProvider => {
            wire::BenefitEnrollmentStatus::PendingProvider
        }
        benefits::BenefitEnrollmentStatus::Acknowledged => {
            wire::BenefitEnrollmentStatus::Acknowledged
        }
        benefits::BenefitEnrollmentStatus::ProviderRejected => {
            wire::BenefitEnrollmentStatus::ProviderRejected
        }
        benefits::BenefitEnrollmentStatus::PayrollHandedOff => {
            wire::BenefitEnrollmentStatus::PayrollHandedOff
        }
        benefits::BenefitEnrollmentStatus::Cancelled => wire::BenefitEnrollmentStatus::Cancelled,
    }) as i32
}

#[cfg(test)]
mod tests {
    #[test]
    fn record_version_wire_contract_is_typed() {
        let source = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../crates/proto/proto/erpx/benefits/v1/service.proto"
        ));
        assert!(source.contains("string owner_context = 3;"));
        assert!(source.contains("string record_type = 4;"));
    }

    #[test]
    fn payroll_handoff_is_explicitly_external() {
        let source = include_str!("future_benefits.rs");
        assert!(source.contains("PayrollHandoffUnavailable"));
        assert!(!source.contains(concat!("accepted", ": true")));
    }
}
