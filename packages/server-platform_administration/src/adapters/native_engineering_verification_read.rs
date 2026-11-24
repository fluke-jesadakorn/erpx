//! PostgreSQL adapter for the owner-local native engineering verification read
//! port. Each statement mirrors the retained TypeScript authority verbatim
//! (only `?` binds become `$n`) so a PG parity test can execute both on one
//! transaction. Nothing here mounts a route, evaluates an attestation or
//! identity, or grants a rollout.
use crate::application::native_engineering_verification_read::EngineeringVerificationReadRepository;
use crate::domain::native_engineering_verification_read::{
    ActiveVerificationQuery, EngineeringVerificationPrior,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection};

pub type EngineeringVerificationReadError = PgTransactionError<BusinessError>;

/// `EngineeringRepository.prior` from
/// `packages/server-platform_administration/src/adapters/engineering-verification.ts`.
pub const PRIOR_SQL: &str = "SELECT source_fingerprint,software_version,release_manifest_sha256 FROM engineering_verification_contexts WHERE company_id=$1 AND run_id=$2";
/// Context sub-read of `engineeringVerificationPredicate` in the same file. The
/// separate database-identity predicate remains in its own native observation
/// port; this is only the persisted-context half of the retained read.
pub const ACTIVE_SQL: &str = "SELECT EXISTS(SELECT 1 FROM engineering_verification_contexts WHERE company_id=$1 AND run_id=$2 AND source_fingerprint=$3 AND software_version=$4 AND release_manifest_sha256=$5 AND enabled AND expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint)";
/// `socialReplyEngineeringLock` from
/// `packages/server-platform_administration/src/adapters/collaboration-reply-rollout.ts`.
pub const REPLY_ROLLOUT_GUARD_SQL: &str = "SELECT company_id FROM engineering_verification_contexts WHERE company_id=$1 FOR SHARE";

pub struct PgEngineeringVerificationRead<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgEngineeringVerificationRead<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    fn require_transaction(&self) -> Result<(), EngineeringVerificationReadError> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("ENGINEERING_VERIFICATION_READ_TRANSACTION_REQUIRED").into());
        }
        Ok(())
    }
}

impl EngineeringVerificationReadRepository for PgEngineeringVerificationRead<'_> {
    type Error = EngineeringVerificationReadError;

    async fn prior(
        &mut self,
        company_id: &str,
        run: &str,
    ) -> Result<Option<EngineeringVerificationPrior>, Self::Error> {
        self.require_transaction()?;
        let row: Option<(String, String, String)> = sqlx::query_as(PRIOR_SQL)
            .bind(company_id)
            .bind(run)
            .fetch_optional(&mut *self.connection)
            .await?;
        Ok(row.map(
            |(source_fingerprint, software_version, release_manifest_sha256)| {
                EngineeringVerificationPrior {
                    source_fingerprint,
                    software_version,
                    release_manifest_sha256,
                }
            },
        ))
    }

    async fn active(&mut self, query: &ActiveVerificationQuery<'_>) -> Result<bool, Self::Error> {
        self.require_transaction()?;
        Ok(sqlx::query_scalar(ACTIVE_SQL)
            .bind(query.company_id)
            .bind(query.run)
            .bind(query.source_fingerprint)
            .bind(query.software_version)
            .bind(query.file_manifest_sha256)
            .fetch_one(&mut *self.connection)
            .await?)
    }

    async fn reply_rollout_guard(&mut self, company_id: &str) -> Result<Vec<String>, Self::Error> {
        self.require_transaction()?;
        Ok(sqlx::query_scalar(REPLY_ROLLOUT_GUARD_SQL)
            .bind(company_id)
            .fetch_all(&mut *self.connection)
            .await?)
    }
}
