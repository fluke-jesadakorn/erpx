//! Narrow background notification session checks. These do not authenticate a
//! browser or authorize business data; the opened Inbox must authorize its source.
pub use crate::adapters::push_session::{current_account, current_company};
