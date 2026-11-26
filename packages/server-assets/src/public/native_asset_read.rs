//! Public surface for the SQL-executing native Asset register read lane.
//!
//! Composition supplies exact registered Holding targets, the Account actor
//! authority and one owner PostgreSQL reader per Holding. The router executes
//! the owner's Asset repository on that Holding pool and returns the complete
//! `{"data": ...}` payload; it never falls back to a TypeScript reader.
pub use crate::adapters::native_asset_read::{
    application::{
        AssetRead, AssetReadActorFuture, AssetReadActorRequest, AssetReadActorSource,
        AssetReadFuture,
    },
    http::{AssetReadQuery, NativeAssetReadTarget, native_asset_read_router},
    postgres::PgAssetRead,
};

use erpx_server_runtime::public::postgres::DatabasePool;
use std::sync::Arc;

/// Bind a Holding pool to the owner reader. The actor authority is still a
/// separate composition-owned target field; no header grants an Asset scope.
pub fn asset_read_for_pool(pool: DatabasePool) -> Arc<dyn AssetRead> {
    Arc::new(PgAssetRead::new(pool))
}
