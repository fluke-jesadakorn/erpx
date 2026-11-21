//! Explicit Agent owner public facade. Consumers use these named exports; no
//! owner-internal path or SQL relation escapes the crate.
pub mod gateway;
pub mod governance;
pub mod providers;
pub mod quota;
pub mod quota_http;
pub mod owner_write;
pub mod owner_write_http;
