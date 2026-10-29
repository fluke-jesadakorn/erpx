//! Actual owner facts only; independent Finance/IAM/purchase policy is retained
//! by the surrounding service coordinator, never borrowed from the issuer.
use super::*;
use erpx_server_finance::public::journal_review::CurrentMaintenancePartsJournalTarget;
use erpx_server_inventory::public::consumption_finance::{
    CurrentMaintenanceConsumptionFinance, read_maintenance_consumption_finance,
};
use erpx_server_maintenance::public::finance_lineage::{
    CurrentMaintenanceFinanceLineage, MaintenanceFinanceLineageRequest,
    read_maintenance_finance_lineage,
};

pub(super) struct MaintenancePartsSource {
    target: CurrentMaintenancePartsJournalTarget,
    consumption: CurrentMaintenanceConsumptionFinance,
    lineage: CurrentMaintenanceFinanceLineage,
}
impl MaintenancePartsSource {
    pub(super) async fn retain(
        c: &mut PgConnection,
        target: CurrentMaintenancePartsJournalTarget,
    ) -> Result<Self, Error> {
        let f = target.facts();
        let consumption = read_maintenance_consumption_finance(
            c,
            &f.holding_id,
            &f.company_id,
            &f.consumption_id,
        )
        .await?
        .ok_or_else(unavailable)?;
        let i = consumption.facts();
        if i.journal_id != f.journal_id
            || i.currency != f.currency
            || i.value_minor.to_string() != f.debit_total
            || i.value_minor.to_string() != f.credit_total
        {
            return Err(unavailable());
        }
        let lineage = read_maintenance_finance_lineage(
            c,
            &MaintenanceFinanceLineageRequest {
                holding_id: i.holding_id.clone(),
                company_id: i.company_id.clone(),
                work_id: i.work_id.clone(),
                branch_id: i.branch_id.clone(),
                department_id: i.department_id.clone(),
                minimum_consumption_version: i.source_version,
            },
        )
        .await?
        .ok_or_else(unavailable)?;
        // No current stock/hold or operational actor gate is added to settlement.
        let value = Self {
            target,
            consumption,
            lineage,
        };
        value.require_current(c).await?;
        Ok(value)
    }
    pub(super) async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.target.require_current(c).await?;
        self.consumption.require_current(c).await?;
        self.lineage.require_current(c).await
    }
}
fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::Forbidden,
        "FINANCE_MAINTENANCE_SOURCE_UNAVAILABLE",
        "The actual immutable Maintenance parts source is unavailable.",
    )
    .into()
}
