use std::{collections::BTreeSet, time::Duration as StdDuration};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use erpx_agent_orchestration::{
    AgentPin, PortError, ProviderPolicy, ProviderPolicyId, ProviderPolicyPort,
};
use erpx_kernel::{AuthorityScope, DataClassification};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::holding::HoldingBoundPool;

const MAX_POLICY_STALENESS: StdDuration = StdDuration::from_secs(5 * 60);

/// Holding-bound current policy authority. Every lookup binds RLS and selects
/// the exact pinned agent definition/version and policy digest. Snapshot JSON
/// is deliberately not consulted: the normalized governance rows are the
/// execution authority.
#[derive(Debug, Clone)]
pub struct SqlxProviderPolicyRepository {
    database: HoldingBoundPool,
    max_staleness: Duration,
}

impl SqlxProviderPolicyRepository {
    pub fn try_new(
        pool: PgPool,
        holding_id: Uuid,
        max_staleness: StdDuration,
    ) -> Result<Self, PortError> {
        if max_staleness.is_zero() || max_staleness > MAX_POLICY_STALENESS {
            return Err(PortError::ProviderDenied);
        }
        Ok(Self {
            database: HoldingBoundPool::try_new(pool, holding_id)?,
            max_staleness: Duration::from_std(max_staleness)
                .map_err(|_| PortError::ProviderDenied)?,
        })
    }

    pub const fn holding_id(&self) -> Uuid {
        self.database.holding_id()
    }
}

