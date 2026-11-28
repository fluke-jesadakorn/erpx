//! Public surface for the native R3 Finance-owned Assets financial reads.
//!
//! Suggested composition mount (`erpx-finance-server`):
//! ```ignore
//! pub mod native_r3_assets_financial_read;
//! // in the private native API builder:
//! .nest_service("/api", erpx_finance_server::public::native_r3_assets_financial_read::router(targets)?)
//! ```
//! Each target carries its exact Host, Holding slug, Company and an
//! `Arc<dyn AssetsFinancialReadBackend>`; composition supplies the authority
//! port that owns session, role, purchase and command context decisions.
pub use crate::adapters::native_r3_assets_financial_read::{
    AssetsFinancialAuthority, AssetsFinancialTarget, PrerequisiteAuthority,
    Prerequisites, PgAssetsFinancialReadBackend, ReadRequest, Workspace,
    WorkspaceAuthority, router,
};
