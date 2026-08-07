use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_pos::{
    PaymentMethod, PosQuery, PosQueryResponse, PosRefundCommand, PosRefundMutationResult,
    PosSaleCommand, PosSaleMutationResult, RefundList, RefundStatus, RefundView, SaleList,
    SaleStatus, SaleView,
};
use erpx_ui_foundation::{CommandGateway, QueryGateway, UiError};

use super::{
    models::{
        PosPaymentItem, PosRefundItem, PosRefundItems, PosRefundLineItem, PosSaleItem,
        PosSaleItems, PosSaleLineItem,
    },
    presenter::PosContent,
};

pub fn present_pos_query(value: QueryResult<PosQueryResponse>) -> QueryResult<Option<PosContent>> {
    let metadata = value.metadata;
    let content = match value.value {
        PosQueryResponse::Sale(value) => value.map(PosSaleItem::from).map(PosContent::Sale),
        PosQueryResponse::Sales(value) => {
            let values = PosSaleItems::from(value);
            (!values.sales.is_empty())
                .then_some(values)
                .map(PosContent::Sales)
        }
        PosQueryResponse::Refund(value) => value.map(PosRefundItem::from).map(PosContent::Refund),
        PosQueryResponse::Refunds(value) => {
            let values = PosRefundItems::from(value);
            (!values.refunds.is_empty())
                .then_some(values)
                .map(PosContent::Refunds)
        }
    };
    QueryResult {
        value: content,
        metadata,
    }
}

