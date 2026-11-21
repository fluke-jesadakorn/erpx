//! Join actual Inventory and Workflow origin witnesses in one Holding unit.
//! This is source evidence only; it does not establish a customer return,
//! current actor authority or any Commercial entitlement.
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

/// Retains actual customer-return provenance, not the current actor or grant.
pub struct CurrentQualityCountCustomerReturn {
    direct: CurrentQualityCountOrigin,
    lineage: CurrentQualityPurchaseLineage<CurrentQualityDispositionCount>,
    commerce: CurrentQualityReturnCommerce,
    approval: CurrentQualityReturnReceipts,
    allocation: Option<CurrentQualityFulfillmentAllocations>,
}
impl CurrentQualityCountCustomerReturn {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.direct.require_current(c).await?;
        self.lineage
            .require_current(c, &mut InventoryLineageCounts)
            .await?;
        self.commerce.require_current(c).await?;
        self.approval.require_current(c).await?;
        if let Some(allocation) = &self.allocation {
            allocation.require_current(c).await?;
        }
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
        self.commerce.require_current(c).await?;
        self.approval.require_current(c).await?;
        if let Some(allocation) = &self.allocation {
            allocation.require_current(c).await?;
        }
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
    let root = lineage.root();
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
    let result = CurrentQualityCountCustomerReturn {
        direct,
        lineage,
        commerce,
        approval,
        allocation,
    };
    result.require_current(c).await?;
    Ok(Some(result))
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
