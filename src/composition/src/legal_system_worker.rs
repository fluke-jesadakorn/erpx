//! System authority can claim/recover/close its exact lease. It never grants
//! personal content access, reads originals or writes inspections/success audit.
use crate::legal_owner_policy::{ActualExternalPolicy, LegalOwnerConfiguration};
use erpx_server_legal::public::{
    LegalProcessingCursor, LegalProcessingJob, LegalProcessingKind, processing_maintenance::*,
    record_policy::*,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, Clock, IdentifierSource};
use sqlx::PgConnection;
use std::sync::Arc;
type Error = PgTransactionError<BusinessError>;

/// Operational evidence is independent of personal/Company permissions.
/// The concrete bootstrap only supplies actual bounded provider probe evidence.
pub trait LegalOperationalAdmission: Clone + Send + Sync + 'static {
    fn permits(&self, kind: LegalProcessingKind, now: i64) -> bool;
}
#[cfg(feature = "legal-document-worker")]
#[derive(Clone)]
pub struct LegalNativeAdmission {
    documents: erpx_server_legal::public::DocumentProviderReadiness,
    #[cfg(feature = "legal-pdf-worker")]
    pdf: Option<erpx_server_legal::public::PdfProviderReadiness>,
}
#[cfg(feature = "legal-document-worker")]
impl LegalNativeAdmission {
    #[cfg(feature = "legal-pdf-worker")]
    pub fn with_pdf(mut self, readiness: erpx_server_legal::public::PdfProviderReadiness) -> Self {
        self.pdf = Some(readiness);
        self
    }
    pub fn documents(readiness: erpx_server_legal::public::DocumentProviderReadiness) -> Self {
        Self {
            documents: readiness,
            #[cfg(feature = "legal-pdf-worker")]
            pdf: None,
        }
    }
}
#[cfg(feature = "legal-document-worker")]
impl LegalOperationalAdmission for LegalNativeAdmission {
    fn permits(&self, kind: LegalProcessingKind, now: i64) -> bool {
        let checked_at = match kind {
            LegalProcessingKind::Ocr => Some(self.documents.checked_at()),
            LegalProcessingKind::Verify => {
                #[cfg(feature = "legal-pdf-worker")]
                {
                    self.pdf.as_ref().map(|proof| proof.checked_at())
                }
                #[cfg(not(feature = "legal-pdf-worker"))]
                {
                    None
                }
            }
        };
        checked_at.is_some_and(|at| now >= at && now.saturating_sub(at) <= 60_000)
    }
}
pub struct ActualSystemClaimPolicy<Admission> {
    configuration: LegalOwnerConfiguration,
    admission: Admission,
}
impl<Admission: LegalOperationalAdmission> ActualSystemClaimPolicy<Admission> {
    pub fn new(configuration: LegalOwnerConfiguration, admission: Admission) -> Self {
        Self {
            configuration,
            admission,
        }
    }
}
impl<Admission: LegalOperationalAdmission> PgLegalSystemClaimPolicy
    for ActualSystemClaimPolicy<Admission>
{
    async fn prepare_sources(&mut self, c: &mut PgConnection) -> Result<(), Error> {
        self.configuration.lock_sources(c).await
    }
    async fn candidate_eligible(
        &mut self,
        c: &mut PgConnection,
        job: &LegalProcessingJob,
        now: i64,
    ) -> Result<bool, Error> {
        if !self.admission.permits(job.kind, now) {
            return Ok(false);
        }
        let facts = legal_system_candidate_facts(
            c,
            &LegalMaintenanceScope {
                holding_id: self.configuration.holding_id.clone(),
                company_id: self.configuration.company_id.clone(),
            },
            job,
        )
        .await?;
        if !facts.enabled {
            return Ok(false);
        }
        let mut policy = ActualExternalPolicy(self.configuration.clone());
        if !policy
            .release(c, &self.configuration.company_id, now)
            .await?
            .allowed
        {
            return Ok(false);
        }
        let requirements = [legal_kind_capability(&facts.kind)?, "legal.documents"]
            .into_iter()
            .map(|capability| LegalPurchaseRequirement {
                capability: capability.to_owned(),
                purpose: LegalPurchasePurpose::SettleExisting,
            })
            .collect::<Vec<_>>();
        match policy
            .purchases(
                c,
                &self.configuration.company_id,
                &requirements,
                self.configuration.commercial_mode,
                now,
            )
            .await
        {
            Ok(facts) => Ok(facts.allowed),
            // A current missing/expired purchase makes this candidate ineligible;
            // persistence/serialization errors must still reach the outer UoW.
            Err(PgTransactionError::Business(error))
                if error.kind == erpx_server_shared::public::FailureKind::Forbidden =>
            {
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }
}
#[derive(Clone)]
pub struct LegalSystemWorker<Admission, ClockSource> {
    unit: PgUnitOfWork,
    configuration: LegalOwnerConfiguration,
    admission: Admission,
    clock: Arc<ClockSource>,
}
impl<Admission: LegalOperationalAdmission, ClockSource: Clock + Send + Sync + 'static>
    LegalSystemWorker<Admission, ClockSource>
{
    pub fn new(
        pool: DatabasePool,
        configuration: LegalOwnerConfiguration,
        admission: Admission,
        clock: Arc<ClockSource>,
    ) -> Self {
        Self {
            unit: PgUnitOfWork::new(pool),
            configuration,
            admission,
            clock,
        }
    }
    pub async fn recover_expired(&self) -> Result<u64, Error> {
        let config = self.configuration.clone();
        let admission = self.admission.clone();
        let clock = self.clock.clone();
        self.unit
            .run(TransactionOptions::default(), move |c| {
                let config = config.clone();
                let admission = admission.clone();
                let clock = clock.clone();
                Box::pin(async move {
                    let mut policy = ActualSystemClaimPolicy::new(config.clone(), admission);
                    let mut ids = WorkerIds;
                    PgLegalProcessingMaintenance::new(
                        scope(&config),
                        c,
                        &mut policy,
                        clock.as_ref(),
                        &mut ids,
                    )
                    .recover_expired()
                    .await
                })
            })
            .await
    }
    pub async fn claim(
        &self,
        after: Option<LegalProcessingCursor>,
    ) -> Result<LegalMaintenanceScan, Error> {
        let config = self.configuration.clone();
        let admission = self.admission.clone();
        let clock = self.clock.clone();
        self.unit
            .run(TransactionOptions::default(), move |c| {
                let config = config.clone();
                let admission = admission.clone();
                let clock = clock.clone();
                let after = after.clone();
                Box::pin(async move {
                    let mut policy = ActualSystemClaimPolicy::new(config.clone(), admission);
                    let mut ids = WorkerIds;
                    PgLegalProcessingMaintenance::new(
                        scope(&config),
                        c,
                        &mut policy,
                        clock.as_ref(),
                        &mut ids,
                    )
                    .claim(after.as_ref())
                    .await
                })
            })
            .await
    }
    pub async fn fail_claim(
        &self,
        claim: ClaimedLegalProcessing,
        code: &'static str,
    ) -> Result<(), Error> {
        let config = self.configuration.clone();
        let admission = self.admission.clone();
        let clock = self.clock.clone();
        self.unit
            .run(TransactionOptions::default(), move |c| {
                let config = config.clone();
                let admission = admission.clone();
                let clock = clock.clone();
                let claim = claim.clone();
                Box::pin(async move {
                    let mut policy = ActualSystemClaimPolicy::new(config.clone(), admission);
                    let mut ids = WorkerIds;
                    PgLegalProcessingMaintenance::new(
                        scope(&config),
                        c,
                        &mut policy,
                        clock.as_ref(),
                        &mut ids,
                    )
                    .fail_claim(&claim, code)
                    .await
                })
            })
            .await
    }
}
fn scope(configuration: &LegalOwnerConfiguration) -> LegalMaintenanceScope {
    LegalMaintenanceScope {
        holding_id: configuration.holding_id.clone(),
        company_id: configuration.company_id.clone(),
    }
}
struct WorkerIds;
impl IdentifierSource for WorkerIds {
    fn next(&mut self) -> erpx_server_shared::public::RecordId {
        erpx_server_shared::public::RecordId::new(uuid::Uuid::new_v4().to_string())
            .expect("generated UUID")
    }
}
