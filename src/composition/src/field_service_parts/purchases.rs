//! Actual Company package witnesses. History is chosen only after Workflow's
//! exact request-hash match; ordinary parts work settles both original scopes.
use super::{Error, changed};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, PurchaseWitness, current_package_catalogue,
};
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_shared::public::{CompanyId, HoldingId, TimestampMillis};
use erpx_server_workflow::public::commands::CommandDisposition;
use sqlx::PgConnection;

pub(super) struct FieldServicePurchases {
    holding: HoldingId,
    company: CompanyId,
    catalogue: CommercialCatalogue,
    witnesses: Vec<PurchaseWitness>,
}
impl FieldServicePurchases {
    pub(super) async fn retain(
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        mode: PackageEnforcementMode,
        disposition: CommandDisposition,
        now: TimestampMillis,
    ) -> Result<Self, Error> {
        let holding = current.holding_id().clone();
        let company = current.selection().workspace().company_id.clone();
        let catalogue = current_package_catalogue()?;
        let mut reader =
            PgCompanyPurchaseReader::new(holding.clone(), company.clone(), c, &catalogue);
        // Original new-command boundary checks retained History before the
        // protected settlement predicate, preserving direct missing-history errors.
        let mut witnesses = vec![
            reader
                .inspect(
                    &PurchaseRequest {
                        capabilities: vec!["service.orders".into(), "service.parts-assets".into()],
                        purpose: CapabilityPurpose::History,
                        mode,
                    },
                    now,
                )
                .await?
                .require_allowed()?
                .clone(),
        ];
        if disposition == CommandDisposition::Execute {
            witnesses.push(
                reader
                    .inspect(
                        &PurchaseRequest {
                            capabilities: vec![
                                "service.orders".into(),
                                "service.parts-assets".into(),
                            ],
                            purpose: CapabilityPurpose::SettleExisting,
                            mode,
                        },
                        now,
                    )
                    .await?
                    .require_allowed()?
                    .clone(),
            );
        }
        Ok(Self {
            holding,
            company,
            catalogue,
            witnesses,
        })
    }
    pub(super) async fn require_current(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        if current.holding_id() != &self.holding
            || current.selection().workspace().company_id != self.company
            || now.get() >= current.expires_at()
        {
            return Err(changed().into());
        }
        let mut reader = PgCompanyPurchaseReader::new(
            self.holding.clone(),
            self.company.clone(),
            c,
            &self.catalogue,
        );
        for witness in &self.witnesses {
            reader.revalidate(witness, now).await?.require_allowed()?;
        }
        Ok(())
    }
}
