use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_integration_contracts::{BoundedContext, identity_access, operations};
use erpx_kernel::{AggregateId, CorrelationId, EventId};
use erpx_operations::{
    ExceptionId, ExceptionQueue, ExceptionStatus, ExceptionView, OperationsCommand,
    OperationsMutationResult, OperationsQuery, OperationsQueryResponse, OperationsSourceCount,
    OperationsSummary, SourceReference, WorkQueuePolicyId, WorkQueuePolicyList,
    WorkQueuePolicyStatus, WorkQueuePolicyView,
};
use erpx_proto::erpx::operations::v1 as wire;
use erpx_ui_foundation::UiError;

use crate::{
    features::operations::gateway::OperationsProtoTransport, transport::typed_cqrs::GrpcChannel,
};

use super::common::{
    AuthenticatedRoute, Wave9Route, command_result, decode_scope, invalid_response, query_result,
    status_ui, timestamp, uuid,
};

type CommandClient =
    wire::operations_command_service_client::OperationsCommandServiceClient<GrpcChannel>;
type QueryClient = wire::operations_query_service_client::OperationsQueryServiceClient<GrpcChannel>;

#[derive(Clone)]
pub(crate) struct OperationsGrpcTransport {
    auth: AuthenticatedRoute,
    commands: CommandClient,
    queries: QueryClient,
}