#[async_trait]
impl ProviderPolicyPort for SqlxProviderPolicyRepository {
    async fn load_policy(
        &self,
        authority_scope: &AuthorityScope,
        pin: &AgentPin,
        at: DateTime<Utc>,
    ) -> Result<ProviderPolicy, PortError> {
        if authority_scope.validate().is_err() || pin.manifest.validate().is_err() {
            return Err(PortError::ProviderDenied);
        }
        let freshness_cutoff = at - self.max_staleness;
        let mut tx = self.database.begin().await?;
        let row = sqlx::query(
            "SELECT p.provider_policy_id,p.policy_version,p.external_provider,p.dpa_ref,
                    p.maximum_classification,p.policy_digest,p.expires_at,
                    p.freshness_checked_at,d.active_scope,d.agent_principal_ref,
                    v.purpose,v.provider_ref,v.model_ref,v.use_case,v.prompt_digest,
                    v.policy_digest AS version_policy_digest,v.maximum_classification AS version_classification,
                    v.security_review_ref,v.privacy_review_ref,v.max_attempts,v.timeout_seconds
             FROM erpx_agent_write.agent_provider_policies p
             JOIN erpx_agent_write.agent_versions v
               ON v.holding_id=p.holding_id AND v.provider_ref=p.provider_ref
              AND v.policy_digest=p.policy_digest
             JOIN erpx_agent_write.agent_definitions d
               ON d.holding_id=v.holding_id AND d.agent_definition_id=v.agent_definition_id
             LEFT JOIN erpx_agent_write.agent_provider_policy_revocations r
               ON r.holding_id=p.holding_id AND r.provider_policy_id=p.provider_policy_id
              AND r.policy_version=p.policy_version
             WHERE p.holding_id=$1 AND p.provider_ref=$2 AND p.policy_digest=$3
               AND p.policy_status='approved' AND p.is_current
               AND p.effective_at<=$4 AND p.expires_at>$4
               AND p.freshness_checked_at BETWEEN $5 AND $4
               AND r.provider_policy_id IS NULL
               AND v.agent_version_id=$6 AND v.agent_definition_id=$7
               AND v.version_status='approved' AND d.definition_status='approved'
               AND d.agent_principal_ref=$8",
        )
        .bind(self.holding_id())
        .bind(&pin.manifest.provider_ref)
        .bind(&pin.manifest.policy_digest)
        .bind(at)
        .bind(freshness_cutoff)
        .bind(pin.manifest.version_id.as_uuid())
        .bind(pin.definition_id.as_uuid())
        .bind(&pin.agent_principal_ref)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?
        .ok_or(PortError::ProviderDenied)?;

        let provider_policy_id: Uuid = row
            .try_get("provider_policy_id")
            .map_err(|_| PortError::Persistence)?;
        let definition_scope: AuthorityScope = serde_json::from_value(
            row.try_get("active_scope")
                .map_err(|_| PortError::Persistence)?,
        )
        .map_err(|_| PortError::Persistence)?;
        if !definition_scope.contains(authority_scope)
            || !pin.manifest.permits_scope(authority_scope)
            || row_text(&row, "agent_principal_ref")? != pin.agent_principal_ref
            || row_text(&row, "purpose")? != pin.manifest.purpose
            || row_text(&row, "provider_ref")? != pin.manifest.provider_ref
            || row_text(&row, "model_ref")? != pin.manifest.model_ref
            || row_text(&row, "use_case")? != pin.manifest.use_case
            || !digest_matches(
                &row_text(&row, "prompt_digest")?,
                &pin.manifest.prompt_digest,
            )
            || !digest_matches(
                &row_text(&row, "version_policy_digest")?,
                &pin.manifest.policy_digest,
            )
            || parse_classification(&row_text(&row, "version_classification")?)?
                != pin.manifest.maximum_classification
            || row_text(&row, "security_review_ref")? != pin.manifest.security_review_ref
            || row_text(&row, "privacy_review_ref")? != pin.manifest.privacy_review_ref
            || positive_u16(
                row.try_get("max_attempts")
                    .map_err(|_| PortError::Persistence)?,
            )? != pin.manifest.max_attempts
            || positive_u32(
                row.try_get("timeout_seconds")
                    .map_err(|_| PortError::Persistence)?,
            )? != pin.manifest.timeout_seconds
        {
            return deny_and_rollback(tx).await;
        }

        let capabilities = load_text_set(
            &mut tx,
            "SELECT capability FROM erpx_agent_write.agent_version_capabilities
             WHERE holding_id=$1 AND agent_version_id=$2 ORDER BY capability",
            self.holding_id(),
            pin.manifest.version_id.as_uuid(),
        )
        .await?;
        let tools = load_text_set(
            &mut tx,
            "SELECT tool_ref FROM erpx_agent_write.agent_version_tools
             WHERE holding_id=$1 AND agent_version_id=$2 ORDER BY tool_ref",
            self.holding_id(),
            pin.manifest.version_id.as_uuid(),
        )
        .await?;
        let knowledge_scopes = load_text_set(
            &mut tx,
            "SELECT knowledge_scope FROM erpx_agent_write.agent_version_knowledge_scopes
             WHERE holding_id=$1 AND agent_version_id=$2 ORDER BY knowledge_scope",
            self.holding_id(),
            pin.manifest.version_id.as_uuid(),
        )
        .await?;
        let evaluation_refs = load_text_set(
            &mut tx,
            "SELECT evaluation_ref FROM erpx_agent_write.agent_version_evaluations
             WHERE holding_id=$1 AND agent_version_id=$2 ORDER BY evaluation_ref",
            self.holding_id(),
            pin.manifest.version_id.as_uuid(),
        )
        .await?;
        let organization_scopes = load_scope_set(
            &mut tx,
            self.holding_id(),
            pin.manifest.version_id.as_uuid(),
        )
        .await?;
        if capabilities != pin.manifest.capabilities
            || tools != pin.manifest.tool_refs
            || knowledge_scopes != pin.manifest.knowledge_scopes
            || evaluation_refs != pin.manifest.evaluation_refs
            || organization_scopes != canonical_scope_set(&pin.manifest.organization_scopes)?
        {
            return deny_and_rollback(tx).await;
        }

        let allowed_models = load_policy_text_set(
            &mut tx,
            "SELECT model_ref FROM erpx_agent_write.agent_provider_policy_models
             WHERE holding_id=$1 AND provider_policy_id=$2 ORDER BY model_ref",
            self.holding_id(),
            provider_policy_id,
        )
        .await?;
        let allowed_use_cases = load_policy_text_set(
            &mut tx,
            "SELECT use_case FROM erpx_agent_write.agent_provider_policy_use_cases
             WHERE holding_id=$1 AND provider_policy_id=$2 ORDER BY use_case",
            self.holding_id(),
            provider_policy_id,
        )
        .await?;
        let allowed_residencies = load_policy_text_set(
            &mut tx,
            "SELECT residency FROM erpx_agent_write.agent_provider_policy_residencies
             WHERE holding_id=$1 AND provider_policy_id=$2 ORDER BY residency",
            self.holding_id(),
            provider_policy_id,
        )
        .await?;
        let approved_evaluation_refs = load_approved_evaluations(
            &mut tx,
            self.holding_id(),
            provider_policy_id,
            pin.manifest.version_id.as_uuid(),
            at,
        )
        .await?;
        if allowed_models.is_empty()
            || allowed_use_cases.is_empty()
            || allowed_residencies.is_empty()
            || approved_evaluation_refs != pin.manifest.evaluation_refs
        {
            return deny_and_rollback(tx).await;
        }

        let policy_version = positive_u64(
            row.try_get("policy_version")
                .map_err(|_| PortError::Persistence)?,
        )?;
        let freshness_checked_at: DateTime<Utc> = row
            .try_get("freshness_checked_at")
            .map_err(|_| PortError::Persistence)?;
        let expires_at: DateTime<Utc> = row
            .try_get("expires_at")
            .map_err(|_| PortError::Persistence)?;
        let fresh_until = std::cmp::min(freshness_checked_at + self.max_staleness, expires_at);
        let policy = ProviderPolicy {
            id: ProviderPolicyId::try_from_uuid(provider_policy_id)
                .map_err(|_| PortError::Persistence)?,
            agent_definition_id: pin.definition_id,
            agent_version_id: pin.manifest.version_id,
            policy_version,
            provider_ref: pin.manifest.provider_ref.clone(),
            external: row
                .try_get("external_provider")
                .map_err(|_| PortError::Persistence)?,
            approved: true,
            dpa_ref: row.try_get("dpa_ref").map_err(|_| PortError::Persistence)?,
            allowed_residencies,
            allowed_models,
            allowed_use_cases,
            maximum_classification: parse_classification(&row_text(
                &row,
                "maximum_classification",
            )?)?,
            policy_digest: row_text(&row, "policy_digest")?,
            approved_evaluation_refs,
            freshness_checked_at,
            fresh_until,
            expires_at,
        };
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        Ok(policy)
    }
}

