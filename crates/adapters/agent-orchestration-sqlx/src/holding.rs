use erpx_agent_orchestration::PortError;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

/// A current holding data-plane connection boundary. The holding is
/// immutable for the adapter lifetime, so no repository operation can select
/// a holding dynamically or fall back to an unscoped pool query.
#[derive(Debug, Clone)]
pub(crate) struct HoldingBoundPool {
    pool: PgPool,
    holding_id: Uuid,
}

impl HoldingBoundPool {
    pub(crate) fn try_new(pool: PgPool, holding_id: Uuid) -> Result<Self, PortError> {
        if holding_id.is_nil() {
            return Err(PortError::Denied);
        }
        Ok(Self { pool, holding_id })
    }

    pub(crate) const fn holding_id(&self) -> Uuid {
        self.holding_id
    }

    pub(crate) async fn begin(&self) -> Result<Transaction<'_, Postgres>, PortError> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| PortError::Persistence)?;
        bind_holding(&mut transaction, self.holding_id).await?;
        Ok(transaction)
    }
}

async fn bind_holding(
    transaction: &mut Transaction<'_, Postgres>,
    holding_id: Uuid,
) -> Result<(), PortError> {
    let expected = holding_id.to_string();
    let bound: String = sqlx::query_scalar("SELECT set_config('app.holding_id',$1,true)")
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
            .connect_lazy("postgres://erpx@127.0.0.1/erpx_agent_holding_contract")
            .expect("syntactically valid lazy pool")
    }

    #[tokio::test]
    async fn binding_requires_one_non_nil_immutable_holding() {
        assert!(matches!(
            HoldingBoundPool::try_new(lazy_pool(), Uuid::nil()),
            Err(PortError::Denied)
        ));
        let holding_id = Uuid::from_u128(41);
        let database = HoldingBoundPool::try_new(lazy_pool(), holding_id).expect("holding binding");
        assert_eq!(database.holding_id(), holding_id);
    }

    #[test]
    fn every_agent_adapter_uses_the_single_verified_binding_implementation() {
        let source = include_str!("holding.rs");
        assert_eq!(source.matches(concat!(".be", "gin()")).count(), 1);
        assert!(source.contains("SELECT set_config('app.holding_id',$1,true)"));
        assert!(source.contains("if bound != expected"));
        assert!(!source.contains(concat!("BYPASS", "RLS")));
    }
}
