//! Native target transports for Manufacturing and Planning.

use std::collections::BTreeSet;

use async_trait::async_trait;
use chrono::{NaiveDate, Utc};
use erpx_infrastructure::RoutedDatabaseSide;
use erpx_integration_contracts::catalog::ContextId;
use erpx_integration_contracts::{
    CurrencyCode, MoneyAmount, Quantity, RecordTypeId, RecordVersionRef, StableRecordRef,
};
use erpx_kernel::{CommandId, CorrelationId, RecordId};
use erpx_manufacturing as manufacturing;
use erpx_manufacturing_sqlx as manufacturing_sqlx;
use erpx_planning as planning;
use erpx_planning_sqlx as planning_sqlx;
use erpx_proto::erpx::{manufacturing::v2 as manufacturing_wire, planning::v2 as planning_wire};
use prost::Message;
use sha2::{Digest, Sha256};
use tonic::{Request, Response, Status};
use uuid::Uuid;

use super::future_auth::{self, FutureInvocation};
use crate::AppState;

const MANUFACTURING_WRITE: &str = "manufacturing.write";
const MANUFACTURING_READ: &str = "manufacturing.read";
const PLANNING_WRITE: &str = "planning.write";
const PLANNING_READ: &str = "planning.read";

#[derive(Clone)]
pub(crate) struct ManufacturingCommandRpc {
    state: AppState,
}

#[derive(Clone)]
pub(crate) struct ManufacturingQueryRpc {
    state: AppState,
}

#[derive(Clone)]
pub(crate) struct PlanningCommandRpc {
    state: AppState,
}

#[derive(Clone)]
pub(crate) struct PlanningQueryRpc {
    state: AppState,
}