async fn deny_and_rollback<T>(tx: Transaction<'_, Postgres>) -> Result<T, PortError> {
    tx.rollback().await.map_err(|_| PortError::Persistence)?;
    Err(PortError::ProviderDenied)
}

async fn load_text_set(
    tx: &mut Transaction<'_, Postgres>,
    query: &'static str,
    holding_id: Uuid,
    version_id: Uuid,
) -> Result<BTreeSet<String>, PortError> {
    sqlx::query_scalar(query)
        .bind(holding_id)
        .bind(version_id)
        .fetch_all(&mut **tx)
        .await
        .map(|values| values.into_iter().collect())
        .map_err(|_| PortError::Persistence)
}

async fn load_policy_text_set(
    tx: &mut Transaction<'_, Postgres>,
    query: &'static str,
    holding_id: Uuid,
    policy_id: Uuid,
) -> Result<BTreeSet<String>, PortError> {
    load_text_set(tx, query, holding_id, policy_id).await
}

async fn load_approved_evaluations(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: Uuid,
    policy_id: Uuid,
    version_id: Uuid,
    at: DateTime<Utc>,
) -> Result<BTreeSet<String>, PortError> {
    sqlx::query_scalar(
        "SELECT evaluation_ref
         FROM erpx_agent_write.agent_provider_policy_evaluations
         WHERE holding_id=$1 AND provider_policy_id=$2 AND agent_version_id=$3
           AND decision_status='approved' AND decided_at<=$4 AND expires_at>$4
         ORDER BY evaluation_ref",
    )
    .bind(holding_id)
    .bind(policy_id)
    .bind(version_id)
    .bind(at)
    .fetch_all(&mut **tx)
    .await
    .map(|values| values.into_iter().collect())
    .map_err(|_| PortError::Persistence)
}

async fn load_scope_set(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: Uuid,
    version_id: Uuid,
) -> Result<BTreeSet<String>, PortError> {
    let values: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT active_scope
         FROM erpx_agent_write.agent_version_organization_scopes
         WHERE holding_id=$1 AND agent_version_id=$2 ORDER BY scope_key",
    )
    .bind(holding_id)
    .bind(version_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    let scopes = values
        .into_iter()
        .map(|value| serde_json::from_value(value).map_err(|_| PortError::Persistence))
        .collect::<Result<Vec<AuthorityScope>, _>>()?;
    canonical_scope_set(&scopes)
}

fn canonical_scope_set(scopes: &[AuthorityScope]) -> Result<BTreeSet<String>, PortError> {
    scopes
        .iter()
        .map(|scope| serde_json::to_string(scope).map_err(|_| PortError::Persistence))
        .collect()
}

fn row_text(row: &sqlx::postgres::PgRow, column: &str) -> Result<String, PortError> {
    row.try_get(column).map_err(|_| PortError::Persistence)
}

fn digest_matches(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn parse_classification(value: &str) -> Result<DataClassification, PortError> {
    match value {
        "public" => Ok(DataClassification::Public),
        "internal" => Ok(DataClassification::Internal),
        "confidential" => Ok(DataClassification::Confidential),
        "restricted" => Ok(DataClassification::Restricted),
        _ => Err(PortError::Persistence),
    }
}

fn positive_u64(value: i64) -> Result<u64, PortError> {
    u64::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or(PortError::Persistence)
}

fn positive_u32(value: i32) -> Result<u32, PortError> {
    u32::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or(PortError::Persistence)
}

fn positive_u16(value: i32) -> Result<u16, PortError> {
    u16::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or(PortError::Persistence)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    #[tokio::test]
    async fn policy_repository_requires_a_non_nil_holding_and_bounded_freshness() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://unused@127.0.0.1/unused")
            .expect("lazy pool");
        assert!(
            SqlxProviderPolicyRepository::try_new(
                pool.clone(),
                Uuid::nil(),
                StdDuration::from_secs(60)
            )
            .is_err()
        );
        assert!(
            SqlxProviderPolicyRepository::try_new(
                pool,
                Uuid::from_u128(1),
                StdDuration::from_secs(301)
            )
            .is_err()
        );
    }

    #[test]
    fn policy_source_is_exact_version_scope_evaluation_and_revocation_bound() {
        let source = include_str!("policy.rs");
        for fence in [
            "v.agent_version_id=$6",
            "v.agent_definition_id=$7",
            "v.version_status='approved'",
            "d.definition_status='approved'",
            "p.freshness_checked_at BETWEEN $5 AND $4",
            "r.provider_policy_id IS NULL",
            "decision_status='approved'",
            "approved_evaluation_refs != pin.manifest.evaluation_refs",
        ] {
            assert!(source.contains(fence), "missing policy fence: {fence}");
        }
        assert!(!source.contains(concat!("BYPASS", "RLS")));
    }
}
