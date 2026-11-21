//! PostgreSQL adapter for the native engineering verification enrollment write.
//! The caller owns one Control transaction; all SQL lives here in the owner
//! crate. No receipt/audit/outbox is fabricated: the context row is the effect.
use crate::application::engineering_enrollment::EngineeringEnrollmentRepository;
use crate::domain::engineering_enrollment::{
    ENROLLMENT_REASON, EngineeringEnrollmentInput, EnrollmentEntry,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection};

pub type EngineeringEnrollmentError = PgTransactionError<BusinessError>;

pub struct PgEngineeringEnrollment<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgEngineeringEnrollment<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }
}

impl EngineeringEnrollmentRepository for PgEngineeringEnrollment<'_> {
    type Error = EngineeringEnrollmentError;

    async fn enroll(
        &mut self,
        input: &EngineeringEnrollmentInput,
        entries: &[EnrollmentEntry],
        expires_at: i64,
        now: i64,
    ) -> Result<(), Self::Error> {
        if !self.connection.is_in_transaction() {
            return Err(
                PersistenceError::new("ENGINEERING_ENROLLMENT_TRANSACTION_REQUIRED").into(),
            );
        }
        for entry in entries {
            sqlx::query(
                "INSERT INTO engineering_verification_contexts(company_id,run_id,source_fingerprint,\
                 software_version,release_manifest_sha256,enabled,expires_at,reason,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) \
                 ON CONFLICT(company_id,run_id) DO UPDATE SET \
                 enabled=excluded.enabled,expires_at=excluded.expires_at",
            )
            .bind(&entry.company_id)
            .bind(&input.run)
            .bind(&input.source_fingerprint)
            .bind(&input.software_version)
            .bind(&input.file_manifest_sha256)
            .bind(entry.enabled)
            .bind(expires_at)
            .bind(ENROLLMENT_REASON)
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
        }
        Ok(())
    }
}
