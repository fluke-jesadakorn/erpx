//! Exact Service Count purchase purposes; no Supply package is substituted.
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, PurchaseWitness, current_package_catalogue,
};
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, HoldingId, TimestampMillis};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
pub struct ServiceCountPurchases {
    holding: HoldingId,
    company: CompanyId,
    catalogue: CommercialCatalogue,
    history: PurchaseWitness,
    new_work: PurchaseWitness,
    settle: PurchaseWitness,
    pub history_allowed: bool,
    pub new_work_allowed: bool,
    pub settle_allowed: bool,
}
impl ServiceCountPurchases {
    pub async fn capture(
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        now: TimestampMillis,
    ) -> Result<Self, Error> {
        let holding = current.holding_id().clone();
        let company = current.selection().workspace().company_id.clone();
        let catalogue = current_package_catalogue()?;
        let mut reader =
            PgCompanyPurchaseReader::new(holding.clone(), company.clone(), c, &catalogue);
        let request = |purpose| PurchaseRequest {
            capabilities: vec![
                "service.orders".into(),
                "service.parts-assets".into(),
                "finance.service-invoice".into(),
            ],
            purpose,
            mode: PackageEnforcementMode::Enforced,
        };
        let h = reader
            .inspect(&request(CapabilityPurpose::History), now)
            .await?;
        let n = reader
            .inspect(&request(CapabilityPurpose::NewWork), now)
            .await?;
        let s = reader
            .inspect(&request(CapabilityPurpose::SettleExisting), now)
            .await?;
        Ok(Self {
            holding,
            company,
            catalogue,
            history_allowed: h.require_allowed().is_ok(),
            new_work_allowed: n.require_allowed().is_ok(),
            settle_allowed: s.require_allowed().is_ok(),
            history: h.witness,
            new_work: n.witness,
            settle: s.witness,
        })
    }
    pub async fn require_for(
        &self,
        c: &mut PgConnection,
        purpose: CapabilityPurpose,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        let mut reader = PgCompanyPurchaseReader::new(
            self.holding.clone(),
            self.company.clone(),
            c,
            &self.catalogue,
        );
        let witness = match purpose {
            CapabilityPurpose::History => &self.history,
            CapabilityPurpose::NewWork => &self.new_work,
            CapabilityPurpose::SettleExisting => &self.settle,
        };
        reader.revalidate(witness, now).await?.require_allowed()?;
        Ok(())
    }
    pub async fn require_same(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        let mut reader = PgCompanyPurchaseReader::new(
            self.holding.clone(),
            self.company.clone(),
            c,
            &self.catalogue,
        );
        let history = reader
            .revalidate(&self.history, now)
            .await?
            .require_allowed()
            .is_ok();
        let new_work = reader
            .revalidate(&self.new_work, now)
            .await?
            .require_allowed()
            .is_ok();
        let settle = reader
            .revalidate(&self.settle, now)
            .await?
            .require_allowed()
            .is_ok();
        if (history, new_work, settle)
            != (
                self.history_allowed,
                self.new_work_allowed,
                self.settle_allowed,
            )
        {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Conflict,
                "PACKAGE_CAPABILITY_CHANGED",
                "The original Company purchase changed.",
            )
            .into());
        }
        Ok(())
    }
}
