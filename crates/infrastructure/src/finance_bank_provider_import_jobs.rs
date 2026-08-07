//! Provider-neutral command and query boundary for Finance bank import jobs.
//!
//! This module owns the transport-independent request, authorization proof,
//! status DTO, and SQLx queue adapter shared by inbound runtimes. Provider
//! credentials and provider response payloads are deliberately excluded. The
//! Worker retains ownership of claiming, scheduling, retries, and execution.

use chrono::{DateTime, NaiveDate, Utc};
use erpx_domain::{
    contracts::{ActorKind, CommandContext, TenantScope},
    finance::{
        FINANCE_AUTHORIZATION_CONTRACT_VERSION, FinanceAccessPath, FinanceAuthorizationDecision,
        FinanceAuthorizationOperation,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    BankProviderAccountBinding, BankProviderCredentialKey, BankStatementPageRequest,
    PersistenceError, SqlxControlPlane,
};

pub const FINANCE_BANK_PROVIDER_IMPORT_MAX_ATTEMPTS: u16 = 8;

const MAX_IMPORT_IDEMPOTENCY_BYTES: usize = 200;
const FINANCE_BANK_PROVIDER_IMPORT_QUEUED_EVENT: &str = "finance.bank_provider_import.queued.v1";
const FINANCE_BANK_PROVIDER_IMPORT_AGGREGATE: &str = "finance_bank_provider_import_job";

/// Redacted contract-validation errors safe to expose across runtime adapters.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum FinanceBankProviderImportError {
    #[error("invalid Finance bank-provider import request")]
    InvalidRequest,
    #[error("Finance bank-provider import authorization was denied")]
    AuthorizationDenied,
    #[error("Finance bank-provider import requires MFA authorization")]
    MfaRequired,
}

/// Facts that bind an import to one provider account and one tenant route.
///
/// The serialized field names intentionally match the existing Worker
/// contract so an upgraded Server can enqueue jobs for the current Worker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceBankProviderImportRequest {
    pub provider_account_ref: String,
    pub provider_key: String,
    pub tenant_id: String,
    pub holding_ref: String,
    pub credential_key: BankProviderCredentialKey,
    pub account: BankProviderAccountBinding,
    pub starts_on: NaiveDate,
    pub ends_on: NaiveDate,
    pub page_size: u32,
    pub import_id: Uuid,
    pub context: CommandContext,
}

impl FinanceBankProviderImportRequest {
    pub fn validate(&self) -> Result<(), FinanceBankProviderImportError> {
        if self.provider_account_ref.trim().is_empty()
            || self.provider_key.trim().is_empty()
            || self.tenant_id.trim().is_empty()
            || self.holding_ref.trim().is_empty()
            || self.starts_on > self.ends_on
            || self.import_id.is_nil()
            || self.account.account().provider() != self.provider_key
            || self.account.account().storage_reference() != self.provider_account_ref
        {
            return Err(FinanceBankProviderImportError::InvalidRequest);
        }
        BankProviderCredentialKey::try_new(self.credential_key.as_str())
            .map_err(|_| FinanceBankProviderImportError::InvalidRequest)?;
        if self.context.actor.principal_ref.trim().is_empty()
            || self.context.actor.principal_ref.len() > 255
            || self
                .context
                .actor
                .principal_ref
                .chars()
                .any(char::is_control)
        {
            return Err(FinanceBankProviderImportError::InvalidRequest);
        }
        self.context
            .validate()
            .map_err(|_| FinanceBankProviderImportError::InvalidRequest)?;
        if matches!(self.context.tenant_scope, TenantScope::Platform)
            || !self.context.tenant_scope.contains(self.account.scope())
        {
            return Err(FinanceBankProviderImportError::AuthorizationDenied);
        }
        BankStatementPageRequest::try_new(
            self.account.clone(),
            self.starts_on,
            self.ends_on,
            None,
            self.page_size,
        )
        .map_err(|_| FinanceBankProviderImportError::InvalidRequest)?;
        Ok(())
    }
}

/// Explicit, decision-derived authorization proof for the high-risk import.
///
/// `CommandContext` has no MFA assertion, so construction is limited to a
/// validated Finance authorization decision and bound to its actor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceBankProviderImportAuthorization {
    actor_ref: String,
    mfa_verified: bool,
    access_path: FinanceAccessPath,
}

