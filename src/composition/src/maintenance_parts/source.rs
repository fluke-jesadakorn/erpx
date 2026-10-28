//! Concrete actual Maintenance/Inventory/Finance/Commercial source composition.
//! Only the enclosing Workflow coordinator can supply exact replay, notification,
//! audit/outbox and commit. No actor/permission labels or permissive port factory.
use super::{Error, changed};
use erpx_server_commerce::public::inventory_consumption::CommercePostingSettings;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, PurchaseWitness, current_package_catalogue, lock_commercial_authority_sources,
};
use erpx_server_finance::public::{
    FinanceActor,
    inventory_consumption::{
        CurrentConsumptionFinance, InventoryConsumptionMapping,
        lock_inventory_consumption_finance_sources, read_inventory_consumption_finance,
    },
    parts_draft::{
        PartsDraftExpected, PartsDraftFacts, PartsOwnerKind, PgPartsDraftOwnerSource,
        lock_owned_parts_draft_sources,
    },
};
use erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation;
use erpx_server_inventory::public::{consumption::*, postgres_consumption::*};
use erpx_server_maintenance::public::{
    MaintenanceTransition,
    work_commands::MaintenanceWorkCommand,
    work_source::{
        CurrentMaintenanceWork, MaintenanceWorkSourcePreparation, PgMaintenanceIssuePartPlan,
    },
    work_wire::MaintenanceChangeAction,
};
use erpx_server_runtime::public::postgres::PgWorkFuture;
use erpx_server_shared::public::{BusinessError, RecordId, TimestampMillis};
use sqlx::PgConnection;