impl From<SaleView> for PosSaleItem {
    fn from(value: SaleView) -> Self {
        Self {
            sale_id: value.sale_id.as_uuid().to_string(),
            version: value.version,
            scope: format!("{:?}", value.scope),
            receipt_code: value.receipt_code,
            register_id: value.register_id.as_uuid().to_string(),
            customer_id: value.customer_id.map(|id| id.as_uuid().to_string()),
            currency: value.currency,
            lines: value
                .lines
                .into_iter()
                .map(|line| {
                    let total_minor = line.total_minor().unwrap_or_default();
                    PosSaleLineItem {
                        line_id: line.line_id.as_uuid().to_string(),
                        item_reference: line.item_reference,
                        description: line.description,
                        quantity: line.quantity,
                        total_minor,
                    }
                })
                .collect(),
            payments: value
                .payments
                .into_iter()
                .map(|payment| PosPaymentItem {
                    payment_id: payment.payment_id.as_uuid().to_string(),
                    amount_minor: payment.amount_minor,
                    method: payment_method_name(payment.method).into(),
                    received_at: payment.received_at.to_rfc3339(),
                })
                .collect(),
            subtotal_minor: value.subtotal_minor,
            tax_minor: value.tax_minor,
            total_minor: value.total_minor,
            paid_minor: value.paid_minor,
            status: sale_status_name(value.status).into(),
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl From<SaleList> for PosSaleItems {
    fn from(value: SaleList) -> Self {
        Self {
            sales: value.sales.into_iter().map(PosSaleItem::from).collect(),
            next_cursor: value.next_cursor,
        }
    }
}

impl From<RefundView> for PosRefundItem {
    fn from(value: RefundView) -> Self {
        Self {
            refund_id: value.refund_id.as_uuid().to_string(),
            version: value.version,
            scope: format!("{:?}", value.scope),
            sale_id: value.sale_id.as_uuid().to_string(),
            currency: value.currency,
            lines: value
                .lines
                .into_iter()
                .map(|line| PosRefundLineItem {
                    sale_line_id: line.sale_line_id.as_uuid().to_string(),
                    quantity: line.quantity,
                    amount_minor: line.amount_minor,
                })
                .collect(),
            total_amount_minor: value.total_amount_minor,
            reason: value.reason,
            requester_principal_ref: value.requester_principal_ref,
            reviewer_principal_ref: value.reviewer_principal_ref,
            status: refund_status_name(value.status).into(),
            requested_at: value.requested_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl From<RefundList> for PosRefundItems {
    fn from(value: RefundList) -> Self {
        Self {
            refunds: value.refunds.into_iter().map(PosRefundItem::from).collect(),
            next_cursor: value.next_cursor,
        }
    }
}

const fn sale_status_name(value: SaleStatus) -> &'static str {
    match value {
        SaleStatus::Draft => "draft",
        SaleStatus::Open => "open",
        SaleStatus::Held => "held",
        SaleStatus::Completed => "completed",
        SaleStatus::Voided => "voided",
    }
}

const fn refund_status_name(value: RefundStatus) -> &'static str {
    match value {
        RefundStatus::Requested => "requested",
        RefundStatus::Approved => "approved",
        RefundStatus::Rejected => "rejected",
        RefundStatus::Posted => "posted",
        RefundStatus::Cancelled => "cancelled",
    }
}

const fn payment_method_name(value: PaymentMethod) -> &'static str {
    match value {
        PaymentMethod::Cash => "cash",
        PaymentMethod::Card => "card",
        PaymentMethod::BankTransfer => "bank_transfer",
        PaymentMethod::MobileWallet => "mobile_wallet",
        PaymentMethod::Voucher => "voucher",
        PaymentMethod::Other => "other",
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait PosProtoTransport: Send + Sync {
    async fn execute_sale(
        &self,
        value: CommandEnvelope<PosSaleCommand>,
    ) -> Result<CommandReceipt<PosSaleMutationResult>, UiError>;

    async fn execute_refund(
        &self,
        value: CommandEnvelope<PosRefundCommand>,
    ) -> Result<CommandReceipt<PosRefundMutationResult>, UiError>;

    async fn execute_query(
        &self,
        value: QueryEnvelope<PosQuery>,
    ) -> Result<QueryResult<PosQueryResponse>, UiError>;
}

#[derive(Debug, Clone)]
pub struct PosGateway<T> {
    transport: T,
}

impl<T> PosGateway<T> {
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
impl<T: PosProtoTransport> CommandGateway<PosSaleCommand> for PosGateway<T> {
    type Response = PosSaleMutationResult;

    async fn execute(
        &self,
        value: CommandEnvelope<PosSaleCommand>,
    ) -> Result<CommandReceipt<Self::Response>, UiError> {
        self.transport.execute_sale(value).await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T: PosProtoTransport> CommandGateway<PosRefundCommand> for PosGateway<T> {
    type Response = PosRefundMutationResult;

    async fn execute(
        &self,
        value: CommandEnvelope<PosRefundCommand>,
    ) -> Result<CommandReceipt<Self::Response>, UiError> {
        self.transport.execute_refund(value).await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T: PosProtoTransport> QueryGateway<PosQuery> for PosGateway<T> {
    type Response = PosQueryResponse;

    async fn execute(
        &self,
        value: QueryEnvelope<PosQuery>,
    ) -> Result<QueryResult<Self::Response>, UiError> {
        self.transport.execute_query(value).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_command<C, R, G: CommandGateway<C, Response = R>>() {}
    fn assert_query<Q, R, G: QueryGateway<Q, Response = R>>() {}

    struct ContractTransport;

    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    impl PosProtoTransport for ContractTransport {
        async fn execute_sale(
            &self,
            _: CommandEnvelope<PosSaleCommand>,
        ) -> Result<CommandReceipt<PosSaleMutationResult>, UiError> {
            unreachable!()
        }
        async fn execute_refund(
            &self,
            _: CommandEnvelope<PosRefundCommand>,
        ) -> Result<CommandReceipt<PosRefundMutationResult>, UiError> {
            unreachable!()
        }
        async fn execute_query(
            &self,
            _: QueryEnvelope<PosQuery>,
        ) -> Result<QueryResult<PosQueryResponse>, UiError> {
            unreachable!()
        }
    }

    #[test]
    fn gateway_covers_all_typed_command_and_query_families() {
        type Gateway = PosGateway<ContractTransport>;
        assert_command::<PosSaleCommand, PosSaleMutationResult, Gateway>();
        assert_command::<PosRefundCommand, PosRefundMutationResult, Gateway>();
        assert_query::<PosQuery, PosQueryResponse, Gateway>();
    }

    #[test]
    fn every_command_variant_stays_typed() {
        let source = include_str!("../../../../../crates/contexts/pos/src/application/mod.rs");
        for operation in [
            "Create(CreateSale)",
            "Open(OpenSale)",
            "Hold(HoldSale)",
            "Resume(ResumeSale)",
            "RecordPayment(RecordPayment)",
            "Complete(CompleteSale)",
            "Void(VoidSale)",
            "Request(RequestRefund)",
            "Approve(ApproveRefund)",
            "Reject(RejectRefund)",
            "Post(PostRefund)",
            "Cancel(CancelRefund)",
        ] {
            assert!(
                source.contains(operation),
                "missing typed operation {operation}"
            );
        }
    }
}
