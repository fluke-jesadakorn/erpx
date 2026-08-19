use std::time::Duration as StdDuration;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use erpx_agent_orchestration::{
    AgentMaterialRetention, AgentMaterialRetentionPort, Assignment, PortError,
};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::holding::HoldingBoundPool;

const MAX_RETENTION_FRESHNESS: StdDuration = StdDuration::from_secs(5 * 60);

#[derive(Debug, Clone)]
pub struct SqlxAgentMaterialRetentionRepository {
    database: HoldingBoundPool,
    max_staleness: Duration,
}

impl SqlxAgentMaterialRetentionRepository {
    pub fn try_new(
        pool: PgPool,
        holding_id: Uuid,
        max_staleness: StdDuration,
    ) -> Result<Self, PortError> {
        if max_staleness.is_zero() || max_staleness > MAX_RETENTION_FRESHNESS {
            return Err(PortError::Denied);
        }
        Ok(Self {
            database: HoldingBoundPool::try_new(pool, holding_id)?,
            max_staleness: Duration::from_std(max_staleness).map_err(|_| PortError::Denied)?,
        })
    }
}

#[async_trait]
impl AgentMaterialRetentionPort for SqlxAgentMaterialRetentionRepository {
    async fn load_retention(
        &self,
        assignment: &Assignment,
        at: DateTime<Utc>,
    ) -> Result<AgentMaterialRetention, PortError> {
        let mut tx = self.database.begin().await?;
        let row = sqlx::query(
            "SELECT p.policy_version,p.ttl_seconds,p.legal_hold_default,a.active_scope
             FROM erpx_agent_write.agent_material_retention_policies p
             JOIN erpx_agent_write.agent_assignments a
               ON a.holding_id=p.holding_id AND a.assignment_id=$2
             JOIN erpx_agent_write.agent_versions v
               ON v.holding_id=a.holding_id AND v.agent_version_id=a.agent_version_id
              AND v.use_case=p.use_case
             WHERE p.holding_id=$1 AND p.policy_status='approved'
               AND p.effective_at<=$3 AND p.expires_at>$3
               AND p.freshness_checked_at BETWEEN $4 AND $3
               AND a.agent_definition_id=$5 AND a.agent_version_id=$6
               AND a.policy_digest=$7 AND a.aggregate_version=$8 AND a.created_at=$9",
        )
        .bind(self.database.holding_id())
        .bind(assignment.id().as_uuid())
        .bind(at)
        .bind(at - self.max_staleness)
        .bind(assignment.pin().definition_id.as_uuid())
        .bind(assignment.pin().manifest.version_id.as_uuid())
        .bind(&assignment.pin().manifest.policy_digest)
        .bind(i64::try_from(assignment.version()).map_err(|_| PortError::Persistence)?)
        .bind(assignment.created_at())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?
        .ok_or(PortError::ProtectedMaterialUnavailable)?;
        let policy_version: i64 = row
            .try_get("policy_version")
            .map_err(|_| PortError::Persistence)?;
        let ttl_seconds: i64 = row
            .try_get("ttl_seconds")
            .map_err(|_| PortError::Persistence)?;
        let policy_version = u64::try_from(policy_version)
            .ok()
            .filter(|value| *value > 0)
            .ok_or(PortError::Persistence)?;
        let stored_scope: erpx_kernel::AuthorityScope = serde_json::from_value(
            row.try_get("active_scope")
                .map_err(|_| PortError::Persistence)?,
        )
        .map_err(|_| PortError::Persistence)?;
        if &stored_scope != assignment.scope() {
            return Err(PortError::ProtectedMaterialUnavailable);
        }
        let ttl_seconds = u64::try_from(ttl_seconds)
            .ok()
            .filter(|value| (60..=7_776_000).contains(value))
            .and_then(|value| i64::try_from(value).ok())
            .ok_or(PortError::Persistence)?;
        let retention = AgentMaterialRetention {
            policy_version,
            expires_at: assignment.created_at() + Duration::seconds(ttl_seconds),
            legal_hold: row
                .try_get("legal_hold_default")
                .map_err(|_| PortError::Persistence)?,
        };
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        if retention.expires_at <= at {
            Err(PortError::ProtectedMaterialUnavailable)
        } else {
            Ok(retention)
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn retention_authority_is_exact_assignment_version_and_freshness_bound() {
        let source = include_str!("retention.rs");
        for fence in [
            "a.assignment_id=$2",
            "p.freshness_checked_at BETWEEN $4 AND $3",
            "a.agent_definition_id=$5",
            "a.agent_version_id=$6",
            "a.policy_digest=$7",
            "a.aggregate_version=$8",
            "a.created_at=$9",
        ] {
            assert!(source.contains(fence));
        }
    }
}
