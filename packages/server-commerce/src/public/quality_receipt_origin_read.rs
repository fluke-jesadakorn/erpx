//! Native receipt-origin read; transport only, no authority grant.
pub use crate::adapters::quality_receipt_origin_read::{
    PgQualityReceiptOriginRead, QualityReceiptOriginFacts, QualityReceiptOriginRead,
    QualityReceiptOriginReadFuture, QualityReceiptOriginReadTarget,
    quality_receipt_origin_read_router,
};
