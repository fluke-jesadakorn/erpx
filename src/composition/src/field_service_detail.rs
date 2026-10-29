//! The existing Service order detail and its retained workspace.
use crate::field_service_reservation_read::FieldServiceRenderedContext;
use crate::field_service_workspace::FieldServiceWorkspace;
use serde::Serialize;
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FieldServiceDetailSpares {
    #[cfg_attr(feature="contracts",ts(type="Array<Record<string, string | number | null>>"),schema(value_type=Vec<Object>))]
    pub stock: Vec<serde_json::Value>,
    #[cfg_attr(feature="contracts",ts(type="Array<Record<string, string | number | null>>"),schema(value_type=Vec<Object>))]
    pub lots: Vec<serde_json::Value>,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceOrderDetail {
    #[serde(flatten)]
    pub workspace: FieldServiceWorkspace,
    #[cfg_attr(feature="contracts",ts(type="Record<string, string | number | null>"),schema(value_type=Object))]
    pub order: serde_json::Value,
    pub spares: FieldServiceDetailSpares,
    #[cfg_attr(feature="contracts",ts(type="Array<Record<string, string | number | null>>"),schema(value_type=Vec<Object>))]
    pub parts: Vec<serde_json::Value>,
    #[cfg_attr(feature="contracts",ts(type="Array<Record<string, string | number | null>>"),schema(value_type=Vec<Object>))]
    pub consumptions: Vec<serde_json::Value>,
    #[cfg_attr(feature="contracts",ts(type="Array<Record<string, string | number | null>>"),schema(value_type=Vec<Object>))]
    pub history: Vec<serde_json::Value>,
    #[cfg_attr(feature="contracts",ts(type="Record<string, string | number | boolean | null> | null"),schema(value_type=Option<Object>))]
    pub billing: Option<serde_json::Value>,
    pub can_edit_visit_files: bool,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FieldServiceOrderDetailResponse {
    pub data: FieldServiceOrderDetail,
    pub context: FieldServiceRenderedContext,
}

use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest,
};
use erpx_server_identity_access::public::effective_authority::{
    AuthorizedWorkContext, HoldingWorkSourcePreparation,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use sqlx::PgConnection;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
type Error = PgTransactionError<BusinessError>;
fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "FIELD_SERVICE_ORDER_NOT_FOUND",
        "Service order not found in this scope.",
    )
}
fn invalid() -> BusinessError {
    crate::field_service_workspace::invalid()
}
#[derive(Clone)]
pub(crate) struct DetailSources;
impl HoldingWorkSourcePreparation for DetailSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            erpx_server_finance::public::field_service_detail::lock_service_detail_finance(c)
                .await?;
            crate::field_service_workspace::WorkspaceSources
                .prepare(c)
                .await?;
            erpx_server_workflow::public::record_origin::lock_record_origin_sources(c).await?;
            erpx_server_field_service::public::detail::lock_detail_sources(c).await?;
            erpx_server_commerce::public::field_service_detail::lock_service_detail_commerce(c)
                .await?;
            erpx_server_quality::public::field_service_detail::lock_service_detail_quality(c)
                .await?;
            erpx_server_inventory::public::field_service_detail::lock_service_detail_inventory(c)
                .await
        })
    }
}
fn row_str<'a>(v: &'a serde_json::Value, k: &str) -> Result<&'a str, BusinessError> {
    v.get(k).and_then(|x| x.as_str()).ok_or_else(invalid)
}
pub(crate) async fn build_detail<C: Clock>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    holding: &HoldingId,
    company: &CompanyId,
    catalogue: &Arc<CommercialCatalogue>,
    clock: &C,
    id: &str,
) -> Result<FieldServiceOrderDetailResponse, Error> {
    let prepared = crate::field_service_workspace::prepare_workspace(
        c, current, holding, company, catalogue, clock,
    )
    .await?;
    let order = prepared
        .reply
        .data
        .orders
        .iter()
        .find(|v| v.get("id").and_then(|x| x.as_str()) == Some(id))
        .cloned()
        .ok_or_else(not_found)?;
    let branch = row_str(&order, "branch_id")?;
    let parts_history = prepared.rights.parts_history;
    let parts_settle = prepared.rights.parts_settle;
    let visits = PgCompanyPurchaseReader::new(holding.clone(), company.clone(), c, catalogue)
        .inspect(
            &PurchaseRequest {
                capabilities: vec!["service.visits".into()],
                purpose: CapabilityPurpose::SettleExisting,
                mode: PackageEnforcementMode::Enforced,
            },
            clock.now(),
        )
        .await?;
    let visits_allowed = visits.require_allowed().is_ok();
    let field =
        erpx_server_field_service::public::detail::read_detail_rows(c, company, id, parts_history)
            .await?;
    let mut products = BTreeSet::new();
    for row in &field.data().parts {
        products.insert(row_str(row, "product_id")?.to_owned());
    }
    let initial = erpx_server_commerce::public::field_service_detail::read_service_detail_commerce(
        c,
        company,
        branch,
        parts_settle,
        &products.iter().cloned().collect::<Vec<_>>(),
    )
    .await?;
    let warehouses = initial
        .data()
        .warehouses
        .iter()
        .map(|x| x.id.clone())
        .collect::<Vec<_>>();
    let stock_keys = initial
        .data()
        .stock
        .iter()
        .map(
            |s| erpx_server_inventory::public::field_service_detail::ServiceDetailStockKey {
                warehouse_id: s.warehouse_id.clone(),
                product_id: s.product_id.clone(),
            },
        )
        .collect::<Vec<_>>();
    let inventory =
        erpx_server_inventory::public::field_service_detail::read_service_detail_inventory(
            c,
            company,
            id,
            &warehouses,
            &stock_keys,
            parts_settle,
            parts_history,
        )
        .await?;
    let mut consumptions = Vec::new();
    let mut journals = BTreeSet::new();
    for text in &inventory.data().consumptions {
        let value: serde_json::Value = serde_json::from_str(text).map_err(|_| invalid())?;
        products.insert(row_str(&value, "product_id")?.to_owned());
        journals.insert(row_str(&value, "journal_id")?.to_owned());
        consumptions.push(value);
    }
    let commerce =
        erpx_server_commerce::public::field_service_detail::read_service_detail_commerce(
            c,
            company,
            branch,
            parts_settle,
            &products.into_iter().collect::<Vec<_>>(),
        )
        .await?;
    let quality = erpx_server_quality::public::field_service_detail::read_service_detail_quality(
        c,
        company,
        &warehouses,
        parts_settle,
    )
    .await?;
    let finance = erpx_server_finance::public::field_service_detail::read_service_detail_journals(
        c,
        company,
        journals.into_iter().collect(),
    )
    .await?;
    let product_map: BTreeMap<_, _> = commerce
        .data()
        .products
        .iter()
        .map(|p| (p.id.as_str(), p))
        .collect();
    let warehouse_map: BTreeMap<_, _> = commerce
        .data()
        .warehouses
        .iter()
        .map(|w| (w.id.as_str(), w))
        .collect();
    let journal_map: BTreeMap<_, _> = finance
        .data()
        .iter()
        .map(|j| (j.id.as_str(), j.status.as_str()))
        .collect();
    let mut parts = Vec::new();
    for row in &field.data().parts {
        let product = product_map
            .get(row_str(row, "product_id")?)
            .ok_or_else(invalid)?;
        let mut row = row.clone();
        let obj = row.as_object_mut().ok_or_else(invalid)?;
        obj.insert("sku".into(), product.sku.clone().into());
        obj.insert("name".into(), product.name.clone().into());
        parts.push(row);
    }
    for row in &mut consumptions {
        let product = product_map
            .get(row_str(row, "product_id")?)
            .ok_or_else(invalid)?;
        let journal = journal_map
            .get(row_str(row, "journal_id")?)
            .ok_or_else(invalid)?;
        let obj = row.as_object_mut().ok_or_else(invalid)?;
        obj.insert("product_name".into(), product.name.clone().into());
        obj.insert("journal_status".into(), (*journal).into());
    }
    let mut stock = Vec::new();
    for s in &initial.data().stock {
        let Some(w) = warehouse_map.get(s.warehouse_id.as_str()) else {
            continue;
        };
        let Some(p) = product_map.get(s.product_id.as_str()).filter(|p| p.active) else {
            continue;
        };
        let holds = quality
            .data()
            .iter()
            .filter(|h| h.warehouse_id == s.warehouse_id && h.product_id == s.product_id);
        let mut whole = false;
        let mut partial: i128 = 0;
        for h in holds {
            if h.quantity_scope == "whole" {
                whole = true
            } else if h.quantity_scope == "partial" {
                partial = partial
                    .checked_add(i128::from(h.held_quantity_milli.unwrap_or(0)))
                    .ok_or_else(invalid)?;
            }
        }
        let available =
            erpx_server_inventory::public::field_service_detail::service_available_milli(
                s.quantity_milli,
                inventory
                    .data()
                    .reservations
                    .iter()
                    .find(|r| r.warehouse_id == s.warehouse_id && r.product_id == s.product_id)
                    .ok_or_else(invalid)?
                    .reserved_milli,
                whole,
                partial,
            );
        stock.push((w.name_rank,p.sku_rank,serde_json::json!({"warehouse_id":s.warehouse_id,"warehouse_name":w.name,"product_id":s.product_id,"sku":p.sku,"product_name":p.name,"unit":p.unit,"tracking_mode":p.tracking_mode,"available_milli":available})));
    }
    stock.sort_by_key(|x| (x.0, x.1));
    stock.truncate(200);
    let stock = stock.into_iter().map(|x| x.2).collect();
    let mut lots = Vec::new();
    for l in &inventory.data().lots {
        if quality.data().iter().any(|h| {
            h.warehouse_id == l.warehouse_id && h.lot_id.as_deref() == Some(l.lot_id.as_str())
        }) {
            continue;
        }
        lots.push(serde_json::json!({"lot_id":l.lot_id,"warehouse_id":l.warehouse_id,"product_id":l.product_id,"lot_code":l.lot_code,"serial_number":l.serial_number,"quantity_milli":l.quantity_milli,"version":l.version,"expiry_date":l.expiry_date}));
    }
    lots.truncate(500);
    let mut billing = field.data().billing.clone();
    let mut field_link = None;
    let mut origin = None;
    let mut invoice = None;
    if billing.is_some() && current.has_permission("finance.read") {
        let candidate =
            erpx_server_field_service::public::detail::read_service_billing_link(c, company, id)
                .await?;
        if let Some(source) = candidate {
            let f = source.facts();
            let request = erpx_server_workflow::public::record_origin::RawRecordOriginRequest {
                command_key: f.key.clone(),
                operation: "field.order.release-billing".into(),
                record_id: id.into(),
                creator_user_id: f.actor.clone(),
                company_id: company.clone(),
            };
            let receipt =
                erpx_server_workflow::public::record_origin::read_raw_record_origin(c, request)
                    .await?;
            if let Some(receipt) = receipt {
                let link=erpx_server_finance::public::field_service_detail::read_service_detail_invoice_link(c,erpx_server_finance::public::field_service_detail::ServiceInvoiceLinkRequest{company:company.clone(),key:f.key.clone(),order:f.order.clone(),customer:f.customer.clone(),currency:f.currency.clone(),amount:f.amount,pricing_date:f.pricing_date.clone()}).await?;
                if let Some(value) = link.invoice_id() {
                    billing
                        .as_mut()
                        .and_then(|v| v.as_object_mut())
                        .ok_or_else(invalid)?
                        .insert("invoice_id".into(), value.into());
                }
                invoice = Some(link);
                origin = Some(receipt);
            }
            field_link = Some(source);
        }
    }
    if let Some(value) = billing.as_mut() {
        value
            .as_object_mut()
            .ok_or_else(invalid)?
            .entry("invoice_id")
            .or_insert(serde_json::Value::Null);
    }
    let has_asset = order.get("asset_id").is_some_and(|v| !v.is_null())
        || order
            .get("requires_asset_capability")
            .and_then(|v| v.as_i64())
            .is_some_and(|v| v > 0);
    let can_edit_visit_files = erpx_server_field_service::public::detail::can_edit_visit_files(
        current.has_permission("records.write"),
        visits_allowed,
        parts_settle,
        has_asset,
        row_str(&order, "status")?,
    );
    let reply = FieldServiceOrderDetailResponse {
        data: FieldServiceOrderDetail {
            workspace: prepared.reply.data.clone(),
            order,
            spares: FieldServiceDetailSpares { stock, lots },
            parts,
            consumptions,
            history: field.data().history.clone(),
            billing,
            can_edit_visit_files,
        },
        context: prepared.reply.context.clone(),
    };
    let serialized = serde_json::to_value(&reply).map_err(|_| invalid())?;
    if !crate::field_service_workspace::safe_json(&serialized) {
        return Err(invalid().into());
    }
    serde_json::to_writer(
        crate::field_service_reservation_read::ReplyBudget(0),
        &serialized,
    )
    .map_err(|_| invalid())?;
    field.require_current(c).await?;
    initial.require_current(c).await?;
    commerce.require_current(c).await?;
    inventory.require_current(c).await?;
    quality.require_current(c).await?;
    finance.require_current(c).await?;
    if let Some(link) = field_link {
        link.require_current(c).await?
    }
    if let Some(receipt) = origin {
        erpx_server_workflow::public::record_origin::verify_raw_record_origin(c, &receipt).await?
    }
    if let Some(link) = invoice {
        link.require_current(c).await?
    }
    let now = clock.now();
    let checked = PgCompanyPurchaseReader::new(holding.clone(), company.clone(), c, catalogue)
        .revalidate(&visits.witness, now)
        .await?;
    if checked.require_allowed().is_ok() != visits_allowed {
        return Err(invalid().into());
    }
    prepared
        .require_current(c, current, holding, company, catalogue, clock)
        .await?;
    Ok(reply)
}
