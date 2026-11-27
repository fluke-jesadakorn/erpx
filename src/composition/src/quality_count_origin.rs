//! Join actual Inventory and Workflow origin witnesses in one Holding unit.
//! This is source evidence only; it does not establish a customer return,
//! current actor authority or any Commercial entitlement.
use erpx_server_commerce::public::quality_receipt_origin::{
    CurrentQualityReceiptCommerce, inspect_quality_receipt_commerce,
};
use erpx_server_commerce::public::quality_return_origin::{
    CurrentQualityReturnCommerce, QualityReturnSelector, inspect_quality_return_commerce,
};
use erpx_server_inventory::public::counts::{
    CurrentDirectQualityCountOrigin, CurrentInventoryQualityCount,
    inspect_direct_quality_count_origin,
};
use erpx_server_inventory::public::quality_disposition_count::{
    CurrentQualityDispositionCount, QualityDispositionCountRequest,
    inspect_quality_disposition_count,
};
use erpx_server_inventory::public::quality_fulfillment_allocation::{
    CurrentQualityFulfillmentAllocations, QualityFulfillmentAllocationRequest,
    inspect_quality_fulfillment_allocations,
};
use erpx_server_quality::public::purchase_lineage::{
    CountLineageRequest, CurrentQualityPurchaseLineage, QualityLineageCountPort,
    QualityLineageRequest, inspect_quality_purchase_lineage,
};
use erpx_server_quality::public::quality_commands::RetainedQualityCountDisposition;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, CompanyId, RecordId, UserId};
use erpx_server_workflow::public::quality_return_receipts::{
    CurrentQualityReturnReceipts, CurrentQualityScrapRequestReceipt, QualityReturnReceiptScope,
    QualityScrapRequestReceiptScope, inspect_quality_return_receipts,
    inspect_quality_scrap_request_receipt,
};
use serde_json::Value;
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;

pub struct CurrentQualityCountOrigin {
    inventory: CurrentDirectQualityCountOrigin,
    receipt: CurrentQualityScrapRequestReceipt,
}

struct InventoryLineageCounts;
impl QualityLineageCountPort for InventoryLineageCounts {
    type Witness = CurrentQualityDispositionCount;
    fn inspect<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        r: CountLineageRequest,
    ) -> PgWorkFuture<'a, Option<Self::Witness>, BusinessError> {
        Box::pin(async move {
            inspect_quality_disposition_count(
                c,
                QualityDispositionCountRequest {
                    company_id: r.company_id,
                    branch_id: r.branch_id,
                    warehouse_id: r.warehouse_id,
                    count_id: r.count_id,
                    hold_id: r.hold_id,
                },
            )
            .await
        })
    }
    fn require_current<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        witness: &'a Self::Witness,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move { witness.require_current(c).await })
    }
}

/// Commerce return facts, the Workflow approval receipt and the Inventory
/// original allocation together. Labels alone never create this relationship.
struct QualityReturnOriginProof {
    commerce: CurrentQualityReturnCommerce,
    approval: CurrentQualityReturnReceipts,
    allocation: Option<CurrentQualityFulfillmentAllocations>,
}
impl QualityReturnOriginProof {
    async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.commerce.require_current(c).await?;
        self.approval.require_current(c).await?;
        if let Some(allocation) = &self.allocation {
            allocation.require_current(c).await?;
        }
        Ok(())
    }
}

/// Prove a customer-return root from the actual owner witnesses. The selector
/// and source label are only lookup keys; every returned fact is compared with
/// the proven lineage root before any capability can be derived.
async fn inspect_quality_return_origin(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&RecordId>,
    root: &Value,
) -> Result<Option<QualityReturnOriginProof>, Error> {
    if !root["spec_id"].is_null() || root["source_type"].as_str() != Some("customer_return") {
        return Ok(None);
    }
    let Some(source) = root["source_id"].as_str() else {
        return Ok(None);
    };
    let lot = root["lot_id"].as_str();
    let selector = if lot.is_some() {
        QualityReturnSelector::TrackedAllocation(source.to_owned())
    } else {
        QualityReturnSelector::UntrackedLine(source.to_owned())
    };
    let commerce = inspect_quality_return_commerce(c, company, branch, selector).await?;
    let Some(facts) = commerce.facts() else {
        return Ok(None);
    };
    if root["company_id"].as_str() != Some(facts.company_id.as_str())
        || root["branch_id"].as_str() != Some(facts.branch_id.as_str())
        || root["warehouse_id"].as_str() != Some(facts.warehouse_id.as_str())
        || root["product_id"].as_str() != Some(facts.product_id.as_str())
        || root["source_id"].as_str() != Some(facts.source_id.as_str())
        || root["created_by"].as_str() != Some(facts.approver.as_str())
        || root["held_quantity_milli"].as_i64() != Some(facts.quantity_milli)
        || lot != facts.lot_id.as_deref()
    {
        return Ok(None);
    }
    let suffix = match facts.lot_ordinal {
        Some(n) => format!("-{}-{n}", facts.line_ordinal),
        None => format!("-{}", facts.line_ordinal),
    };
    let Some(key) = root["id"]
        .as_str()
        .and_then(|v| v.strip_prefix("quality-return-"))
        .and_then(|v| v.strip_suffix(&suffix))
        .filter(|v| !v.is_empty())
    else {
        return Ok(None);
    };
    let return_id = RecordId::new(facts.return_id.as_str())?;
    let approver = UserId::new(facts.approver.as_str())?;
    let approval = inspect_quality_return_receipts(
        c,
        QualityReturnReceiptScope {
            company,
            return_id: &return_id,
            actor: &approver,
        },
    )
    .await?;
    if !approval.command_ids().any(|id| id == key) {
        return Ok(None);
    }
    let allocation = match (
        &facts.fulfillment_allocation_id,
        &facts.lot_id,
        facts.lot_ordinal,
    ) {
        (Some(id), Some(lot), Some(_)) => {
            let proof = inspect_quality_fulfillment_allocations(
                c,
                QualityFulfillmentAllocationRequest::OriginalAllocation {
                    allocation_id: id.clone(),
                    fulfillment_line_id: facts.fulfillment_line_id.clone(),
                    lot_id: lot.clone(),
                },
            )
            .await?;
            if proof.rows().len() != 1 {
                return Ok(None);
            }
            Some(proof)
        }
        (None, None, None) => None,
        _ => return Ok(None),
    };
    Ok(Some(QualityReturnOriginProof {
        commerce,
        approval,
        allocation,
    }))
}

