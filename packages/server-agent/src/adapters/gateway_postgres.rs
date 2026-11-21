//! PostgreSQL AI gateway policy, statement, cost and reconciliation adapter.
//! The caller owns the Holding transaction; this adapter never opens one and
//! never stores prompts, completions or credentials.
use crate::application::gateway::{AiGatewayPorts, AiSetEnabledOutcome};
use crate::domain::gateway::{
    AiExistingCost, AiGatewayRow, AiPolicyChange, AiPolicyFacts, AiProviderStatement,
    AiReconciliationFact, AiSetEnabledInput, AiStatementEvidence, AiVerifiedCost,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection, Row};

pub type AiGatewayError = PgTransactionError<BusinessError>;

pub struct PgAiGateway<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgAiGateway<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    async fn require_transaction(connection: &mut PgConnection) -> Result<(), AiGatewayError> {
        if !connection.is_in_transaction() {
            return Err(PersistenceError::new("AI_GATEWAY_TRANSACTION_REQUIRED").into());
        }
        Ok(())
    }
}

impl AiGatewayPorts for PgAiGateway<'_> {
    type Error = AiGatewayError;

    async fn policy(&mut self, start: i64, end: i64) -> Result<Option<AiPolicyFacts>, Self::Error> {
        Self::require_transaction(&mut *self.connection).await?;
        let row = sqlx::query(
            "SELECT p.platform_enabled,p.monthly_request_limit::bigint monthly_request_limit,\
             p.monthly_spend_cap_satang,p.reservation_satang,p.updated_by,p.updated_at,\
             (SELECT count(*)::bigint FROM ai_gateway_requests r WHERE r.created_at>=$1 AND r.created_at<$2) monthly_requests,\
             (SELECT coalesce(sum(coalesce(e.thb_satang,r.reservation_satang)),0)::bigint FROM ai_gateway_requests r \
               LEFT JOIN ai_provider_cost_evidence e ON e.request_id=r.id \
               WHERE r.funding_source='erpx' AND r.created_at>=$1 AND r.created_at<$2) monthly_reserved_or_verified_satang,\
             (SELECT count(*)::bigint FROM ai_gateway_requests r \
               LEFT JOIN ai_provider_cost_evidence e ON e.request_id=r.id \
               WHERE r.funding_source='erpx' AND r.status='succeeded' AND e.reconciled_at IS NULL \
                 AND r.created_at>=$1 AND r.created_at<$2) pending_platform_cost_requests \
             FROM ai_gateway_policies p WHERE p.singleton=1",
        )
        .bind(start)
        .bind(end)
        .fetch_optional(&mut *self.connection)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(AiPolicyFacts {
            platform_enabled: row.try_get("platform_enabled")?,
            monthly_request_limit: row.try_get("monthly_request_limit")?,
            monthly_spend_cap_satang: row.try_get("monthly_spend_cap_satang")?,
            reservation_satang: row.try_get("reservation_satang")?,
            updated_by: row.try_get("updated_by")?,
            updated_at: row.try_get("updated_at")?,
            monthly_requests: row.try_get("monthly_requests")?,
            monthly_reserved_or_verified_satang: row
                .try_get("monthly_reserved_or_verified_satang")?,
            pending_platform_cost_requests: row.try_get("pending_platform_cost_requests")?,
        }))
    }

    async fn set_enabled(
        &mut self,
        input: &AiSetEnabledInput,
        actor_id: &str,
        audit_id: &str,
        now: i64,
    ) -> Result<AiSetEnabledOutcome, Self::Error> {
        Self::require_transaction(&mut *self.connection).await?;
        let current = sqlx::query(
            "SELECT platform_enabled,updated_at FROM ai_gateway_policies WHERE singleton=1 FOR UPDATE",
        )
        .fetch_optional(&mut *self.connection)
        .await?;
        let Some(current) = current else {
            return Ok(AiSetEnabledOutcome::Missing);
        };
        let current_enabled: bool = current.try_get("platform_enabled")?;
        let current_updated_at: i64 = current.try_get("updated_at")?;
        if current_updated_at != input.expected_updated_at {
            return Ok(AiSetEnabledOutcome::Changed);
        }
        if current_enabled == input.enabled {
            return Ok(AiSetEnabledOutcome::Unchanged);
        }
        let updated = sqlx::query(
            "UPDATE ai_gateway_policies SET platform_enabled=$1,updated_by=$2,updated_at=$3 WHERE singleton=1",
        )
        .bind(input.enabled)
        .bind(actor_id)
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        let audited = sqlx::query(
            "INSERT INTO ai_gateway_policy_audit(id,actor_id,reason,platform_enabled,monthly_request_limit,monthly_spend_cap_satang,reservation_satang,created_at) \
             SELECT $1,$2,$3,p.platform_enabled,p.monthly_request_limit,p.monthly_spend_cap_satang,p.reservation_satang,$4 \
             FROM ai_gateway_policies p WHERE p.singleton=1",
        )
        .bind(audit_id)
        .bind(actor_id)
        .bind(&input.reason)
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        if updated.rows_affected() != 1 || audited.rows_affected() != 1 {
            return Ok(AiSetEnabledOutcome::Missing);
        }
        Ok(AiSetEnabledOutcome::Updated)
    }

    async fn update_policy(
        &mut self,
        input: &AiPolicyChange,
        actor_id: &str,
        audit_id: &str,
        now: i64,
    ) -> Result<bool, Self::Error> {
        Self::require_transaction(&mut *self.connection).await?;
        let updated = sqlx::query(
            "UPDATE ai_gateway_policies SET platform_enabled=$1,monthly_request_limit=$2,\
             monthly_spend_cap_satang=$3,reservation_satang=$4,updated_by=$5,updated_at=$6 \
             WHERE singleton=1",
        )
        .bind(input.enabled)
        .bind(input.monthly_request_limit)
        .bind(input.monthly_spend_cap_satang)
        .bind(input.reservation_satang)
        .bind(actor_id)
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        let audited = sqlx::query(
            "INSERT INTO ai_gateway_policy_audit(id,actor_id,reason,platform_enabled,monthly_request_limit,monthly_spend_cap_satang,reservation_satang,created_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(audit_id)
        .bind(actor_id)
        .bind(&input.reason)
        .bind(input.enabled)
        .bind(input.monthly_request_limit)
        .bind(input.monthly_spend_cap_satang)
        .bind(input.reservation_satang)
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        Ok(updated.rows_affected() == 1 && audited.rows_affected() == 1)
    }

    async fn insert_statement(
        &mut self,
        input: &AiProviderStatement,
        now: i64,
    ) -> Result<bool, Self::Error> {
        Self::require_transaction(&mut *self.connection).await?;
        let result = sqlx::query(
            "INSERT INTO ai_provider_statements(id,holding_id,provider,month,currency,amount_minor,statement_digest,verified_by,verified_at) \
             SELECT $1,h.holding_id,$2,$3,$4,$5,$6,$7,$8 \
             FROM beta_bootstrap h WHERE h.singleton=1 AND h.holding_id=$9 \
             ON CONFLICT(id) DO NOTHING",
        )
        .bind(&input.id)
        .bind(&input.provider)
        .bind(&input.month)
        .bind(&input.currency)
        .bind(input.amount_minor)
        .bind(&input.statement_digest)
        .bind(&input.verified_by)
        .bind(now)
        .bind(&input.holding_id)
        .execute(&mut *self.connection)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    async fn statement_evidence(
        &mut self,
        id: &str,
    ) -> Result<Option<AiStatementEvidence>, Self::Error> {
        Self::require_transaction(&mut *self.connection).await?;
        let row = sqlx::query(
            "SELECT holding_id,provider,month,currency,amount_minor,statement_digest \
             FROM ai_provider_statements WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(|row| {
            Ok(AiStatementEvidence {
                holding_id: row.try_get("holding_id")?,
                provider: row.try_get("provider")?,
                month: row.try_get("month")?,
                currency: row.try_get("currency")?,
                amount_minor: row.try_get("amount_minor")?,
                statement_digest: row.try_get("statement_digest")?,
            })
        })
        .transpose()
    }

    async fn insert_cost_evidence(
        &mut self,
        input: &AiVerifiedCost,
        now: i64,
    ) -> Result<bool, Self::Error> {
        Self::require_transaction(&mut *self.connection).await?;
        let result = sqlx::query(
            "INSERT INTO ai_provider_cost_evidence(request_id,provider_statement_id,currency,amount_minor,thb_satang,fx_evidence_ref,verified_by,verified_at,reconciled_at) \
             SELECT r.id,$1,$2,$3,$4,$5,$6,$7,NULL \
             FROM ai_gateway_requests r JOIN ai_provider_statements s ON s.id=$8 \
             WHERE r.id=$9 AND r.status='succeeded' AND r.funding_source='erpx' \
               AND s.holding_id=r.holding_id AND s.provider=r.provider AND s.currency=$10 \
               AND s.month=to_char(to_timestamp(r.created_at/1000.0) AT TIME ZONE 'Asia/Bangkok','YYYY-MM') \
               AND s.reconciled_at IS NULL \
             ON CONFLICT(request_id) DO NOTHING",
        )
        .bind(&input.provider_statement_id)
        .bind(&input.currency)
        .bind(input.amount_minor)
        .bind(input.thb_satang)
        .bind(input.fx_evidence_ref.as_deref())
        .bind(&input.verified_by)
        .bind(now)
        .bind(&input.provider_statement_id)
        .bind(&input.request_id)
        .bind(&input.currency)
        .execute(&mut *self.connection)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    async fn existing_cost_evidence(
        &mut self,
        request_id: &str,
    ) -> Result<Option<AiExistingCost>, Self::Error> {
        Self::require_transaction(&mut *self.connection).await?;
        let row = sqlx::query(
            "SELECT provider_statement_id,currency,amount_minor,thb_satang,fx_evidence_ref \
             FROM ai_provider_cost_evidence WHERE request_id=$1",
        )
        .bind(request_id)
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(|row| {
            Ok(AiExistingCost {
                provider_statement_id: row.try_get("provider_statement_id")?,
                currency: row.try_get("currency")?,
                amount_minor: row.try_get("amount_minor")?,
                thb_satang: row.try_get("thb_satang")?,
                fx_evidence_ref: row.try_get("fx_evidence_ref")?,
            })
        })
        .transpose()
    }

    async fn reconcile(&mut self, statement_id: &str, now: i64) -> Result<bool, Self::Error> {
        Self::require_transaction(&mut *self.connection).await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
            .bind(format!("ai-statement:{statement_id}"))
            .execute(&mut *self.connection)
            .await?;
        let reconciled = sqlx::query(
            "UPDATE ai_provider_statements s SET reconciled_at=$1 WHERE s.id=$2 AND s.reconciled_at IS NULL \
             AND s.amount_minor=(SELECT coalesce(sum(e.amount_minor),0) FROM ai_provider_cost_evidence e WHERE e.provider_statement_id=s.id) \
             AND NOT EXISTS(SELECT 1 FROM ai_provider_cost_evidence e WHERE e.provider_statement_id=s.id AND e.currency<>s.currency)",
        )
        .bind(now)
        .bind(statement_id)
        .execute(&mut *self.connection)
        .await?;
        sqlx::query(
            "UPDATE ai_provider_cost_evidence e SET reconciled_at=s.reconciled_at \
             FROM ai_provider_statements s \
             WHERE e.provider_statement_id=s.id AND s.id=$1 AND s.reconciled_at IS NOT NULL AND e.reconciled_at IS NULL",
        )
        .bind(statement_id)
        .execute(&mut *self.connection)
        .await?;
        Ok(reconciled.rows_affected() == 1)
    }

    async fn existing_reconciliation(
        &mut self,
        statement_id: &str,
    ) -> Result<Option<AiReconciliationFact>, Self::Error> {
        Self::require_transaction(&mut *self.connection).await?;
        let row = sqlx::query("SELECT reconciled_at FROM ai_provider_statements WHERE id=$1")
            .bind(statement_id)
            .fetch_optional(&mut *self.connection)
            .await?;
        row.map(|row| {
            Ok(AiReconciliationFact {
                reconciled_at: row.try_get("reconciled_at")?,
            })
        })
        .transpose()
    }

    async fn safe_rows(&mut self, start: i64, end: i64) -> Result<Vec<AiGatewayRow>, Self::Error> {
        Self::require_transaction(&mut *self.connection).await?;
        let rows = sqlx::query(
            "SELECT r.id,r.usage_id,r.holding_id,r.company_id,r.branch_id,r.connection_id,\
             r.funding_source,r.provider,r.model,r.feature,r.status,r.input_tokens,r.output_tokens,\
             r.units_json::text units_json,r.created_at,r.completed_at,\
             e.provider_statement_id,e.currency,e.amount_minor,e.thb_satang,e.verified_at,e.reconciled_at \
             FROM ai_gateway_requests r LEFT JOIN ai_provider_cost_evidence e ON e.request_id=r.id \
             WHERE r.created_at>=$1 AND r.created_at<$2 ORDER BY r.created_at,r.id",
        )
        .bind(start)
        .bind(end)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(AiGatewayRow {
                    id: row.try_get("id")?,
                    usage_id: row.try_get("usage_id")?,
                    holding_id: row.try_get("holding_id")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    connection_id: row.try_get("connection_id")?,
                    funding_source: row.try_get("funding_source")?,
                    provider: row.try_get("provider")?,
                    model: row.try_get("model")?,
                    feature: row.try_get("feature")?,
                    status: row.try_get("status")?,
                    input_tokens: row.try_get("input_tokens")?,
                    output_tokens: row.try_get("output_tokens")?,
                    units_json: row.try_get("units_json")?,
                    created_at: row.try_get("created_at")?,
                    completed_at: row.try_get("completed_at")?,
                    provider_statement_id: row.try_get("provider_statement_id")?,
                    currency: row.try_get("currency")?,
                    amount_minor: row.try_get("amount_minor")?,
                    thb_satang: row.try_get("thb_satang")?,
                    verified_at: row.try_get("verified_at")?,
                    reconciled_at: row.try_get("reconciled_at")?,
                })
            })
            .collect()
    }
}
