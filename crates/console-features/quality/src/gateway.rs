use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_quality::{
    CancelInspection, CreateInspection, CreateInspectionResult, GetInspection, GetInspectionResult,
    InspectionList, InspectionMutationResult, ListInspections, RecordInspectionResult,
    ReleaseInspection, StartInspection,
};
use erpx_ui_foundation::{CommandGateway, QueryGateway, UiError};

/// Typed boundary implemented by the target-specific protobuf/Tonic adapter.
///
/// Generated transport messages are intentionally absent from this contract:
/// the adapter converts them at this boundary and neither presenters nor views
/// can observe them.
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait QualityProtoTransport: Send + Sync {
    async fn create(
        &self,
        envelope: CommandEnvelope<CreateInspection>,
    ) -> Result<CommandReceipt<CreateInspectionResult>, UiError>;

    async fn start(
        &self,
        envelope: CommandEnvelope<StartInspection>,
    ) -> Result<CommandReceipt<InspectionMutationResult>, UiError>;

    async fn record_result(
        &self,
        envelope: CommandEnvelope<RecordInspectionResult>,
    ) -> Result<CommandReceipt<InspectionMutationResult>, UiError>;

    async fn release(
        &self,
        envelope: CommandEnvelope<ReleaseInspection>,
    ) -> Result<CommandReceipt<InspectionMutationResult>, UiError>;

    async fn cancel(
        &self,
        envelope: CommandEnvelope<CancelInspection>,
    ) -> Result<CommandReceipt<InspectionMutationResult>, UiError>;

    async fn get(
        &self,
        envelope: QueryEnvelope<GetInspection>,
    ) -> Result<QueryResult<GetInspectionResult>, UiError>;

    async fn list(
        &self,
        envelope: QueryEnvelope<ListInspections>,
    ) -> Result<QueryResult<InspectionList>, UiError>;
}

/// Quality gateway exposed to controllers and read-your-writes coordinators.
///
/// One transport instance serves every typed Quality operation while the
/// `CommandGateway`/`QueryGateway` implementations keep each call statically
/// paired with its response.
#[derive(Debug, Clone)]
pub struct QualityGateway<T> {
    transport: T,
}

impl<T> QualityGateway<T> {
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

macro_rules! command_gateway {
    ($command:ty, $response:ty, $method:ident) => {
        #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
        #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
        impl<T> CommandGateway<$command> for QualityGateway<T>
        where
            T: QualityProtoTransport,
        {
            type Response = $response;

            async fn execute(
                &self,
                envelope: CommandEnvelope<$command>,
            ) -> Result<CommandReceipt<Self::Response>, UiError> {
                self.transport.$method(envelope).await
            }
        }
    };
}

command_gateway!(CreateInspection, CreateInspectionResult, create);
command_gateway!(StartInspection, InspectionMutationResult, start);
command_gateway!(
    RecordInspectionResult,
    InspectionMutationResult,
    record_result
);
command_gateway!(ReleaseInspection, InspectionMutationResult, release);
command_gateway!(CancelInspection, InspectionMutationResult, cancel);

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T> QueryGateway<GetInspection> for QualityGateway<T>
where
    T: QualityProtoTransport,
{
    type Response = GetInspectionResult;

    async fn execute(
        &self,
        envelope: QueryEnvelope<GetInspection>,
    ) -> Result<QueryResult<Self::Response>, UiError> {
        self.transport.get(envelope).await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T> QueryGateway<ListInspections> for QualityGateway<T>
where
    T: QualityProtoTransport,
{
    type Response = InspectionList;

    async fn execute(
        &self,
        envelope: QueryEnvelope<ListInspections>,
    ) -> Result<QueryResult<Self::Response>, UiError> {
        self.transport.list(envelope).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ContractOnlyTransport;

    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    impl QualityProtoTransport for ContractOnlyTransport {
        async fn create(
            &self,
            _: CommandEnvelope<CreateInspection>,
        ) -> Result<CommandReceipt<CreateInspectionResult>, UiError> {
            unreachable!("compile-time contract test")
        }

        async fn start(
            &self,
            _: CommandEnvelope<StartInspection>,
        ) -> Result<CommandReceipt<InspectionMutationResult>, UiError> {
            unreachable!("compile-time contract test")
        }

        async fn record_result(
            &self,
            _: CommandEnvelope<RecordInspectionResult>,
        ) -> Result<CommandReceipt<InspectionMutationResult>, UiError> {
            unreachable!("compile-time contract test")
        }

        async fn release(
            &self,
            _: CommandEnvelope<ReleaseInspection>,
        ) -> Result<CommandReceipt<InspectionMutationResult>, UiError> {
            unreachable!("compile-time contract test")
        }

        async fn cancel(
            &self,
            _: CommandEnvelope<CancelInspection>,
        ) -> Result<CommandReceipt<InspectionMutationResult>, UiError> {
            unreachable!("compile-time contract test")
        }

        async fn get(
            &self,
            _: QueryEnvelope<GetInspection>,
        ) -> Result<QueryResult<GetInspectionResult>, UiError> {
            unreachable!("compile-time contract test")
        }

        async fn list(
            &self,
            _: QueryEnvelope<ListInspections>,
        ) -> Result<QueryResult<InspectionList>, UiError> {
            unreachable!("compile-time contract test")
        }
    }

    #[test]
    fn gateway_implements_every_typed_quality_operation() {
        fn command<C, R, G>()
        where
            G: CommandGateway<C, Response = R>,
        {
        }

        fn query<Q, R, G>()
        where
            G: QueryGateway<Q, Response = R>,
        {
        }

        type Gateway = QualityGateway<ContractOnlyTransport>;
        command::<CreateInspection, CreateInspectionResult, Gateway>();
        command::<StartInspection, InspectionMutationResult, Gateway>();
        command::<RecordInspectionResult, InspectionMutationResult, Gateway>();
        command::<ReleaseInspection, InspectionMutationResult, Gateway>();
        command::<CancelInspection, InspectionMutationResult, Gateway>();
        query::<GetInspection, GetInspectionResult, Gateway>();
        query::<ListInspections, InspectionList, Gateway>();
    }
}
