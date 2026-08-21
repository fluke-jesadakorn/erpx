//! Authorization-first target transport for the Field Service bounded context.

use std::collections::BTreeSet;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use erpx_field_service as field;
use erpx_field_service_sqlx as field_sqlx;
use erpx_infrastructure::RoutedDatabaseSide;
use erpx_integration_contracts::{
    CurrencyCode, MoneyAmount, Quantity, RecordTypeId, RecordVersionRef, StableRecordRef,
    catalog::ContextId,
};
use erpx_kernel::{CommandId, CorrelationId, PrincipalId, RecordId};
use erpx_proto::{common, erpx::field_service as wire};
use prost::Message;
use sha2::{Digest, Sha256};
use tonic::{Request, Response, Status};
use uuid::Uuid;

use super::future_auth::{self, FutureInvocation};
use crate::AppState;

const FIELD_WRITE: &str = "field-service.write";
const FIELD_READ: &str = "field-service.read";

#[derive(Clone)]
pub(crate) struct FieldServiceCommandRpc {
    state: AppState,
}

#[derive(Clone)]
pub(crate) struct FieldServiceQueryRpc {
    state: AppState,
}

impl FieldServiceCommandRpc {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

impl FieldServiceQueryRpc {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[derive(Debug, Clone)]
struct FieldAccess {
    actor: field::ActorContext,
    write: bool,
}

#[async_trait]
impl field::AuthorizationPort for FieldAccess {
    async fn authorize(
        &self,
        actor: &field::ActorContext,
        target: &erpx_kernel::AuthorityScope,
        action: field::FieldServiceAction,
    ) -> Result<field::AuthorizedFieldServiceAccess, field::PortError> {
        let action_writes = !matches!(action, field::FieldServiceAction::View);
        if actor != &self.actor
            || action_writes != self.write
            || !actor.authority_scope.contains(target)
        {
            return Err(field::PortError::Denied);
        }
        Ok(field::AuthorizedFieldServiceAccess {
            actor: actor.clone(),
            target_scope: target.clone(),
            action,
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct BillingHandoffUnavailable;

#[async_trait]
impl field::BillingHandoffPort for BillingHandoffUnavailable {
    async fn release(
        &self,
        _authority: &field::AuthorizedFieldServiceAccess,
        _handoff: &field::FieldServiceBillingHandoff,
    ) -> Result<field::BillingHandoffReceipt, field::PortError> {
        Err(field::PortError::Unavailable)
    }
}

fn actor(invocation: &FutureInvocation) -> field::ActorContext {
    field::ActorContext {
        holding_id: invocation.holding_id,
        principal_id: invocation.principal_id,
        authority_scope: invocation.scope.clone(),
    }
}

async fn command_invocation<T>(
    state: &AppState,
    request: &Request<T>,
    context: Option<&common::CommandContext>,
    target_scope: Option<&common::AuthorityScope>,
) -> Result<(FutureInvocation, erpx_kernel::AuthorityScope), Status> {
    let invocation =
        future_auth::command(state, request, context, "field-service", FIELD_WRITE).await?;
    let target = decode_scope(target_scope)?;
    if !invocation.scope.contains(&target) {
        return Err(Status::permission_denied(
            "service-order scope is outside authenticated authority",
        ));
    }
    Ok((invocation, target))
}

async fn query_invocation<T>(
    state: &AppState,
    request: &Request<T>,
    context: Option<&common::QueryContext>,
    target_scope: Option<&common::AuthorityScope>,
) -> Result<(FutureInvocation, erpx_kernel::AuthorityScope), Status> {
    let invocation =
        future_auth::query(state, request, context, "field-service", FIELD_READ).await?;
    let target = decode_scope(target_scope)?;
    if !invocation.scope.contains(&target) {
        return Err(Status::permission_denied(
            "service-order scope is outside authenticated authority",
        ));
    }
    Ok((invocation, target))
}

fn identity<M: Message>(
    context: &common::CommandContext,
    request: &M,
) -> Result<field::MutationIdentity, Status> {
    Ok(field::MutationIdentity {
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

fn stable_ref(value: Option<wire::RecordRef>) -> Result<StableRecordRef, Status> {
    let value = value.ok_or_else(|| Status::invalid_argument("record reference is required"))?;
    let owner = ContextId::ALL
        .into_iter()
        .find(|candidate| candidate.slug() == value.owner_context)
        .ok_or_else(|| Status::invalid_argument("record owner context is invalid"))?;
    StableRecordRef::try_new(
        RecordId::try_from_uuid(uuid(&value.record_id, "record_id")?)
            .map_err(|_| Status::invalid_argument("record_id is invalid"))?,
        owner,
        RecordTypeId::try_new(value.record_type)
            .map_err(|_| Status::invalid_argument("record type is invalid"))?,
    )
    .map_err(|_| Status::invalid_argument("record reference is invalid"))
}

fn version_ref(value: Option<wire::RecordVersionRef>) -> Result<RecordVersionRef, Status> {
    let value = value.ok_or_else(|| Status::invalid_argument("record version is required"))?;
    RecordVersionRef::try_new(
        stable_ref(Some(wire::RecordRef {
            record_id: value.record_id,
            owner_context: value.owner_context,
            record_type: value.record_type,
        }))?,
        value.version,
    )
    .map_err(|_| Status::invalid_argument("record version is invalid"))
}

fn quantity(value: Option<wire::FixedQuantity>) -> Result<Quantity, Status> {
    let value = value.ok_or_else(|| Status::invalid_argument("quantity is required"))?;
    Quantity::try_new(
        value.units,
        u8::try_from(value.scale).map_err(|_| Status::invalid_argument("scale is invalid"))?,
    )
    .map_err(|_| Status::invalid_argument("quantity is invalid"))
}

fn money(value: Option<wire::MoneyAmount>) -> Result<MoneyAmount, Status> {
    let value = value.ok_or_else(|| Status::invalid_argument("money amount is required"))?;
    Ok(MoneyAmount::new(
        value.minor_units,
        CurrencyCode::try_new(&value.currency)
            .map_err(|_| Status::invalid_argument("currency is invalid"))?,
    ))
}

fn principal(value: &str, field_name: &'static str) -> Result<PrincipalId, Status> {
    PrincipalId::try_from_uuid(uuid(value, field_name)?)
        .map_err(|_| Status::invalid_argument(format!("{field_name} is invalid")))
}

fn uuid(value: &str, field_name: &'static str) -> Result<Uuid, Status> {
    Uuid::parse_str(value).map_err(|_| Status::invalid_argument(format!("{field_name} is invalid")))
}

fn date(value: &str, field_name: &'static str) -> Result<NaiveDate, Status> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| Status::invalid_argument(format!("{field_name} is invalid")))
}

fn timestamp(value: &str, field_name: &'static str) -> Result<DateTime<Utc>, Status> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| Status::invalid_argument(format!("{field_name} is invalid")))
}

fn expected(context: &common::CommandContext) -> Option<u64> {
    (context.expected_aggregate_version > 0).then_some(context.expected_aggregate_version)
}

fn response(
    context: &common::CommandContext,
    receipt: field::CommitReceipt,
    record_type: &str,
    lifecycle_status: impl Into<String>,
) -> wire::FieldServiceMutationResponse {
    let record_id = receipt
        .aggregate_key
        .rsplit_once(':')
        .map_or(receipt.aggregate_key.as_str(), |(_, id)| id)
        .to_owned();
    wire::FieldServiceMutationResponse {
        receipt: Some(common::CommandReceipt {
            command_id: context.command_id.clone(),
            correlation_id: context.correlation_id.clone(),
            aggregate: Some(common::AggregateVersion {
                context: "field-service".into(),
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

fn status(error: field::FieldServiceError) -> Status {
    match error {
        field::FieldServiceError::Neutral => Status::not_found("item unavailable"),
        field::FieldServiceError::Conflict => Status::aborted("version conflict"),
        field::FieldServiceError::Invalid(message) => Status::invalid_argument(message),
        field::FieldServiceError::HandoffRejected(message) => Status::failed_precondition(message),
        field::FieldServiceError::Unavailable => {
            Status::unavailable("field-service dependency unavailable")
        }
        field::FieldServiceError::Internal => Status::internal("field-service operation failed"),
    }
}

fn lifecycle(value: impl serde::Serialize) -> Result<String, Status> {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .ok_or_else(|| Status::internal("field-service lifecycle encoding failed"))
}

fn required_command(
    value: Option<&common::CommandContext>,
) -> Result<&common::CommandContext, Status> {
    value.ok_or_else(|| Status::invalid_argument("command context is required"))
}

#[tonic::async_trait]
impl wire::field_service_command_service_server::FieldServiceCommandService
    for FieldServiceCommandRpc
{
    async fn activate_agreement(
        &self,
        request: Request<wire::ActivateServiceAgreementRequest>,
    ) -> Result<Response<wire::FieldServiceMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, target) =
            command_invocation(&self.state, &request, Some(context), body.scope.as_ref()).await?;
        if context.expected_aggregate_version != 0 {
            return Err(Status::failed_precondition(
                "activate agreement cannot carry an expected version",
            ));
        }
        let identity = identity(context, body)?;
        let agreement_id = field::ServiceAgreementId::try_from_uuid(uuid(
            &body.service_agreement_id,
            "service_agreement_id",
        )?)
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let decided_at = Utc::now();
        let human_decision = future_auth::human_decision(
            &invocation,
            field::agreement_activate_decision_ref(identity.command_id, agreement_id),
            decided_at,
        )?;
        let agreement = field::ServiceAgreement::activate(
            agreement_id,
            target,
            version_ref(body.customer.clone())?,
            body.covered_assets
                .iter()
                .cloned()
                .map(|value| stable_ref(Some(value)))
                .collect::<Result<BTreeSet<_>, _>>()?,
            body.covered_locations
                .iter()
                .cloned()
                .map(|value| stable_ref(Some(value)))
                .collect::<Result<BTreeSet<_>, _>>()?,
            body.service_codes.iter().cloned().collect(),
            date(&body.valid_from, "valid_from")?,
            date(&body.valid_through, "valid_through")?,
            &human_decision,
            identity.command_id,
            decided_at,
        )
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let actor = actor(&invocation);
        let service = field::FieldServiceCommandService::new(
            FieldAccess {
                actor: actor.clone(),
                write: true,
            },
            field_sqlx::SqlxFieldServiceWriteRepository::new(pool),
            BillingHandoffUnavailable,
        );
        let receipt = service
            .activate_agreement(&actor, identity, agreement, human_decision, decided_at)
            .await
            .map_err(status)?;
        Ok(Response::new(response(
            context,
            receipt,
            "field-service.agreement",
            "active",
        )))
    }

    async fn create_order(
        &self,
        request: Request<wire::CreateServiceOrderRequest>,
    ) -> Result<Response<wire::FieldServiceMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, target) =
            command_invocation(&self.state, &request, Some(context), body.scope.as_ref()).await?;
        if context.expected_aggregate_version != 0 {
            return Err(Status::failed_precondition(
                "create order cannot carry an expected version",
            ));
        }
        let service_request = field::ServiceRequest {
            id: field::ServiceRequestId::try_from_uuid(uuid(
                &body.service_request_id,
                "service_request_id",
            )?)
            .map_err(|error| Status::invalid_argument(error.to_string()))?,
            version: body.service_request_version,
            scope: target,
            customer: version_ref(body.customer.clone())?,
            asset: version_ref(body.asset.clone())?,
            location: stable_ref(body.location.clone())?,
            service_code: body.service_code.clone(),
            requested_at: timestamp(&body.requested_at, "requested_at")?,
        };
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let actor = actor(&invocation);
        let service = field::FieldServiceCommandService::new(
            FieldAccess {
                actor: actor.clone(),
                write: true,
            },
            field_sqlx::SqlxFieldServiceWriteRepository::new(pool),
            BillingHandoffUnavailable,
        );
        let receipt = service
            .create_order(
                &actor,
                identity(context, body)?,
                field::ServiceOrderId::try_from_uuid(uuid(
                    &body.service_order_id,
                    "service_order_id",
                )?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
                service_request,
                field::ServiceAgreementId::try_from_uuid(uuid(
                    &body.service_agreement_id,
                    "service_agreement_id",
                )?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
                body.service_agreement_version,
                body.required_skills.iter().cloned().collect(),
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(
            context,
            receipt,
            "field-service.visit",
            "draft",
        )))
    }

    async fn schedule_order(
        &self,
        request: Request<wire::ScheduleServiceOrderRequest>,
    ) -> Result<Response<wire::FieldServiceMutationResponse>, Status> {
        let body = request.get_ref();
        let qualification = field::TechnicianQualification {
            technician: version_ref(body.technician.clone())?,
            principal_id: principal(&body.technician_principal_id, "technician_principal_id")?,
            skills: body.verified_skills.iter().cloned().collect(),
            valid_through: date(
                &body.qualification_valid_through,
                "qualification_valid_through",
            )?,
            evidence: version_ref(body.qualification_evidence.clone())?,
        };
        let scheduled = date(&body.scheduled_on, "scheduled_on")?;
        self.mutate(
            &request,
            body.order_scope.as_ref(),
            &body.service_order_id,
            field::FieldServiceAction::Schedule,
            "field_service.order_scheduled",
            move |order, _| {
                order.schedule(
                    field::Connectivity::Online,
                    scheduled,
                    qualification,
                    Utc::now(),
                )
            },
        )
        .await
    }

    async fn reserve_part(
        &self,
        request: Request<wire::ReserveServicePartRequest>,
    ) -> Result<Response<wire::FieldServiceMutationResponse>, Status> {
        let body = request.get_ref();
        let reserved = quantity(body.reserved_quantity)?;
        let part = field::PartReservation {
            part: version_ref(body.part.clone())?,
            reserved,
            issued: Quantity::try_new(0, reserved.scale())
                .map_err(|_| Status::invalid_argument("reserved quantity is invalid"))?,
            inventory_reservation: version_ref(body.inventory_reservation.clone())?,
        };
        self.mutate(
            &request,
            body.order_scope.as_ref(),
            &body.service_order_id,
            field::FieldServiceAction::Schedule,
            "field_service.part_reserved",
            move |order, _| order.reserve_part(field::Connectivity::Online, part, Utc::now()),
        )
        .await
    }

    async fn dispatch_order(
        &self,
        request: Request<wire::DispatchServiceOrderRequest>,
    ) -> Result<Response<wire::FieldServiceMutationResponse>, Status> {
        let body = request.get_ref();
        let agreement_id = field::ServiceAgreementId::try_from_uuid(uuid(
            &body.service_agreement_id,
            "service_agreement_id",
        )?)
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let agreement_version = body.service_agreement_version;
        let dispatch_id = field::DispatchId::try_from_uuid(uuid(&body.dispatch_id, "dispatch_id")?)
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        self.mutate(
            &request,
            body.order_scope.as_ref(),
            &body.service_order_id,
            field::FieldServiceAction::Dispatch,
            "field_service.order_dispatched",
            move |order, agreement| {
                let agreement = agreement.ok_or(field::FieldServiceDomainError::CoverageDenied)?;
                if agreement.id() != agreement_id || agreement.version() != agreement_version {
                    return Err(field::FieldServiceDomainError::CoverageDenied);
                }
                order.dispatch(
                    field::Connectivity::Online,
                    dispatch_id,
                    agreement,
                    Utc::now(),
                )
            },
        )
        .await
    }

    async fn start_visit(
        &self,
        request: Request<wire::StartServiceVisitRequest>,
    ) -> Result<Response<wire::FieldServiceMutationResponse>, Status> {
        let body = request.get_ref();
        let visit_id = field::VisitId::try_from_uuid(uuid(&body.visit_id, "visit_id")?)
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let customer = version_ref(body.presented_customer.clone())?;
        let asset = version_ref(body.presented_asset.clone())?;
        self.mutate(
            &request,
            body.order_scope.as_ref(),
            &body.service_order_id,
            field::FieldServiceAction::ExecuteVisit,
            "field_service.visit_started",
            move |order, _| {
                order.start_visit(
                    field::Connectivity::Online,
                    visit_id,
                    &customer,
                    &asset,
                    Utc::now(),
                )
            },
        )
        .await
    }

    async fn record_exception(
        &self,
        request: Request<wire::RecordServiceExceptionRequest>,
    ) -> Result<Response<wire::FieldServiceMutationResponse>, Status> {
        let body = request.get_ref();
        let exception = field::ServiceException {
            id: field::ExceptionId::try_from_uuid(uuid(&body.exception_id, "exception_id")?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
            kind: decode_exception_kind(body.kind)?,
            reason: body.reason.clone(),
            evidence: version_ref(body.evidence.clone())?,
            recorded_at: timestamp(&body.recorded_at, "recorded_at")?,
        };
        self.mutate(
            &request,
            body.order_scope.as_ref(),
            &body.service_order_id,
            field::FieldServiceAction::RecordException,
            "field_service.exception_recorded",
            move |order, _| order.record_exception(field::Connectivity::Online, exception),
        )
        .await
    }

    async fn issue_part(
        &self,
        request: Request<wire::IssueServicePartRequest>,
    ) -> Result<Response<wire::FieldServiceMutationResponse>, Status> {
        let body = request.get_ref();
        let fact_id = field::PartFactId::try_from_uuid(uuid(&body.part_fact_id, "part_fact_id")?)
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let part_id = uuid(&body.part_record_id, "part_record_id")?;
        let quantity = quantity(body.issued_quantity)?;
        self.mutate(
            &request,
            body.order_scope.as_ref(),
            &body.service_order_id,
            field::FieldServiceAction::ExecuteVisit,
            "field_service.part_issued",
            move |order, _| {
                order
                    .issue_part(
                        field::Connectivity::Online,
                        fact_id,
                        part_id,
                        quantity,
                        Utc::now(),
                    )
                    .map(|_| ())
            },
        )
        .await
    }

    async fn complete_visit(
        &self,
        request: Request<wire::CompleteServiceVisitRequest>,
    ) -> Result<Response<wire::FieldServiceMutationResponse>, Status> {
        let body = request.get_ref();
        let evidence = body
            .completion_evidence
            .iter()
            .cloned()
            .map(|value| version_ref(Some(value)))
            .collect::<Result<Vec<_>, _>>()?;
        let signature = decode_signature(body.customer_signature.clone())?;
        self.mutate(
            &request,
            body.order_scope.as_ref(),
            &body.service_order_id,
            field::FieldServiceAction::ExecuteVisit,
            "field_service.visit_completed",
            move |order, _| {
                order.complete_visit(field::Connectivity::Online, evidence, signature, Utc::now())
            },
        )
        .await
    }

    async fn release_billing(
        &self,
        request: Request<wire::ReleaseFieldServiceBillingRequest>,
    ) -> Result<Response<wire::FieldServiceMutationResponse>, Status> {
        let body = request.get_ref();
        let context = required_command(body.context.as_ref())?;
        let (invocation, target) = command_invocation(
            &self.state,
            &request,
            Some(context),
            body.order_scope.as_ref(),
        )
        .await?;
        let identity = identity(context, body)?;
        let order_id = service_order_id(&body.service_order_id)?;
        let handoff_id =
            field::BillingHandoffId::try_from_uuid(uuid(&body.handoff_id, "handoff_id")?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let expected = expected(context)
            .ok_or_else(|| Status::failed_precondition("expected aggregate version is required"))?;
        let decided_at = Utc::now();
        let human_decision = future_auth::human_decision(
            &invocation,
            field::billing_release_decision_ref(
                identity.command_id,
                order_id,
                expected,
                handoff_id,
            ),
            decided_at,
        )?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let actor = actor(&invocation);
        let service = field::FieldServiceCommandService::new(
            FieldAccess {
                actor: actor.clone(),
                write: true,
            },
            field_sqlx::SqlxFieldServiceWriteRepository::new(pool),
            BillingHandoffUnavailable,
        );
        let receipt = service
            .release_billing(
                &actor,
                identity,
                order_id,
                &target,
                handoff_id,
                money(body.amount.clone())?,
                Some(expected),
                human_decision,
                decided_at,
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(
            context,
            receipt,
            "field-service.handoff",
            "billing_handed_off",
        )))
    }
}

impl FieldServiceCommandRpc {
    async fn mutate<T, F>(
        &self,
        request: &Request<T>,
        target_scope: Option<&common::AuthorityScope>,
        order_id: &str,
        action: field::FieldServiceAction,
        event_type: &'static str,
        change: F,
    ) -> Result<Response<wire::FieldServiceMutationResponse>, Status>
    where
        T: Message + CommandContextAccess,
        F: FnOnce(
            &mut field::ServiceOrder,
            Option<&field::ServiceAgreement>,
        ) -> Result<(), field::FieldServiceDomainError>,
    {
        let context = required_command(request.get_ref().context())?;
        let (invocation, target) =
            command_invocation(&self.state, request, Some(context), target_scope).await?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let actor = actor(&invocation);
        let service = field::FieldServiceCommandService::new(
            FieldAccess {
                actor: actor.clone(),
                write: true,
            },
            field_sqlx::SqlxFieldServiceWriteRepository::new(pool),
            BillingHandoffUnavailable,
        );
        let receipt = service
            .mutate_order(
                &actor,
                identity(context, request.get_ref())?,
                service_order_id(order_id)?,
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
            "field-service.visit",
            "updated",
        )))
    }
}

trait CommandContextAccess {
    fn context(&self) -> Option<&common::CommandContext>;
}

macro_rules! impl_context_access {
    ($($type:ty),+ $(,)?) => {$ (
        impl CommandContextAccess for $type {
            fn context(&self) -> Option<&common::CommandContext> { self.context.as_ref() }
        }
    )+ };
}

impl_context_access!(
    wire::ScheduleServiceOrderRequest,
    wire::ReserveServicePartRequest,
    wire::DispatchServiceOrderRequest,
    wire::StartServiceVisitRequest,
    wire::RecordServiceExceptionRequest,
    wire::IssueServicePartRequest,
    wire::CompleteServiceVisitRequest,
);

#[tonic::async_trait]
impl wire::field_service_query_service_server::FieldServiceQueryService for FieldServiceQueryRpc {
    async fn get_order(
        &self,
        request: Request<wire::GetServiceOrderRequest>,
    ) -> Result<Response<wire::GetServiceOrderResponse>, Status> {
        let body = request.get_ref();
        let (invocation, target) = query_invocation(
            &self.state,
            &request,
            body.context.as_ref(),
            body.order_scope.as_ref(),
        )
        .await?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Read).await?;
        let actor = actor(&invocation);
        let service = field::FieldServiceQueryService::new(
            FieldAccess {
                actor: actor.clone(),
                write: false,
            },
            field_sqlx::SqlxFieldServiceReadRepository::new(pool, invocation.holding_id)
                .map_err(|_| Status::unavailable("field-service read model is unavailable"))?,
        );
        let result = service
            .get_order(&actor, &target, service_order_id(&body.service_order_id)?)
            .await
            .map_err(status)?;
        let order = &result.order;
        Ok(Response::new(wire::GetServiceOrderResponse {
            order: Some(wire::ServiceOrderProjection {
                service_order_id: order.id().as_uuid().to_string(),
                source_version: order.version(),
                scope: Some(encode_scope(order.scope())),
                service_request_id: order.request_id().as_uuid().to_string(),
                service_agreement_id: order.agreement_id().as_uuid().to_string(),
                customer_record_id: order.customer().record().id().as_uuid().to_string(),
                asset_record_id: order.asset().record().id().as_uuid().to_string(),
                location_record_id: order.location().id().as_uuid().to_string(),
                service_code: order.service_code().into(),
                technician_record_id: order
                    .technician()
                    .map(|value| value.technician.record().id().as_uuid().to_string()),
                dispatch_id: order.dispatch_id().map(|value| value.as_uuid().to_string()),
                visit_id: order.visit_id().map(|value| value.as_uuid().to_string()),
                status: encode_order_status(order.status()),
                billing_handoff_id: order
                    .billing_handoff_id()
                    .map(|value| value.as_uuid().to_string()),
                source_position: result.source_position,
                projected_at: result.projected_at.to_rfc3339(),
            }),
            projection: Some(current_projection(result.projected_at)),
        }))
    }

    async fn get_record(
        &self,
        request: Request<wire::GetFieldServiceRecordRequest>,
    ) -> Result<Response<wire::GetFieldServiceRecordResponse>, Status> {
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
        let service = field::FieldServiceQueryService::new(
            FieldAccess {
                actor: actor.clone(),
                write: false,
            },
            field_sqlx::SqlxFieldServiceReadRepository::new(pool, invocation.holding_id)
                .map_err(|_| Status::unavailable("field-service read model is unavailable"))?,
        );
        let result = service
            .get_record(&actor, &target, uuid(&body.record_id, "record_id")?)
            .await
            .map_err(status)?;
        let record = &result.record;
        Ok(Response::new(wire::GetFieldServiceRecordResponse {
            record: Some(wire::FieldServiceRecordProjection {
                record_id: record.id().to_string(),
                source_version: record.version(),
                scope: Some(encode_scope(record.scope())),
                record_kind: record.kind().token().into(),
                subject_record_id: record.subject().id().as_uuid().to_string(),
                lifecycle_status: lifecycle(record.status())?,
                source_position: result.source_position,
                projected_at: result.projected_at.to_rfc3339(),
            }),
            projection: Some(current_projection(result.projected_at)),
        }))
    }
}

fn service_order_id(value: &str) -> Result<field::ServiceOrderId, Status> {
    field::ServiceOrderId::try_from_uuid(uuid(value, "service_order_id")?)
        .map_err(|error| Status::invalid_argument(error.to_string()))
}

fn decode_exception_kind(value: i32) -> Result<field::ExceptionKind, Status> {
    match wire::ServiceExceptionKind::try_from(value) {
        Ok(wire::ServiceExceptionKind::CustomerMismatch) => {
            Ok(field::ExceptionKind::CustomerMismatch)
        }
        Ok(wire::ServiceExceptionKind::AssetMismatch) => Ok(field::ExceptionKind::AssetMismatch),
        Ok(wire::ServiceExceptionKind::SignatureRefused) => {
            Ok(field::ExceptionKind::SignatureRefused)
        }
        Ok(wire::ServiceExceptionKind::CoverageMismatch) => {
            Ok(field::ExceptionKind::CoverageMismatch)
        }
        Ok(wire::ServiceExceptionKind::InventoryMismatch) => {
            Ok(field::ExceptionKind::InventoryMismatch)
        }
        _ => Err(Status::invalid_argument("exception kind is invalid")),
    }
}

fn decode_signature(
    value: Option<wire::CustomerSignatureEvidence>,
) -> Result<field::CustomerSignature, Status> {
    let value = value.ok_or_else(|| Status::invalid_argument("customer signature is required"))?;
    let evidence = version_ref(value.evidence)?;
    if value.signed {
        if value.refusal_exception_id.is_some()
            || value.refusal_reason.is_some()
            || value.witnessed_by_principal_id.is_some()
        {
            return Err(Status::invalid_argument(
                "signed evidence cannot include refusal fields",
            ));
        }
        return Ok(field::CustomerSignature::Signed { evidence });
    }
    let exception_id = value
        .refusal_exception_id
        .as_deref()
        .ok_or_else(|| Status::invalid_argument("refusal_exception_id is required"))?;
    Ok(field::CustomerSignature::Refused {
        exception_id: field::ExceptionId::try_from_uuid(uuid(
            exception_id,
            "refusal_exception_id",
        )?)
        .map_err(|error| Status::invalid_argument(error.to_string()))?,
        reason: value
            .refusal_reason
            .ok_or_else(|| Status::invalid_argument("refusal_reason is required"))?,
        witnessed_by: principal(
            value
                .witnessed_by_principal_id
                .as_deref()
                .ok_or_else(|| Status::invalid_argument("witnessed_by_principal_id is required"))?,
            "witnessed_by_principal_id",
        )?,
        evidence,
    })
}

fn decode_scope(
    value: Option<&common::AuthorityScope>,
) -> Result<erpx_kernel::AuthorityScope, Status> {
    use common::authority_scope::Scope;
    let scope = value
        .and_then(|value| value.scope.as_ref())
        .ok_or_else(|| Status::invalid_argument("record scope is required"))?;
    let decoded = match scope {
        Scope::Platform(_) => erpx_kernel::AuthorityScope::Platform,
        Scope::Holding(value) => erpx_kernel::AuthorityScope::Holding {
            holding_id: uuid(&value.holding_id, "holding_id")?,
        },
        Scope::Company(value) => erpx_kernel::AuthorityScope::Company {
            holding_id: uuid(&value.holding_id, "holding_id")?,
            company_id: uuid(&value.company_id, "company_id")?,
        },
        Scope::Branch(value) => erpx_kernel::AuthorityScope::Branch {
            holding_id: uuid(&value.holding_id, "holding_id")?,
            company_id: uuid(&value.company_id, "company_id")?,
            branch_id: uuid(&value.branch_id, "branch_id")?,
        },
        Scope::Subtree(value) => erpx_kernel::AuthorityScope::Subtree {
            holding_id: uuid(&value.holding_id, "holding_id")?,
            organization_node_id: uuid(&value.organization_node_id, "organization_node_id")?,
        },
    };
    decoded
        .validate()
        .map_err(|_| Status::invalid_argument("record scope is invalid"))?;
    Ok(decoded)
}

fn encode_scope(value: &erpx_kernel::AuthorityScope) -> common::AuthorityScope {
    use common::authority_scope::Scope;
    common::AuthorityScope {
        scope: Some(match value {
            erpx_kernel::AuthorityScope::Platform => Scope::Platform(common::PlatformScope {}),
            erpx_kernel::AuthorityScope::Holding { holding_id } => {
                Scope::Holding(common::HoldingScope {
                    holding_id: holding_id.to_string(),
                })
            }
            erpx_kernel::AuthorityScope::Company {
                holding_id,
                company_id,
            } => Scope::Company(common::CompanyScope {
                holding_id: holding_id.to_string(),
                company_id: company_id.to_string(),
            }),
            erpx_kernel::AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Scope::Branch(common::BranchScope {
                holding_id: holding_id.to_string(),
                company_id: company_id.to_string(),
                branch_id: branch_id.to_string(),
            }),
            erpx_kernel::AuthorityScope::Subtree {
                holding_id,
                organization_node_id,
            } => Scope::Subtree(common::SubtreeScope {
                holding_id: holding_id.to_string(),
                organization_node_id: organization_node_id.to_string(),
            }),
        }),
    }
}

fn current_projection(projected_at: DateTime<Utc>) -> common::ProjectionMetadata {
    common::ProjectionMetadata {
        checkpoint: None,
        freshness: common::ProjectionFreshnessState::Current as i32,
        lag_millis: Utc::now()
            .signed_duration_since(projected_at)
            .to_std()
            .ok()
            .and_then(|duration| u64::try_from(duration.as_millis()).ok()),
    }
}

fn encode_order_status(value: field::ServiceOrderStatus) -> i32 {
    (match value {
        field::ServiceOrderStatus::Draft => wire::ServiceOrderStatus::Draft,
        field::ServiceOrderStatus::Scheduled => wire::ServiceOrderStatus::Scheduled,
        field::ServiceOrderStatus::Dispatched => wire::ServiceOrderStatus::Dispatched,
        field::ServiceOrderStatus::InProgress => wire::ServiceOrderStatus::InProgress,
        field::ServiceOrderStatus::Completed => wire::ServiceOrderStatus::Completed,
        field::ServiceOrderStatus::BillingHandedOff => wire::ServiceOrderStatus::BillingHandedOff,
        field::ServiceOrderStatus::Closed => wire::ServiceOrderStatus::Closed,
        field::ServiceOrderStatus::Cancelled => wire::ServiceOrderStatus::Cancelled,
        field::ServiceOrderStatus::Exception => wire::ServiceOrderStatus::Exception,
    }) as i32
}

#[cfg(test)]
mod tests {
    #[test]
    fn external_billing_acknowledgement_is_never_fabricated() {
        let source = include_str!("future_field_service.rs");
        assert!(source.contains("BillingHandoffUnavailable"));
        assert!(!source.contains(concat!("accepted", ": true")));
    }

    #[test]
    fn mutations_are_online_only() {
        let source = include_str!("future_field_service.rs");
        assert!(source.contains("field::Connectivity::Online"));
        assert!(!source.contains(concat!("field::Connectivity", "::Offline")));
    }
}