/// Retains actual customer-return provenance, not the current actor or grant.
pub struct CurrentQualityCountCustomerReturn {
    direct: CurrentQualityCountOrigin,
    lineage: CurrentQualityPurchaseLineage<CurrentQualityDispositionCount>,
    origin: QualityReturnOriginProof,
}
impl CurrentQualityCountCustomerReturn {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.direct.require_current(c).await?;
        self.lineage
            .require_current(c, &mut InventoryLineageCounts)
            .await?;
        self.origin.require_current(c).await?;
        Ok(())
    }
    pub async fn require_after_count(
        &self,
        c: &mut PgConnection,
        finalized: &RetainedQualityCountDisposition<CurrentInventoryQualityCount>,
    ) -> Result<(), Error> {
        self.direct.require_current(c).await?;
        self.lineage
            .require_after_count(c, &mut InventoryLineageCounts, finalized)
            .await?;
        self.origin.require_current(c).await?;
        Ok(())
    }
}

/// The caller must acquire all relevant owner fences before its first source
/// snapshot. A nonqualifying source returns None; it never grants permission.
pub async fn inspect_quality_count_customer_return(
    c: &mut PgConnection,
    direct: CurrentQualityCountOrigin,
    company: &CompanyId,
    branch: Option<&RecordId>,
) -> Result<Option<CurrentQualityCountCustomerReturn>, Error> {
    direct.require_current(c).await?;
    let lineage = inspect_quality_purchase_lineage(
        c,
        &mut InventoryLineageCounts,
        QualityLineageRequest {
            company_id: company.clone(),
            branch_id: branch.map(|v| v.as_str().to_owned()),
            hold_id: direct.inventory.hold_id().to_owned(),
        },
    )
    .await?;
    let Some(origin) = inspect_quality_return_origin(c, company, branch, lineage.root()).await?
    else {
        return Ok(None);
    };
    let result = CurrentQualityCountCustomerReturn {
        direct,
        lineage,
        origin,
    };
    result.require_current(c).await?;
    Ok(Some(result))
}

/// The proven original-purchase kind of a Quality hold. Advanced is the
/// least-privilege fallback when no owner witness proves a narrower origin.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QualityCancelSourceKind {
    Advanced,
    Receipt,
    CustomerReturn,
}

/// A receipt root is proven by the Inventory quarantine allocation joined to
/// the Commerce purchase-line facts; the label alone is never sufficient.
async fn inspect_quality_receipt_origin(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&RecordId>,
    root: &Value,
) -> Result<Option<(CurrentQualityReceiptCommerce, CurrentQualityFulfillmentAllocations)>, Error> {
    let Some(hold_id) = root["id"].as_str() else {
        return Ok(None);
    };
    let Some(lot) = root["lot_id"].as_str() else {
        return Ok(None);
    };
    let quantity = root["held_quantity_milli"].as_i64();
    let allocations = inspect_quality_fulfillment_allocations(
        c,
        QualityFulfillmentAllocationRequest::QuarantineHold {
            hold_id: hold_id.to_owned(),
        },
    )
    .await?;
    let mut line = None;
    for row in allocations.rows() {
        if row.lot_id == lot && Some(row.quantity_milli) == quantity {
            if line.is_some() {
                return Ok(None);
            }
            line = Some(row.fulfillment_line_id.clone());
        }
    }
    let Some(line) = line else {
        return Ok(None);
    };
    let commerce = inspect_quality_receipt_commerce(c, company, branch, &line).await?;
    let Some(facts) = commerce.facts() else {
        return Ok(None);
    };
    if Some(facts.company_id.as_str()) != root["company_id"].as_str()
        || Some(facts.branch_id.as_str()) != root["branch_id"].as_str()
        || Some(facts.warehouse_id.as_str()) != root["warehouse_id"].as_str()
        || Some(facts.product_id.as_str()) != root["product_id"].as_str()
        || facts.fulfillment_line_id != line
    {
        return Ok(None);
    }
    Ok(Some((commerce, allocations)))
}

