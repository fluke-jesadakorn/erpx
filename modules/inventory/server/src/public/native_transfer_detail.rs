//! Stable public export for the native stock-transfer detail owner slice.
//! Composition mounts `router_for_holdings`; no TypeScript fallback is exposed.

pub use crate::adapters::native_transfer_detail::{
    not_found, read_transfer_detail, router, router_for_holdings, statement,
    InventoryTransferDetailRead, PgInventoryTransferDetailRead, TransferDetailFuture,
    TransferDetailRequest, TransferDetailResponse, TransferDetailTarget, OWNER_SQL, READ_PATH,
    VALUE_COUNT,
};
