//! Finance-owned POS journal posting on the caller's shared transaction.
pub use crate::adapters::pos_journal::{
    PosJournalLine, PosJournalPosting, post_pos_journal,
};
