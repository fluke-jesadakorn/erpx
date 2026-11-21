//! PostgreSQL per-Company AI quota ledger. The retained owner used one
//! transaction advisory lock to serialize every AI reservation for a Company;
//! this adapter keeps the same lock key and the bounded request facts.
use crate::application::quota::CompanyQuotaPorts;
use crate::domain::quota::{FundingSource, QuotaFinish, QuotaReservation};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection};

pub type QuotaError = PgTransactionError<BusinessError>;

pub struct PgCompanyQuota<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgCompanyQuota<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    async fn require_transaction(connection: &mut PgConnection) -> Result<(), QuotaError> {
        if !connection.is_in_transaction() {
            return Err(PersistenceError::new("AI_QUOTA_TRANSACTION_REQUIRED").into());
        }
        Ok(())
    }
}

impl CompanyQuotaPorts for PgCompanyQuota<'_> {
    type Error = QuotaError;

    async fn reserve(&mut self, reservation: &QuotaReservation) -> Result<bool, Self::Error> {
        Self::require_transaction(&mut *self.connection).await?;
        let key = format!("ai-request-budget:{}", reservation.company_id);
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(key)
            .execute(&mut *self.connection)
            .await?;
        let scope_valid: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM organization_nodes WHERE id=$1 AND kind='company' AND status='active')",
        )
        .bind(&reservation.company_id)
        .fetch_one(&mut *self.connection)
        .await?;
        if !scope_valid {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Forbidden,
                "AI_QUOTA_SCOPE",
                "An active Company scope is required for AI quota.",
            )
            .into());
        }
        let existing = sqlx::query_scalar::<_, Option<String>>(
            "SELECT company_id FROM ai_gateway_requests WHERE id=$1",
        )
        .bind(&reservation.request_id)
        .fetch_optional(&mut *self.connection)
        .await?;
        if let Some(existing_company) = existing
            && existing_company == Some(reservation.company_id.clone())
        {
            return Ok(true);
        }
        let reservation_satang = match reservation.funding_source {
            FundingSource::Erpx => reservation.reservation_satang,
            FundingSource::Byok => 0,
        };
        let inserted = sqlx::query(
            "INSERT INTO ai_gateway_requests(id,usage_id,holding_id,company_id,branch_id,connection_id,funding_source,provider,model,feature,status,reservation_satang,created_at) \
             SELECT $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'reserved',$11,$12 \
             WHERE (SELECT count(*) FROM ai_gateway_requests r WHERE r.company_id=$4 AND r.created_at>=$13 AND r.created_at<$14)<$15 \
             ON CONFLICT(id) DO NOTHING",
        )
        .bind(&reservation.request_id)
        .bind(&reservation.usage_id)
        .bind(&reservation.holding_id)
        .bind(&reservation.company_id)
        .bind(reservation.branch_id.as_deref())
        .bind(reservation.connection_id.as_deref())
        .bind(reservation.funding_source.as_str())
        .bind(&reservation.provider)
        .bind(&reservation.model)
        .bind(&reservation.feature)
        .bind(reservation_satang)
        .bind(reservation.now)
        .bind(reservation.window_start)
        .bind(reservation.window_end)
        .bind(reservation.monthly_request_limit)
        .execute(&mut *self.connection)
        .await
        .map_err(|error| PgTransactionError::Persistence(PersistenceError::from(error)))?;
        Ok(inserted.rows_affected() == 1)
    }

    async fn finish(&mut self, finish: &QuotaFinish) -> Result<bool, Self::Error> {
        Self::require_transaction(&mut *self.connection).await?;
        let units_json = serde_json::to_string(&finish.units).map_err(|_| {
            BusinessError::invalid("AI_QUOTA_INPUT_INVALID", "Provide valid AI quota units.")
        })?;
        let changed = sqlx::query(
            "UPDATE ai_gateway_requests SET status=$1,input_tokens=$2,output_tokens=$3,units_json=$4::jsonb,completed_at=$5 \
             WHERE id=$6 AND company_id=$7 AND status='reserved'",
        )
        .bind(finish.outcome.as_str())
        .bind(finish.input_tokens)
        .bind(finish.output_tokens)
        .bind(units_json)
        .bind(finish.now)
        .bind(&finish.request_id)
        .bind(&finish.company_id)
        .execute(&mut *self.connection)
        .await
        .map_err(|error| PgTransactionError::Persistence(PersistenceError::from(error)))?;
        Ok(changed.rows_affected() == 1)
    }
}
