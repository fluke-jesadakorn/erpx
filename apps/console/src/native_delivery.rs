//! Direct Native gRPC delivery adapter.

use async_trait::async_trait;
use erpx_delivery_contract::{
    AuthenticationRequest, BootstrapRequest, CollaborationEvent, CollaborationSubscription,
    CommandReceipt, CommandRequest, ConsoleGateway, DeliveryError, PublicMarketingFormRequest,
    PublicMarketingPreviewRequest, QueryRequest, RouteData, SessionBootstrap,
};

const NATIVE_GRPC_ENDPOINT_ENV: &str = "ERPX_NATIVE_GRPC_ENDPOINT";

/// Native owns a Tonic channel and never traverses the browser BFF or a
/// WebView. Operation-specific generated clients are selected by the
/// platform composition as their capabilities become available.
#[derive(Clone, Debug)]
pub struct NativeGrpcGateway {
    channel: tonic::transport::Channel,
}

impl NativeGrpcGateway {
    pub fn from_environment() -> Result<Self, DeliveryError> {
        let endpoint = std::env::var(NATIVE_GRPC_ENDPOINT_ENV)
            .map_err(|_| DeliveryError::unavailable(None))?;
        Self::connect_lazy(&endpoint)
    }

    pub fn connect_lazy(endpoint: &str) -> Result<Self, DeliveryError> {
        let endpoint = tonic::transport::Endpoint::from_shared(endpoint.to_owned())
            .map_err(|_| DeliveryError::validation("native gRPC endpoint"))?;
        if endpoint.uri().scheme_str() != Some("https") {
            return Err(DeliveryError::validation(
                "native gRPC endpoint requires TLS",
            ));
        }
        Ok(Self {
            channel: endpoint.connect_lazy(),
        })
    }

    pub fn channel(&self) -> tonic::transport::Channel {
        self.channel.clone()
    }
}

#[async_trait(?Send)]
impl ConsoleGateway for NativeGrpcGateway {
    async fn bootstrap(
        &self,
        _request: BootstrapRequest,
    ) -> Result<SessionBootstrap, DeliveryError> {
        Err(DeliveryError::unavailable(Some(500)))
    }

    async fn query(&self, _request: QueryRequest) -> Result<RouteData, DeliveryError> {
        Err(DeliveryError::unavailable(Some(500)))
    }

    async fn command(&self, _request: CommandRequest) -> Result<CommandReceipt, DeliveryError> {
        Err(DeliveryError::unavailable(Some(500)))
    }

    async fn authenticate(
        &self,
        _request: AuthenticationRequest,
    ) -> Result<SessionBootstrap, DeliveryError> {
        Err(DeliveryError::unavailable(Some(500)))
    }

    async fn submit_public_marketing_form(
        &self,
        _request: PublicMarketingFormRequest,
    ) -> Result<CommandReceipt, DeliveryError> {
        Err(DeliveryError::unavailable(Some(500)))
    }

    async fn public_marketing_preview(
        &self,
        _request: PublicMarketingPreviewRequest,
    ) -> Result<RouteData, DeliveryError> {
        Err(DeliveryError::unavailable(Some(500)))
    }

    async fn collaboration_stream(
        &self,
        _request: CollaborationSubscription,
    ) -> Result<Vec<CollaborationEvent>, DeliveryError> {
        Err(DeliveryError::unavailable(Some(500)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn native_gateway_owns_a_direct_tonic_channel() {
        let gateway = NativeGrpcGateway::connect_lazy("https://127.0.0.1:50051")
            .expect("valid gRPC endpoint");
        let _channel = gateway.channel();
    }

    #[test]
    fn native_gateway_rejects_plaintext_transport() {
        assert!(NativeGrpcGateway::connect_lazy("http://127.0.0.1:50051").is_err());
    }
}
