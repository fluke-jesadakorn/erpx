use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_operations::{
    ExceptionQueue, ExceptionStatus, ExceptionView, OperationsCommand, OperationsMutationResult,
    OperationsQuery, OperationsQueryResponse, OperationsSummary, WorkQueuePolicyList,
    WorkQueuePolicyStatus, WorkQueuePolicyView,
};
use erpx_ui_foundation::{CommandGateway, QueryGateway, UiError};

use super::{
    models::{
        OperationsExceptionItem, OperationsExceptionItems, OperationsSourceCountItem,
        OperationsSummaryItem, WorkQueuePolicyItem, WorkQueuePolicyItems,
    },
    presenter::OperationsContent,
};

pub fn present_mutation(value: &OperationsMutationResult) -> OperationsContent {
    match value {
        OperationsMutationResult::Exception(value) => {
            OperationsContent::Exception(value.clone().into())
        }
        OperationsMutationResult::WorkQueuePolicy(value) => {
            OperationsContent::WorkQueuePolicy(value.clone().into())
        }
    }
}

pub fn present_query(
    value: QueryResult<OperationsQueryResponse>,
) -> QueryResult<Option<OperationsContent>> {
    let metadata = value.metadata;
    let value = match value.value {
        OperationsQueryResponse::Exception(value) => value
            .map(OperationsExceptionItem::from)
            .map(OperationsContent::Exception),
        OperationsQueryResponse::Queue(value) => {
            let items = OperationsExceptionItems::from(value);
            (!items.exceptions.is_empty())
                .then_some(items)
                .map(OperationsContent::Exceptions)
        }
        OperationsQueryResponse::WorkQueuePolicies(value) => {
            let items = WorkQueuePolicyItems::from(value);
            (!items.policies.is_empty())
                .then_some(items)
                .map(OperationsContent::WorkQueuePolicies)
        }
        OperationsQueryResponse::Summary(value) => Some(OperationsContent::Summary(value.into())),
    };
    QueryResult { value, metadata }
}

