use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_sales::{
    QuoteStatus, QuoteView, RevenueArrangementStatus, RevenueArrangementView, SalesCommand,
    SalesMutationResult, SalesOrderLine, SalesOrderList, SalesOrderStatus, SalesOrderView,
    SalesQuery, SalesQueryResponse,
};
use erpx_ui_foundation::{CommandGateway, QueryGateway, UiError};

use super::{
    models::{
        RevenueArrangementItem, SalesLineItem, SalesOrderItem, SalesOrderItems, SalesQuoteItem,
    },
    presenter::SalesContent,
};

pub fn present_mutation(value: &SalesMutationResult) -> SalesContent {
    match value {
        SalesMutationResult::Quote(value) => {
            SalesContent::Quote(SalesQuoteItem::from(value.clone()))
        }
        SalesMutationResult::SalesOrder(value) => {
            SalesContent::Order(SalesOrderItem::from(value.clone()))
        }
        SalesMutationResult::RevenueArrangement(value) => {
            SalesContent::RevenueArrangement(RevenueArrangementItem::from(value.as_ref().clone()))
        }
    }
}

pub fn present_query(value: QueryResult<SalesQueryResponse>) -> QueryResult<Option<SalesContent>> {
    let metadata = value.metadata;
    let value = match value.value {
        SalesQueryResponse::Quote(value) => {
            value.map(SalesQuoteItem::from).map(SalesContent::Quote)
        }
        SalesQueryResponse::SalesOrder(value) => {
            value.map(SalesOrderItem::from).map(SalesContent::Order)
        }
        SalesQueryResponse::SalesOrders(value) => {
            let items = SalesOrderItems::from(value);
            (!items.orders.is_empty())
                .then_some(items)
                .map(SalesContent::Orders)
        }
        SalesQueryResponse::RevenueArrangement(value) => value
            .map(|value| RevenueArrangementItem::from(*value))
            .map(SalesContent::RevenueArrangement),
    };
    QueryResult { value, metadata }
}

impl From<&SalesOrderLine> for SalesLineItem {
    fn from(value: &SalesOrderLine) -> Self {
        Self {
            line_id: value.line_id.as_uuid().to_string(),
            item_code: value.item_code.clone(),
            description: value.description.clone(),
            quantity: value.quantity,
            line_value_minor: value.line_value_minor().unwrap_or_default(),
            currency: value.currency.clone(),
        }
    }
}

