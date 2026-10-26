use crate::{application::scheduled_legal::ScheduledLegalReadPort, domain::scheduled_legal::*};
use erpx_server_platform_administration::public::legal_release::{
    EngineeringReadConfiguration, LegalReleaseCatalogue, LegalReleaseReadPort,
    PgLegalReleaseReader, lock_legal_release_sources,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
pub async fn lock_scheduled_legal_sources(
    connection: &mut PgConnection,
    mode: ScheduledReleaseMode,
) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("SCHEDULED_RELEASE_OWNER_TRANSACTION_REQUIRED").into());
    }
    if matches!(mode, ScheduledReleaseMode::Production) {
        sqlx::query("LOCK TABLE company_feature_rollouts IN SHARE MODE")
            .execute(&mut *connection)
            .await?;
        lock_legal_release_sources(connection).await?;
    }
    Ok(())
}
pub struct ScheduledLegalInspection {
    pub allowed: bool,
    pub revision: String,
}
pub struct PgScheduledLegalReader<'c> {
    connection: &'c mut PgConnection,
    mode: ScheduledReleaseMode,
    artifact: &'c str,
    catalogue: &'c LegalReleaseCatalogue,
    engineering: Option<&'c EngineeringReadConfiguration>,
}
impl<'c> PgScheduledLegalReader<'c> {
    pub fn new(
        connection: &'c mut PgConnection,
        mode: ScheduledReleaseMode,
        artifact: &'c str,
        catalogue: &'c LegalReleaseCatalogue,
        engineering: Option<&'c EngineeringReadConfiguration>,
    ) -> Self {
        Self {
            connection,
            mode,
            artifact,
            catalogue,
            engineering,
        }
    }
}
impl ScheduledLegalReadPort for PgScheduledLegalReader<'_> {
    type Error = Error;
    type Inspection = ScheduledLegalInspection;
    async fn inspect(
        &mut self,
        company: &CompanyId,
        now: TimestampMillis,
    ) -> Result<ScheduledLegalInspection, Error> {
        lock_scheduled_legal_sources(self.connection, self.mode).await?;
        if matches!(self.mode, ScheduledReleaseMode::Development) {
            return Ok(ScheduledLegalInspection {
                allowed: true,
                revision: hex::encode(Sha256::digest(format!(
                    "scheduled-legal-development-v1:{}",
                    company.as_str()
                ))),
            });
        }
        let rollout=sqlx::query("SELECT rollout_state FROM company_feature_rollouts WHERE company_id=$1 AND module_id='legal' FOR SHARE").bind(company.as_str()).fetch_optional(&mut *self.connection).await?;
        let rollout = rollout
            .map(|r| r.try_get::<String, _>("rollout_state"))
            .transpose()?;
        let release = PgLegalReleaseReader::new(
            self.connection,
            self.artifact,
            self.catalogue,
            self.engineering,
        )?
        .inspect(company, now)
        .await?;
        let allowed = scheduled_legal_allowed(
            self.mode,
            rollout.as_deref(),
            release.accepted,
            release.engineering_allowed,
        );
        let revision = hex::encode(Sha256::digest(
            serde_json::to_vec(&(company.as_str(), rollout, release.revision, allowed)).map_err(
                |_| {
                    BusinessError::invalid(
                        "SCHEDULED_RELEASE_ENCODING",
                        "Current release evidence could not be encoded.",
                    )
                },
            )?,
        ));
        Ok(ScheduledLegalInspection { allowed, revision })
    }
}
