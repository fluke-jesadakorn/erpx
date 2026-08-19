//! Explicit holding governance maintenance. These transitions are operational
//! lifecycle enforcement, never Agent invocations or event/schedule triggers.

use chrono::{DateTime, Utc};
use erpx_agent_orchestration::PortError;
use sqlx::PgPool;
use uuid::Uuid;

use crate::holding::HoldingBoundPool;
use crate::{experience::expire_one_due_session, runtime::recover_one_expired_assignment};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AgentMaintenanceOutcome {
    pub expired_session: bool,
    pub recovered_lease: bool,
}

impl AgentMaintenanceOutcome {
    pub const fn changed(self) -> bool {
        self.expired_session || self.recovered_lease
    }
}

#[derive(Debug, Clone)]
pub struct SqlxAgentGovernanceMaintenance {
    database: HoldingBoundPool,
}

impl SqlxAgentGovernanceMaintenance {
    pub fn try_new(pool: PgPool, holding_id: Uuid) -> Result<Self, PortError> {
        Ok(Self {
            database: HoldingBoundPool::try_new(pool, holding_id)?,
        })
    }

    pub const fn holding_id(&self) -> Uuid {
        self.database.holding_id()
    }

    /// Performs at most one transition of each kind. The trusted worker loop
    /// controls cadence and bounded repetition, and claims no Agent work here.
    pub async fn run_once(&self, at: DateTime<Utc>) -> Result<AgentMaintenanceOutcome, PortError> {
        let mut tx = self.database.begin().await?;
        let expired_session = expire_one_due_session(&mut tx, self.holding_id(), at).await?;
        let recovered_lease =
            recover_one_expired_assignment(&mut tx, self.holding_id(), at).await?;
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        Ok(AgentMaintenanceOutcome {
            expired_session,
            recovered_lease,
        })
    }
}

#[cfg(test)]
mod tests {
    use sqlx::postgres::PgPoolOptions;

    use super::*;

    fn lazy_pool() -> PgPool {
        PgPoolOptions::new()
            .connect_lazy("postgres://erpx@127.0.0.1/erpx_agent_maintenance_contract")
            .expect("lazy pool")
    }

    #[tokio::test]
    async fn maintenance_requires_an_exact_non_nil_holding() {
        assert!(matches!(
            SqlxAgentGovernanceMaintenance::try_new(lazy_pool(), Uuid::nil()),
            Err(PortError::Denied)
        ));
        let holding_id = Uuid::from_u128(701);
        let maintenance =
            SqlxAgentGovernanceMaintenance::try_new(lazy_pool(), holding_id).expect("maintenance");
        assert_eq!(maintenance.holding_id(), holding_id);
    }

    #[test]
    fn queue_claims_do_not_hide_governance_maintenance() {
        let runtime = include_str!("runtime.rs");
        let claim = runtime
            .split("async fn claim_due")
            .nth(1)
            .and_then(|source| source.split("async fn load_prompt_material").next())
            .expect("claim body");
        assert!(!claim.contains("recover_one_expired_assignment"));
        let source = include_str!("maintenance.rs");
        assert!(source.contains("expire_one_due_session"));
        assert!(source.contains("recover_one_expired_assignment"));
        assert!(!source.contains(concat!("Invocation", "Origin")));
    }
}
