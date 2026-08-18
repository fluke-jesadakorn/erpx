//! Direct Native gRPC v2 delivery adapter.

use async_trait::async_trait;
use erpx_delivery_contract::{
    AuthenticationRequestV2, BootstrapRequestV2, CollaborationEventV2, CollaborationSubscriptionV2,
    CommandReceiptV2, CommandRequestV2, ConsoleGateway, DeliveryErrorV2,
    PublicMarketingFormRequestV2, PublicMarketingPreviewRequestV2, QueryRequestV2, RouteDataV2,
    SessionBootstrapV2,
};

const NATIVE_GRPC_ENDPOINT_ENV: &str = "ERPX_NATIVE_GRPC_ENDPOINT";

/// Native owns a Tonic channel and never traverses the browser BFF or a
/// WebView. Operation-specific generated v2 clients are selected by the
/// platform composition as their capabilities become available.
#[derive(Clone, Debug)]
pub struct NativeGrpcGatewayV2 {
    channel: tonic::transport::Channel,
}

impl NativeGrpcGatewayV2 {
    pub fn from_environment() -> Result<Self, DeliveryErrorV2> {
        let endpoint = std::env::var(NATIVE_GRPC_ENDPOINT_ENV)
            .map_err(|_| DeliveryErrorV2::unavailable(None))?;
        Self::connect_lazy(&endpoint)
    }

    pub fn connect_lazy(endpoint: &str) -> Result<Self, DeliveryErrorV2> {
        let endpoint = tonic::transport::Endpoint::from_shared(endpoint.to_owned())
            .map_err(|_| DeliveryErrorV2::validation("native gRPC endpoint"))?;
        if endpoint.uri().scheme_str() != Some("https") {
            return Err(DeliveryErrorV2::validation(
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
impl ConsoleGateway for NativeGrpcGatewayV2 {
    async fn bootstrap(
        &self,
        _request: BootstrapRequestV2,
    ) -> Result<SessionBootstrapV2, DeliveryErrorV2> {
        Err(DeliveryErrorV2::unavailable(Some(500)))
    }

    async fn query(&self, _request: QueryRequestV2) -> Result<RouteDataV2, DeliveryErrorV2> {
        Err(DeliveryErrorV2::unavailable(Some(500)))
    }

    async fn command(
        &self,
        _request: CommandRequestV2,
    ) -> Result<CommandReceiptV2, DeliveryErrorV2> {
        Err(DeliveryErrorV2::unavailable(Some(500)))
    }

    async fn authenticate(
        &self,
        _request: AuthenticationRequestV2,
    ) -> Result<SessionBootstrapV2, DeliveryErrorV2> {
        Err(DeliveryErrorV2::unavailable(Some(500)))
    }

    async fn submit_public_marketing_form(
        &self,
        _request: PublicMarketingFormRequestV2,
    ) -> Result<CommandReceiptV2, DeliveryErrorV2> {
        Err(DeliveryErrorV2::unavailable(Some(500)))
    }

    async fn public_marketing_preview(
        &self,
        _request: PublicMarketingPreviewRequestV2,
    ) -> Result<RouteDataV2, DeliveryErrorV2> {
        Err(DeliveryErrorV2::unavailable(Some(500)))
    }

    async fn collaboration_stream(
        &self,
        _request: CollaborationSubscriptionV2,
    ) -> Result<Vec<CollaborationEventV2>, DeliveryErrorV2> {
        Err(DeliveryErrorV2::unavailable(Some(500)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn native_gateway_owns_a_direct_tonic_channel() {
        let gateway = NativeGrpcGatewayV2::connect_lazy("https://127.0.0.1:50051")
            .expect("valid gRPC endpoint");
        let _channel = gateway.channel();
    }

    #[test]
    fn native_gateway_rejects_plaintext_transport() {
        assert!(NativeGrpcGatewayV2::connect_lazy("http://127.0.0.1:50051").is_err());
    }
}