impl OperationsGrpcTransport {
    pub(crate) fn try_new(
        channel: GrpcChannel,
        access_token: impl Into<String>,
        route: Wave9Route,
    ) -> Result<Self, UiError> {
        Ok(Self {
            auth: AuthenticatedRoute::try_new(access_token, route)?,
            commands: CommandClient::new(channel.clone()),
            queries: QueryClient::new(channel),
        })
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl OperationsProtoTransport for OperationsGrpcTransport {
    async fn command(
        &self,
        envelope: CommandEnvelope<OperationsCommand>,
    ) -> Result<CommandReceipt<OperationsMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let expected_command = envelope.command_id;
        let expected_correlation = envelope.correlation_id;
        let mut client = self.commands.clone();
        match envelope.command {
            OperationsCommand::Reassign(value) => {
                let request = wire::ReassignOperationsExceptionRequest {
                    metadata: Some(metadata),
                    exception_id: value.exception_id.as_uuid().to_string(),
                    assignee_id: value.assignee.as_uuid().to_string(),
                    manager_id: value.manager.as_uuid().to_string(),
                };
                let response = client
                    .reassign_exception(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("reassign Operations exception", status))?
                    .into_inner();
                exception_command(response, expected_command, expected_correlation)
            }
            OperationsCommand::Acknowledge(value) => {
                let request = wire::AcknowledgeOperationsExceptionRequest {
                    metadata: Some(metadata),
                    exception_id: value.exception_id.as_uuid().to_string(),
                };
                let response = client
                    .acknowledge_exception(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("acknowledge Operations exception", status))?
                    .into_inner();
                exception_command(response, expected_command, expected_correlation)
            }
            OperationsCommand::Escalate(value) => {
                let request = wire::EscalateOperationsExceptionRequest {
                    metadata: Some(metadata),
                    exception_id: value.exception_id.as_uuid().to_string(),
                    manager_id: value.manager.as_uuid().to_string(),
                };
                let response = client
                    .escalate_exception(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("escalate Operations exception", status))?
                    .into_inner();
                exception_command(response, expected_command, expected_correlation)
            }
            OperationsCommand::DecideApproval(value) => {
                let request = wire::DecideOperationsApprovalRequest {
                    metadata: Some(metadata),
                    exception_id: value.exception_id.as_uuid().to_string(),
                    decision: encode_approval_decision(value.decision),
                    reason_code: value.reason_code,
                };
                let response = client
                    .decide_approval(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("decide Operations approval", status))?
                    .into_inner();
                exception_command(response, expected_command, expected_correlation)
            }
            OperationsCommand::Resolve(value) => {
                let request = wire::ResolveOperationsExceptionRequest {
                    metadata: Some(metadata),
                    exception_id: value.exception_id.as_uuid().to_string(),
                };
                let response = client
                    .resolve_exception(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("resolve Operations exception", status))?
                    .into_inner();
                exception_command(response, expected_command, expected_correlation)
            }
            OperationsCommand::Close(value) => {
                let request = wire::CloseOperationsExceptionRequest {
                    metadata: Some(metadata),
                    exception_id: value.exception_id.as_uuid().to_string(),
                };
                let response = client
                    .close_exception(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("close Operations exception", status))?
                    .into_inner();
                exception_command(response, expected_command, expected_correlation)
            }
            OperationsCommand::PublishWorkQueuePolicy(value) => {
                let allowed_sources = value
                    .allowed_sources
                    .into_iter()
                    .map(encode_source)
                    .collect::<Result<_, _>>()?;
                let request = wire::PublishOperationsWorkQueuePolicyCqrsRequest {
                    metadata: Some(metadata),
                    policy_id: value.policy_id.as_uuid().to_string(),
                    name: value.name,
                    allowed_sources,
                    escalation_after_minutes: value.escalation_after_minutes,
                };
                let response = client
                    .publish_work_queue_policy(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("publish Operations work-queue policy", status))?
                    .into_inner();
                let policy = decode_policy(
                    response
                        .policy
                        .ok_or_else(|| invalid_response("decode Operations policy"))?,
                )?;
                command_result(
                    response.receipt,
                    OperationsMutationResult::WorkQueuePolicy(policy),
                    expected_command,
                    expected_correlation,
                )
            }
        }
    }

    async fn query(
        &self,
        envelope: QueryEnvelope<OperationsQuery>,
    ) -> Result<QueryResult<OperationsQueryResponse>, UiError> {
        let metadata = self.auth.query(&envelope)?;
        let actor = envelope.actor.clone();
        let expected_correlation = envelope.correlation_id;
        let consistency = envelope.consistency.clone();
        let mut client = self.queries.clone();
        match envelope.query {
            OperationsQuery::GetException(value) => {
                let request = wire::GetOperationsExceptionRequest {
                    metadata: Some(metadata),
                    exception_id: value.exception_id.as_uuid().to_string(),
                };
                let response = client
                    .get_exception(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("get Operations exception", status))?
                    .into_inner();
                let exception = response.exception.map(decode_exception).transpose()?;
                query_result(
                    response.metadata,
                    OperationsQueryResponse::Exception(exception),
                    expected_correlation,
                    consistency,
                )
            }
            OperationsQuery::ListQueue(value) => {
                let source_context = value.filter.source_context.map(encode_source).transpose()?;
                let request = wire::ListOperationsExceptionQueueRequest {
                    metadata: Some(metadata),
                    statuses: value
                        .filter
                        .statuses
                        .into_iter()
                        .map(encode_status)
                        .collect(),
                    source_context,
                    assignee_id: value
                        .filter
                        .assignee
                        .map(|value| value.as_uuid().to_string()),
                    due_before: value.filter.due_before.map(|value| value.to_rfc3339()),
                    page_size: u32::from(value.filter.page_size),
                    cursor: value.filter.cursor,
                };
                let response = client
                    .list_exception_queue(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("list Operations exception queue", status))?
                    .into_inner();
                let exceptions = response
                    .exceptions
                    .into_iter()
                    .map(decode_exception)
                    .collect::<Result<_, _>>()?;
                query_result(
                    response.metadata,
                    OperationsQueryResponse::Queue(ExceptionQueue {
                        exceptions,
                        next_cursor: response.next_cursor,
                    }),
                    expected_correlation,
                    consistency,
                )
            }
            OperationsQuery::ListWorkQueuePolicies(value) => {
                let request = wire::ListOperationsWorkQueuePoliciesCqrsRequest {
                    metadata: Some(metadata),
                    page_size: u32::from(value.filter.page_size),
                    cursor: value.filter.cursor,
                };
                let response = client
                    .list_work_queue_policies(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("list Operations work-queue policies", status))?
                    .into_inner();
                let policies = response
                    .policies
                    .into_iter()
                    .map(decode_policy)
                    .collect::<Result<_, _>>()?;
                query_result(
                    response.metadata,
                    OperationsQueryResponse::WorkQueuePolicies(WorkQueuePolicyList {
                        policies,
                        next_cursor: response.next_cursor,
                    }),
                    expected_correlation,
                    consistency,
                )
            }
            OperationsQuery::GetSummary(_) => {
                let request = wire::GetOperationsSummaryCqrsRequest {
                    metadata: Some(metadata),
                };
                let response = client
                    .get_summary(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("get Operations summary", status))?
                    .into_inner();
                let summary = decode_summary(
                    response
                        .summary
                        .ok_or_else(|| invalid_response("decode Operations summary"))?,
                )?;
                query_result(
                    response.metadata,
                    OperationsQueryResponse::Summary(summary),
                    expected_correlation,
                    consistency,
                )
            }
        }
    }
}

fn exception_command(
    response: wire::OperationsExceptionMutationResponse,
    command_id: erpx_kernel::CommandId,
    correlation_id: CorrelationId,
) -> Result<CommandReceipt<OperationsMutationResult>, UiError> {
    let exception = decode_exception(
        response
            .exception
            .ok_or_else(|| invalid_response("decode Operations exception"))?,
    )?;
    command_result(
        response.receipt,
        OperationsMutationResult::Exception(exception),
        command_id,
        correlation_id,
    )
}

fn decode_exception(value: wire::OperationsExceptionRecord) -> Result<ExceptionView, UiError> {
    let source = value
        .source
        .ok_or_else(|| invalid_response("decode Operations source"))?;
    Ok(ExceptionView {
        exception_id: ExceptionId::try_from_uuid(uuid(&value.exception_id)?)
            .map_err(|_| invalid_response("decode Operations exception"))?,
        version: value.aggregate_version,
        reference: value.reference,
        scope: decode_scope(value.scope)?,
        source: SourceReference {
            context: decode_source(source.context)?,
            event_id: EventId::try_from_uuid(uuid(&source.event_id)?)
                .map_err(|_| invalid_response("decode Operations source"))?,
            event_type: source.event_type,
            aggregate_kind: source.aggregate_type,
            aggregate_id: AggregateId::try_from_uuid(uuid(&source.aggregate_id)?)
                .map_err(|_| invalid_response("decode Operations source"))?,
            aggregate_version: source.aggregate_version,
            correlation_id: CorrelationId::try_from_uuid(uuid(&source.correlation_id)?)
                .map_err(|_| invalid_response("decode Operations source"))?,
            occurred_at: timestamp(&source.occurred_at)?,
        },
        policy_ref: value.policy_ref,
        category: value.category,
        severity: value.severity,
        assignee: identity_access::PrincipalRef::try_from_uuid(uuid(&value.assignee_id)?)
            .map_err(|_| invalid_response("decode Operations assignee"))?,
        manager: identity_access::PrincipalRef::try_from_uuid(uuid(&value.manager_id)?)
            .map_err(|_| invalid_response("decode Operations manager"))?,
        due_at: timestamp(&value.due_at)?,
        escalation_after_minutes: value.escalation_after_minutes,
        status: decode_status(value.status)?,
        created_at: timestamp(&value.created_at)?,
        updated_at: timestamp(&value.updated_at)?,
    })
}

fn decode_policy(
    value: wire::OperationsWorkQueuePolicyRecord,
) -> Result<WorkQueuePolicyView, UiError> {
    Ok(WorkQueuePolicyView {
        policy_id: WorkQueuePolicyId::try_from_uuid(uuid(&value.policy_id)?)
            .map_err(|_| invalid_response("decode Operations policy"))?,
        version: value.aggregate_version,
        scope: decode_scope(value.scope)?,
        name: value.name,
        allowed_sources: value
            .allowed_sources
            .into_iter()
            .map(decode_source)
            .collect::<Result<_, _>>()?,
        escalation_after_minutes: value.escalation_after_minutes,
        status: match wire::OperationsWorkQueuePolicyStatus::try_from(value.status).ok() {
            Some(wire::OperationsWorkQueuePolicyStatus::Active) => WorkQueuePolicyStatus::Active,
            Some(wire::OperationsWorkQueuePolicyStatus::Paused) => WorkQueuePolicyStatus::Paused,
            Some(wire::OperationsWorkQueuePolicyStatus::Retired) => WorkQueuePolicyStatus::Retired,
            _ => return Err(invalid_response("decode Operations policy status")),
        },
        created_at: timestamp(&value.created_at)?,
        updated_at: timestamp(&value.updated_at)?,
    })
}

fn decode_summary(value: wire::OperationsSummaryRecord) -> Result<OperationsSummary, UiError> {
    Ok(OperationsSummary {
        read_row_count: value.read_row_count,
        exception_count: value.exception_count,
        work_queue_policy_count: value.work_queue_policy_count,
        event_count: value.event_count,
        source_reference_count: value.source_reference_count,
        source_counts: value
            .source_counts
            .into_iter()
            .map(|value| {
                Ok(OperationsSourceCount {
                    source_context: decode_source(value.source_context)?,
                    reference_count: value.reference_count,
                })
            })
            .collect::<Result<_, UiError>>()?,
        latest_event_at: value
            .latest_event_at
            .map(|value| timestamp(&value))
            .transpose()?,
    })
}

fn encode_approval_decision(value: operations::ApprovalDecision) -> i32 {
    match value {
        operations::ApprovalDecision::Approved => wire::OperationsApprovalDecision::Approved as i32,
        operations::ApprovalDecision::Rejected => wire::OperationsApprovalDecision::Rejected as i32,
        operations::ApprovalDecision::Cancelled => {
            wire::OperationsApprovalDecision::Cancelled as i32
        }
    }
}

fn encode_status(value: ExceptionStatus) -> i32 {
    match value {
        ExceptionStatus::Assigned => wire::OperationsExceptionStatus::Assigned as i32,
        ExceptionStatus::Acknowledged => wire::OperationsExceptionStatus::Acknowledged as i32,
        ExceptionStatus::Escalated => wire::OperationsExceptionStatus::Escalated as i32,
        ExceptionStatus::Resolved => wire::OperationsExceptionStatus::Resolved as i32,
        ExceptionStatus::Closed => wire::OperationsExceptionStatus::Closed as i32,
    }
}

fn decode_status(value: i32) -> Result<ExceptionStatus, UiError> {
    match wire::OperationsExceptionStatus::try_from(value).ok() {
        Some(wire::OperationsExceptionStatus::Assigned) => Ok(ExceptionStatus::Assigned),
        Some(wire::OperationsExceptionStatus::Acknowledged) => Ok(ExceptionStatus::Acknowledged),
        Some(wire::OperationsExceptionStatus::Escalated) => Ok(ExceptionStatus::Escalated),
        Some(wire::OperationsExceptionStatus::Resolved) => Ok(ExceptionStatus::Resolved),
        Some(wire::OperationsExceptionStatus::Closed) => Ok(ExceptionStatus::Closed),
        _ => Err(invalid_response("decode Operations exception status")),
    }
}

fn encode_source(value: BoundedContext) -> Result<i32, UiError> {
    Ok(match value {
        BoundedContext::Commercial => wire::OperationsSourceContext::Commercial as i32,
        BoundedContext::Customer => wire::OperationsSourceContext::Customer as i32,
        BoundedContext::Marketing => wire::OperationsSourceContext::Marketing as i32,
        BoundedContext::Crm => wire::OperationsSourceContext::Crm as i32,
        BoundedContext::Procurement => wire::OperationsSourceContext::Procurement as i32,
        BoundedContext::Inventory => wire::OperationsSourceContext::Inventory as i32,
        BoundedContext::Assets => wire::OperationsSourceContext::Assets as i32,
        BoundedContext::Maintenance => wire::OperationsSourceContext::Maintenance as i32,
        BoundedContext::Quality => wire::OperationsSourceContext::Quality as i32,
        BoundedContext::Sales => wire::OperationsSourceContext::Sales as i32,
        BoundedContext::PeoplePayroll => wire::OperationsSourceContext::PeoplePayroll as i32,
        BoundedContext::Pos => wire::OperationsSourceContext::Pos as i32,
        BoundedContext::Finance => wire::OperationsSourceContext::Finance as i32,
        BoundedContext::Support => wire::OperationsSourceContext::Support as i32,
        BoundedContext::IdentityAccess
        | BoundedContext::Organization
        | BoundedContext::PlatformAdministration
        | BoundedContext::Brand
        | BoundedContext::Legal
        | BoundedContext::Operations => {
            return Err(erpx_ui_foundation::UiError::invalid(
                "This source context is not supported by Operations.",
            ));
        }
    })
}

fn decode_source(value: i32) -> Result<BoundedContext, UiError> {
    match wire::OperationsSourceContext::try_from(value).ok() {
        Some(wire::OperationsSourceContext::Commercial) => Ok(BoundedContext::Commercial),
        Some(wire::OperationsSourceContext::Customer) => Ok(BoundedContext::Customer),
        Some(wire::OperationsSourceContext::Marketing) => Ok(BoundedContext::Marketing),
        Some(wire::OperationsSourceContext::Crm) => Ok(BoundedContext::Crm),
        Some(wire::OperationsSourceContext::Procurement) => Ok(BoundedContext::Procurement),
        Some(wire::OperationsSourceContext::Inventory) => Ok(BoundedContext::Inventory),
        Some(wire::OperationsSourceContext::Assets) => Ok(BoundedContext::Assets),
        Some(wire::OperationsSourceContext::Maintenance) => Ok(BoundedContext::Maintenance),
        Some(wire::OperationsSourceContext::Quality) => Ok(BoundedContext::Quality),
        Some(wire::OperationsSourceContext::Sales) => Ok(BoundedContext::Sales),
        Some(wire::OperationsSourceContext::PeoplePayroll) => Ok(BoundedContext::PeoplePayroll),
        Some(wire::OperationsSourceContext::Pos) => Ok(BoundedContext::Pos),
        Some(wire::OperationsSourceContext::Finance) => Ok(BoundedContext::Finance),
        Some(wire::OperationsSourceContext::Support) => Ok(BoundedContext::Support),
        _ => Err(invalid_response("decode Operations source context")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_and_unspecified_sources_fail_closed() {
        assert!(encode_source(BoundedContext::Operations).is_err());
        assert!(decode_source(0).is_err());
    }

    #[test]
    fn unspecified_status_is_not_a_domain_status() {
        assert!(decode_status(0).is_err());
    }
}
