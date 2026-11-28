//! Public Support notification projection port.
//!
//! Collaboration owns `staff_notifications`. These exports let composition and
//! native callers emit the eleven closed Support notification effects without
//! a TypeScript writer. Callers execute the returned INSERT in their own
//! serializable unit and supply only their own cross-owner projection SQL.

pub use crate::adapters::support_notification::{
    NotificationStatement, SupportNotificationEffect, SupportNotificationFacts,
    SupportNotificationRow, SupportSourceFragments,
};
