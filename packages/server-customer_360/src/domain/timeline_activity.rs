//! Native Customer 360 timeline activity read facts.
//!
//! This is the read-only native replacement for the retained TypeScript
//! `customerTimelineEvents` projection. It reproduces the same federated
//! Customer 360 sources over the caller's serializable Holding transaction,
//! but returns validated rows instead of raw SQL. No source record is written;
//! every source owner remains authoritative. The query is bounded by a
//! one-year window and an optional selected customer so no unbounded activity
//! can leave the owner.
use crate::domain::timeline::CustomerTimelineItem;
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use serde::Serialize;

/// Hard ceiling on the activity rows one bounded read may return. A source
/// that exceeds it fails closed rather than streaming an unbounded federation.
pub const MAX_ACTIVITY_ROWS: usize = 5_000;
/// Bound on the optional free-text search carried by an activity read.
pub const MAX_QUERY_LEN: usize = 200;
const MAX_RANGE_MS: i64 = 366 * 24 * 60 * 60 * 1000;
const ID_LIMIT: usize = 300;
const NAME_LIMIT: usize = 300;

pub(crate) fn activity_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CUSTOMER_TIMELINE_ACTIVITY_INVALID",
        "The current customer timeline activity source is invalid.",
    )
}

fn query_invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_TIMELINE_ACTIVITY_QUERY_INVALID",
        "Use one bounded customer timeline activity request.",
    )
}

/// A bounded read over the federated activity projection. `from` is inclusive
/// and `to` is exclusive, matching the retained TypeScript metric window.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomerTimelineActivityQuery {
    pub from: i64,
    pub to: i64,
    pub customer_id: Option<String>,
    pub query: Option<String>,
}

impl CustomerTimelineActivityQuery {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.from < 0 || self.to <= self.from || self.to - self.from > MAX_RANGE_MS {
            return Err(query_invalid());
        }
        if let Some(customer_id) = &self.customer_id {
            if RecordId::new(customer_id.clone()).is_err() {
                return Err(query_invalid());
            }
        }
        if self
            .query
            .as_deref()
            .is_some_and(|value| value.len() > MAX_QUERY_LEN || value.chars().any(char::is_control))
        {
            return Err(query_invalid());
        }
        Ok(())
    }
}

/// One row of the federated customer activity projection. The first eight
/// fields are the public `CustomerTimelineItem`; `customer_id` and
/// `customer_name` are the owning party facts the projection joins.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CustomerTimelineActivityRow {
    pub id: String,
    pub source_id: String,
    pub source_type: String,
    pub title: String,
    pub status: String,
    pub version: i64,
    pub at: i64,
    pub href: String,
    pub customer_id: String,
    pub customer_name: String,
}

impl CustomerTimelineActivityRow {
    pub fn validate(&self) -> Result<(), BusinessError> {
        CustomerTimelineItem {
            id: self.id.clone(),
            source_id: self.source_id.clone(),
            source_type: self.source_type.clone(),
            title: self.title.clone(),
            status: self.status.clone(),
            version: self.version,
            at: self.at,
            href: self.href.clone(),
        }
        .validate()?;
        if RecordId::new(self.customer_id.clone()).is_err()
            || self.customer_name.is_empty()
            || self.customer_name.len() > NAME_LIMIT
            || self.customer_name.chars().any(char::is_control)
            || self.id.len() > ID_LIMIT
        {
            return Err(activity_invalid());
        }
        Ok(())
    }
}