impl FinanceBankProviderImportAuthorization {
    pub fn from_decision(
        decision: &FinanceAuthorizationDecision,
        context: &CommandContext,
    ) -> Result<Self, FinanceBankProviderImportError> {
        if decision.operation != FinanceAuthorizationOperation::ImportBankStatement
            || decision.required_permission
                != FinanceAuthorizationOperation::ImportBankStatement.required_permission()
            || decision.contract_version != FINANCE_AUTHORIZATION_CONTRACT_VERSION
            || !decision.high_risk
            || !decision.mfa_verified
        {
            return Err(FinanceBankProviderImportError::MfaRequired);
        }
        let authorization = Self {
            actor_ref: context.actor.principal_ref.clone(),
            mfa_verified: decision.mfa_verified,
            access_path: decision.access_path.clone(),
        };
        authorization.validate(context)?;
        Ok(authorization)
    }

    fn validate(&self, context: &CommandContext) -> Result<(), FinanceBankProviderImportError> {
        context
            .validate()
            .map_err(|_| FinanceBankProviderImportError::AuthorizationDenied)?;
        if self.actor_ref != context.actor.principal_ref
            || self.actor_ref.trim().is_empty()
            || !self.mfa_verified
            || matches!(context.tenant_scope, TenantScope::Platform)
        {
            return Err(FinanceBankProviderImportError::AuthorizationDenied);
        }
        if matches!(
            context.actor.kind,
            ActorKind::TenantUser | ActorKind::PlatformUser
        ) && context.actor.session_id.is_none()
        {
            return Err(FinanceBankProviderImportError::AuthorizationDenied);
        }
        if let FinanceAccessPath::LocalBreakGlass { evidence_ref } = &self.access_path
            && (context.actor.kind != ActorKind::TenantUser
                || context.actor.represented_principal_ref.is_some()
                || context.actor.session_id.is_none()
                || evidence_ref.trim().is_empty()
                || evidence_ref.len() > 255
                || evidence_ref.chars().any(char::is_control))
        {
            return Err(FinanceBankProviderImportError::AuthorizationDenied);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceBankProviderImportJobStatus {
    pub job_id: Uuid,
    pub tenant_id: String,
    pub holding_ref: String,
    pub import_id: Uuid,
    pub state: String,
    pub source_fence: u64,
    pub attempts: u16,
    pub max_attempts: u16,
    pub available_at: DateTime<Utc>,
    pub lease_until: Option<DateTime<Utc>>,
    pub last_failure_class: Option<String>,
    pub completed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

/// SQLx command/query adapter for the Server-facing bank import job boundary.
///
/// Enqueue writes the job, audit fact, and outbox intent atomically to the
/// control write database. Status reads exclusively from the control read
/// model. Worker lifecycle methods remain in the Worker-owned adapter.
#[derive(Debug, Clone)]
pub struct SqlxFinanceBankProviderImportJobSource {
    write_pool: PgPool,
    read_pool: PgPool,
}

impl SqlxFinanceBankProviderImportJobSource {
    pub fn new(control: &SqlxControlPlane) -> Self {
        Self {
            write_pool: control.pools().write.clone(),
            read_pool: control.pools().read.clone(),
        }
    }

    pub async fn enqueue(
        &self,
        request: &FinanceBankProviderImportRequest,
        authorization: &FinanceBankProviderImportAuthorization,
        source_fence: u64,
        available_at: DateTime<Utc>,
        max_attempts: u16,
    ) -> Result<Uuid, PersistenceError> {
        request
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        authorization
            .validate(&request.context)
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        if source_fence == 0
            || max_attempts == 0
            || max_attempts > FINANCE_BANK_PROVIDER_IMPORT_MAX_ATTEMPTS
        {
            return Err(PersistenceError::InvalidData(
                "Finance bank import job bounds are invalid".into(),
            ));
        }
        let idempotency_key = request.context.idempotency_key.as_str();
        if idempotency_key.trim().is_empty()
            || idempotency_key.len() > MAX_IMPORT_IDEMPOTENCY_BYTES
            || idempotency_key.chars().any(char::is_control)
        {
            return Err(PersistenceError::InvalidData(
                "Finance bank import idempotency key is invalid".into(),
            ));
        }

        let request_value = serde_json::to_value(request)?;
        let authorization_value = serde_json::to_value(authorization)?;
        let request_hash = hash_request(&request_value, &authorization_value);
        let job_id = Uuid::new_v4();
        let mut tx = self.write_pool.begin().await?;
        if let Some(row) = sqlx::query(
            "SELECT job_id, request_hash
             FROM erpx_control.finance_bank_provider_import_jobs
             WHERE tenant_id=$1 AND holding_ref=$2 AND import_id=$3
             FOR UPDATE",
        )
        .bind(&request.tenant_id)
        .bind(&request.holding_ref)
        .bind(request.import_id)
        .fetch_optional(&mut *tx)
        .await?
        {
            let existing_hash: String = row.try_get("request_hash")?;
            let existing_id: Uuid = row.try_get("job_id")?;
            if existing_hash != request_hash {
                return Err(PersistenceError::IdempotencyConflict {
                    key: idempotency_key.into(),
                });
            }
            tx.commit().await?;
            self.sync_status(existing_id).await?;
            return Ok(existing_id);
        }

        sqlx::query(
            "INSERT INTO erpx_control.finance_bank_provider_import_jobs
                (job_id, tenant_id, holding_ref, import_id, provider_account_ref,
                 provider_key, credential_key_ref, source_fence, request,
                 authorization_facts, request_hash, state, available_at, max_attempts,
                 attempts, created_at, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,'pending',$12,$13,0,now(),now())",
        )
        .bind(job_id)
        .bind(&request.tenant_id)
        .bind(&request.holding_ref)
        .bind(request.import_id)
        .bind(&request.provider_account_ref)
        .bind(&request.provider_key)
        .bind(request.credential_key.as_str())
        .bind(as_i64(source_fence, "Finance bank import source fence")?)
        .bind(request_value)
        .bind(authorization_value)
        .bind(request_hash)
        .bind(available_at)
        .bind(i16::try_from(max_attempts).map_err(|_| {
            PersistenceError::InvalidData("Finance bank import attempt bound overflow".into())
        })?)
        .execute(&mut *tx)
        .await?;

        let event_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("finance-bank-provider-import:{job_id}").as_bytes(),
        );
        let event_key = format!("finance.bank_provider_import.queued:{job_id}");
        let tenant_scope = serde_json::to_value(&request.context.tenant_scope)?;
        let actor = serde_json::to_value(&request.context.actor)?;
        let payload = json!({
            "job_id": job_id,
            "tenant_id": request.tenant_id,
            "holding_ref": request.holding_ref,
            "import_id": request.import_id,
            "provider_account_ref": request.provider_account_ref,
            "provider_key": request.provider_key,
            "source_fence": source_fence,
        });
        sqlx::query(
            "INSERT INTO erpx_control.outbox_events
                (id, event_type, event_version, aggregate_type, aggregate_id,
                 aggregate_version, tenant_scope, actor, correlation_id,
                 causation_id, idempotency_key, payload, occurred_at, available_at)
             VALUES ($1,$2,1,$3,$4,1,$5,$6,$7,$8,$9,$10,now(),$11)
             ON CONFLICT (idempotency_key) DO NOTHING",
        )
        .bind(event_id)
        .bind(FINANCE_BANK_PROVIDER_IMPORT_QUEUED_EVENT)
        .bind(FINANCE_BANK_PROVIDER_IMPORT_AGGREGATE)
        .bind(job_id)
        .bind(tenant_scope)
        .bind(&actor)
        .bind(request.context.correlation_id)
        .bind(request.context.causation_id)
        .bind(&event_key)
        .bind(payload)
        .bind(available_at)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_control.audit_events
                (id, actor_ref, customer_account_id, action, purpose,
                 correlation_id, recorded_at, payload)
             VALUES ($1,$2,NULL,$3,$4,$5,now(),$6)",
        )
        .bind(Uuid::new_v5(&event_id, b"audit"))
        .bind(&request.context.actor.principal_ref)
        .bind("finance.bank_provider_import.queued")
        .bind(&request.context.audit.purpose)
        .bind(request.context.correlation_id)
        .bind(json!({ "job_id": job_id, "source_fence": source_fence }))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.sync_status(job_id).await?;
        Ok(job_id)
    }

    pub async fn status(
        &self,
        job_id: Uuid,
    ) -> Result<Option<FinanceBankProviderImportJobStatus>, PersistenceError> {
        if job_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "Finance bank import job ID is nil".into(),
            ));
        }
        let row = sqlx::query(
            "SELECT job_id, tenant_id, holding_ref, import_id, state,
                    source_fence, attempts, max_attempts, available_at,
                    lease_until, last_failure_class, completed_at, updated_at
             FROM erpx_control.finance_bank_provider_import_job_status
             WHERE job_id=$1",
        )
        .bind(job_id)
        .fetch_optional(&self.read_pool)
        .await?;
        row.map(status_from_row).transpose()
    }

    async fn sync_status(&self, job_id: Uuid) -> Result<(), PersistenceError> {
        let row = sqlx::query(
            "SELECT job_id, tenant_id, holding_ref, import_id, state,
                    source_fence, attempts, max_attempts, available_at,
                    lease_until, last_failure_class, completed_at, updated_at
             FROM erpx_control.finance_bank_provider_import_jobs
             WHERE job_id=$1",
        )
        .bind(job_id)
        .fetch_optional(&self.write_pool)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "finance bank provider import job",
            id: job_id,
        })?;
        let status = status_from_row(row)?;
        sqlx::query(
            "INSERT INTO erpx_control.finance_bank_provider_import_job_status
                (job_id, tenant_id, holding_ref, import_id, state, source_fence,
                 attempts, max_attempts, available_at, lease_until,
                 last_failure_class, completed_at, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
             ON CONFLICT (job_id) DO UPDATE SET
                 state=EXCLUDED.state, source_fence=EXCLUDED.source_fence,
                 attempts=EXCLUDED.attempts, max_attempts=EXCLUDED.max_attempts,
                 available_at=EXCLUDED.available_at, lease_until=EXCLUDED.lease_until,
                 last_failure_class=EXCLUDED.last_failure_class,
                 completed_at=EXCLUDED.completed_at, updated_at=EXCLUDED.updated_at",
        )
        .bind(status.job_id)
        .bind(&status.tenant_id)
        .bind(&status.holding_ref)
        .bind(status.import_id)
        .bind(&status.state)
        .bind(as_i64(
            status.source_fence,
            "Finance bank status source fence",
        )?)
        .bind(i16::try_from(status.attempts).map_err(|_| {
            PersistenceError::InvalidData("Finance bank status attempt overflow".into())
        })?)
        .bind(i16::try_from(status.max_attempts).map_err(|_| {
            PersistenceError::InvalidData("Finance bank status max-attempt overflow".into())
        })?)
        .bind(status.available_at)
        .bind(status.lease_until)
        .bind(status.last_failure_class.as_deref())
        .bind(status.completed_at)
        .bind(status.updated_at)
        .execute(&self.read_pool)
        .await?;
        Ok(())
    }
}

