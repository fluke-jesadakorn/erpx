//! Actual owner references and Finance state for Maintenance close. It grants
//! no permission. Work's late CAS precedes the original pending-journal denial.
use erpx_server_finance::public::journal_settlement::{
    JournalSettlementImage, JournalSettlementScan,
};
use erpx_server_inventory::public::maintenance_settlement::{
    CurrentMaintenanceSettlementReferences, MaintenanceSettlementScope,
    read_maintenance_settlement_references,
};
use erpx_server_maintenance::public::work_source::{
    CurrentMaintenanceWork, MaintenanceWorkSourceFacts,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind, HoldingId};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
pub struct CurrentMaintenancePartsSettlement {
    facts: MaintenanceWorkSourceFacts,
    holding: HoldingId,
    references: CurrentMaintenanceSettlementReferences,
    journals: JournalSettlementImage,
}
impl CurrentMaintenancePartsSettlement {
    pub async fn retain(
        c: &mut PgConnection,
        source: &CurrentMaintenanceWork<'_>,
    ) -> Result<Self, Error> {
        let facts = source.facts().clone();
        let holding = source.holding_id().clone();
        let references = read_maintenance_settlement_references(
            c,
            MaintenanceSettlementScope {
                holding_id: holding.clone(),
                company_id: facts.company_id.clone(),
                work_id: facts.id.clone(),
            },
        )
        .await?;
        let journals = scan(c, &references).await?;
        Ok(Self {
            facts,
            holding,
            references,
            journals,
        })
    }
    /// Checks only the original factual set; after intentional Work mutation the
    /// caller must also retain its original Work/Asset/Identity effect receipt.
    pub async fn require_current(
        &self,
        c: &mut PgConnection,
        source: &CurrentMaintenanceWork<'_>,
    ) -> Result<(), Error> {
        if source.facts() != &self.facts || source.holding_id() != &self.holding {
            return Err(changed().into());
        }
        self.references.require_current(c).await?;
        let fresh = scan(c, &self.references).await?;
        self.journals.require_same(c, &fresh).await
    }
    /// Invoke after the original Work CAS but before Work/effect writes. Exact B0
    /// generic conflict; no fresh Finance/operational permission is introduced.
    pub fn require_posted(&self) -> Result<(), BusinessError> {
        if self.journals.blocks_source_closure() {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "CONFLICT",
                "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
            ));
        }
        Ok(())
    }
}
async fn scan(
    c: &mut PgConnection,
    source: &CurrentMaintenanceSettlementReferences,
) -> Result<JournalSettlementImage, Error> {
    let mut image = JournalSettlementScan::begin(c).await?;
    let mut after = None;
    while let Some(reference) = source.next(c, after.as_deref()).await? {
        image.observe(c, &reference.journal_id).await?;
        after = Some(reference.consumption_id);
    }
    Ok(image.finish())
}
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "MAINTENANCE_SETTLEMENT_SOURCE_CHANGED",
        "The original Maintenance settlement source changed.",
    )
}
