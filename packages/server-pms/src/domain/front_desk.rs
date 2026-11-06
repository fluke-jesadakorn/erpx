//! PMS front-desk queue read facts. These projections are presentation facts
//! only; they are not a current authorization proof, source purchase or command
//! capability. Cross-owner staff labels and package entitlement arrive through
//! public owner ports supplied by composition.
use super::room_board::{
    MAX_PAGE, MAX_RECORDS, PAGE_SIZE, PmsPropertyRow, PmsRecord, cp_len, ecma_trim, parse_date,
    validate_record,
};
use erpx_server_shared::public::{BusinessError, FailureKind};

/// The front desk reads stays in these queues/states only. Reservation states
/// stay on the reservations workflow and are not advertised here.
pub const FRONT_DESK_STATUSES: &[&str] = &[
    "",
    "arrivals",
    "departures",
    "reserved",
    "in_house",
    "checked_out",
    "cancelled",
    "no_show",
    "completed",
];

pub const FRONT_DESK_SECTION: &str = "front-desk";
pub const FRONT_DESK_KIND: &str = "stay";

#[derive(Clone, Debug, Default)]
pub struct PmsFrontDeskSelection {
    pub property_id: Option<String>,
    /// One of [`FRONT_DESK_STATUSES`].
    pub status: String,
    pub q: String,
    pub page: u32,
    pub selected_id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
}

impl PmsFrontDeskSelection {
    /// Retained Zod `q` uses `.trim().max(200)`; ids use `idField`
    /// (char length only, no trim).
    pub fn normalize(&mut self) {
        self.q = ecma_trim(&self.q).to_owned();
    }
    pub fn validate(&self) -> Result<(), BusinessError> {
        if !FRONT_DESK_STATUSES.contains(&self.status.as_str())
            || cp_len(ecma_trim(&self.q)) > 200
            || self.page > MAX_PAGE
            || self
                .property_id
                .as_ref()
                .is_some_and(|value| value.is_empty() || cp_len(value) > 200)
            || self
                .selected_id
                .as_ref()
                .is_some_and(|value| value.is_empty() || cp_len(value) > 200)
        {
            return Err(invalid());
        }
        match (&self.from, &self.to) {
            (None, None) => Ok(()),
            (Some(from), Some(to)) => {
                let from = parse_date(from).ok_or_else(invalid_range)?;
                let to = parse_date(to).ok_or_else(invalid_range)?;
                if from > to || to - from > 93 {
                    return Err(invalid_range());
                }
                Ok(())
            }
            _ => Err(invalid_range()),
        }
    }
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PMS_FRONT_DESK_REQUEST_INVALID",
        "Use one bounded front-desk selection.",
    )
}

fn invalid_range() -> BusinessError {
    BusinessError::invalid(
        "PMS_LIST_RANGE_INVALID",
        "Choose a list date range of at most 93 days.",
    )
}

pub fn read_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_FRONT_DESK_READ_INVALID",
        "The hotel front desk could not be read.",
    )
}

pub fn read_limit(message: &'static str) -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_FRONT_DESK_READ_LIMIT",
        message,
    )
}

#[derive(Clone, Debug)]
pub struct PmsFrontDeskData {
    pub property: PmsPropertyRow,
    /// Bounded page plus the related guest, reservation, guest-account and
    /// room facts the retained front-desk screen renders.
    pub records: Vec<PmsRecord>,
    pub ids: Vec<String>,
    pub has_next: bool,
    /// Optional PMS-owned issued documents for the visible guest accounts.
    pub documents: Vec<serde_json::Value>,
    pub from: String,
    pub to: String,
}

pub fn validate_front_desk_data(
    actor: &super::room_board::PmsBoardActor,
    data: PmsFrontDeskData,
) -> Result<PmsFrontDeskData, BusinessError> {
    if data.property.company_id != actor.company_id
        || actor
            .branch_id
            .as_ref()
            .is_some_and(|branch| branch != &data.property.branch_id)
        || data.records.len() > MAX_RECORDS
        || data.ids.len() > PAGE_SIZE
        || data.documents.len() > 200
        || data.from.len() != 10
        || data.to.len() != 10
        || data.from > data.to
    {
        return Err(read_invalid());
    }
    for record in &data.records {
        validate_record(record)?;
    }
    for document in &data.documents {
        let Some(document) = document.as_object() else {
            return Err(read_invalid());
        };
        if ["id", "guest_account_id", "kind"].iter().any(|key| {
            !document
                .get(*key)
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| !value.is_empty() && value.len() <= 200)
        }) {
            return Err(read_invalid());
        }
    }
    let visible: std::collections::BTreeSet<&str> =
        data.records.iter().map(|row| row.id.as_str()).collect();
    if data.ids.iter().any(|id| !visible.contains(id.as_str())) {
        return Err(read_invalid());
    }
    Ok(data)
}
