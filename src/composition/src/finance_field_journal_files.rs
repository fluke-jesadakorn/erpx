//! Native Files authority for supporting evidence of an actual Field cost journal.
//! Finance, Inventory, Field, Workflow and Commercial witnesses stay on this unit.
use crate::{
    field_service_source_record::SourceRecordContext, finance_field_journal_history::*,
    finance_journal_create::FinanceJournalCreateConfiguration,
};
use erpx_server_commercial::public::purchases::*;
use erpx_server_files::public::field_visit_intake::{
    FieldIntakeAuthority, FieldIntakeScope, IntakeOwner,
};
use erpx_server_identity_access::public::{
    effective_authority::{AuthorizedWorkContext, HoldingWorkSourcePreparation},
    selected_command_authority::require_selected_actor_current,
};
use erpx_server_runtime::public::postgres::PgWorkFuture;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use sqlx::PgConnection;
use std::collections::BTreeMap;
use tokio::sync::Mutex;
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_JOURNAL_EVIDENCE_CHANGED",
        "The original Field cost journal or its evidence authority changed.",
    )
}
#[derive(Clone)]
pub struct FinanceFieldJournalIntakeSources;
impl HoldingWorkSourcePreparation for FinanceFieldJournalIntakeSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_field_journal_history_sources(c).await?;
            erpx_server_files::public::field_visit_link::lock_field_visit_link_sources(c).await?;
            erpx_server_manage_erpx::public::lock_beta_policy_sources(c).await
        })
    }
}
struct EvidenceProof {
    source: CurrentFieldJournalHistory,
    settle: Option<(PurchaseWitness, bool, Option<PurchaseWitness>)>,
}
impl EvidenceProof {
    async fn require_settle<C: Clock>(
        &self,
        c: &mut PgConnection,
        config: &FinanceJournalCreateConfiguration,
        clock: &C,
    ) -> Result<(), erpx_server_runtime::public::postgres::PgTransactionError<BusinessError>> {
        if let Some((base, allowed, included)) = &self.settle {
            let mut reader = PgCompanyPurchaseReader::new(
                config.holding.clone(),
                config.company.clone(),
                c,
                &config.catalogue,
            );
            if reader
                .revalidate(base, clock.now())
                .await?
                .require_allowed()
                .is_ok()
                != *allowed
            {
                return Err(changed().into());
            }
            if let Some(included) = included {
                reader
                    .revalidate(included, clock.now())
                    .await?
                    .require_allowed()?;
            }
        }
        Ok(())
    }
}
pub(crate) struct JournalEvidenceVersionAuthority<'a, C> {
    pub current: &'a AuthorizedWorkContext,
    pub config: &'a FinanceJournalCreateConfiguration,
    pub clock: &'a C,
}
impl<C: Clock + Send + Sync>
    erpx_server_finance::public::field_journal_evidence::PgFieldJournalEvidenceAuthority
    for JournalEvidenceVersionAuthority<'_, C>
{
    async fn current(
        &mut self,
        c: &mut PgConnection,
    ) -> Result<
        erpx_server_finance::public::FinanceActor,
        erpx_server_runtime::public::postgres::PgTransactionError<BusinessError>,
    > {
        let actor = self.current;
        if actor.holding_id() != &self.config.holding
            || actor.selection().workspace().company_id != self.config.company
            || self.clock.now().get() >= actor.expires_at()
        {
            return Err(changed().into());
        }
        actor.require_permission("finance.read")?;
        actor.require_permission("finance.write")?;
        actor.require_permission("module.finance.read")?;
        require_selected_actor_current(c, actor, self.clock.now().get()).await?;
        Ok(crate::finance_journal_actor::finance_actor(actor))
    }
}
pub struct FinanceFieldJournalIntakeAuthority<'a, C> {
    current: &'a AuthorizedWorkContext,
    config: &'a FinanceJournalCreateConfiguration,
    clock: &'a C,
    manage: bool,
    proofs: Mutex<BTreeMap<(String, bool), EvidenceProof>>,
}
impl<'a, C: Clock + Send + Sync> FinanceFieldJournalIntakeAuthority<'a, C> {
    pub fn for_read(
        current: &'a AuthorizedWorkContext,
        config: &'a FinanceJournalCreateConfiguration,
        clock: &'a C,
    ) -> Self {
        Self {
            current,
            config,
            clock,
            manage: false,
            proofs: Mutex::new(BTreeMap::new()),
        }
    }
    pub fn for_manage(
        current: &'a AuthorizedWorkContext,
        config: &'a FinanceJournalCreateConfiguration,
        clock: &'a C,
    ) -> Self {
        Self {
            manage: true,
            ..Self::for_read(current, config, clock)
        }
    }
    fn context(&self) -> SourceRecordContext<'_, C> {
        SourceRecordContext {
            purchase_locks: None,
            actor: self.current,
            holding: &self.config.holding,
            company: &self.config.company,
            catalogue: &self.config.catalogue,
            clock: self.clock,
        }
    }
    pub(crate) async fn require_after_version(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        id: &str,
        write: bool,
        version: i64,
        updated_at: i64,
    ) -> Result<(), erpx_server_runtime::public::postgres::PgTransactionError<BusinessError>> {
        self.actor(current, write)?;
        require_selected_actor_current(c, current, self.clock.now().get()).await?;
        let proofs = self.proofs.lock().await;
        let proof = proofs.get(&(id.to_owned(), write)).ok_or_else(changed)?;
        Box::pin(
            proof
                .source
                .after_evidence(c, &self.context(), id, version, updated_at),
        )
        .await?;
        proof.require_settle(c, self.config, self.clock).await?;
        Ok(())
    }
    fn actor(&self, current: &AuthorizedWorkContext, write: bool) -> Result<(), BusinessError> {
        if !std::ptr::eq(current, self.current)
            || current.holding_id() != &self.config.holding
            || current.selection().workspace().company_id != self.config.company
            || self.clock.now().get() >= current.expires_at()
        {
            return Err(changed());
        }
        current.require_permission("module.finance.read")?;
        current.require_permission("finance.read")?;
        if write || self.manage {
            current.require_permission("finance.write")?;
        }
        Ok(())
    }
}
impl<C: Clock + Send + Sync> FieldIntakeAuthority for FinanceFieldJournalIntakeAuthority<'_, C> {
    fn owner_kind(&self) -> IntakeOwner {
        IntakeOwner::FieldJournal
    }
    fn authorize<'a>(
        &'a self,
        c: &'a mut PgConnection,
        current: &'a AuthorizedWorkContext,
        id: &'a str,
        write: bool,
    ) -> PgWorkFuture<'a, FieldIntakeScope, BusinessError> {
        Box::pin(async move {
            self.actor(current, write)?;
            require_selected_actor_current(c, current, self.clock.now().get()).await?;
            let source = Box::pin(read_field_journal_history(c, &self.context(), id)).await?;
            let settle = if write || self.manage {
                let mut reader = PgCompanyPurchaseReader::new(
                    self.config.holding.clone(),
                    self.config.company.clone(),
                    c,
                    &self.config.catalogue,
                );
                let request = |cap: &str| PurchaseRequest {
                    capabilities: vec![cap.into()],
                    purpose: CapabilityPurpose::SettleExisting,
                    mode: PackageEnforcementMode::Enforced,
                };
                let base = reader
                    .inspect(&request("finance.accounting"), self.clock.now())
                    .await?;
                let allowed = base.require_allowed().is_ok();
                let included = if allowed {
                    None
                } else {
                    let result = reader
                        .inspect(&request("finance.service-invoice"), self.clock.now())
                        .await?;
                    result.require_allowed()?;
                    Some(result.witness)
                };
                Some((base.witness, allowed, included))
            } else {
                None
            };
            let scope = FieldIntakeScope {
                company_id: self.config.company.clone(),
                branch_id: None,
                department_id: None,
                version: source.version()?,
            };
            let mut proofs = self.proofs.lock().await;
            if proofs.contains_key(&(id.to_owned(), write)) {
                return Err(changed().into());
            }
            proofs.insert((id.to_owned(), write), EvidenceProof { source, settle });
            Ok(scope)
        })
    }
    fn revalidate<'a>(
        &'a self,
        c: &'a mut PgConnection,
        current: &'a AuthorizedWorkContext,
        id: &'a str,
        write: bool,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            self.actor(current, write)?;
            require_selected_actor_current(c, current, self.clock.now().get()).await?;
            let proofs = self.proofs.lock().await;
            let proof = proofs.get(&(id.to_owned(), write)).ok_or_else(changed)?;
            Box::pin(proof.source.require_current(c, &self.context())).await?;
            proof.require_settle(c, self.config, self.clock).await?;
            Ok(())
        })
    }
}