fn status_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<FinanceBankProviderImportJobStatus, PersistenceError> {
    Ok(FinanceBankProviderImportJobStatus {
        job_id: row.try_get("job_id")?,
        tenant_id: row.try_get("tenant_id")?,
        holding_ref: row.try_get("holding_ref")?,
        import_id: row.try_get("import_id")?,
        state: row.try_get("state")?,
        source_fence: as_u64(
            row.try_get("source_fence")?,
            "Finance bank status source fence",
        )?,
        attempts: u16::try_from(row.try_get::<i16, _>("attempts")?).map_err(|_| {
            PersistenceError::InvalidData("Finance bank status attempts invalid".into())
        })?,
        max_attempts: u16::try_from(row.try_get::<i16, _>("max_attempts")?).map_err(|_| {
            PersistenceError::InvalidData("Finance bank status max attempts invalid".into())
        })?,
        available_at: row.try_get("available_at")?,
        lease_until: row.try_get("lease_until")?,
        last_failure_class: row.try_get("last_failure_class")?,
        completed_at: row.try_get("completed_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn hash_request(request: &serde_json::Value, authorization: &serde_json::Value) -> String {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "request": request,
        "authorization": authorization,
    }))
    .unwrap_or_default();
    hex::encode(Sha256::digest(bytes))
}

