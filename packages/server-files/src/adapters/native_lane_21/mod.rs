//! Native migration lane 21 for files.
//!
//! Owner-local successor of the four live `SELECT`s in
//! `adapters/commercial-usage-facts.ts` and the two in
//! `adapters/finance-tax-signatures.ts`. The Rust side owns the statements,
//! the projection and the response shape; the retained TypeScript modules are
//! strict transports that fail closed when these routes are unavailable.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{
    CommercialUsageRead, TaxSignatureRead, UsageReadFuture, handle, handle_tax,
};
pub use domain::{
    EarliestFacts, KIND_EARLIEST_ORIGINAL, KIND_RECONCILIATION, KIND_STORED_ORIGINALS,
    KIND_UNRECONCILED_ORIGINALS, ReconciliationFacts, StorageRow, TAX_METADATA, TAX_WORKSPACE,
    TaxSignatureFacts, TaxSignatureMetadata, TaxSignatureReadKind, TaxSignatureRequest,
    TaxSignatureResponse, TaxSignatureRow, UnreconciledFacts, UsageFacts, UsageReadKind,
    UsageReadRequest, UsageResponse, response_witness, tax_response_witness, valid_id, valid_slug,
    validate_request, validate_tax_request,
};
pub use http::{LaneTarget, router, router_for_holdings, router_with_reader};
pub use postgres::{
    EARLIEST_ORIGINAL_SQL, PgCommercialUsageRead, PgTaxSignatureRead, RECONCILIATION_SQL,
    STORED_ORIGINALS_SQL, TAX_METADATA_SQL, TAX_WORKSPACE_SQL, UNRECONCILED_ORIGINALS_SQL,
};
