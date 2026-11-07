//! Quality receipt-hold owner effect. It grants no actor or purchase authority.
pub use crate::adapters::goods_receipt_hold::{
    GoodsReceiptHoldFacts, GoodsReceiptHoldSource, PreparedGoodsReceiptHold,
    RetainedGoodsReceiptHold, lock_goods_receipt_hold_sources, prepare_goods_receipt_hold,
};