/// Fixed utility-only owner bundle, before the authorizer's first Holding
/// snapshot. Actual row policy still runs through the retained Work reference.
#[derive(Clone)]
pub struct MaintenancePartsSourcePreparation;
impl HoldingWorkSourcePreparation for MaintenancePartsSourcePreparation {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_owned_parts_draft_sources(c).await?;
            lock_commercial_authority_sources(c).await?;
            erpx_server_collaboration::public::maintenance_notifications::lock_maintenance_notification_sources(c).await?;
            MaintenanceWorkSourcePreparation.prepare(c).await?;
            lock_unreserved_consumption_sources(c).await?;
            // This utility never queries holding-specific Finance identities.
            let mut finance = MaintenanceFinanceFacts { holding: None };
            lock_inventory_consumption_sources(c, &mut finance).await
        })
    }
}
/// Real Finance public-facade bridge used by the Maintenance composition. The
/// Holding identity comes from the sealed native current Work context.
pub(super) struct MaintenanceFinanceFacts {
    pub(super) holding: Option<erpx_server_shared::public::HoldingId>,
}
impl PgConsumptionFinancePort for MaintenanceFinanceFacts {
    type Source = CurrentConsumptionFinance;
    async fn lock_sources(&mut self, c: &mut PgConnection) -> Result<(), Error> {
        lock_inventory_consumption_finance_sources(c).await
    }
    async fn inspect(
        &mut self,
        c: &mut PgConnection,
        s: &CommercePostingSettings,
        date: &str,
    ) -> Result<Self::Source, Error> {
        let holding = self.holding.as_ref().ok_or_else(changed)?;
        read_inventory_consumption_finance(
            c,
            holding,
            &InventoryConsumptionMapping {
                company_id: s.company_id.clone(),
                chart_id: s.chart_id.clone(),
                inventory_account_id: s.inventory_account_id.clone(),
                cogs_account_id: s.cogs_account_id.clone(),
            },
            date,
        )
        .await
    }
    fn config(s: &Self::Source) -> Option<ConsumptionPostingConfig> {
        s.config().map(|c| ConsumptionPostingConfig {
            chart_id: c.chart_id.clone(),
            currency: c.currency.clone(),
            base_currency: c.base_currency.clone(),
            inventory_account_id: c.inventory_account_id.clone(),
            expense_account_id: c.cogs_account_id.clone(),
        })
    }
    fn periods(s: &Self::Source) -> Vec<RecordId> {
        s.period_ids().to_vec()
    }
    async fn require_current(
        &mut self,
        c: &mut PgConnection,
        s: &Self::Source,
    ) -> Result<(), Error> {
        s.require_current(c).await
    }
}
/// Sealed actual-owner implementation of Finance's mandatory parts source. It
/// borrows the first Inventory and Work proofs throughout every Finance phase.
/// Package mode is trusted runtime configuration, never inbound command data.
pub struct PgMaintenancePartsSource<'p, 'a> {
    source: &'p CurrentMaintenanceWork<'a>,
    work: &'p PgMaintenanceIssuePartPlan,
    inventory: &'p PgUnreservedConsumptionEffects<CurrentConsumptionFinance>,
    facts: PartsDraftFacts,
    catalogue: CommercialCatalogue,
    purchase: PurchaseWitness,
}
impl<'p, 'a> PgMaintenancePartsSource<'p, 'a> {
    /// Invoke only for a new command after original exact Workflow replay. An
    /// exact existing receipt needs its own History purchase path instead.
    pub async fn retain(
        c: &mut PgConnection,
        source: &'p CurrentMaintenanceWork<'a>,
        work: &'p PgMaintenanceIssuePartPlan,
        inventory: &'p PgUnreservedConsumptionEffects<CurrentConsumptionFinance>,
        mode: PackageEnforcementMode,
        now: TimestampMillis,
    ) -> Result<Self, Error> {
        // The sealed owner source was minted only after current records.write.
        work.require_current_source(c, source, now).await?;
        let command = work.command();
        let p = inventory.plan();
        let i = inventory.identity();
        let MaintenanceChangeAction::IssuePart {
            warehouse_id,
            product_id,
            quantity_milli,
            entry_date,
            allocations,
        } = &command.input().action
        else {
            return Err(changed().into());
        };
        if i.owner_key != *command.key()
            || i.work_id != source.facts().id
            || i.work_version_after != work.version_after()
            || i.department_id != source.facts().department_id
            || i.actor_id != *source.actor_user_id()
            || i.reason != command.input().reason
            || p.selection.company_id != source.facts().company_id
            || p.selection.branch_id != source.facts().branch_id
            || p.selection.warehouse_id.as_str() != warehouse_id
            || p.selection.product_id.as_str() != product_id
            || p.selection.quantity_milli != *quantity_milli
            || &p.selection.entry_date != entry_date
            || allocations.len() != p.selection.allocations.len()
            || allocations
                .iter()
                .zip(&p.selection.allocations)
                .any(|(a, b)| {
                    a.lot_id != b.lot_id
                        || a.quantity_milli != b.quantity_milli
                        || a.balance_version != b.balance_version
                })
        {
            return Err(changed().into());
        }
        let facts = PartsDraftFacts {
            actor: FinanceActor {
                user_id: source.actor_user_id().clone(),
                holding_id: source.holding_id().clone(),
                company_id: source.facts().company_id.clone(),
                assignment_id: source.actor_assignment_id().clone(),
            },
            owner_key: command.key().clone(),
            kind: PartsOwnerKind::Maintenance,
            source_id: erpx_server_finance::public::parts_draft::PartsDraftSourceIdentity::Existing(
                source.facts().id.clone(),
            ),
            source_version: work.version_after(),
            branch_id: source.facts().branch_id.clone(),
            department_id: source.facts().department_id.clone(),
            quantity_milli: p.valuation.quantity_milli,
            value_minor: p.valuation.value_minor,
            consumption_at: i.created_at,
            reason: command.input().reason.clone(),
        };
        let catalogue = current_package_catalogue()?;
        let purchase = PgCompanyPurchaseReader::new(
            source.holding_id().clone(),
            source.facts().company_id.clone(),
            c,
            &catalogue,
        )
        .inspect(
            &PurchaseRequest {
                capabilities: vec!["assets.parts-costs".into()],
                purpose: CapabilityPurpose::SettleExisting,
                mode,
            },
            now,
        )
        .await?
        .require_allowed()?
        .clone();
        Ok(Self {
            source,
            work,
            inventory,
            facts,
            catalogue,
            purchase,
        })
    }
    async fn purchase_current(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        PgCompanyPurchaseReader::new(
            self.facts.actor.holding_id.clone(),
            self.facts.actor.company_id.clone(),
            c,
            &self.catalogue,
        )
        .revalidate(&self.purchase, now)
        .await?
        .require_allowed()?;
        Ok(())
    }
    fn expected(&self, e: &PartsDraftExpected) -> Result<(), BusinessError> {
        let p = self.inventory.plan();
        let i = self.inventory.identity();
        if e.consumption_id().as_str() != self.inventory.consumption_id()
            || e.journal_id() != i.journal_id.as_str()
            || e.currency() != p.config.currency
            || e.period_id() != p.period_id.as_str()
            || e.value_minor() != p.valuation.value_minor
            || e.consumption_created_at() != i.created_at.get()
            || i.created_at.get() < self.work.application_at().get()
        {
            return Err(changed());
        }
        Ok(())
    }
}
impl PgPartsDraftOwnerSource for PgMaintenancePartsSource<'_, '_> {
    fn facts(&self) -> &PartsDraftFacts {
        &self.facts
    }
    fn finance_source(&self) -> &CurrentConsumptionFinance {
        self.inventory.finance_source()
    }
    async fn require_before(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.work
            .require_current_source(c, self.source, now)
            .await?;
        self.purchase_current(c, now).await?;
        let mut f = MaintenanceFinanceFacts {
            holding: Some(self.facts.actor.holding_id.clone()),
        };
        self.inventory.require_current_sources(c, &mut f).await
    }
    async fn require_apply(
        &self,
        c: &mut PgConnection,
        e: &PartsDraftExpected,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.expected(e)?;
        self.work.require_before(c, self.source, now).await?;
        self.purchase_current(c, now).await?;
        let mut f = MaintenanceFinanceFacts {
            holding: Some(self.facts.actor.holding_id.clone()),
        };
        self.inventory.require_before(c, &mut f).await
    }
    async fn require_after(
        &self,
        c: &mut PgConnection,
        e: &PartsDraftExpected,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.expected(e)?;
        self.work.require_after(c, self.source, now).await?;
        self.inventory.require_inventory_after(c).await?;
        self.purchase_current(c, now).await
    }
}