impl From<QuoteView> for SalesQuoteItem {
    fn from(value: QuoteView) -> Self {
        Self {
            quote_id: value.quote_id.as_uuid().to_string(),
            version: value.version,
            scope: format!("{:?}", value.scope),
            opportunity_id: value.opportunity.as_uuid().to_string(),
            customer_id: value.customer.as_uuid().to_string(),
            quote_code: value.quote_code,
            currency: value.currency,
            lines: value.lines.iter().map(SalesLineItem::from).collect(),
            total_minor: value.total_minor,
            valid_until: value.valid_until.to_rfc3339(),
            status: quote_status(value.status).into(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl From<SalesOrderView> for SalesOrderItem {
    fn from(value: SalesOrderView) -> Self {
        Self {
            sales_order_id: value.sales_order_id.as_uuid().to_string(),
            version: value.version,
            scope: format!("{:?}", value.scope),
            quote_id: value.quote_id.as_uuid().to_string(),
            opportunity_id: value.opportunity.as_uuid().to_string(),
            customer_id: value.customer.as_uuid().to_string(),
            order_code: value.order_code,
            currency: value.currency,
            lines: value.lines.iter().map(SalesLineItem::from).collect(),
            total_minor: value.total_minor,
            status: order_status(value.status).into(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl From<SalesOrderList> for SalesOrderItems {
    fn from(value: SalesOrderList) -> Self {
        Self {
            orders: value.orders.into_iter().map(SalesOrderItem::from).collect(),
            next_cursor: value.next_cursor,
        }
    }
}

impl From<RevenueArrangementView> for RevenueArrangementItem {
    fn from(value: RevenueArrangementView) -> Self {
        Self {
            arrangement_id: value.arrangement_id.as_uuid().to_string(),
            version: value.version,
            scope: format!("{:?}", value.scope),
            sales_order_id: value.sales_order_id.as_uuid().to_string(),
            customer_id: value.customer.as_uuid().to_string(),
            order_code: value.order_code,
            currency: value.currency,
            status: arrangement_status(value.status).into(),
            obligation_count: value.obligations.len(),
            fulfillment_count: value.fulfillments.len(),
            allocated_minor: value
                .obligations
                .iter()
                .map(|obligation| obligation.allocated_minor())
                .sum(),
            accepted_minor: value
                .obligations
                .iter()
                .map(|obligation| obligation.accepted_minor())
                .sum(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

const fn quote_status(value: QuoteStatus) -> &'static str {
    match value {
        QuoteStatus::Draft => "draft",
        QuoteStatus::Sent => "sent",
        QuoteStatus::Accepted => "accepted",
        QuoteStatus::Rejected => "rejected",
        QuoteStatus::Expired => "expired",
    }
}

const fn order_status(value: SalesOrderStatus) -> &'static str {
    match value {
        SalesOrderStatus::Draft => "draft",
        SalesOrderStatus::Confirmed => "confirmed",
        SalesOrderStatus::PartiallyFulfilled => "partially_fulfilled",
        SalesOrderStatus::Fulfilled => "fulfilled",
        SalesOrderStatus::Cancelled => "cancelled",
    }
}

const fn arrangement_status(value: RevenueArrangementStatus) -> &'static str {
    match value {
        RevenueArrangementStatus::Active => "active",
        RevenueArrangementStatus::Completed => "completed",
        RevenueArrangementStatus::Cancelled => "cancelled",
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait SalesProtoTransport: Send + Sync {
    async fn command(
        &self,
        envelope: CommandEnvelope<SalesCommand>,
    ) -> Result<CommandReceipt<SalesMutationResult>, UiError>;

    async fn query(
        &self,
        envelope: QueryEnvelope<SalesQuery>,
    ) -> Result<QueryResult<SalesQueryResponse>, UiError>;
}

#[derive(Debug, Clone)]
pub struct SalesGateway<T> {
    transport: T,
}

impl<T> SalesGateway<T> {
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
impl<T: SalesProtoTransport> CommandGateway<SalesCommand> for SalesGateway<T> {
    type Response = SalesMutationResult;

    async fn execute(
        &self,
        envelope: CommandEnvelope<SalesCommand>,
    ) -> Result<CommandReceipt<Self::Response>, UiError> {
        self.transport.command(envelope).await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T: SalesProtoTransport> QueryGateway<SalesQuery> for SalesGateway<T> {
    type Response = SalesQueryResponse;

    async fn execute(
        &self,
        envelope: QueryEnvelope<SalesQuery>,
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
    impl SalesProtoTransport for Fake {
        async fn command(
            &self,
            _: CommandEnvelope<SalesCommand>,
        ) -> Result<CommandReceipt<SalesMutationResult>, UiError> {
            unreachable!()
        }

        async fn query(
            &self,
            _: QueryEnvelope<SalesQuery>,
        ) -> Result<QueryResult<SalesQueryResponse>, UiError> {
            unreachable!()
        }
    }

    #[test]
    fn gateway_is_typed_and_wire_free() {
        fn command<G: CommandGateway<SalesCommand, Response = SalesMutationResult>>() {}
        fn query<G: QueryGateway<SalesQuery, Response = SalesQueryResponse>>() {}
        command::<SalesGateway<Fake>>();
        query::<SalesGateway<Fake>>();
        assert!(!include_str!("gateway.rs").contains(concat!("erpx_", "proto")));
    }

    #[test]
    fn gateway_is_the_only_core_to_ui_mapping_boundary() {
        assert!(include_str!("gateway.rs").contains("impl From<QuoteView> for SalesQuoteItem"));
        for source in [
            include_str!("models.rs"),
            include_str!("presenter.rs"),
            include_str!("view.rs"),
        ] {
            assert!(!source.contains(concat!("erpx_", "sales")));
        }
    }
}