impl ManufacturingCommandRpc {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}
impl ManufacturingQueryRpc {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}
impl PlanningCommandRpc {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}
impl PlanningQueryRpc {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[derive(Debug, Clone)]
struct ManufacturingAccess {
    actor: manufacturing::ActorContext,
    write: bool,
}

#[async_trait]
impl manufacturing::AuthorizationPort for ManufacturingAccess {
    async fn authorize(
        &self,
        actor: &manufacturing::ActorContext,
        target_scope: &erpx_kernel::TenantScope,
        action: manufacturing::ManufacturingAction,
    ) -> Result<manufacturing::AuthorizedManufacturingAccess, manufacturing::PortError> {
        let action_writes = !matches!(
            action,
            manufacturing::ManufacturingAction::View | manufacturing::ManufacturingAction::List
        );
        if actor != &self.actor
            || action_writes != self.write
            || !self.actor.authority_scope.contains(target_scope)
        {
            return Err(manufacturing::PortError::Denied);
        }
        Ok(manufacturing::AuthorizedManufacturingAccess {
            actor: actor.clone(),
            target_scope: target_scope.clone(),
            action,
        })
    }
}

#[derive(Debug, Clone)]
struct PlanningAccess {
    actor: planning::ActorContext,
    write: bool,
}

#[async_trait]
impl planning::AuthorizationPort for PlanningAccess {
    async fn authorize(
        &self,
        actor: &planning::ActorContext,
        target_scope: &erpx_kernel::TenantScope,
        action: planning::PlanningAction,
    ) -> Result<planning::AuthorizedPlanningAccess, planning::PortError> {
        let action_writes = !matches!(
            action,
            planning::PlanningAction::View | planning::PlanningAction::List
        );
        if actor != &self.actor
            || action_writes != self.write
            || !self.actor.authority_scope.contains(target_scope)
        {
            return Err(planning::PortError::Denied);
        }
        Ok(planning::AuthorizedPlanningAccess {
            actor: actor.clone(),
            target_scope: target_scope.clone(),
            action,
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct RecommendationHandoffUnavailable;

#[async_trait]
impl planning::RecommendationHandoffPort for RecommendationHandoffUnavailable {
    async fn release(
        &self,
        _authority: &planning::AuthorizedPlanningAccess,
        _release: &planning::RecommendationRelease,
    ) -> Result<planning::RecommendationReleaseReceipt, planning::PortError> {
        // Planning may propose and approve independently, but the owner-context
        // buy/make/transfer acknowledgement is a separate typed integration.
        Err(planning::PortError::Unavailable)
    }
}

fn manufacturing_actor(invocation: &FutureInvocation) -> manufacturing::ActorContext {
    manufacturing::ActorContext {
        tenant_id: invocation.tenant_id,
        principal_id: invocation.principal_id,
        authority_scope: invocation.scope.clone(),
    }
}

fn planning_actor(invocation: &FutureInvocation) -> planning::ActorContext {
    planning::ActorContext {
        tenant_id: invocation.tenant_id,
        principal_id: invocation.principal_id,
        authority_scope: invocation.scope.clone(),
    }
}

fn command_uuid(value: &str, field: &'static str) -> Result<Uuid, Status> {
    Uuid::parse_str(value).map_err(|_| Status::invalid_argument(format!("{field} is invalid")))
}

fn correlation(value: &str) -> Result<CorrelationId, Status> {
    CorrelationId::try_from_uuid(command_uuid(value, "correlation_id")?)
        .map_err(|_| Status::invalid_argument("correlation_id is invalid"))
}

fn manufacturing_identity<M: Message>(
    context: &erpx_proto::common::v2::CommandContext,
    request: &M,
) -> Result<manufacturing::MutationIdentity, Status> {
    Ok(manufacturing::MutationIdentity {
        command_id: CommandId::try_from_uuid(command_uuid(&context.command_id, "command_id")?)
            .map_err(|_| Status::invalid_argument("command_id is invalid"))?,
        idempotency_key: context.idempotency_key.clone(),
        request_digest: hex::encode(Sha256::digest(request.encode_to_vec())),
    })
}

fn planning_identity<M: Message>(
    context: &erpx_proto::common::v2::CommandContext,
    request: &M,
) -> Result<planning::MutationIdentity, Status> {
    Ok(planning::MutationIdentity {
        command_id: CommandId::try_from_uuid(command_uuid(&context.command_id, "command_id")?)
            .map_err(|_| Status::invalid_argument("command_id is invalid"))?,
        idempotency_key: context.idempotency_key.clone(),
        request_digest: hex::encode(Sha256::digest(request.encode_to_vec())),
    })
}

fn quantity(value: Option<manufacturing_wire::FixedQuantity>) -> Result<Quantity, Status> {
    let value = value.ok_or_else(|| Status::invalid_argument("quantity is required"))?;
    Quantity::try_new(
        value.units,
        u8::try_from(value.scale).map_err(|_| Status::invalid_argument("scale is invalid"))?,
    )
    .map_err(|_| Status::invalid_argument("quantity is invalid"))
}

fn planning_quantity(value: Option<planning_wire::FixedQuantity>) -> Result<Quantity, Status> {
    let value = value.ok_or_else(|| Status::invalid_argument("quantity is required"))?;
    Quantity::try_new(
        value.units,
        u8::try_from(value.scale).map_err(|_| Status::invalid_argument("scale is invalid"))?,
    )
    .map_err(|_| Status::invalid_argument("quantity is invalid"))
}

fn stable_reference(
    record_id: &str,
    owner_context: &str,
    record_type: &str,
) -> Result<StableRecordRef, Status> {
    let id = RecordId::try_from_uuid(command_uuid(record_id, "record_id")?)
        .map_err(|_| Status::invalid_argument("record_id is invalid"))?;
    let owner = ContextId::ALL
        .into_iter()
        .find(|value| value.slug() == owner_context)
        .ok_or_else(|| Status::invalid_argument("owner_context is invalid"))?;
    let record_type = RecordTypeId::try_new(record_type)
        .map_err(|_| Status::invalid_argument("record_type is invalid"))?;
    StableRecordRef::try_new(id, owner, record_type)
        .map_err(|_| Status::invalid_argument("record reference is invalid"))
}

fn manufacturing_reference(
    value: Option<manufacturing_wire::RecordRef>,
) -> Result<StableRecordRef, Status> {
    let value = value.ok_or_else(|| Status::invalid_argument("record reference is required"))?;
    stable_reference(&value.record_id, &value.owner_context, &value.record_type)
}

fn planning_reference(value: Option<planning_wire::RecordRef>) -> Result<StableRecordRef, Status> {
    let value = value.ok_or_else(|| Status::invalid_argument("record reference is required"))?;
    stable_reference(&value.record_id, &value.owner_context, &value.record_type)
}

fn planning_version_ref(
    value: Option<planning_wire::VersionPin>,
) -> Result<RecordVersionRef, Status> {
    let value = value.ok_or_else(|| Status::invalid_argument("version pin is required"))?;
    RecordVersionRef::try_new(planning_reference(value.record)?, value.version)
        .map_err(|_| Status::invalid_argument("version pin is invalid"))
}

fn ensure_scope(
    invocation: &FutureInvocation,
    value: Option<&erpx_proto::common::v2::TenantScope>,
) -> Result<(), Status> {
    let presented = decode_wire_scope(value)?;
    if presented != invocation.scope {
        return Err(Status::permission_denied(
            "record scope does not match authenticated authority",
        ));
    }
    Ok(())
}

fn enforce_aggregate_version(
    context: Option<&erpx_proto::common::v2::QueryContext>,
    record_id: &str,
    actual_version: u64,
) -> Result<(), Status> {
    use erpx_proto::common::v2::query_consistency::Requirement;
    let requirement = context
        .and_then(|context| context.consistency.as_ref())
        .and_then(|consistency| consistency.requirement.as_ref())
        .ok_or_else(|| Status::invalid_argument("query consistency is required"))?;
    match requirement {
        Requirement::Eventual(_) => Ok(()),
        Requirement::AggregateVersion(target)
            if target.aggregate_id == record_id && target.version > 0 =>
        {
            if actual_version < target.version {
                Err(Status::failed_precondition(
                    "read model has not reached the required aggregate version",
                ))
            } else {
                Ok(())
            }
        }
        Requirement::AggregateVersion(_) | Requirement::ReadYourWrites(_) => Err(
            Status::invalid_argument("aggregate-version consistency target is invalid"),
        ),
    }
}

fn lifecycle_token(value: impl serde::Serialize) -> Result<String, Status> {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .ok_or_else(|| Status::internal("record lifecycle cannot be serialized"))
}

fn decode_wire_scope(
    value: Option<&erpx_proto::common::v2::TenantScope>,
) -> Result<erpx_kernel::TenantScope, Status> {
    use erpx_proto::common::v2::tenant_scope::Scope;
    let scope = value
        .and_then(|value| value.scope.as_ref())
        .ok_or_else(|| Status::invalid_argument("record scope is required"))?;
    let uuid = |value: &str| {
        Uuid::parse_str(value).map_err(|_| Status::invalid_argument("scope identifier is invalid"))
    };
    let scope = match scope {
        Scope::Platform(_) => erpx_kernel::TenantScope::Platform,
        Scope::Holding(value) => erpx_kernel::TenantScope::Holding {
            holding_id: uuid(&value.holding_id)?,
        },
        Scope::Company(value) => erpx_kernel::TenantScope::Company {
            holding_id: uuid(&value.holding_id)?,
            company_id: uuid(&value.company_id)?,
        },
        Scope::Branch(value) => erpx_kernel::TenantScope::Branch {
            holding_id: uuid(&value.holding_id)?,
            company_id: uuid(&value.company_id)?,
            branch_id: uuid(&value.branch_id)?,
        },
        Scope::Subtree(value) => erpx_kernel::TenantScope::Subtree {
            holding_id: uuid(&value.holding_id)?,
            company_ids: value
                .company_ids
                .iter()
                .map(|value| uuid(value))
                .collect::<Result<_, _>>()?,
            branch_ids: value
                .branch_ids
                .iter()
                .map(|value| uuid(value))
                .collect::<Result<_, _>>()?,
        },
    };
    scope
        .validate()
        .map_err(|_| Status::invalid_argument("record scope is invalid"))?;
    Ok(scope)
}

fn manufacturing_receipt(
    value: manufacturing::CommitReceipt,
) -> manufacturing_wire::MutationReceipt {
    manufacturing_wire::MutationReceipt {
        aggregate_id: value
            .aggregate_key
            .rsplit_once(':')
            .map_or(value.aggregate_key.as_str(), |(_, id)| id)
            .to_owned(),
        aggregate_version: value.version,
        replayed: value.replayed,
    }
}

fn planning_receipt(value: planning::CommitReceipt) -> planning_wire::MutationReceipt {
    planning_wire::MutationReceipt {
        aggregate_ids: value
            .aggregate_keys
            .into_iter()
            .map(|key| {
                key.rsplit_once(':')
                    .map_or(key.clone(), |(_, id)| id.to_owned())
            })
            .collect(),
        replayed: value.replayed,
        aggregate_versions: value.aggregate_versions,
    }
}

fn manufacturing_status(error: manufacturing::ManufacturingError) -> Status {
    match error {
        manufacturing::ManufacturingError::Neutral => Status::not_found("item unavailable"),
        manufacturing::ManufacturingError::Conflict => Status::aborted("version conflict"),
        manufacturing::ManufacturingError::Invalid(message) => Status::invalid_argument(message),
        manufacturing::ManufacturingError::Unavailable => {
            Status::unavailable("manufacturing dependency unavailable")
        }
        manufacturing::ManufacturingError::Internal => {
            Status::internal("manufacturing operation failed")
        }
    }
}

fn planning_status(error: planning::PlanningError) -> Status {
    match error {
        planning::PlanningError::Neutral => Status::not_found("item unavailable"),
        planning::PlanningError::Conflict => Status::aborted("version conflict"),
        planning::PlanningError::Invalid(message) => Status::invalid_argument(message),
        planning::PlanningError::HandoffRejected(message) => Status::failed_precondition(message),
        planning::PlanningError::Unavailable => {
            Status::unavailable("planning dependency unavailable")
        }
        planning::PlanningError::Internal => Status::internal("planning operation failed"),
    }
}

#[allow(clippy::too_many_arguments)]
async fn mutate_manufacturing_order<M, F>(
    state: &AppState,
    request: &Request<M>,
    context: &erpx_proto::common::v2::CommandContext,
    order_scope: Option<&erpx_proto::common::v2::TenantScope>,
    order_id: &str,
    action: manufacturing::ManufacturingAction,
    event_type: &'static str,
    change: F,
) -> Result<Response<manufacturing_wire::MutationReceipt>, Status>
where
    M: Message,
    F: FnOnce(
        &mut manufacturing::ProductionOrder,
    ) -> Result<(), manufacturing::ManufacturingDomainError>,
{
    let invocation = future_auth::command(
        state,
        request,
        Some(context),
        "manufacturing",
        MANUFACTURING_WRITE,
    )
    .await?;
    ensure_scope(&invocation, order_scope)?;
    let pool = future_auth::pool(state, &invocation, RoutedDatabaseSide::Write).await?;
    let actor = manufacturing_actor(&invocation);
    let access = ManufacturingAccess {
        actor: actor.clone(),
        write: true,
    };
    let service = manufacturing::ManufacturingCommandService::new(
        access,
        manufacturing_sqlx::SqlxManufacturingWriteRepository::new(pool),
    );
    let receipt = service
        .mutate_order(
            &actor,
            manufacturing_identity(context, request.get_ref())?,
            manufacturing::ProductionOrderId::try_from_uuid(command_uuid(order_id, "order_id")?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
            &invocation.scope,
            (context.expected_aggregate_version > 0).then_some(context.expected_aggregate_version),
            action,
            event_type,
            change,
        )
        .await
        .map_err(manufacturing_status)?;
    Ok(Response::new(manufacturing_receipt(receipt)))
}

#[tonic::async_trait]
impl manufacturing_wire::manufacturing_command_service_server::ManufacturingCommandService
    for ManufacturingCommandRpc
{
    async fn publish_specification(
        &self,
        request: Request<manufacturing_wire::PublishSpecificationRequest>,
    ) -> Result<Response<manufacturing_wire::MutationReceipt>, Status> {
        let body = request.get_ref();
        let context = body
            .context
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("command context is required"))?;
        let invocation = future_auth::command(
            &self.state,
            &request,
            Some(context),
            "manufacturing",
            MANUFACTURING_WRITE,
        )
        .await?;
        ensure_scope(&invocation, body.scope.as_ref())?;
        let mode = match body.mode.as_str() {
            "discrete" => manufacturing::ManufacturingMode::Discrete,
            "process" => manufacturing::ManufacturingMode::Process,
            _ => return Err(Status::invalid_argument("manufacturing mode is invalid")),
        };
        let materials = body
            .materials
            .iter()
            .map(|line| {
                Ok(manufacturing::MaterialLine {
                    line: u16::try_from(line.line)
                        .map_err(|_| Status::invalid_argument("material line is invalid"))?,
                    item: manufacturing_reference(line.item.clone())?,
                    quantity: quantity(line.quantity)?,
                    role: match line.role.as_str() {
                        "component" => manufacturing::MaterialRole::Component,
                        "process_input" => manufacturing::MaterialRole::ProcessInput,
                        "co_product" => manufacturing::MaterialRole::CoProduct,
                        "by_product" => manufacturing::MaterialRole::ByProduct,
                        _ => return Err(Status::invalid_argument("material role is invalid")),
                    },
                    expected_scrap_basis_points: u16::try_from(line.expected_scrap_basis_points)
                        .map_err(|_| Status::invalid_argument("scrap rate is invalid"))?,
                })
            })
            .collect::<Result<Vec<_>, Status>>()?;
        let routing = body
            .routing
            .iter()
            .map(|step| {
                Ok(manufacturing::RoutingStep {
                    sequence: u16::try_from(step.sequence)
                        .map_err(|_| Status::invalid_argument("routing sequence is invalid"))?,
                    operation_code: step.operation_code.clone(),
                    work_center: manufacturing_reference(step.work_center.clone())?,
                    setup_seconds: step.setup_seconds,
                    run_seconds_per_output: step.run_seconds_per_output,
                })
            })
            .collect::<Result<Vec<_>, Status>>()?;
        let now = Utc::now();
        let specification = manufacturing::ManufacturingSpecification::draft(
            manufacturing::SpecificationId::try_from_uuid(command_uuid(
                &body.specification_id,
                "specification_id",
            )?)
            .map_err(|error| Status::invalid_argument(error.to_string()))?,
            invocation.scope.clone(),
            mode,
            manufacturing_reference(body.output_item.clone())?,
            quantity(body.output_quantity)?,
            materials,
            manufacturing::RoutingId::try_from_uuid(command_uuid(&body.routing_id, "routing_id")?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
            routing,
            now,
        )
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let actor = manufacturing_actor(&invocation);
        let access = ManufacturingAccess {
            actor: actor.clone(),
            write: true,
        };
        let service = manufacturing::ManufacturingCommandService::new(
            access,
            manufacturing_sqlx::SqlxManufacturingWriteRepository::new(pool),
        );
        let receipt = service
            .publish_specification(
                &actor,
                manufacturing_identity(context, body)?,
                correlation(&context.correlation_id)?,
                specification,
                now,
            )
            .await
            .map_err(manufacturing_status)?;
        Ok(Response::new(manufacturing_receipt(receipt)))
    }

    async fn create_order(
        &self,
        request: Request<manufacturing_wire::CreateOrderRequest>,
    ) -> Result<Response<manufacturing_wire::MutationReceipt>, Status> {
        let body = request.get_ref();
        let context = body
            .context
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("command context is required"))?;
        let invocation = future_auth::command(
            &self.state,
            &request,
            Some(context),
            "manufacturing",
            MANUFACTURING_WRITE,
        )
        .await?;
        ensure_scope(&invocation, body.order_scope.as_ref())?;
        if context.expected_aggregate_version != 0 {
            return Err(Status::failed_precondition(
                "create order cannot carry an expected version",
            ));
        }
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let actor = manufacturing_actor(&invocation);
        let access = ManufacturingAccess {
            actor: actor.clone(),
            write: true,
        };
        let service = manufacturing::ManufacturingCommandService::new(
            access,
            manufacturing_sqlx::SqlxManufacturingWriteRepository::new(pool),
        );
        let receipt = service
            .create_order(
                &actor,
                manufacturing_identity(context, body)?,
                manufacturing::ProductionOrderId::try_from_uuid(command_uuid(
                    &body.order_id,
                    "order_id",
                )?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
                &invocation.scope,
                manufacturing::SpecificationId::try_from_uuid(command_uuid(
                    &body.specification_id,
                    "specification_id",
                )?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
                body.specification_version,
                quantity(body.planned_output)?,
                correlation(&context.correlation_id)?,
                Utc::now(),
            )
            .await
            .map_err(manufacturing_status)?;
        Ok(Response::new(manufacturing_receipt(receipt)))
    }

    async fn release_order(
        &self,
        request: Request<manufacturing_wire::OrderCommandRequest>,
    ) -> Result<Response<manufacturing_wire::MutationReceipt>, Status> {
        let body = request.get_ref();
        let context = required_command_context(body.context.as_ref())?;
        mutate_manufacturing_order(
            &self.state,
            &request,
            context,
            body.order_scope.as_ref(),
            &body.order_id,
            manufacturing::ManufacturingAction::ReleaseOrder,
            "manufacturing.order_released",
            |order| order.release(Utc::now()),
        )
        .await
    }

    async fn reserve_material(
        &self,
        request: Request<manufacturing_wire::MaterialCommandRequest>,
    ) -> Result<Response<manufacturing_wire::MutationReceipt>, Status> {
        let body = request.get_ref();
        let context = required_command_context(body.context.as_ref())?;
        let line = u16::try_from(body.material_line)
            .map_err(|_| Status::invalid_argument("material line is invalid"))?;
        let requested = quantity(body.quantity)?;
        mutate_manufacturing_order(
            &self.state,
            &request,
            context,
            body.order_scope.as_ref(),
            &body.order_id,
            manufacturing::ManufacturingAction::ExecuteOrder,
            "manufacturing.material_reserved",
            move |order| order.reserve_material(line, requested, Utc::now()),
        )
        .await
    }

    async fn start_order(
        &self,
        request: Request<manufacturing_wire::OrderCommandRequest>,
    ) -> Result<Response<manufacturing_wire::MutationReceipt>, Status> {
        let body = request.get_ref();
        let context = required_command_context(body.context.as_ref())?;
        mutate_manufacturing_order(
            &self.state,
            &request,
            context,
            body.order_scope.as_ref(),
            &body.order_id,
            manufacturing::ManufacturingAction::ExecuteOrder,
            "manufacturing.order_started",
            |order| order.start(Utc::now()),
        )
        .await
    }

    async fn issue_material(
        &self,
        request: Request<manufacturing_wire::MaterialCommandRequest>,
    ) -> Result<Response<manufacturing_wire::MutationReceipt>, Status> {
        let body = request.get_ref();
        let context = required_command_context(body.context.as_ref())?;
        let fact =
            manufacturing::MaterialFactId::try_from_uuid(command_uuid(&body.fact_id, "fact_id")?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let line = u16::try_from(body.material_line)
            .map_err(|_| Status::invalid_argument("material line is invalid"))?;
        let requested = quantity(body.quantity)?;
        mutate_manufacturing_order(
            &self.state,
            &request,
            context,
            body.order_scope.as_ref(),
            &body.order_id,
            manufacturing::ManufacturingAction::ExecuteOrder,
            "manufacturing.material_issued",
            move |order| {
                order
                    .issue_material(fact, line, requested, Utc::now())
                    .map(|_| ())
            },
        )
        .await
    }

    async fn report_labor(
        &self,
        request: Request<manufacturing_wire::LaborResultRequest>,
    ) -> Result<Response<manufacturing_wire::MutationReceipt>, Status> {
        let body = request.get_ref();
        let context = required_command_context(body.context.as_ref())?;
        let result = manufacturing::LaborResult {
            id: manufacturing::LaborFactId::try_from_uuid(command_uuid(&body.fact_id, "fact_id")?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
            routing_sequence: u16::try_from(body.routing_sequence)
                .map_err(|_| Status::invalid_argument("routing sequence is invalid"))?,
            labor_seconds: body.labor_seconds,
            machine_seconds: body.machine_seconds,
            at: Utc::now(),
        };
        mutate_manufacturing_order(
            &self.state,
            &request,
            context,
            body.order_scope.as_ref(),
            &body.order_id,
            manufacturing::ManufacturingAction::ExecuteOrder,
            "manufacturing.labor_reported",
            move |order| order.report_labor(result).map(|_| ()),
        )
        .await
    }

    async fn report_result(
        &self,
        request: Request<manufacturing_wire::ProductionResultRequest>,
    ) -> Result<Response<manufacturing_wire::MutationReceipt>, Status> {
        let body = request.get_ref();
        let context = required_command_context(body.context.as_ref())?;
        let result = manufacturing::ProductionResult {
            id: manufacturing::ResultFactId::try_from_uuid(command_uuid(&body.fact_id, "fact_id")?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
            item: manufacturing_reference(body.item.clone())?,
            lot_or_serial: body.lot_or_serial.clone(),
            good: quantity(body.good)?,
            scrap: quantity(body.scrap)?,
            is_co_product: body.is_co_product,
            source_lots: body.source_lots.clone(),
            at: Utc::now(),
        };
        mutate_manufacturing_order(
            &self.state,
            &request,
            context,
            body.order_scope.as_ref(),
            &body.order_id,
            manufacturing::ManufacturingAction::ExecuteOrder,
            "manufacturing.result_reported",
            move |order| order.report_result(result).map(|_| ()),
        )
        .await
    }

    async fn apply_quality_disposition(
        &self,
        request: Request<manufacturing_wire::QualityDispositionRequest>,
    ) -> Result<Response<manufacturing_wire::MutationReceipt>, Status> {
        let body = request.get_ref();
        let context = required_command_context(body.context.as_ref())?;
        let authority = body
            .authority
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("quality authority is required"))?;
        let disposition = manufacturing::QualityDisposition {
            id: manufacturing::DispositionId::try_from_uuid(command_uuid(
                &body.disposition_id,
                "disposition_id",
            )?)
            .map_err(|error| Status::invalid_argument(error.to_string()))?,
            result_id: manufacturing::ResultFactId::try_from_uuid(command_uuid(
                &body.result_fact_id,
                "result_fact_id",
            )?)
            .map_err(|error| Status::invalid_argument(error.to_string()))?,
            quantity: quantity(body.quantity)?,
            disposition: match body.disposition.as_str() {
                "accepted" => manufacturing::Disposition::Accepted,
                "rejected" => manufacturing::Disposition::Rejected,
                "hold" => manufacturing::Disposition::Hold,
                _ => return Err(Status::invalid_argument("quality disposition is invalid")),
            },
            authority_record: manufacturing_reference(authority.record.clone())?,
            source_version: authority.version,
        };
        mutate_manufacturing_order(
            &self.state,
            &request,
            context,
            body.order_scope.as_ref(),
            &body.order_id,
            manufacturing::ManufacturingAction::RecordQuality,
            "manufacturing.quality_disposition_applied",
            move |order| order.apply_disposition(disposition, Utc::now()).map(|_| ()),
        )
        .await
    }

    async fn record_cost(
        &self,
        request: Request<manufacturing_wire::CostFactRequest>,
    ) -> Result<Response<manufacturing_wire::MutationReceipt>, Status> {
        let body = request.get_ref();
        let context = required_command_context(body.context.as_ref())?;
        let owner = body
            .owner
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("cost owner is required"))?;
        let currency = CurrencyCode::try_new(&body.currency)
            .map_err(|_| Status::invalid_argument("currency is invalid"))?;
        let cost = manufacturing::ManufacturingCostFact {
            id: manufacturing::CostFactId::try_from_uuid(command_uuid(
                &body.cost_fact_id,
                "cost_fact_id",
            )?)
            .map_err(|error| Status::invalid_argument(error.to_string()))?,
            amount: MoneyAmount::new(body.amount_minor, currency),
            category: body.category.clone(),
            owner_record: manufacturing_reference(owner.record.clone())?,
            owner_version: owner.version,
            reconciled: body.reconciled,
        };
        mutate_manufacturing_order(
            &self.state,
            &request,
            context,
            body.order_scope.as_ref(),
            &body.order_id,
            manufacturing::ManufacturingAction::RecordCost,
            "manufacturing.cost_recorded",
            move |order| order.record_cost(cost, Utc::now()).map(|_| ()),
        )
        .await
    }

    async fn complete_order(
        &self,
        request: Request<manufacturing_wire::OrderCommandRequest>,
    ) -> Result<Response<manufacturing_wire::MutationReceipt>, Status> {
        let body = request.get_ref();
        let context = required_command_context(body.context.as_ref())?;
        mutate_manufacturing_order(
            &self.state,
            &request,
            context,
            body.order_scope.as_ref(),
            &body.order_id,
            manufacturing::ManufacturingAction::ExecuteOrder,
            "manufacturing.order_completed",
            |order| order.complete(Utc::now()),
        )
        .await
    }

    async fn close_order(
        &self,
        request: Request<manufacturing_wire::OrderCommandRequest>,
    ) -> Result<Response<manufacturing_wire::MutationReceipt>, Status> {
        let body = request.get_ref();
        let context = required_command_context(body.context.as_ref())?;
        mutate_manufacturing_order(
            &self.state,
            &request,
            context,
            body.order_scope.as_ref(),
            &body.order_id,
            manufacturing::ManufacturingAction::CloseOrder,
            "manufacturing.order_closed",
            |order| order.close(Utc::now()),
        )
        .await
    }
}

fn required_command_context(
    context: Option<&erpx_proto::common::v2::CommandContext>,
) -> Result<&erpx_proto::common::v2::CommandContext, Status> {
    context.ok_or_else(|| Status::invalid_argument("command context is required"))
}

async fn manufacturing_order_query(
    state: &AppState,
    request: &Request<manufacturing_wire::GetManufacturingRecordRequest>,
) -> Result<Response<manufacturing_wire::ManufacturingRecordResponse>, Status> {
    let body = request.get_ref();
    let context = body
        .context
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("query context is required"))?;
    let invocation = future_auth::query(
        state,
        request,
        Some(context),
        "manufacturing",
        MANUFACTURING_READ,
    )
    .await?;
    ensure_scope(&invocation, body.record_scope.as_ref())?;
    let pool = future_auth::pool(state, &invocation, RoutedDatabaseSide::Read).await?;
    let actor = manufacturing_actor(&invocation);
    let access = ManufacturingAccess {
        actor: actor.clone(),
        write: false,
    };
    let read = manufacturing_sqlx::SqlxManufacturingReadRepository::new(pool, invocation.tenant_id)
        .map_err(|_| Status::unavailable("manufacturing read model is unavailable"))?;
    let projection = manufacturing::application::ManufacturingQueryService::new(access, read)
        .get_order(
            &actor,
            &invocation.scope,
            manufacturing::ProductionOrderId::try_from_uuid(command_uuid(
                &body.record_id,
                "record_id",
            )?)
            .map_err(|error| Status::invalid_argument(error.to_string()))?,
        )
        .await
        .map_err(manufacturing_status)?;
    Ok(Response::new(
        manufacturing_wire::ManufacturingRecordResponse {
            record_id: projection.order.id().as_uuid().to_string(),
            version: projection.order.version(),
            lifecycle_state: lifecycle_token(projection.order.status())?,
            projection_json_utf8: serde_json::to_vec(&projection.order)
                .map_err(|_| Status::internal("manufacturing projection serialization failed"))?,
            source_position: projection.source_position,
        },
    ))
}

async fn manufacturing_record_query(
    state: &AppState,
    request: &Request<manufacturing_wire::GetManufacturingRecordRequest>,
) -> Result<Response<manufacturing_wire::ManufacturingRecordResponse>, Status> {
    let body = request.get_ref();
    let context = body
        .context
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("query context is required"))?;
    let invocation = future_auth::query(
        state,
        request,
        Some(context),
        "manufacturing",
        MANUFACTURING_READ,
    )
    .await?;
    ensure_scope(&invocation, body.record_scope.as_ref())?;
    let pool = future_auth::pool(state, &invocation, RoutedDatabaseSide::Read).await?;
    let actor = manufacturing_actor(&invocation);
    let access = ManufacturingAccess {
        actor: actor.clone(),
        write: false,
    };
    let read = manufacturing_sqlx::SqlxManufacturingReadRepository::new(pool, invocation.tenant_id)
        .map_err(|_| Status::unavailable("manufacturing read model is unavailable"))?;
    let projection = manufacturing::application::ManufacturingQueryService::new(access, read)
        .get_record(
            &actor,
            &invocation.scope,
            command_uuid(&body.record_id, "record_id")?,
        )
        .await
        .map_err(manufacturing_status)?;
    Ok(Response::new(
        manufacturing_wire::ManufacturingRecordResponse {
            record_id: projection.record.id().to_string(),
            version: projection.record.version(),
            lifecycle_state: lifecycle_token(projection.record.status())?,
            projection_json_utf8: serde_json::to_vec(&projection.record)
                .map_err(|_| Status::internal("manufacturing projection serialization failed"))?,
            source_position: projection.source_position,
        },
    ))
}

#[tonic::async_trait]
impl manufacturing_wire::manufacturing_query_service_server::ManufacturingQueryService
    for ManufacturingQueryRpc
{
    async fn get_specification(
        &self,
        request: Request<manufacturing_wire::GetManufacturingRecordRequest>,
    ) -> Result<Response<manufacturing_wire::ManufacturingRecordResponse>, Status> {
        let body = request.get_ref();
        let invocation = future_auth::query(
            &self.state,
            &request,
            body.context.as_ref(),
            "manufacturing",
            MANUFACTURING_READ,
        )
        .await?;
        ensure_scope(&invocation, body.record_scope.as_ref())?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Read).await?;
        let actor = manufacturing_actor(&invocation);
        let access = ManufacturingAccess {
            actor: actor.clone(),
            write: false,
        };
        let read =
            manufacturing_sqlx::SqlxManufacturingReadRepository::new(pool, invocation.tenant_id)
                .map_err(|_| Status::unavailable("manufacturing read model is unavailable"))?;
        let projection = manufacturing::application::ManufacturingQueryService::new(access, read)
            .get_specification(
                &actor,
                &invocation.scope,
                manufacturing::SpecificationId::try_from_uuid(command_uuid(
                    &body.record_id,
                    "record_id",
                )?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
            )
            .await
            .map_err(manufacturing_status)?;
        enforce_aggregate_version(
            body.context.as_ref(),
            &body.record_id,
            projection.specification.reference().version,
        )?;
        Ok(Response::new(
            manufacturing_wire::ManufacturingRecordResponse {
                record_id: projection
                    .specification
                    .reference()
                    .specification_id
                    .as_uuid()
                    .to_string(),
                version: projection.specification.reference().version,
                lifecycle_state: lifecycle_token(projection.specification.status())?,
                projection_json_utf8: serde_json::to_vec(&projection.specification).map_err(
                    |_| Status::internal("manufacturing projection serialization failed"),
                )?,
                source_position: projection.source_position,
            },
        ))
    }

    async fn get_order(
        &self,
        request: Request<manufacturing_wire::GetManufacturingRecordRequest>,
    ) -> Result<Response<manufacturing_wire::ManufacturingRecordResponse>, Status> {
        manufacturing_order_query(&self.state, &request).await
    }

    async fn list_orders(
        &self,
        request: Request<manufacturing_wire::ListManufacturingRecordsRequest>,
    ) -> Result<Response<manufacturing_wire::ListManufacturingRecordsResponse>, Status> {
        let body = request.get_ref();
        let invocation = future_auth::query(
            &self.state,
            &request,
            body.context.as_ref(),
            "manufacturing",
            MANUFACTURING_READ,
        )
        .await?;
        ensure_scope(&invocation, body.record_scope.as_ref())?;
        Err(Status::failed_precondition(
            "Manufacturing order list read port is not available",
        ))
    }

    async fn get_genealogy(
        &self,
        request: Request<manufacturing_wire::GetManufacturingRecordRequest>,
    ) -> Result<Response<manufacturing_wire::ManufacturingRecordResponse>, Status> {
        manufacturing_order_query(&self.state, &request).await
    }

    async fn get_wip_cost(
        &self,
        request: Request<manufacturing_wire::GetManufacturingRecordRequest>,
    ) -> Result<Response<manufacturing_wire::ManufacturingRecordResponse>, Status> {
        manufacturing_order_query(&self.state, &request).await
    }

    async fn get_record(
        &self,
        request: Request<manufacturing_wire::GetManufacturingRecordRequest>,
    ) -> Result<Response<manufacturing_wire::ManufacturingRecordResponse>, Status> {
        manufacturing_record_query(&self.state, &request).await
    }
}

#[allow(clippy::too_many_arguments)]
async fn mutate_planning_recommendation<M, F>(
    state: &AppState,
    request: &Request<M>,
    context: &erpx_proto::common::v2::CommandContext,
    scope: Option<&erpx_proto::common::v2::TenantScope>,
    recommendation_id: &str,
    action: planning::PlanningAction,
    event_type: &'static str,
    change: F,
) -> Result<Response<planning_wire::MutationReceipt>, Status>
where
    M: Message,
    F: FnOnce(&mut planning::PlanningRecommendation) -> Result<(), planning::PlanningDomainError>,
{
    let invocation =
        future_auth::command(state, request, Some(context), "planning", PLANNING_WRITE).await?;
    ensure_scope(&invocation, scope)?;
    let pool = future_auth::pool(state, &invocation, RoutedDatabaseSide::Write).await?;
    let actor = planning_actor(&invocation);
    let access = PlanningAccess {
        actor: actor.clone(),
        write: true,
    };
    let service = planning::PlanningCommandService::new(
        access,
        planning_sqlx::SqlxPlanningWriteRepository::new(pool),
        RecommendationHandoffUnavailable,
    );
    let receipt = service
        .mutate_recommendation(
            &actor,
            planning_identity(context, request.get_ref())?,
            planning::RecommendationId::try_from_uuid(command_uuid(
                recommendation_id,
                "recommendation_id",
            )?)
            .map_err(|error| Status::invalid_argument(error.to_string()))?,
            &invocation.scope,
            (context.expected_aggregate_version > 0).then_some(context.expected_aggregate_version),
            correlation(&context.correlation_id)?,
            action,
            event_type,
            change,
        )
        .await
        .map_err(planning_status)?;
    Ok(Response::new(planning_receipt(receipt)))
}

#[tonic::async_trait]
impl planning_wire::planning_command_service_server::PlanningCommandService for PlanningCommandRpc {
    async fn create_scenario(
        &self,
        request: Request<planning_wire::CreateScenarioRequest>,
    ) -> Result<Response<planning_wire::MutationReceipt>, Status> {
        let body = request.get_ref();
        let context = required_command_context(body.context.as_ref())?;
        let invocation = future_auth::command(
            &self.state,
            &request,
            Some(context),
            "planning",
            PLANNING_WRITE,
        )
        .await?;
        ensure_scope(&invocation, body.scope.as_ref())?;
        if context.expected_aggregate_version != 0 {
            return Err(Status::failed_precondition(
                "create scenario cannot carry an expected version",
            ));
        }
        let modes = body
            .modes
            .iter()
            .map(|mode| match mode.as_str() {
                "mrp" => Ok(planning::PlanningMode::Mrp),
                "drp" => Ok(planning::PlanningMode::Drp),
                "capacity" => Ok(planning::PlanningMode::Capacity),
                _ => Err(Status::invalid_argument("planning mode is invalid")),
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        let inputs = body
            .input_pins
            .iter()
            .cloned()
            .map(|value| planning_version_ref(Some(value)))
            .collect::<Result<Vec<_>, _>>()?;
        let demands = body
            .demands
            .iter()
            .map(|value| {
                Ok(planning::DemandFact {
                    id: planning::DemandId::try_from_uuid(command_uuid(
                        &value.demand_id,
                        "demand_id",
                    )?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                    item: planning_reference(value.item.clone())?,
                    location: planning_reference(value.location.clone())?,
                    quantity: planning_quantity(value.quantity)?,
                    required_on: NaiveDate::parse_from_str(&value.required_on, "%Y-%m-%d")
                        .map_err(|_| Status::invalid_argument("required_on is invalid"))?,
                    source: planning_version_ref(value.source.clone())?,
                })
            })
            .collect::<Result<Vec<_>, Status>>()?;
        let supplies = body
            .supplies
            .iter()
            .map(|value| {
                Ok(planning::SupplyFact {
                    id: planning::SupplyId::try_from_uuid(command_uuid(
                        &value.supply_id,
                        "supply_id",
                    )?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                    item: planning_reference(value.item.clone())?,
                    location: planning_reference(value.location.clone())?,
                    quantity: planning_quantity(value.quantity)?,
                    available_on: NaiveDate::parse_from_str(&value.available_on, "%Y-%m-%d")
                        .map_err(|_| Status::invalid_argument("available_on is invalid"))?,
                    source: planning_version_ref(value.source.clone())?,
                })
            })
            .collect::<Result<Vec<_>, Status>>()?;
        let capacity = body
            .capacity
            .iter()
            .map(|value| {
                Ok(planning::CapacityBucket {
                    resource: planning_reference(value.resource.clone())?,
                    bucket: NaiveDate::parse_from_str(&value.bucket, "%Y-%m-%d")
                        .map_err(|_| Status::invalid_argument("capacity bucket is invalid"))?,
                    available_seconds: value.available_seconds,
                })
            })
            .collect::<Result<Vec<_>, Status>>()?;
        let capacity_requirements = body
            .capacity
            .iter()
            .map(|value| {
                Ok(planning::CapacityRequirement {
                    demand_id: planning::DemandId::try_from_uuid(command_uuid(
                        &value.demand_id,
                        "demand_id",
                    )?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                    resource: planning_reference(value.resource.clone())?,
                    bucket: NaiveDate::parse_from_str(&value.bucket, "%Y-%m-%d")
                        .map_err(|_| Status::invalid_argument("capacity bucket is invalid"))?,
                    required_seconds: value.required_seconds,
                })
            })
            .collect::<Result<Vec<_>, Status>>()?;
        let sourcing = body
            .sourcing
            .iter()
            .map(|value| {
                Ok(planning::SourcingRule {
                    item: planning_reference(value.item.clone())?,
                    kind: match value.kind.as_str() {
                        "buy" => planning::RecommendationKind::Buy,
                        "make" => planning::RecommendationKind::Make,
                        "transfer" => planning::RecommendationKind::Transfer,
                        _ => return Err(Status::invalid_argument("sourcing kind is invalid")),
                    },
                    source_location: value
                        .source_location
                        .clone()
                        .map(|value| planning_reference(Some(value)))
                        .transpose()?,
                    lead_time_days: u16::try_from(value.lead_time_days)
                        .map_err(|_| Status::invalid_argument("lead time is invalid"))?,
                })
            })
            .collect::<Result<Vec<_>, Status>>()?;
        let scenario = planning::PlanningScenario::draft(
            planning::ScenarioId::try_from_uuid(command_uuid(&body.scenario_id, "scenario_id")?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
            invocation.scope.clone(),
            modes,
            body.input_position,
            inputs,
            planning::PlanningPolicy {
                policy_id: command_uuid(&body.policy_id, "policy_id")?,
                version: body.policy_version,
                sourcing,
                allow_late_recommendations: false,
            },
            demands,
            supplies,
            capacity,
            capacity_requirements,
            Utc::now(),
        )
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let actor = planning_actor(&invocation);
        let access = PlanningAccess {
            actor: actor.clone(),
            write: true,
        };
        let service = planning::PlanningCommandService::new(
            access,
            planning_sqlx::SqlxPlanningWriteRepository::new(pool),
            RecommendationHandoffUnavailable,
        );
        let receipt = service
            .create_scenario(
                &actor,
                planning_identity(context, body)?,
                correlation(&context.correlation_id)?,
                scenario,
            )
            .await
            .map_err(planning_status)?;
        Ok(Response::new(planning_receipt(receipt)))
    }

    async fn freeze_scenario(
        &self,
        request: Request<planning_wire::ScenarioCommandRequest>,
    ) -> Result<Response<planning_wire::MutationReceipt>, Status> {
        let body = request.get_ref();
        let context = required_command_context(body.context.as_ref())?;
        let invocation = future_auth::command(
            &self.state,
            &request,
            Some(context),
            "planning",
            PLANNING_WRITE,
        )
        .await?;
        ensure_scope(&invocation, body.scenario_scope.as_ref())?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let actor = planning_actor(&invocation);
        let access = PlanningAccess {
            actor: actor.clone(),
            write: true,
        };
        let service = planning::PlanningCommandService::new(
            access,
            planning_sqlx::SqlxPlanningWriteRepository::new(pool),
            RecommendationHandoffUnavailable,
        );
        let receipt = service
            .mutate_scenario(
                &actor,
                planning_identity(context, body)?,
                planning::ScenarioId::try_from_uuid(command_uuid(
                    &body.scenario_id,
                    "scenario_id",
                )?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
                &invocation.scope,
                (context.expected_aggregate_version > 0)
                    .then_some(context.expected_aggregate_version),
                correlation(&context.correlation_id)?,
                planning::PlanningAction::FreezeScenario,
                "planning.scenario_frozen",
                |scenario| scenario.freeze(Utc::now()),
            )
            .await
            .map_err(planning_status)?;
        Ok(Response::new(planning_receipt(receipt)))
    }

    async fn run_scenario(
        &self,
        request: Request<planning_wire::RunScenarioRequest>,
    ) -> Result<Response<planning_wire::MutationReceipt>, Status> {
        let body = request.get_ref();
        let context = required_command_context(body.context.as_ref())?;
        let invocation = future_auth::command(
            &self.state,
            &request,
            Some(context),
            "planning",
            PLANNING_WRITE,
        )
        .await?;
        ensure_scope(&invocation, body.scenario_scope.as_ref())?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let actor = planning_actor(&invocation);
        let access = PlanningAccess {
            actor: actor.clone(),
            write: true,
        };
        let service = planning::PlanningCommandService::new(
            access,
            planning_sqlx::SqlxPlanningWriteRepository::new(pool),
            RecommendationHandoffUnavailable,
        );
        let receipt = service
            .run_scenario(
                &actor,
                planning_identity(context, body)?,
                planning::ScenarioId::try_from_uuid(command_uuid(
                    &body.scenario_id,
                    "scenario_id",
                )?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
                &invocation.scope,
                (context.expected_aggregate_version > 0)
                    .then_some(context.expected_aggregate_version),
                planning::RunId::try_from_uuid(command_uuid(&body.run_id, "run_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                correlation(&context.correlation_id)?,
                Utc::now(),
            )
            .await
            .map_err(planning_status)?;
        Ok(Response::new(planning_receipt(receipt)))
    }

    async fn override_recommendation(
        &self,
        request: Request<planning_wire::RecommendationOverrideRequest>,
    ) -> Result<Response<planning_wire::MutationReceipt>, Status> {
        let body = request.get_ref();
        let context = required_command_context(body.context.as_ref())?;
        let principal = future_auth::command(
            &self.state,
            &request,
            Some(context),
            "planning",
            PLANNING_WRITE,
        )
        .await?
        .principal_id
        .as_uuid();
        let value = planning::RecommendationOverride {
            id: planning::OverrideId::try_from_uuid(command_uuid(
                &body.override_id,
                "override_id",
            )?)
            .map_err(|error| Status::invalid_argument(error.to_string()))?,
            previous_quantity: planning_quantity(body.previous_quantity)?,
            revised_quantity: planning_quantity(body.revised_quantity)?,
            reason: body.reason.clone(),
            principal_id: principal,
            at: Utc::now(),
        };
        mutate_planning_recommendation(
            &self.state,
            &request,
            context,
            body.recommendation_scope.as_ref(),
            &body.recommendation_id,
            planning::PlanningAction::OverrideRecommendation,
            "planning.recommendation_overridden",
            move |recommendation| recommendation.override_quantity(value).map(|_| ()),
        )
        .await
    }

    async fn approve_recommendation(
        &self,
        request: Request<planning_wire::RecommendationCommandRequest>,
    ) -> Result<Response<planning_wire::MutationReceipt>, Status> {
        let body = request.get_ref();
        let context = required_command_context(body.context.as_ref())?;
        mutate_planning_recommendation(
            &self.state,
            &request,
            context,
            body.recommendation_scope.as_ref(),
            &body.recommendation_id,
            planning::PlanningAction::ApproveRecommendation,
            "planning.recommendation_approved",
            planning::PlanningRecommendation::approve,
        )
        .await
    }

    async fn reject_recommendation(
        &self,
        request: Request<planning_wire::RecommendationCommandRequest>,
    ) -> Result<Response<planning_wire::MutationReceipt>, Status> {
        let body = request.get_ref();
        let context = required_command_context(body.context.as_ref())?;
        mutate_planning_recommendation(
            &self.state,
            &request,
            context,
            body.recommendation_scope.as_ref(),
            &body.recommendation_id,
            planning::PlanningAction::RejectRecommendation,
            "planning.recommendation_rejected",
            planning::PlanningRecommendation::reject,
        )
        .await
    }

    async fn release_recommendation(
        &self,
        request: Request<planning_wire::RecommendationCommandRequest>,
    ) -> Result<Response<planning_wire::MutationReceipt>, Status> {
        let body = request.get_ref();
        let context = required_command_context(body.context.as_ref())?;
        let invocation = future_auth::command(
            &self.state,
            &request,
            Some(context),
            "planning",
            PLANNING_WRITE,
        )
        .await?;
        ensure_scope(&invocation, body.recommendation_scope.as_ref())?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Write).await?;
        let actor = planning_actor(&invocation);
        let access = PlanningAccess {
            actor: actor.clone(),
            write: true,
        };
        let service = planning::PlanningCommandService::new(
            access,
            planning_sqlx::SqlxPlanningWriteRepository::new(pool),
            RecommendationHandoffUnavailable,
        );
        let receipt = service
            .release_recommendation(
                &actor,
                planning_identity(context, body)?,
                planning::RecommendationId::try_from_uuid(command_uuid(
                    &body.recommendation_id,
                    "recommendation_id",
                )?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
                &invocation.scope,
                (context.expected_aggregate_version > 0)
                    .then_some(context.expected_aggregate_version),
                planning::ReleaseId::try_from_uuid(command_uuid(&body.release_id, "release_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                correlation(&context.correlation_id)?,
                Utc::now(),
            )
            .await
            .map_err(planning_status)?;
        Ok(Response::new(planning_receipt(receipt)))
    }
}

async fn planning_run_query(
    state: &AppState,
    request: &Request<planning_wire::GetPlanningRecordRequest>,
) -> Result<Response<planning_wire::PlanningRecordResponse>, Status> {
    let body = request.get_ref();
    let invocation = future_auth::query(
        state,
        request,
        body.context.as_ref(),
        "planning",
        PLANNING_READ,
    )
    .await?;
    ensure_scope(&invocation, body.record_scope.as_ref())?;
    let pool = future_auth::pool(state, &invocation, RoutedDatabaseSide::Read).await?;
    let actor = planning_actor(&invocation);
    let access = PlanningAccess {
        actor: actor.clone(),
        write: false,
    };
    let read = planning_sqlx::SqlxPlanningReadRepository::new(pool, invocation.tenant_id)
        .map_err(|_| Status::unavailable("planning read model is unavailable"))?;
    let projection = planning::application::PlanningQueryService::new(access, read)
        .get_run(
            &actor,
            &invocation.scope,
            planning::RunId::try_from_uuid(command_uuid(&body.record_id, "record_id")?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
        )
        .await
        .map_err(planning_status)?;
    Ok(Response::new(planning_wire::PlanningRecordResponse {
        record_id: projection.run.id.as_uuid().to_string(),
        version: 1,
        lifecycle_state: "completed".into(),
        projection_json_utf8: serde_json::to_vec(&projection.run)
            .map_err(|_| Status::internal("planning projection serialization failed"))?,
        source_position: projection.source_position,
    }))
}

async fn planning_plan_query(
    state: &AppState,
    request: &Request<planning_wire::GetPlanningRecordRequest>,
) -> Result<Response<planning_wire::PlanningRecordResponse>, Status> {
    let body = request.get_ref();
    let invocation = future_auth::query(
        state,
        request,
        body.context.as_ref(),
        "planning",
        PLANNING_READ,
    )
    .await?;
    ensure_scope(&invocation, body.record_scope.as_ref())?;
    let pool = future_auth::pool(state, &invocation, RoutedDatabaseSide::Read).await?;
    let actor = planning_actor(&invocation);
    let access = PlanningAccess {
        actor: actor.clone(),
        write: false,
    };
    let read = planning_sqlx::SqlxPlanningReadRepository::new(pool, invocation.tenant_id)
        .map_err(|_| Status::unavailable("planning read model is unavailable"))?;
    let projection = planning::application::PlanningQueryService::new(access, read)
        .get_plan(
            &actor,
            &invocation.scope,
            command_uuid(&body.record_id, "record_id")?,
        )
        .await
        .map_err(planning_status)?;
    Ok(Response::new(planning_wire::PlanningRecordResponse {
        record_id: projection.plan.id().to_string(),
        version: projection.plan.version(),
        lifecycle_state: lifecycle_token(projection.plan.status())?,
        projection_json_utf8: serde_json::to_vec(&projection.plan)
            .map_err(|_| Status::internal("planning projection serialization failed"))?,
        source_position: projection.source_position,
    }))
}

#[tonic::async_trait]
impl planning_wire::planning_query_service_server::PlanningQueryService for PlanningQueryRpc {
    async fn get_scenario(
        &self,
        request: Request<planning_wire::GetPlanningRecordRequest>,
    ) -> Result<Response<planning_wire::PlanningRecordResponse>, Status> {
        planning_governed_record_query(&self.state, &request, "scenario").await
    }

    async fn get_recommendation(
        &self,
        request: Request<planning_wire::GetPlanningRecordRequest>,
    ) -> Result<Response<planning_wire::PlanningRecordResponse>, Status> {
        planning_governed_record_query(&self.state, &request, "recommendation").await
    }

    async fn get_run(
        &self,
        request: Request<planning_wire::GetPlanningRecordRequest>,
    ) -> Result<Response<planning_wire::PlanningRecordResponse>, Status> {
        planning_run_query(&self.state, &request).await
    }

    async fn list_recommendations(
        &self,
        request: Request<planning_wire::ListPlanningRecordsRequest>,
    ) -> Result<Response<planning_wire::ListPlanningRecordsResponse>, Status> {
        planning_list_gap(&self.state, &request, "recommendation").await
    }

    async fn list_shortages(
        &self,
        request: Request<planning_wire::ListPlanningRecordsRequest>,
    ) -> Result<Response<planning_wire::ListPlanningRecordsResponse>, Status> {
        planning_list_gap(&self.state, &request, "shortage").await
    }

    async fn list_capacity_overloads(
        &self,
        request: Request<planning_wire::ListPlanningRecordsRequest>,
    ) -> Result<Response<planning_wire::ListPlanningRecordsResponse>, Status> {
        planning_list_gap(&self.state, &request, "capacity overload").await
    }

    async fn get_plan(
        &self,
        request: Request<planning_wire::GetPlanningRecordRequest>,
    ) -> Result<Response<planning_wire::PlanningRecordResponse>, Status> {
        planning_plan_query(&self.state, &request).await
    }
}

async fn planning_governed_record_query(
    state: &AppState,
    request: &Request<planning_wire::GetPlanningRecordRequest>,
    expected_kind: &'static str,
) -> Result<Response<planning_wire::PlanningRecordResponse>, Status> {
    let body = request.get_ref();
    let invocation = future_auth::query(
        state,
        request,
        body.context.as_ref(),
        "planning",
        PLANNING_READ,
    )
    .await?;
    ensure_scope(&invocation, body.record_scope.as_ref())?;
    let pool = future_auth::pool(state, &invocation, RoutedDatabaseSide::Read).await?;
    let actor = planning_actor(&invocation);
    let access = PlanningAccess {
        actor: actor.clone(),
        write: false,
    };
    let read = planning_sqlx::SqlxPlanningReadRepository::new(pool, invocation.tenant_id)
        .map_err(|_| Status::unavailable("planning read model is unavailable"))?;
    let projection = planning::application::PlanningQueryService::new(access, read)
        .get_record(
            &actor,
            &invocation.scope,
            command_uuid(&body.record_id, "record_id")?,
        )
        .await
        .map_err(planning_status)?;
    let (record_id, version, lifecycle_state, document, actual_kind) = match projection.record {
        planning::PlanningGovernedRecord::Scenario(value) => (
            value.id().as_uuid(),
            value.version(),
            lifecycle_token(value.status())?,
            serde_json::to_vec(&value),
            "scenario",
        ),
        planning::PlanningGovernedRecord::Recommendation(value) => (
            value.id.as_uuid(),
            value.version,
            lifecycle_token(value.status)?,
            serde_json::to_vec(&value),
            "recommendation",
        ),
    };
    if actual_kind != expected_kind {
        return Err(Status::not_found("Planning record was not found"));
    }
    enforce_aggregate_version(body.context.as_ref(), &body.record_id, version)?;
    Ok(Response::new(planning_wire::PlanningRecordResponse {
        record_id: record_id.to_string(),
        version,
        lifecycle_state,
        projection_json_utf8: document
            .map_err(|_| Status::internal("planning projection serialization failed"))?,
        source_position: projection.source_position,
    }))
}

async fn planning_list_gap(
    state: &AppState,
    request: &Request<planning_wire::ListPlanningRecordsRequest>,
    family: &'static str,
) -> Result<Response<planning_wire::ListPlanningRecordsResponse>, Status> {
    let body = request.get_ref();
    let invocation = future_auth::query(
        state,
        request,
        body.context.as_ref(),
        "planning",
        PLANNING_READ,
    )
    .await?;
    ensure_scope(&invocation, body.record_scope.as_ref())?;
    Err(Status::failed_precondition(format!(
        "Planning {family} list read port is not available"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_tokens_are_typed_not_labels_or_urls() {
        assert!(stable_reference("not-an-id", "inventory", "inventory.item").is_err());
        assert!(
            stable_reference(&Uuid::new_v4().to_string(), "unknown", "inventory.item").is_err()
        );
    }

    #[test]
    fn owner_handoff_is_explicitly_external_not_fake_success() {
        let source = include_str!("future_manufacturing_planning.rs");
        assert!(source.contains("RecommendationHandoffUnavailable"));
        assert!(source.contains("Err(planning::PortError::Unavailable)"));
    }

    #[test]
    fn aggregate_version_consistency_is_exact_and_fail_closed() {
        use erpx_proto::common::v2::{
            AggregateVersionConsistency, QueryConsistency, query_consistency::Requirement,
        };
        let record_id = Uuid::new_v4().to_string();
        let context = erpx_proto::common::v2::QueryContext {
            consistency: Some(QueryConsistency {
                requirement: Some(Requirement::AggregateVersion(AggregateVersionConsistency {
                    aggregate_type: "authorized-reference".into(),
                    aggregate_id: record_id.clone(),
                    version: 7,
                })),
            }),
            ..Default::default()
        };
        assert!(enforce_aggregate_version(Some(&context), &record_id, 7).is_ok());
        assert_eq!(
            enforce_aggregate_version(Some(&context), &record_id, 6)
                .expect_err("projection must not claim an unmet version")
                .code(),
            tonic::Code::FailedPrecondition
        );
        assert_eq!(
            enforce_aggregate_version(Some(&context), &Uuid::new_v4().to_string(), 7)
                .expect_err("cross-record consistency target")
                .code(),
            tonic::Code::InvalidArgument
        );
    }
}
