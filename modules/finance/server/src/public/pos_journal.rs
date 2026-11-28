//! Finance-owned POS journal posting on the caller's shared transaction.
pub use crate::adapters::pos_journal::{
    PosInvoiceRecord, PosJournalLine, PosJournalPosting, PosReceiptRecord, insert_pos_invoice,
    insert_pos_receipt, insert_pos_refund_payment, post_pos_journal,
};
