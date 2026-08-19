use async_trait::async_trait;
use erpx_agent_orchestration::{
    AgentDefinition, AgentDefinitionId, AgentDefinitionRepository, PortError,
};
use erpx_kernel::AuthorityScope;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{holding::HoldingBoundPool, normalized};

#[derive(Debug, Clone)]
pub struct SqlxAgentRegistry {
    database: HoldingBoundPool,
}

impl SqlxAgentRegistry {
    pub fn try_new(pool: PgPool, holding_id: Uuid) -> Result<Self, PortError> {
        Ok(Self {
            database: HoldingBoundPool::try_new(pool, holding_id)?,
        })
    }

    pub const fn holding_id(&self) -> Uuid {
        self.database.holding_id()
    }
}

#[async_trait]
impl AgentDefinitionRepository for SqlxAgentRegistry {
    async fn load_definition(
        &self,
        authority_scope: &AuthorityScope,
        id: AgentDefinitionId,
    ) -> Result<Option<AgentDefinition>, PortError> {
        let mut transaction = self.database.begin().await?;
        let definition =
            normalized::load_definition(&mut transaction, self.holding_id(), id).await?;
        transaction
            .commit()
            .await
            .map_err(|_| PortError::Persistence)?;
        let Some(definition) = definition else {
            return Ok(None);
        };
        if !authority_scope.contains(definition.scope()) {
            return Err(PortError::Denied);
        }
        Ok(Some(definition))
    }
}