/// The proven original purchase of the hold a cancellation settles. This is a
/// source witness only; current actor, permission and purchase authority stay
/// with the caller's serializable command unit.
pub struct CurrentQualityCancelPurchaseSource {
    lineage: CurrentQualityPurchaseLineage<CurrentQualityDispositionCount>,
    kind: QualityCancelSourceKind,
    receipt: Option<(CurrentQualityReceiptCommerce, CurrentQualityFulfillmentAllocations)>,
    origin: Option<QualityReturnOriginProof>,
}
impl CurrentQualityCancelPurchaseSource {
    pub fn kind(&self) -> QualityCancelSourceKind {
        self.kind
    }
    pub fn capabilities(&self) -> &'static [&'static str] {
        match self.kind {
            QualityCancelSourceKind::Advanced => &["supply.quality"],
            QualityCancelSourceKind::Receipt => &["supply.receiving", "supply.stock"],
            QualityCancelSourceKind::CustomerReturn => &["sales.orders", "inventory.stock-core"],
        }
    }

    /// Re-walk the proven remaining-hold ancestry and re-read every owner
    /// witness in the caller's unit, including the final after-image check.
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.lineage
            .require_current(c, &mut InventoryLineageCounts)
            .await?;
        if let Some((commerce, allocation)) = &self.receipt {
            commerce.require_current(c).await?;
            allocation.require_current(c).await?;
        }
        if let Some(origin) = &self.origin {
            origin.require_current(c).await?;
        }
        Ok(())
    }
}

/// Caller must acquire the Quality lineage, Commerce, Inventory, Workflow and
/// notification fences before this first source snapshot. A nonqualifying
/// origin falls back to advanced; it never grants a narrower purchase.
pub async fn inspect_quality_cancel_purchase_source(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&RecordId>,
    hold_id: &RecordId,
) -> Result<CurrentQualityCancelPurchaseSource, Error> {
    let lineage = inspect_quality_purchase_lineage(
        c,
        &mut InventoryLineageCounts,
        QualityLineageRequest {
            company_id: company.clone(),
            branch_id: branch.map(|v| v.as_str().to_owned()),
            hold_id: hold_id.as_str().to_owned(),
        },
    )
    .await?;
    let (kind, receipt, origin) = if lineage.root()["spec_id"].is_null() {
        match lineage.root()["source_type"].as_str() {
            Some("goods_receipt") => {
                match inspect_quality_receipt_origin(c, company, branch, lineage.root()).await? {
                    Some(proof) => (QualityCancelSourceKind::Receipt, Some(proof), None),
                    None => (QualityCancelSourceKind::Advanced, None, None),
                }
            }
            Some("customer_return") => {
                match inspect_quality_return_origin(c, company, branch, lineage.root()).await? {
                    Some(proof) => (QualityCancelSourceKind::CustomerReturn, None, Some(proof)),
                    None => (QualityCancelSourceKind::Advanced, None, None),
                }
            }
            _ => (QualityCancelSourceKind::Advanced, None, None),
        }
    } else {
        (QualityCancelSourceKind::Advanced, None, None)
    };
    let result = CurrentQualityCancelPurchaseSource {
        lineage,
        kind,
        receipt,
        origin,
    };
    result.require_current(c).await?;
    Ok(result)
}

impl CurrentQualityCountOrigin {
    pub fn inventory(&self) -> &CurrentDirectQualityCountOrigin {
        &self.inventory
    }

    /// The caller retains this through Workflow finalization and separately
    /// revalidates live actor, return lineage and purchase witnesses.
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.inventory.require_current(c).await?;
        self.receipt.require_current(c).await?;
        Ok(())
    }
}

/// All Inventory, Quality and Workflow fences must be acquired before the
/// first protected snapshot. Missing owner or receipt source does not qualify.
pub async fn inspect_quality_count_origin(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&RecordId>,
    count_id: &RecordId,
) -> Result<Option<CurrentQualityCountOrigin>, Error> {
    let Some(inventory) = inspect_direct_quality_count_origin(
        c,
        company.as_str(),
        branch.map(RecordId::as_str),
        count_id.as_str(),
    )
    .await?
    else {
        return Ok(None);
    };
    let hold_id = RecordId::new(inventory.hold_id())?;
    let creator = UserId::new(inventory.creator())?;
    let Some(receipt) = inspect_quality_scrap_request_receipt(
        c,
        QualityScrapRequestReceiptScope {
            command_id: inventory.source_command_id(),
            company,
            hold_id: &hold_id,
            count_id,
            actor: &creator,
        },
    )
    .await?
    else {
        return Ok(None);
    };
    let result = CurrentQualityCountOrigin { inventory, receipt };
    result.require_current(c).await?;
    Ok(Some(result))
}
