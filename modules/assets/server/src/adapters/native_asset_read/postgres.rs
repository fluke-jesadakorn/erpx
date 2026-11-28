//! PostgreSQL execution for the native Asset register read lane.
//!
//! This is the only layer that touches sqlx. It acquires one Holding unit and
//! runs the existing owner read handlers on the physical Asset repository, so
//! the row mapping, scoped predicates, bounded limits and Finance visibility
//! are the owner's authoritative ones rather than a new parallel projection.
use super::{
    application::{AssetRead, AssetReadFuture},
    domain,
};
use crate::public::asset_register::{
    AssetActor, AssetDetail, AssetWorkspace, PgAssetRegister, asset_detail, asset_workspace,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::BusinessError;

fn to_business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => domain::unavailable(),
    }
}

/// Owner reader bound to a Holding pool. The actor comes from composition's
/// proof; no caller-controlled scope reaches this adapter.
pub struct PgAssetRead {
    pool: DatabasePool,
}

impl PgAssetRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl AssetRead for PgAssetRead {
    fn workspace<'a>(&'a self, actor: AssetActor) -> AssetReadFuture<'a, AssetWorkspace> {
        Box::pin(async move {
            let unit = PgUnitOfWork::new(self.pool.clone());
            unit.run::<AssetWorkspace, BusinessError, _>(
                TransactionOptions::default(),
                move |connection| {
                    let actor = actor.clone();
                    Box::pin(async move {
                        let mut repository = PgAssetRegister::new(connection, actor.clone());
                        asset_workspace(&actor, &mut repository).await
                    })
                },
            )
            .await
            .map_err(to_business)
        })
    }

    fn detail<'a>(&'a self, actor: AssetActor, id: &'a str) -> AssetReadFuture<'a, AssetDetail> {
        let id = id.to_owned();
        Box::pin(async move {
            let unit = PgUnitOfWork::new(self.pool.clone());
            unit.run::<AssetDetail, BusinessError, _>(
                TransactionOptions::default(),
                move |connection| {
                    let actor = actor.clone();
                    let id = id.clone();
                    Box::pin(async move {
                        let mut repository = PgAssetRegister::new(connection, actor.clone());
                        asset_detail(&actor, &mut repository, &id).await
                    })
                },
            )
            .await
            .map_err(to_business)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistence_failure_fails_closed_without_a_second_reader() {
        let error = to_business(PgTransactionError::Persistence(
            erpx_platform_runtime_server::public::postgres::PersistenceError::new("PROBE"),
        ));
        assert_eq!(error.code, "ASSETS_NATIVE_READ_UNAVAILABLE");
        let business = to_business(PgTransactionError::Business(BusinessError::invalid(
            "ASSET_NOT_FOUND",
            "not found",
        )));
        assert_eq!(business.code, "ASSET_NOT_FOUND");
    }
}
