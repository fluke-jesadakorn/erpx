//! Owner-local native execution for the Asset register read family.
//!
//! A registered Holding target binds one actor authority source (composition
//! proof) and one owner reader. The router accepts only a bounded optional
//! Asset id; the reader executes the owner's SQL on the Holding pool. No SQL,
//! predicate or payload is accepted from the wire and no TypeScript reader is
//! consulted on this path.
pub(crate) mod application;
pub(crate) mod domain;
pub(crate) mod http;
pub(crate) mod postgres;
