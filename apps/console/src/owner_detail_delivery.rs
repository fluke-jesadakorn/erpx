//! Same-origin Phase 5 owner-detail delivery.
//!
//! The browser sends only stable catalog identities and typed intents. This
//! boundary owns the HttpOnly credential, resolves the active holding route,
//! calls the owning RPC, validates projection freshness, and separately
//! authorizes every linked source through the Item Directory.

#[cfg(target_arch = "wasm32")]
use async_trait::async_trait;
use dioxus::prelude::dioxus_fullstack;
#[cfg(feature = "server")]
use dioxus::prelude::dioxus_server;
#[cfg(target_arch = "wasm32")]
use erpx_console_feature_finance::FinanceOwnerGateway;
use erpx_console_feature_finance::{
    FinanceOwnerDetailState, FinanceOwnerGatewayError, FinanceOwnerMutation,
    FinanceOwnerMutationReceipt,
};
#[cfg(target_arch = "wasm32")]
use erpx_console_feature_procurement::ProcurementOwnerGateway;
use erpx_console_feature_procurement::{
    ProcurementOwnerDetailState, ProcurementOwnerGatewayError, ProcurementOwnerMutation,
    ProcurementOwnerMutationReceipt,
};
use erpx_integration_contracts::StableRecordRef;

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct WebSameOriginProcurementOwnerGateway;

#[cfg(target_arch = "wasm32")]
#[async_trait(?Send)]
impl ProcurementOwnerGateway for WebSameOriginProcurementOwnerGateway {
    async fn load(
        &self,
        record: StableRecordRef,
    ) -> Result<ProcurementOwnerDetailState, ProcurementOwnerGatewayError> {
        load_procurement_owner(record)
            .await
            .map_err(|_| ProcurementOwnerGatewayError::Unavailable)?
    }

