use async_trait::async_trait;
use erpx_agent_orchestration::{
    AgentDefinition, AgentDefinitionId, AgentDefinitionRepository, PortError,
};
use erpx_kernel::TenantScope;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{normalized, tenant::TenantBoundPool};

#[derive(Debug, Clone)]
pub struct SqlxAgentRegistry {
    database: TenantBoundPool,
}

impl SqlxAgentRegistry {
    pub fn try_new(pool: PgPool, tenant_id: Uuid) -> Result<Self, PortError> {
        Ok(Self {
            database: TenantBoundPool::try_new(pool, tenant_id)?,
        })
    }

    pub const fn tenant_id(&self) -> Uuid {
        self.database.tenant_id()
    }
}

#[async_trait]
impl AgentDefinitionRepository for SqlxAgentRegistry {
    async fn load_definition(
        &self,
        authority_scope: &TenantScope,
        id: AgentDefinitionId,
    ) -> Result<Option<AgentDefinition>, PortError> {
        let mut transaction = self.database.begin().await?;
        let definition =
            normalized::load_definition(&mut transaction, self.tenant_id(), id).await?;
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
