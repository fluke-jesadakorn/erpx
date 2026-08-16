use erpx_agent_orchestration::PortError;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

/// A generation-v2 tenant data-plane connection boundary. The tenant is
/// immutable for the adapter lifetime, so no repository operation can select
/// a tenant dynamically or fall back to an unscoped pool query.
#[derive(Debug, Clone)]
pub(crate) struct TenantBoundPool {
    pool: PgPool,
    tenant_id: Uuid,
}

impl TenantBoundPool {
    pub(crate) fn try_new(pool: PgPool, tenant_id: Uuid) -> Result<Self, PortError> {
        if tenant_id.is_nil() {
            return Err(PortError::Denied);
        }
        Ok(Self { pool, tenant_id })
    }

    pub(crate) const fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    pub(crate) async fn begin(&self) -> Result<Transaction<'_, Postgres>, PortError> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| PortError::Persistence)?;
        bind_tenant(&mut transaction, self.tenant_id).await?;
        Ok(transaction)
    }
}

async fn bind_tenant(
    transaction: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
) -> Result<(), PortError> {
    let expected = tenant_id.to_string();
    let bound: String = sqlx::query_scalar("SELECT set_config('app.tenant_id',$1,true)")
        .bind(&expected)
        .fetch_one(&mut **transaction)
        .await
        .map_err(|_| PortError::Persistence)?;
    if bound != expected {
        return Err(PortError::Denied);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use sqlx::postgres::PgPoolOptions;

    use super::*;

    fn lazy_pool() -> PgPool {
        PgPoolOptions::new()
            .connect_lazy("postgres://erpx@127.0.0.1/erpx_agent_tenant_contract")
            .expect("syntactically valid lazy pool")
    }

    #[tokio::test]
    async fn binding_requires_one_non_nil_immutable_tenant() {
        assert!(matches!(
            TenantBoundPool::try_new(lazy_pool(), Uuid::nil()),
            Err(PortError::Denied)
        ));
        let tenant_id = Uuid::from_u128(41);
        let database = TenantBoundPool::try_new(lazy_pool(), tenant_id).expect("tenant binding");
        assert_eq!(database.tenant_id(), tenant_id);
    }

    #[test]
    fn every_agent_adapter_uses_the_single_verified_binding_implementation() {
        let source = include_str!("tenant.rs");
        assert_eq!(source.matches(concat!(".be", "gin()")).count(), 1);
        assert!(source.contains("SELECT set_config('app.tenant_id',$1,true)"));
        assert!(source.contains("if bound != expected"));
        assert!(!source.contains(concat!("BYPASS", "RLS")));
    }
}
