//! Portable Records drawer-detail (supplement) policy.
//!
//! The drawer detail read is still owned by Records: it selects one
//! Company/scope-limited `workspace_records` row, its visible outgoing links and
//! one bounded page of the transition timeline. Cross-owner source admission is
//! supplied by the composition-owned guard; this module never reads another
//! owner's SQL, opens a transaction or grants access.
use crate::domain::record_context::{
    HistoryRow, MAX_HISTORY, RecordContextActor, RecordView, validate_request,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, json};

pub const CONTRACT_VERSION: &str = "records.record-details.v1";
/// One drawer page. Reuses the retained transition-history bound so a detail
/// read can never ask the owner for an unbounded timeline scan.
pub const PAGE_SIZE: usize = MAX_HISTORY;
/// Hard upper bound for a caller-selected page. The application multiplies this
/// by a positive page size before it reaches SQL.
pub const MAX_PAGE: u32 = 100_000;

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, code, message)
}
pub fn page_invalid() -> BusinessError {
    invalid(
        "RECORD_DETAILS_PAGE_INVALID",
        "Choose a valid source record page.",
    )
}

/// Exact validated drawer-detail request. Fields stay private so callers cannot
/// bypass the module, record or page bounds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordDetailsQuery {
    module_id: String,
    record_id: String,
    page: u32,
}

impl RecordDetailsQuery {
    pub fn new(
        module_id: &str,
        record_id: &str,
        page: Option<&str>,
    ) -> Result<Self, BusinessError> {
        validate_request(module_id, record_id)?;
        let page = match page {
            None => 1,
            Some("") => return Err(page_invalid()),
            Some(value) => value
                .parse::<u32>()
                .ok()
                .filter(|page| (1..=MAX_PAGE).contains(page))
                .ok_or_else(page_invalid)?,
        };
        Ok(Self {
            module_id: module_id.to_owned(),
            record_id: record_id.to_owned(),
            page,
        })
    }

    pub fn module_id(&self) -> &str {
        &self.module_id
    }

    pub fn record_id(&self) -> &str {
        &self.record_id
    }

    pub fn page(&self) -> u32 {
        self.page
    }

    /// Exclusive SQL offset for the selected page. Page 1 reads from the start.
    pub fn offset(&self) -> i64 {
        i64::from(self.page.saturating_sub(1)) * PAGE_SIZE as i64
    }

    /// One row over the page bound detects a following page without a count.
    pub fn fetch_limit(&self) -> usize {
        PAGE_SIZE + 1
    }
}

/// Validate a bounded history slice and keep only the current page.
///
/// The adapter fetches `PAGE_SIZE + 1` rows; the extra row is the `hasNext`
/// witness and is never returned to the browser.
pub fn history_page(mut rows: Vec<HistoryRow>) -> Result<(Vec<HistoryRow>, bool), BusinessError> {
    if rows.len() > PAGE_SIZE + 1 {
        return Err(crate::domain::record_context::source_invalid());
    }
    let has_next = rows.len() > PAGE_SIZE;
    rows.truncate(PAGE_SIZE);
    let rows = crate::domain::record_context::validate_history(rows)?;
    Ok((rows, has_next))
}

fn related_wire(target_module: &str, target_id: &str) -> Value {
    json!({"kind": target_module, "id": target_id})
}

fn history_wire(row: &HistoryRow) -> Value {
    json!({
        "from_status": row.from_status,
        "to_status": row.to_status,
        "action": row.action,
        "note": row.note,
        "actor_id": row.actor_id,
        "record_version": row.record_version,
        "created_at": row.created_at,
    })
}

/// Exact drawer supplement for one Records-owned workspace record.
///
/// `kind` is the stable drawer label owned by `feature-records`; `row` is the
/// same validated record view the context read returns, so the drawer renders
/// the owner row instead of a second transport shape.
pub fn details_wire(
    record: RecordView,
    related: Vec<(String, String)>,
    history: Vec<HistoryRow>,
    page: u32,
    has_next: bool,
) -> Value {
    json!({
        "kind": "workspace_record",
        "row": record.to_wire(),
        "related": related
            .iter()
            .map(|(module_id, id)| related_wire(module_id, id))
            .collect::<Vec<_>>(),
        "history": history.iter().map(history_wire).collect::<Vec<_>>(),
        "page": page,
        "hasNext": has_next,
    })
}

/// Current render facts are policy inputs only; a detail read never grants.
pub fn require_current(
    actor: &RecordContextActor,
    now: i64,
    module_id: &str,
) -> Result<(), BusinessError> {
    actor.require_read(now)?;
    if !actor.permits_module(module_id) {
        return Err(crate::domain::record_context::module_forbidden());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history(created_at: i64) -> HistoryRow {
        HistoryRow {
            from_status: "draft".into(),
            to_status: "in_review".into(),
            action: "submit".into(),
            note: String::new(),
            actor_id: "maker".into(),
            record_version: 1,
            created_at,
        }
    }

    #[test]
    fn page_bounds_are_strict() {
        assert_eq!(
            RecordDetailsQuery::new("operations", "rec-1", None)
                .unwrap()
                .page(),
            1
        );
        assert_eq!(
            RecordDetailsQuery::new("operations", "rec-1", Some("3"))
                .unwrap()
                .offset(),
            (2 * PAGE_SIZE) as i64
        );
        assert!(RecordDetailsQuery::new("operations", "rec-1", Some("")).is_err());
        assert!(RecordDetailsQuery::new("operations", "rec-1", Some("0")).is_err());
        assert!(RecordDetailsQuery::new("operations", "rec-1", Some("-1")).is_err());
        assert!(RecordDetailsQuery::new("operations", "rec-1", Some("100001")).is_err());
        assert!(RecordDetailsQuery::new("Operations", "rec-1", None).is_err());
        assert!(RecordDetailsQuery::new("operations", "../rec", None).is_err());
    }

    #[test]
    fn history_page_keeps_the_next_page_witness_out_of_the_payload() {
        let (rows, next) = history_page((0..PAGE_SIZE as i64).map(history).collect()).unwrap();
        assert_eq!(rows.len(), PAGE_SIZE);
        assert!(!next);
        let (rows, next) = history_page((0..=PAGE_SIZE as i64).map(history).collect()).unwrap();
        assert_eq!(rows.len(), PAGE_SIZE);
        assert!(next);
        assert!(history_page((0..=(PAGE_SIZE + 1) as i64).map(history).collect()).is_err());
    }
}
