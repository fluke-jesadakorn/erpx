//! Actual owner-to-owner read assembly. No private foreign tables or defaults.
use crate::legal_queue_source::LegalQueuedSource;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, lock_commercial_authority_sources,
};
use erpx_server_files::public::canonical_original::lock_canonical_original_sources;
use erpx_server_identity_access::public::effective_authority::{
    AuthorizedWorkContext, PgQueuedWorkSourcePort, QueuedWorkBinding,
};
use erpx_server_legal::public::{CurrentLegalProcessingSource, record_policy::*};
use erpx_server_platform_administration::public::legal_release::{
    EngineeringReadConfiguration, LegalReleaseCatalogue,
};
use erpx_server_pms::public::scheduled_legal::{
    PgScheduledLegalReader, ScheduledLegalReadPort, ScheduledReleaseMode,
    lock_scheduled_legal_sources,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, CompanyId, HoldingId, TimestampMillis};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use std::sync::Arc;
type Error = PgTransactionError<BusinessError>;

/// Trusted bootstrap configuration only. Not deserializable from a request or
/// historical actor snapshot; release and purchase configuration are explicit.
#[derive(Clone)]
pub struct LegalOwnerConfiguration {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub release_mode: ScheduledReleaseMode,
    pub commercial_mode: LegalCommercialMode,
    pub artifact_version: String,
    pub release_catalogue: Arc<LegalReleaseCatalogue>,
    pub purchase_catalogue: Arc<CommercialCatalogue>,
    pub engineering: Option<EngineeringReadConfiguration>,
}
impl LegalOwnerConfiguration {
    pub async fn lock_sources(&self, c: &mut PgConnection) -> Result<(), Error> {
        lock_commercial_authority_sources(c).await?;
        lock_legal_record_sources(c).await?;
        lock_scheduled_legal_sources(c, self.release_mode).await?;
        lock_canonical_original_sources(c).await
    }
    pub fn queued_source(&self, source: LegalQueuedSource) -> ConfiguredLegalQueuedSource {
        ConfiguredLegalQueuedSource {
            source,
            configuration: self.clone(),
        }
    }
}
#[derive(Clone)]
pub struct ConfiguredLegalQueuedSource {
    source: LegalQueuedSource,
    configuration: LegalOwnerConfiguration,
}
impl PgQueuedWorkSourcePort for ConfiguredLegalQueuedSource {
    type Source = CurrentLegalProcessingSource;
    fn load_source<'c>(
        &'c self,
        c: &'c mut PgConnection,
        now: i64,
    ) -> PgWorkFuture<'c, Self::Source, BusinessError> {
        Box::pin(async move {
            self.configuration.lock_sources(c).await?;
            let source = self.source.load_source(c, now).await?;
            if source.holding_id() != &self.configuration.holding_id
                || source.company_id() != &self.configuration.company_id
            {
                return Err(scope_error().into());
            }
            Ok(source)
        })
    }
    fn binding(&self, source: &Self::Source) -> Result<QueuedWorkBinding, BusinessError> {
        self.source.binding(source)
    }
    fn require_current<'c>(
        &'c self,
        c: &'c mut PgConnection,
        source: &'c Self::Source,
        now: i64,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        self.source.require_current(c, source, now)
    }
}
pub(crate) fn scope_error() -> BusinessError {
    BusinessError::invalid(
        "LEGAL_OWNER_SCOPE_CHANGED",
        "The actual Legal owner scope changed.",
    )
}
pub(crate) fn actor(context: &AuthorizedWorkContext) -> LegalRecordActor {
    LegalRecordActor {
        holding_id: context.holding_id().clone(),
        company_id: context.selection().workspace().company_id.clone(),
        user_id: context.user_id().clone(),
        branch_id: context.selection().workspace().branch_id.clone(),
        department_id: context.selection().workspace().department_id.clone(),
        role: context.selection().assignment().role.clone(),
        permissions: ["legal.read", "legal.edit", "legal.export", "legal.manage"]
            .into_iter()
            .filter(|p| context.has_permission(p))
            .map(str::to_owned)
            .collect(),
    }
}
pub(crate) fn authority_revision(
    context: &AuthorizedWorkContext,
    policy: &LegalRecordWitness,
) -> Result<String, BusinessError> {
    Ok(hex::encode(Sha256::digest(
        format!(
            "{}:{}",
            QueuedWorkBinding::from_current(context).to_json()?,
            policy.revision()
        )
        .as_bytes(),
    )))
}
#[derive(Clone)]
pub(crate) struct ActualExternalPolicy(pub LegalOwnerConfiguration);
impl PgLegalRecordExternalPolicy for ActualExternalPolicy {
    async fn lock_sources(&mut self, c: &mut PgConnection) -> Result<(), Error> {
        self.0.lock_sources(c).await
    }
    async fn release(
        &mut self,
        c: &mut PgConnection,
        company: &CompanyId,
        now: i64,
    ) -> Result<LegalReleaseObservation, Error> {
        if company != &self.0.company_id {
            return Err(scope_error().into());
        }
        let release = PgScheduledLegalReader::new(
            c,
            self.0.release_mode,
            &self.0.artifact_version,
            &self.0.release_catalogue,
            self.0.engineering.as_ref(),
        )
        .inspect(company, TimestampMillis::new(now))
        .await?;
        Ok(LegalReleaseObservation {
            allowed: release.allowed,
            revision: release.revision,
        })
    }
    async fn purchases(
        &mut self,
        c: &mut PgConnection,
        company: &CompanyId,
        requirements: &[LegalPurchaseRequirement],
        mode: LegalCommercialMode,
        now: i64,
    ) -> Result<LegalPurchaseObservation, Error> {
        if company != &self.0.company_id || mode != self.0.commercial_mode {
            return Err(scope_error().into());
        }
        let mode = match mode {
            LegalCommercialMode::Disabled => PackageEnforcementMode::Disabled,
            LegalCommercialMode::Shadow => PackageEnforcementMode::Shadow,
            LegalCommercialMode::Enforced => PackageEnforcementMode::Enforced,
        };
        let mut revisions = Vec::new();
        for (purpose, native) in [
            (LegalPurchasePurpose::History, CapabilityPurpose::History),
            (LegalPurchasePurpose::NewWork, CapabilityPurpose::NewWork),
            (
                LegalPurchasePurpose::SettleExisting,
                CapabilityPurpose::SettleExisting,
            ),
        ] {
            let capabilities: Vec<_> = requirements
                .iter()
                .filter(|r| r.purpose == purpose)
                .map(|r| r.capability.clone())
                .collect();
            if capabilities.is_empty() {
                continue;
            }
            let mut reader = PgCompanyPurchaseReader::new(
                self.0.holding_id.clone(),
                company.clone(),
                c,
                &self.0.purchase_catalogue,
            );
            let inspection = reader
                .inspect(
                    &PurchaseRequest {
                        capabilities,
                        purpose: native,
                        mode,
                    },
                    TimestampMillis::new(now),
                )
                .await?;
            inspection.require_allowed()?;
            // Local proof only. A fresh phase remints its own proof and cannot
            // inherit an initial transaction's purchase witness.
            let current = reader
                .revalidate(&inspection.witness, TimestampMillis::new(now))
                .await?;
            current.require_allowed()?;
            revisions.push(current.revision);
        }
        if revisions.is_empty() {
            return Err(scope_error().into());
        }
        Ok(LegalPurchaseObservation {
            allowed: true,
            revision: hex::encode(Sha256::digest(revisions.join(":"))),
        })
    }
}

impl erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation
    for LegalOwnerConfiguration
{
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move { self.lock_sources(c).await })
    }
}