fn as_i64(value: u64, label: &'static str) -> Result<i64, PersistenceError> {
    i64::try_from(value).map_err(|_| PersistenceError::InvalidData(format!("{label} overflow")))
}

fn as_u64(value: i64, label: &'static str) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| PersistenceError::InvalidData(format!("{label} invalid")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_domain::contracts::{ActorAttribution, ActorKind};

    fn request(session_id: Option<Uuid>) -> FinanceBankProviderImportRequest {
        let holding_id = Uuid::from_u128(1);
        let company_id = Uuid::from_u128(2);
        let scope = TenantScope::Company {
            holding_id,
            company_id,
        };
        let account = BankProviderAccountBinding::try_new(
            Uuid::from_u128(3),
            crate::BankProviderAccountRef::try_new("mockbank", "acct-opaque-1").unwrap(),
            scope.clone(),
            holding_id,
            company_id,
            "THB",
            "Operating account",
        )
        .unwrap();
        let context = CommandContext::try_new(
            "bank-import-contract-test",
            ActorAttribution {
                principal_ref: "finance-user".into(),
                kind: ActorKind::TenantUser,
                session_id,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope,
            "authorized bank import",
        )
        .unwrap();
        FinanceBankProviderImportRequest {
            provider_account_ref: "mockbank:acct-opaque-1".into(),
            provider_key: "mockbank".into(),
            tenant_id: "tenant-opaque-1".into(),
            holding_ref: "holding-opaque-1".into(),
            credential_key: BankProviderCredentialKey::try_new("vault/finance/mockbank").unwrap(),
            account,
            starts_on: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            ends_on: NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
            page_size: 100,
            import_id: Uuid::from_u128(4),
            context,
        }
    }

    fn authorized_decision() -> FinanceAuthorizationDecision {
        FinanceAuthorizationDecision {
            contract_version: FINANCE_AUTHORIZATION_CONTRACT_VERSION,
            operation: FinanceAuthorizationOperation::ImportBankStatement,
            required_permission: "finance.write".into(),
            high_risk: true,
            mfa_verified: true,
            distinct_approver: None,
            access_path: FinanceAccessPath::Normal,
        }
    }

    #[test]
    fn request_preserves_the_worker_json_contract() {
        let request = request(Some(Uuid::from_u128(5)));
        request.validate().unwrap();
        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(value["provider_key"], "mockbank");
        assert_eq!(value["provider_account_ref"], "mockbank:acct-opaque-1");
        assert!(value.get("credential_key").is_some());
        assert!(value.get("context").is_some());
    }

    #[test]
    fn authorization_requires_mfa_and_a_bound_live_session() {
        let request_without_session = request(None);
        assert_eq!(
            FinanceBankProviderImportAuthorization::from_decision(
                &authorized_decision(),
                &request_without_session.context,
            ),
            Err(FinanceBankProviderImportError::AuthorizationDenied)
        );

        let request = request(Some(Uuid::from_u128(5)));
        let mut missing_mfa = authorized_decision();
        missing_mfa.mfa_verified = false;
        assert_eq!(
            FinanceBankProviderImportAuthorization::from_decision(&missing_mfa, &request.context,),
            Err(FinanceBankProviderImportError::MfaRequired)
        );
        assert!(
            FinanceBankProviderImportAuthorization::from_decision(
                &authorized_decision(),
                &request.context,
            )
            .is_ok()
        );
    }

    #[test]
    fn request_hash_is_stable_for_the_same_contract_values() {
        let request = request(Some(Uuid::from_u128(5)));
        let authorization = FinanceBankProviderImportAuthorization::from_decision(
            &authorized_decision(),
            &request.context,
        )
        .unwrap();
        let request_value = serde_json::to_value(request).unwrap();
        let authorization_value = serde_json::to_value(authorization).unwrap();
        assert_eq!(
            hash_request(&request_value, &authorization_value),
            hash_request(&request_value, &authorization_value)
        );
    }
}
