//! Raw receipt source for Quality return lineage; this grants no authority.
pub use crate::adapters::quality_return_receipts::{
    CurrentQualityReturnReceipts, QualityReturnReceipt, QualityReturnReceiptScope,
    inspect_quality_return_receipts, lock_quality_return_receipts,
};

pub use crate::adapters::quality_return_receipts::{
    CurrentQualityScrapRequestReceipt, QualityScrapRequestReceiptScope,
    inspect_quality_scrap_request_receipt, lock_quality_scrap_request_receipts,
};
