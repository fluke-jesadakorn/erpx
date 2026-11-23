//! Public owner-read registry facade. The command-plan route on the mounted
//! customer-access router resolves these kinds; the TS bridge sends only the
//! kind and its positional values.
pub use crate::adapters::owner_read_sql::{
    NativeReadStatement, RD_DUPLICATE_REVIEW_HISTORY, RD_IDENTITY_CHANGE_HISTORY,
    RD_SALES_REVIEW_COUNTS, is_read, read_statement,
};