    async fn mutate(
        &self,
        mutation: ProcurementOwnerMutation,
    ) -> Result<ProcurementOwnerMutationReceipt, ProcurementOwnerGatewayError> {
        mutate_procurement_owner(mutation)
            .await
            .map_err(|_| ProcurementOwnerGatewayError::Unavailable)?
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct WebSameOriginFinanceOwnerGateway;

#[cfg(target_arch = "wasm32")]
#[async_trait(?Send)]
impl FinanceOwnerGateway for WebSameOriginFinanceOwnerGateway {
    async fn load(
        &self,
        record: StableRecordRef,
    ) -> Result<FinanceOwnerDetailState, FinanceOwnerGatewayError> {
        load_finance_owner(record)
            .await
            .map_err(|_| FinanceOwnerGatewayError::Unavailable)?
    }

    async fn mutate(
        &self,
        mutation: FinanceOwnerMutation,
    ) -> Result<FinanceOwnerMutationReceipt, FinanceOwnerGatewayError> {
        mutate_finance_owner(mutation)
            .await
            .map_err(|_| FinanceOwnerGatewayError::Unavailable)?
    }
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/owner-detail/procurement/load")]
pub async fn load_procurement_owner(
    record: StableRecordRef,
) -> dioxus::prelude::ServerFnResult<
    Result<ProcurementOwnerDetailState, ProcurementOwnerGatewayError>,
> {
    if !matches!(
        record.record_type().as_str(),
        "procurement.requisition" | "procurement.purchase-order" | "procurement.receipt"
    ) {
        return Ok(Err(ProcurementOwnerGatewayError::Invalid));
    }
    #[cfg(feature = "server")]
    {
        return Ok(server::load_procurement(record)
            .await
            .map_err(server::procurement_error));
    }
    #[allow(unreachable_code)]
    Ok(Err(ProcurementOwnerGatewayError::Unavailable))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/owner-detail/procurement/mutate")]
pub async fn mutate_procurement_owner(
    mutation: ProcurementOwnerMutation,
) -> dioxus::prelude::ServerFnResult<
    Result<ProcurementOwnerMutationReceipt, ProcurementOwnerGatewayError>,
> {
    #[cfg(feature = "server")]
    {
        crate::delivery::validate_mutation_request()?;
        return Ok(server::mutate_procurement(mutation)
            .await
            .map_err(server::procurement_error));
    }
    #[allow(unreachable_code)]
    Ok(Err(ProcurementOwnerGatewayError::Unavailable))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/owner-detail/finance/load")]
pub async fn load_finance_owner(
    record: StableRecordRef,
) -> dioxus::prelude::ServerFnResult<Result<FinanceOwnerDetailState, FinanceOwnerGatewayError>> {
    if !matches!(
        record.record_type().as_str(),
        "finance.vendor-invoice" | "finance.payment"
    ) {
        return Ok(Err(FinanceOwnerGatewayError::Invalid));
    }
    #[cfg(feature = "server")]
    {
        return Ok(server::load_finance(record)
            .await
            .map_err(server::finance_error));
    }
    #[allow(unreachable_code)]
    Ok(Err(FinanceOwnerGatewayError::Unavailable))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/owner-detail/finance/mutate")]
pub async fn mutate_finance_owner(
    mutation: FinanceOwnerMutation,
) -> dioxus::prelude::ServerFnResult<Result<FinanceOwnerMutationReceipt, FinanceOwnerGatewayError>>
{
    #[cfg(feature = "server")]
    {
        crate::delivery::validate_mutation_request()?;
        return Ok(server::mutate_finance(mutation)
            .await
            .map_err(server::finance_error));
    }
    #[allow(unreachable_code)]
    Ok(Err(FinanceOwnerGatewayError::Unavailable))
}

#[cfg(feature = "server")]
mod server {
    use chrono::{DateTime, Utc};
    use erpx_console_feature_finance::{
        FinanceOwnerCapabilities, FinanceOwnerRecord, FinanceSourceReference, PaymentOwnerDetail,
        VendorInvoiceOwnerDetail,
    };
    use erpx_console_feature_procurement::{
        GoodsReceiptOwnerDetail, ProcurementOwnerCapabilities, ProcurementOwnerRecord,
        ProcurementSourceReference, PurchaseOrderOwnerDetail, RequisitionOwnerDetail,
    };
    use erpx_console_transport_core::CatalogRecord;
    use erpx_delivery_contract::{DeliveryError, DeliveryErrorMeta};
    use erpx_proto::{
        common::{CommandReceipt, ProjectionFreshnessState, QueryMetadata},
        erpx::{finance as finance_wire, procurement as procurement_wire},
    };
    use tonic::transport::Channel;
    use uuid::Uuid;

    use super::*;
    use crate::feature_delivery::{FeatureConsistency, server as shared};

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Freshness {
        Current,
        Stale,
        Unavailable,
    }

    fn meta(retryable: bool) -> DeliveryErrorMeta {
        DeliveryErrorMeta {
            correlation_id: Uuid::new_v4(),
            retryable,
            retry_after_ms: retryable.then_some(500),
        }
    }

    fn invalid() -> DeliveryError {
        DeliveryError::Internal(meta(false))
    }

    pub(super) fn procurement_error(error: DeliveryError) -> ProcurementOwnerGatewayError {
        match error {
            DeliveryError::Unauthenticated(_)
            | DeliveryError::Forbidden(_)
            | DeliveryError::NotFound(_) => ProcurementOwnerGatewayError::Denied,
            DeliveryError::Conflict(_) => ProcurementOwnerGatewayError::Conflict,
            DeliveryError::Unavailable(_) => ProcurementOwnerGatewayError::Offline,
            DeliveryError::Validation(_) => ProcurementOwnerGatewayError::Invalid,
            _ => ProcurementOwnerGatewayError::Unavailable,
        }
    }

    pub(super) fn finance_error(error: DeliveryError) -> FinanceOwnerGatewayError {
        match error {
            DeliveryError::Unauthenticated(_)
            | DeliveryError::Forbidden(_)
            | DeliveryError::NotFound(_) => FinanceOwnerGatewayError::Denied,
            DeliveryError::Conflict(_) => FinanceOwnerGatewayError::Conflict,
            DeliveryError::Unavailable(_) => FinanceOwnerGatewayError::Offline,
            DeliveryError::Validation(_) => FinanceOwnerGatewayError::Invalid,
            _ => FinanceOwnerGatewayError::Unavailable,
        }
    }

    fn freshness(metadata: Option<QueryMetadata>) -> Result<Freshness, DeliveryError> {
        let projection = metadata
            .and_then(|value| value.projection)
            .ok_or_else(|| DeliveryError::Unavailable(meta(true)))?;
        match ProjectionFreshnessState::try_from(projection.freshness).ok() {
            Some(ProjectionFreshnessState::Current) => Ok(Freshness::Current),
            Some(ProjectionFreshnessState::Stale | ProjectionFreshnessState::Rebuilding) => {
                Ok(Freshness::Stale)
            }
            Some(ProjectionFreshnessState::Unavailable) => Ok(Freshness::Unavailable),
            _ => Err(DeliveryError::Unavailable(meta(true))),
        }
    }

    fn owner_state(
        detail: ProcurementOwnerRecord,
        freshness: Freshness,
        capabilities: Option<ProcurementOwnerCapabilities>,
    ) -> ProcurementOwnerDetailState {
        match (freshness, capabilities) {
            (Freshness::Current, Some(capabilities)) => ProcurementOwnerDetailState::Fresh {
                detail,
                capabilities,
            },
            (Freshness::Current, None) => ProcurementOwnerDetailState::CurrentReadOnly { detail },
            (Freshness::Stale, _) => ProcurementOwnerDetailState::Stale { detail },
            (Freshness::Unavailable, _) => {
                ProcurementOwnerDetailState::Unavailable { retryable: true }
            }
        }
    }

    fn finance_state(
        detail: FinanceOwnerRecord,
        freshness: Freshness,
        capabilities: Option<FinanceOwnerCapabilities>,
    ) -> FinanceOwnerDetailState {
        match (freshness, capabilities) {
            (Freshness::Current, Some(capabilities)) => FinanceOwnerDetailState::Fresh {
                detail,
                capabilities,
            },
            (Freshness::Current, None) => FinanceOwnerDetailState::CurrentReadOnly { detail },
            (Freshness::Stale, _) => FinanceOwnerDetailState::Stale { detail },
            (Freshness::Unavailable, _) => FinanceOwnerDetailState::Unavailable { retryable: true },
        }
    }

    fn capability_evaluation_is_fresh(evaluated_at: &str) -> bool {
        DateTime::parse_from_rfc3339(evaluated_at)
            .ok()
            .map(|value| value.with_timezone(&Utc))
            .is_some_and(|value| {
                let age = Utc::now().signed_duration_since(value);
                age.num_seconds() >= -5 && age.num_seconds() <= 60
            })
    }

    fn procurement_capabilities(
        value: Option<procurement_wire::ProcurementOwnerCapabilities>,
        source_version: u64,
        record_type: &str,
        freshness: Freshness,
    ) -> Option<ProcurementOwnerCapabilities> {
        if freshness != Freshness::Current {
            return None;
        }
        let value = value?;
        if value.source_version != source_version
            || !capability_evaluation_is_fresh(&value.evaluated_at)
        {
            return None;
        }
        let capabilities = ProcurementOwnerCapabilities {
            approve_requisition: value.approve_requisition,
            release_purchase_order: value.release_purchase_order,
            accept_goods_receipt: value.accept_goods_receipt,
        };
        match record_type {
            "procurement.requisition"
                if !capabilities.release_purchase_order && !capabilities.accept_goods_receipt =>
            {
                Some(capabilities)
            }
            "procurement.purchase-order"
                if !capabilities.approve_requisition && !capabilities.accept_goods_receipt =>
            {
                Some(capabilities)
            }
            "procurement.receipt"
                if !capabilities.approve_requisition && !capabilities.release_purchase_order =>
            {
                Some(capabilities)
            }
            _ => None,
        }
    }

    fn finance_capabilities(
        value: Option<finance_wire::FinancePayablesOwnerCapabilities>,
        source_version: u64,
        record_type: &str,
        freshness: Freshness,
    ) -> Option<FinanceOwnerCapabilities> {
        if freshness != Freshness::Current {
            return None;
        }
        let value = value?;
        if value.source_version != source_version
            || !capability_evaluation_is_fresh(&value.evaluated_at)
        {
            return None;
        }
        let capabilities = FinanceOwnerCapabilities {
            post_vendor_invoice: value.post_vendor_invoice,
            release_payment: value.release_payment,
        };
        match record_type {
            "finance.vendor-invoice" if !capabilities.release_payment => Some(capabilities),
            "finance.payment" if !capabilities.post_vendor_invoice => Some(capabilities),
            _ => None,
        }
    }

    fn status_text(value: &'static str) -> String {
        value.replace('_', "-")
    }

    fn procurement_record(
        record_id: &str,
        record_type: &str,
    ) -> Result<StableRecordRef, DeliveryError> {
        let descriptor =
            erpx_integration_contracts::catalog::record_type(record_type).ok_or_else(invalid)?;
        CatalogRecord::try_from_segments(descriptor.module, descriptor.record, record_id)
            .map(|value| value.record().clone())
            .map_err(|_| invalid())
    }

    fn procurement_source(
        value: procurement_wire::RecordVersionRef,
        expected_type: &str,
    ) -> Result<(StableRecordRef, u64), DeliveryError> {
        if value.version == 0
            || value.owner_context != "procurement"
            || value.record_type != expected_type
        {
            return Err(invalid());
        }
        Ok((
            procurement_record(&value.record_id, expected_type)?,
            value.version,
        ))
    }

    fn finance_source(
        value: finance_wire::FinanceVersionedRecordRef,
        expected_kind: &str,
        expected_type: &str,
    ) -> Result<(StableRecordRef, u64), DeliveryError> {
        if value.aggregate_version == 0
            || value.context != "procurement"
            || value.aggregate_kind != expected_kind
        {
            return Err(invalid());
        }
        Ok((
            procurement_record(&value.aggregate_id, expected_type)?,
            value.aggregate_version,
        ))
    }

    async fn authorized_source(
        authority: &shared::FeatureAuthority,
        channel: Channel,
        record: StableRecordRef,
        version: u64,
    ) -> Result<(String, String), DeliveryError> {
        use erpx_proto::erpx::interface::{DirectoryFreshness, ResolveReferenceRequest};
        use erpx_proto::erpx::reference::reference_query_service_client::ReferenceQueryServiceClient;

        let request = ResolveReferenceRequest {
            record_id: record.id().to_string(),
            authority_scope: Some(shared::reference_scope(authority)?),
            purpose: "phase5_owner_source".into(),
            record_type: None,
        };
        let response = ReferenceQueryServiceClient::new(channel)
            .resolve(shared::authenticated(authority, request)?)
            .await
            .map_err(shared::delivery_status)?
            .into_inner();
        if !response.found
            || DirectoryFreshness::try_from(response.freshness).ok()
                != Some(DirectoryFreshness::Fresh)
        {
            return Err(DeliveryError::Unavailable(meta(true)));
        }
        let descriptor = response.descriptor.ok_or_else(invalid)?;
        let wire_record = descriptor.record.ok_or_else(invalid)?;
        let display_label = descriptor
            .display_label
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| DeliveryError::Unavailable(meta(true)))?;
        if wire_record.id != record.id().to_string()
            || wire_record.owner_context != record.owner_context().slug()
            || wire_record.record_type != record.record_type().as_str()
            || descriptor.canonical_route != record.canonical_detail_route()
            || version == 0
        {
            return Err(invalid());
        }
        Ok((display_label, descriptor.canonical_route))
    }

    fn request_metadata(
        authority: &shared::FeatureAuthority,
        route: erpx_proto::common::HoldingRoute,
        module: &'static str,
        purpose: &'static str,
    ) -> erpx_proto::common::QueryRequestMetadata {
        shared::query_metadata(
            authority,
            &FeatureConsistency::Eventual,
            module,
            purpose,
            Some(route),
            Uuid::new_v4(),
        )
    }

    pub(super) async fn load_procurement(
        record: StableRecordRef,
    ) -> Result<ProcurementOwnerDetailState, DeliveryError> {
        let authority = shared::feature_authority()?;
        let route = shared::resolve_holding_route(
            &authority,
            &FeatureConsistency::Eventual,
            "procurement",
            "resolve procurement owner route",
        )
        .await?;
        let channel = shared::channel(&authority).await?;
        let mut client =
            procurement_wire::procurement_query_service_client::ProcurementQueryServiceClient::new(
                channel.clone(),
            );
        let id = record.id().to_string();
        match record.record_type().as_str() {
            "procurement.requisition" => {
                let response = client
                    .get_requisition(shared::authenticated(
                        &authority,
                        procurement_wire::GetPurchaseRequisitionRequest {
                            metadata: Some(request_metadata(
                                &authority,
                                route,
                                "procurement",
                                "get purchase requisition owner detail",
                            )),
                            requisition_id: id.clone(),
                        },
                    )?)
                    .await
                    .map_err(shared::delivery_status)?
                    .into_inner();
                let projection = freshness(response.metadata)?;
                let owner_capabilities = response.owner_capabilities;
                let value = response
                    .requisition
                    .ok_or_else(|| DeliveryError::NotFound(meta(false)))?;
                if value.requisition_id != id || value.source_version == 0 {
                    return Err(invalid());
                }
                let mut total_minor = 0_i64;
                for line in &value.lines {
                    if line.currency != value.currency || line.requested_quantity <= 0 {
                        return Err(invalid());
                    }
                    total_minor = total_minor
                        .checked_add(
                            line.requested_quantity
                                .checked_mul(line.estimated_unit_price_minor)
                                .ok_or_else(invalid)?,
                        )
                        .ok_or_else(invalid)?;
                }
                let status = match procurement_wire::PurchaseRequisitionStatus::try_from(
                    value.status,
                )
                .ok()
                {
                    Some(procurement_wire::PurchaseRequisitionStatus::Draft) => "draft",
                    Some(procurement_wire::PurchaseRequisitionStatus::Submitted) => "submitted",
                    Some(procurement_wire::PurchaseRequisitionStatus::Approved) => "approved",
                    Some(procurement_wire::PurchaseRequisitionStatus::Rejected) => "rejected",
                    Some(procurement_wire::PurchaseRequisitionStatus::Cancelled) => "cancelled",
                    _ => return Err(invalid()),
                };
                let capabilities = procurement_capabilities(
                    owner_capabilities,
                    value.source_version,
                    "procurement.requisition",
                    projection,
                );
                Ok(owner_state(
                    ProcurementOwnerRecord::Requisition(RequisitionOwnerDetail {
                        record,
                        version: value.source_version,
                        requisition_code: value.requisition_code,
                        requester_label: value.requesting_department_code,
                        business_purpose: value.budget_code.unwrap_or_default(),
                        currency: value.currency,
                        total_minor,
                        status: status_text(status),
                    }),
                    projection,
                    capabilities,
                ))
            }
            "procurement.purchase-order" => {
                let response = client
                    .get(shared::authenticated(
                        &authority,
                        procurement_wire::GetPurchaseOrderRequest {
                            metadata: Some(request_metadata(
                                &authority,
                                route,
                                "procurement",
                                "get purchase order owner detail",
                            )),
                            purchase_order_id: id.clone(),
                        },
                    )?)
                    .await
                    .map_err(shared::delivery_status)?
                    .into_inner();
                let projection = freshness(response.metadata)?;
                let owner_capabilities = response.owner_capabilities;
                let value = response
                    .order
                    .ok_or_else(|| DeliveryError::NotFound(meta(false)))?;
                if value.purchase_order_id != id || value.source_version == 0 {
                    return Err(invalid());
                }
                let mut total_minor = 0_i64;
                for line in &value.lines {
                    if line.currency != value.currency || line.ordered_quantity <= 0 {
                        return Err(invalid());
                    }
                    total_minor = total_minor
                        .checked_add(
                            line.ordered_quantity
                                .checked_mul(line.unit_price_minor)
                                .ok_or_else(invalid)?,
                        )
                        .ok_or_else(invalid)?;
                }
                let (source_record, source_version) = procurement_source(
                    value.source_requisition.ok_or_else(invalid)?,
                    "procurement.requisition",
                )?;
                let (display_label, canonical_route) = authorized_source(
                    &authority,
                    channel.clone(),
                    source_record.clone(),
                    source_version,
                )
                .await?;
                let source_requisition = ProcurementSourceReference {
                    record: source_record,
                    version: source_version,
                    display_label,
                    canonical_route,
                };
                let contract = if let Some(source) = value.contract {
                    if source.version == 0
                        || source.owner_context != "legal"
                        || source.record_type != "legal.contract"
                    {
                        return Err(invalid());
                    }
                    let source_record = procurement_record(&source.record_id, "legal.contract")?;
                    let (display_label, canonical_route) = authorized_source(
                        &authority,
                        channel.clone(),
                        source_record.clone(),
                        source.version,
                    )
                    .await?;
                    Some(ProcurementSourceReference {
                        record: source_record,
                        version: source.version,
                        display_label,
                        canonical_route,
                    })
                } else {
                    None
                };
                let status =
                    match procurement_wire::PurchaseOrderStatus::try_from(value.status).ok() {
                        Some(procurement_wire::PurchaseOrderStatus::Draft) => "draft",
                        Some(procurement_wire::PurchaseOrderStatus::Released) => "released",
                        Some(procurement_wire::PurchaseOrderStatus::Cancelled) => "cancelled",
                        _ => return Err(invalid()),
                    };
                let capabilities = procurement_capabilities(
                    owner_capabilities,
                    value.source_version,
                    "procurement.purchase-order",
                    projection,
                );
                Ok(owner_state(
                    ProcurementOwnerRecord::PurchaseOrder(PurchaseOrderOwnerDetail {
                        record,
                        version: value.source_version,
                        purchase_order_code: value.order_code,
                        supplier_label: value.supplier_id,
                        currency: value.currency,
                        total_minor,
                        status: status_text(status),
                        source_requisition,
                        contract,
                    }),
                    projection,
                    capabilities,
                ))
            }
            "procurement.receipt" => {
                let response = client
                    .get_goods_receipt(shared::authenticated(
                        &authority,
                        procurement_wire::GetGoodsReceiptRequest {
                            metadata: Some(request_metadata(
                                &authority,
                                route,
                                "procurement",
                                "get goods receipt owner detail",
                            )),
                            goods_receipt_id: id.clone(),
                        },
                    )?)
                    .await
                    .map_err(shared::delivery_status)?
                    .into_inner();
                let projection = freshness(response.metadata)?;
                let owner_capabilities = response.owner_capabilities;
                let value = response
                    .goods_receipt
                    .ok_or_else(|| DeliveryError::NotFound(meta(false)))?;
                if value.goods_receipt_id != id || value.source_version == 0 {
                    return Err(invalid());
                }
                let (source_record, source_version) = procurement_source(
                    value.source_purchase_order.ok_or_else(invalid)?,
                    "procurement.purchase-order",
                )?;
                let (display_label, canonical_route) =
                    authorized_source(&authority, channel, source_record.clone(), source_version)
                        .await?;
                let status = match procurement_wire::GoodsReceiptStatus::try_from(value.status).ok()
                {
                    Some(procurement_wire::GoodsReceiptStatus::PendingAcceptance) => {
                        "pending-acceptance"
                    }
                    Some(procurement_wire::GoodsReceiptStatus::Accepted) => "accepted",
                    Some(procurement_wire::GoodsReceiptStatus::Rejected) => "rejected",
                    Some(procurement_wire::GoodsReceiptStatus::Cancelled) => "cancelled",
                    _ => return Err(invalid()),
                };
                let capabilities = procurement_capabilities(
                    owner_capabilities,
                    value.source_version,
                    "procurement.receipt",
                    projection,
                );
                Ok(owner_state(
                    ProcurementOwnerRecord::GoodsReceipt(GoodsReceiptOwnerDetail {
                        record,
                        version: value.source_version,
                        receipt_code: value.receipt_code,
                        received_by_label: format!("{} receipt lines", value.lines.len()),
                        received_at: value.updated_at,
                        status: status_text(status),
                        purchase_order: ProcurementSourceReference {
                            record: source_record,
                            version: source_version,
                            display_label,
                            canonical_route,
                        },
                    }),
                    projection,
                    capabilities,
                ))
            }
            _ => Err(DeliveryError::Validation(meta(false))),
        }
    }

    pub(super) async fn load_finance(
        record: StableRecordRef,
    ) -> Result<FinanceOwnerDetailState, DeliveryError> {
        let authority = shared::feature_authority()?;
        let route = shared::resolve_holding_route(
            &authority,
            &FeatureConsistency::Eventual,
            "finance",
            "resolve finance owner route",
        )
        .await?;
        let channel = shared::channel(&authority).await?;
        let mut client = finance_wire::finance_payables_query_service_client::FinancePayablesQueryServiceClient::new(channel.clone());
        let id = record.id().to_string();
        match record.record_type().as_str() {
            "finance.vendor-invoice" => {
                let response = client
                    .get_vendor_invoice(shared::authenticated(
                        &authority,
                        finance_wire::GetFinanceVendorInvoiceRequest {
                            metadata: Some(request_metadata(
                                &authority,
                                route,
                                "finance",
                                "get vendor invoice owner detail",
                            )),
                            vendor_invoice_id: id.clone(),
                        },
                    )?)
                    .await
                    .map_err(shared::delivery_status)?
                    .into_inner();
                let projection = freshness(response.metadata)?;
                let owner_capabilities = response.owner_capabilities;
                let value = response
                    .vendor_invoice
                    .ok_or_else(|| DeliveryError::NotFound(meta(false)))?;
                if value.vendor_invoice_id != id || value.source_version == 0 {
                    return Err(invalid());
                }
                let total = value.total_amount.ok_or_else(invalid)?;
                let (po_record, po_version) = finance_source(
                    value.purchase_order.ok_or_else(invalid)?,
                    "purchase_order",
                    "procurement.purchase-order",
                )?;
                let (receipt_record, receipt_version) = finance_source(
                    value.receipt.ok_or_else(invalid)?,
                    "receipt",
                    "procurement.receipt",
                )?;
                let (po_label, po_route) =
                    authorized_source(&authority, channel.clone(), po_record.clone(), po_version)
                        .await?;
                let (receipt_label, receipt_route) =
                    authorized_source(&authority, channel, receipt_record.clone(), receipt_version)
                        .await?;
                let status =
                    match finance_wire::FinanceVendorInvoiceStatus::try_from(value.status).ok() {
                        Some(finance_wire::FinanceVendorInvoiceStatus::Draft) => "draft",
                        Some(finance_wire::FinanceVendorInvoiceStatus::Posted) => "posted",
                        _ => return Err(invalid()),
                    };
                let capabilities = finance_capabilities(
                    owner_capabilities,
                    value.source_version,
                    "finance.vendor-invoice",
                    projection,
                );
                Ok(finance_state(
                    FinanceOwnerRecord::VendorInvoice(VendorInvoiceOwnerDetail {
                        record,
                        version: value.source_version,
                        invoice_code: value.invoice_number,
                        supplier_label: value.supplier_id,
                        currency: total.currency,
                        gross_minor: total.minor_units,
                        outstanding_minor: if status == "posted" {
                            0
                        } else {
                            total.minor_units
                        },
                        status: status_text(status),
                        purchase_order: Some(FinanceSourceReference {
                            record: po_record,
                            version: po_version,
                            display_label: po_label,
                            canonical_route: po_route,
                        }),
                        goods_receipts: vec![FinanceSourceReference {
                            record: receipt_record,
                            version: receipt_version,
                            display_label: receipt_label,
                            canonical_route: receipt_route,
                        }],
                    }),
                    projection,
                    capabilities,
                ))
            }
            "finance.payment" => {
                let response = client
                    .get_payment(shared::authenticated(
                        &authority,
                        finance_wire::GetFinancePaymentRequest {
                            metadata: Some(request_metadata(
                                &authority,
                                route,
                                "finance",
                                "get payment owner detail",
                            )),
                            payment_id: id.clone(),
                        },
                    )?)
                    .await
                    .map_err(shared::delivery_status)?
                    .into_inner();
                let projection = freshness(response.metadata)?;
                let owner_capabilities = response.owner_capabilities;
                let value = response
                    .payment
                    .ok_or_else(|| DeliveryError::NotFound(meta(false)))?;
                if value.payment_id != id || value.source_version == 0 {
                    return Err(invalid());
                }
                let amount = value.amount.ok_or_else(invalid)?;
                let status = match finance_wire::FinancePaymentStatus::try_from(value.status).ok() {
                    Some(finance_wire::FinancePaymentStatus::Draft) => "draft",
                    Some(finance_wire::FinancePaymentStatus::Released) => "released",
                    _ => return Err(invalid()),
                };
                let capabilities = finance_capabilities(
                    owner_capabilities,
                    value.source_version,
                    "finance.payment",
                    projection,
                );
                Ok(finance_state(
                    FinanceOwnerRecord::Payment(PaymentOwnerDetail {
                        record,
                        version: value.source_version,
                        payment_code: value.payment_code,
                        payee_label: value.supplier_id,
                        currency: amount.currency,
                        amount_minor: amount.minor_units,
                        status: status_text(status),
                        invoices: Vec::new(),
                    }),
                    projection,
                    capabilities,
                ))
            }
            _ => Err(DeliveryError::Validation(meta(false))),
        }
    }

    fn receipt(
        record: StableRecordRef,
        expected_version: u64,
        value: Option<CommandReceipt>,
    ) -> Result<(StableRecordRef, u64, bool), DeliveryError> {
        let value = value.ok_or_else(invalid)?;
        let aggregate = value.aggregate.ok_or_else(invalid)?;
        if aggregate.aggregate_id != record.id().to_string()
            || aggregate.version <= expected_version
            || value.committed_events.is_empty()
        {
            return Err(invalid());
        }
        Ok((record, aggregate.version, value.replayed))
    }

    pub(super) async fn mutate_procurement(
        mutation: ProcurementOwnerMutation,
    ) -> Result<ProcurementOwnerMutationReceipt, DeliveryError> {
        let authority = shared::feature_authority()?;
        let route = shared::resolve_holding_route(
            &authority,
            &FeatureConsistency::Eventual,
            "procurement",
            "resolve procurement owner command route",
        )
        .await?;
        let mut client = procurement_wire::procurement_command_service_client::ProcurementCommandServiceClient::new(shared::channel(&authority).await?);
        let (record, expected_version, wire_receipt) = match mutation {
            ProcurementOwnerMutation::ApproveRequisition {
                record,
                expected_version,
            } => {
                if record.record_type().as_str() != "procurement.requisition"
                    || expected_version == 0
                {
                    return Err(DeliveryError::Validation(meta(false)));
                }
                let correlation = Uuid::new_v4();
                let response = client
                    .approve_requisition(shared::authenticated(
                        &authority,
                        procurement_wire::PurchaseRequisitionIdentityRequest {
                            metadata: Some(shared::owner_command_metadata(
                                &authority,
                                route,
                                "procurement",
                                "approve-requisition",
                                record.id().as_uuid(),
                                expected_version,
                                correlation,
                            )),
                            requisition_id: record.id().to_string(),
                        },
                    )?)
                    .await
                    .map_err(shared::delivery_status)?
                    .into_inner();
                (record, expected_version, response.receipt)
            }
            ProcurementOwnerMutation::ReleasePurchaseOrder {
                record,
                expected_version,
            } => {
                if record.record_type().as_str() != "procurement.purchase-order"
                    || expected_version == 0
                {
                    return Err(DeliveryError::Validation(meta(false)));
                }
                let correlation = Uuid::new_v4();
                let response = client
                    .release(shared::authenticated(
                        &authority,
                        procurement_wire::ReleasePurchaseOrderRequest {
                            metadata: Some(shared::owner_command_metadata(
                                &authority,
                                route,
                                "procurement",
                                "release-purchase-order",
                                record.id().as_uuid(),
                                expected_version,
                                correlation,
                            )),
                            purchase_order_id: record.id().to_string(),
                        },
                    )?)
                    .await
                    .map_err(shared::delivery_status)?
                    .into_inner();
                (record, expected_version, response.receipt)
            }
            ProcurementOwnerMutation::AcceptGoodsReceipt {
                record,
                expected_version,
            } => {
                if record.record_type().as_str() != "procurement.receipt" || expected_version == 0 {
                    return Err(DeliveryError::Validation(meta(false)));
                }
                let correlation = Uuid::new_v4();
                let response = client
                    .accept_goods_receipt(shared::authenticated(
                        &authority,
                        procurement_wire::GoodsReceiptIdentityRequest {
                            metadata: Some(shared::owner_command_metadata(
                                &authority,
                                route,
                                "procurement",
                                "accept-goods-receipt",
                                record.id().as_uuid(),
                                expected_version,
                                correlation,
                            )),
                            goods_receipt_id: record.id().to_string(),
                        },
                    )?)
                    .await
                    .map_err(shared::delivery_status)?
                    .into_inner();
                (record, expected_version, response.receipt)
            }
        };
        let (record, version, replayed) = receipt(record, expected_version, wire_receipt)?;
        Ok(ProcurementOwnerMutationReceipt {
            record,
            version,
            replayed,
        })
    }

    fn finance_wire_source(
        record: &StableRecordRef,
        version: u64,
        aggregate_kind: &'static str,
        expected_type: &'static str,
    ) -> Result<finance_wire::FinanceVersionedRecordRef, DeliveryError> {
        if version == 0 || record.record_type().as_str() != expected_type {
            return Err(DeliveryError::Validation(meta(false)));
        }
        Ok(finance_wire::FinanceVersionedRecordRef {
            context: "procurement".into(),
            aggregate_kind: aggregate_kind.into(),
            aggregate_id: record.id().to_string(),
            aggregate_version: version,
        })
    }

    pub(super) async fn mutate_finance(
        mutation: FinanceOwnerMutation,
    ) -> Result<FinanceOwnerMutationReceipt, DeliveryError> {
        let authority = shared::feature_authority()?;
        let route = shared::resolve_holding_route(
            &authority,
            &FeatureConsistency::Eventual,
            "finance",
            "resolve finance owner command route",
        )
        .await?;
        let mut client = finance_wire::finance_payables_command_service_client::FinancePayablesCommandServiceClient::new(shared::channel(&authority).await?);
        let (record, expected_version, wire_receipt) = match mutation {
            FinanceOwnerMutation::PostVendorInvoice {
                record,
                expected_version,
                purchase_order,
                purchase_order_version,
                goods_receipt,
                goods_receipt_version,
            } => {
                if record.record_type().as_str() != "finance.vendor-invoice"
                    || expected_version == 0
                {
                    return Err(DeliveryError::Validation(meta(false)));
                }
                let correlation = Uuid::new_v4();
                let response = client
                    .post_vendor_invoice(shared::authenticated(
                        &authority,
                        finance_wire::PostFinanceVendorInvoiceRequest {
                            metadata: Some(shared::owner_command_metadata(
                                &authority,
                                route,
                                "finance",
                                "post-vendor-invoice",
                                record.id().as_uuid(),
                                expected_version,
                                correlation,
                            )),
                            vendor_invoice_id: record.id().to_string(),
                            purchase_order: Some(finance_wire_source(
                                &purchase_order,
                                purchase_order_version,
                                "purchase_order",
                                "procurement.purchase-order",
                            )?),
                            receipt: Some(finance_wire_source(
                                &goods_receipt,
                                goods_receipt_version,
                                "receipt",
                                "procurement.receipt",
                            )?),
                        },
                    )?)
                    .await
                    .map_err(shared::delivery_status)?
                    .into_inner();
                (record, expected_version, response.receipt)
            }
            FinanceOwnerMutation::ReleasePayment {
                record,
                expected_version,
            } => {
                if record.record_type().as_str() != "finance.payment" || expected_version == 0 {
                    return Err(DeliveryError::Validation(meta(false)));
                }
                let correlation = Uuid::new_v4();
                let response = client
                    .release_payment(shared::authenticated(
                        &authority,
                        finance_wire::ReleaseFinancePaymentRequest {
                            metadata: Some(shared::owner_command_metadata(
                                &authority,
                                route,
                                "finance",
                                "release-payment",
                                record.id().as_uuid(),
                                expected_version,
                                correlation,
                            )),
                            payment_id: record.id().to_string(),
                        },
                    )?)
                    .await
                    .map_err(shared::delivery_status)?
                    .into_inner();
                (record, expected_version, response.receipt)
            }
        };
        let (record, version, replayed) = receipt(record, expected_version, wire_receipt)?;
        Ok(FinanceOwnerMutationReceipt {
            record,
            version,
            replayed,
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn owner_delivery_is_same_origin_and_capability_fail_closed() {
        let source = include_str!("owner_detail_delivery.rs");
        assert!(source.contains("/api/server-fn/owner-detail/procurement/load"));
        assert!(source.contains("/api/server-fn/owner-detail/finance/mutate"));
        assert!(source.contains("CurrentReadOnly"));
        assert!(source.contains("authorized_source"));
        assert!(source.contains("DirectoryFreshness::Fresh"));
        assert!(source.contains("validate_mutation_request"));
        assert!(source.contains("Freshness::Current"));
        assert!(source.contains("value.source_version != source_version"));
        assert!(source.contains("capability_evaluation_is_fresh"));
        let navigation_derived_capabilities = ["action_", "capabilities"].concat();
        assert!(!source.contains(&navigation_derived_capabilities));
    }
}