impl From<ExceptionView> for OperationsExceptionItem {
    fn from(value: ExceptionView) -> Self {
        Self {
            exception_id: value.exception_id.as_uuid().to_string(),
            version: value.version,
            reference: value.reference,
            scope: format!("{:?}", value.scope),
            source_context: format!("{:?}", value.source.context).to_ascii_lowercase(),
            source_event_id: value.source.event_id.to_string(),
            source_event_type: value.source.event_type,
            policy_ref: value.policy_ref,
            category: value.category,
            severity: value.severity,
            assignee_id: value.assignee.as_uuid().to_string(),
            manager_id: value.manager.as_uuid().to_string(),
            due_at: value.due_at.to_rfc3339(),
            status: exception_status(value.status).into(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl From<ExceptionQueue> for OperationsExceptionItems {
    fn from(value: ExceptionQueue) -> Self {
        Self {
            exceptions: value.exceptions.into_iter().map(Into::into).collect(),
            next_cursor: value.next_cursor,
        }
    }
}

impl From<WorkQueuePolicyView> for WorkQueuePolicyItem {
    fn from(value: WorkQueuePolicyView) -> Self {
        Self {
            policy_id: value.policy_id.as_uuid().to_string(),
            version: value.version,
            scope: format!("{:?}", value.scope),
            name: value.name,
            allowed_sources: value
                .allowed_sources
                .into_iter()
                .map(|value| format!("{value:?}").to_ascii_lowercase())
                .collect(),
            escalation_after_minutes: value.escalation_after_minutes,
            status: policy_status(value.status).into(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl From<WorkQueuePolicyList> for WorkQueuePolicyItems {
    fn from(value: WorkQueuePolicyList) -> Self {
        Self {
            policies: value.policies.into_iter().map(Into::into).collect(),
            next_cursor: value.next_cursor,
        }
    }
}

impl From<OperationsSummary> for OperationsSummaryItem {
    fn from(value: OperationsSummary) -> Self {
        Self {
            read_row_count: value.read_row_count,
            exception_count: value.exception_count,
            work_queue_policy_count: value.work_queue_policy_count,
            event_count: value.event_count,
            source_reference_count: value.source_reference_count,
            source_counts: value
                .source_counts
                .into_iter()
                .map(|value| OperationsSourceCountItem {
                    source_context: format!("{:?}", value.source_context).to_ascii_lowercase(),
                    reference_count: value.reference_count,
                })
                .collect(),
            latest_event_at: value.latest_event_at.map(|value| value.to_rfc3339()),
        }
    }
}

const fn exception_status(value: ExceptionStatus) -> &'static str {
    match value {
        ExceptionStatus::Assigned => "assigned",
        ExceptionStatus::Acknowledged => "acknowledged",
        ExceptionStatus::Escalated => "escalated",
        ExceptionStatus::Resolved => "resolved",
        ExceptionStatus::Closed => "closed",
    }
}
const fn policy_status(value: WorkQueuePolicyStatus) -> &'static str {
    match value {
        WorkQueuePolicyStatus::Active => "active",
        WorkQueuePolicyStatus::Paused => "paused",
        WorkQueuePolicyStatus::Retired => "retired",
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait OperationsProtoTransport: Send + Sync {
    async fn command(
        &self,
        envelope: CommandEnvelope<OperationsCommand>,
    ) -> Result<CommandReceipt<OperationsMutationResult>, UiError>;
    async fn query(
        &self,
        envelope: QueryEnvelope<OperationsQuery>,
    ) -> Result<QueryResult<OperationsQueryResponse>, UiError>;
}

#[derive(Debug, Clone)]
pub struct OperationsGateway<T> {
    transport: T,
}
impl<T> OperationsGateway<T> {
    pub const fn new(transport: T) -> Self {
        Self { transport }
    }
    pub const fn transport(&self) -> &T {
        &self.transport
    }
    pub fn into_transport(self) -> T {
        self.transport
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T: OperationsProtoTransport> CommandGateway<OperationsCommand> for OperationsGateway<T> {
    type Response = OperationsMutationResult;
    async fn execute(
        &self,
        envelope: CommandEnvelope<OperationsCommand>,
    ) -> Result<CommandReceipt<Self::Response>, UiError> {
        self.transport.command(envelope).await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T: OperationsProtoTransport> QueryGateway<OperationsQuery> for OperationsGateway<T> {
    type Response = OperationsQueryResponse;
    async fn execute(
        &self,
        envelope: QueryEnvelope<OperationsQuery>,
    ) -> Result<QueryResult<Self::Response>, UiError> {
        self.transport.query(envelope).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake;
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    impl OperationsProtoTransport for Fake {
        async fn command(
            &self,
            _: CommandEnvelope<OperationsCommand>,
        ) -> Result<CommandReceipt<OperationsMutationResult>, UiError> {
            unreachable!()
        }
        async fn query(
            &self,
            _: QueryEnvelope<OperationsQuery>,
        ) -> Result<QueryResult<OperationsQueryResponse>, UiError> {
            unreachable!()
        }
    }

    #[test]
    fn gateway_is_typed_in_both_directions() {
        fn command<G: CommandGateway<OperationsCommand, Response = OperationsMutationResult>>() {}
        fn query<G: QueryGateway<OperationsQuery, Response = OperationsQueryResponse>>() {}
        command::<OperationsGateway<Fake>>();
        query::<OperationsGateway<Fake>>();
    }

    #[test]
    fn only_gateway_imports_operations_core_or_transport_contracts() {
        let source = include_str!("gateway.rs");
        assert!(source.contains(concat!("erpx_", "operations")));
        for source in [
            include_str!("models.rs"),
            include_str!("presenter.rs"),
            include_str!("view.rs"),
        ] {
            assert!(!source.contains(concat!("erpx_", "operations")));
            assert!(!source.contains(concat!("erpx_", "proto")));
            assert!(!source.contains(concat!("tonic", "::")));
        }
    }
}
