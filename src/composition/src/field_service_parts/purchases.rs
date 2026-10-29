//! Actual Company package witnesses. HTTP source preflight chooses History from
//! the source receipt identity tuple; new work settles both original scopes.
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
    #[cfg(feature = "business-http")]
    pub(super) async fn retain_agreement_create(
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        asset: bool,
        disposition: CommandDisposition,
        now: TimestampMillis,
    ) -> Result<Self, Error> {
        let holding = current.holding_id().clone();
        let company = current.selection().workspace().company_id.clone();
        let catalogue = current_package_catalogue()?;
        let mut reader =
            PgCompanyPurchaseReader::new(holding.clone(), company.clone(), c, &catalogue);
        let mut history = vec!["service.orders".into(), "service.agreements".into()];
        let mut create = vec!["service.agreements".into()];
        if asset {
            history.push("service.parts-assets".into());
            create.push("service.parts-assets".into());
        }
        let mut witnesses = vec![
            reader
                .inspect(
                    &PurchaseRequest {
                        capabilities: history,
                        purpose: CapabilityPurpose::History,
                        mode: PackageEnforcementMode::Enforced,
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
                            capabilities: create,
                            purpose: CapabilityPurpose::NewWork,
                            mode: PackageEnforcementMode::Enforced,
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
    #[cfg(feature = "business-http")]
    pub(super) async fn retain_agreement_review(
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        asset: bool,
        disposition: CommandDisposition,
        action: erpx_server_field_service::public::agreement_review::AgreementReviewAction,
        now: TimestampMillis,
    ) -> Result<Self, Error> {
        use erpx_server_field_service::public::agreement_review::AgreementReviewAction;
        let holding = current.holding_id().clone();
        let company = current.selection().workspace().company_id.clone();
        let catalogue = current_package_catalogue()?;
        let mut reader =
            PgCompanyPurchaseReader::new(holding.clone(), company.clone(), c, &catalogue);
        let mut history = vec!["service.orders".into(), "service.agreements".into()];
        let mut action_capabilities = vec!["service.agreements".into()];
        if asset {
            history.push("service.parts-assets".into());
            action_capabilities.push("service.parts-assets".into());
        }
        let mut witnesses = vec![
            reader
                .inspect(
                    &PurchaseRequest {
                        capabilities: history,
                        purpose: CapabilityPurpose::History,
                        mode: PackageEnforcementMode::Enforced,
                    },
                    now,
                )
                .await?
                .require_allowed()?
                .clone(),
        ];
        if disposition == CommandDisposition::Execute {
            let purpose = if action == AgreementReviewAction::Activate {
                CapabilityPurpose::NewWork
            } else {
                CapabilityPurpose::SettleExisting
            };
            witnesses.push(
                reader
                    .inspect(
                        &PurchaseRequest {
                            capabilities: action_capabilities,
                            purpose,
                            mode: PackageEnforcementMode::Enforced,
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
    #[cfg(feature = "business-http")]
    pub(super) async fn retain_visit(
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        asset: bool,
        disposition: CommandDisposition,
        action: erpx_server_field_service::public::visit::VisitAction,
        now: TimestampMillis,
    ) -> Result<Self, Error> {
        let holding = current.holding_id().clone();
        let company = current.selection().workspace().company_id.clone();
        let catalogue = current_package_catalogue()?;
        let mut reader =
            PgCompanyPurchaseReader::new(holding.clone(), company.clone(), c, &catalogue);
        let mut capabilities = vec!["service.orders".into()];
        match action {
            erpx_server_field_service::public::visit::VisitAction::Cancel => {}
            erpx_server_field_service::public::visit::VisitAction::Schedule
            | erpx_server_field_service::public::visit::VisitAction::Dispatch => {
                capabilities.push("service.dispatch".into())
            }
            _ => capabilities.push("service.visits".into()),
        }
        if asset {
            capabilities.push("service.parts-assets".into());
        }
        let mut witnesses = Vec::new();
        for purpose in [
            CapabilityPurpose::History,
            CapabilityPurpose::SettleExisting,
        ] {
            if purpose == CapabilityPurpose::SettleExisting
                && disposition == CommandDisposition::Replay
            {
                continue;
            }
            witnesses.push(
                reader
                    .inspect(
                        &PurchaseRequest {
                            capabilities: capabilities.clone(),
                            purpose,
                            mode: PackageEnforcementMode::Enforced,
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