/// Actual selected source and normalized command supply all scope/actor
/// identities. No expected-version comparison occurs before Finance prep.
pub async fn prepare_issue_part_inventory<
    W: erpx_server_shared::public::Clock + Sync,
    G: erpx_server_shared::public::Clock + Sync,
>(
    c: &mut PgConnection,
    source: &CurrentMaintenanceWork<'_>,
    command: &MaintenanceWorkCommand,
    write_clock: &W,
    guard_clock: &G,
) -> Result<PgUnreservedConsumptionEffects<CurrentConsumptionFinance>, Error> {
    source.require_current(c, guard_clock.now()).await?;
    source.require_action(&MaintenanceTransition::IssuePart)?;
    if command.work_id() != &source.facts().id {
        return Err(changed().into());
    }
    let MaintenanceChangeAction::IssuePart {
        warehouse_id,
        product_id,
        quantity_milli,
        entry_date,
        allocations,
    } = &command.input().action
    else {
        return Err(changed().into());
    };
    let mut f = MaintenanceFinanceFacts {
        holding: Some(source.holding_id().clone()),
    };
    let (prepared, at) = prepare_inventory_consumption_at(
        c,
        &mut f,
        ConsumptionSelection {
            company_id: source.facts().company_id.clone(),
            branch_id: source.facts().branch_id.clone(),
            warehouse_id: RecordId::new(warehouse_id)?,
            product_id: RecordId::new(product_id)?,
            entry_date: entry_date.clone(),
            quantity_milli: *quantity_milli,
            allocations: allocations
                .iter()
                .map(|a| LotAllocation {
                    lot_id: a.lot_id.clone(),
                    quantity_milli: a.quantity_milli,
                    balance_version: a.balance_version,
                })
                .collect(),
        },
        write_clock,
    )
    .await?;
    let ids = erpx_server_finance::public::parts_draft::parts_draft_identity(command.key())?;
    let version = command
        .input()
        .expected_version
        .checked_add(1)
        .ok_or_else(changed)?;
    prepared
        .retain_unreserved_effects(
            c,
            MaintenanceConsumptionIdentity {
                owner_key: command.key().clone(),
                work_id: source.facts().id.clone(),
                work_version_after: version,
                department_id: source.facts().department_id.clone(),
                actor_id: source.actor_user_id().clone(),
                reason: command.input().reason.clone(),
                created_at: at,
                journal_id: ids.journal_id().clone(),
            },
        )
        .await
}
