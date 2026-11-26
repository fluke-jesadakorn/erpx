//! Portable Support ticket/incident event-history model. No SQL, HTTP, clock or
//! notification effect is reachable here. The projections mirror the retained
//! TypeScript `supportDetail`/`incidentDetail` history rows field for field,
//! including the immutable `evidence_json` value.
use erpx_server_shared::public::BusinessError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Already-authorized reader facts. Row visibility is enforced by owner SQL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupportEventHistoryScope {
    pub company_id: String,
    pub user_id: String,
    pub support_read: bool,
}

impl SupportEventHistoryScope {
    pub fn can_read(&self) -> bool {
        self.support_read
    }
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

/// Complete retained `support_ticket_events` row. Keys stay the exact snake_case
/// column names the TypeScript reader returned; jsonb stays a JSON value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TicketEventRow {
    pub id: String,
    pub ticket_id: String,
    pub version: i64,
    pub action: String,
    pub actor_id: String,
    pub reason: String,
    /// General reports may have no assigned owner or target date.
    pub owner_id: Option<String>,
    pub due_date: Option<String>,
    pub status: String,
    pub created_at: i64,
    pub evidence_json: Value,
}

/// Complete retained `support_incident_events` row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IncidentEventRow {
    pub id: String,
    pub incident_id: String,
    pub version: i64,
    pub action: String,
    pub actor_id: String,
    pub reason: String,
    pub created_at: i64,
    pub evidence_json: Value,
}

/// Project ticket event rows in the retained descending-version order. A row
/// with a blank identity or non-positive version is not a stored immutable
/// event and fails closed instead of reaching the response.
pub fn ticket_events(rows: Vec<TicketEventRow>) -> Result<Vec<TicketEventRow>, BusinessError> {
    let mut events = rows;
    for event in &events {
        if event.id.is_empty()
            || event.ticket_id.is_empty()
            || event.version < 1
            || event.action.is_empty()
        {
            return Err(invalid(
                "SUPPORT_TICKET_HISTORY_INVALID",
                "Provide a valid ticket event row.",
            ));
        }
    }
    events.sort_by(|a, b| b.version.cmp(&a.version).then_with(|| a.id.cmp(&b.id)));
    Ok(events)
}

/// Project incident event rows in the retained descending-version order.
pub fn incident_events(
    rows: Vec<IncidentEventRow>,
) -> Result<Vec<IncidentEventRow>, BusinessError> {
    let mut events = rows;
    for event in &events {
        if event.id.is_empty()
            || event.incident_id.is_empty()
            || event.version < 1
            || event.action.is_empty()
        {
            return Err(invalid(
                "SUPPORT_INCIDENT_HISTORY_INVALID",
                "Provide a valid incident event row.",
            ));
        }
    }
    events.sort_by(|a, b| b.version.cmp(&a.version).then_with(|| a.id.cmp(&b.id)));
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ticket() -> TicketEventRow {
        TicketEventRow {
            id: "support-ticket-1:1".into(),
            ticket_id: "support-ticket-1".into(),
            version: 1,
            action: "create".into(),
            actor_id: "creator".into(),
            reason: "Reported".into(),
            owner_id: Some("owner".into()),
            due_date: Some("2026-11-30".into()),
            status: "open".into(),
            created_at: 1_793_757_000_000,
            evidence_json: json!({"kind": "report"}),
        }
    }

    fn incident() -> IncidentEventRow {
        IncidentEventRow {
            id: "support-incident-1:1".into(),
            incident_id: "support-incident-1".into(),
            version: 1,
            action: "create".into(),
            actor_id: "creator".into(),
            reason: "Reported".into(),
            created_at: 1_793_757_000_000,
            evidence_json: json!({"kind": "outage"}),
        }
    }

    #[test]
    fn ticket_rows_keep_every_retained_field() {
        let events = ticket_events(vec![ticket()]).unwrap();
        let value = serde_json::to_value(&events).unwrap();
        assert_eq!(value[0]["id"], "support-ticket-1:1");
        assert_eq!(value[0]["ticket_id"], "support-ticket-1");
        assert_eq!(value[0]["version"], 1);
        assert_eq!(value[0]["action"], "create");
        assert_eq!(value[0]["actor_id"], "creator");
        assert_eq!(value[0]["reason"], "Reported");
        assert_eq!(value[0]["owner_id"], "owner");
        assert_eq!(value[0]["due_date"], "2026-11-30");
        assert_eq!(value[0]["status"], "open");
        assert_eq!(value[0]["created_at"], 1_793_757_000_000_i64);
        assert_eq!(value[0]["evidence_json"]["kind"], "report");
        assert_eq!(value[0].as_object().unwrap().len(), 11);
    }

    #[test]
    fn incident_rows_keep_every_retained_field() {
        let events = incident_events(vec![incident()]).unwrap();
        let value = serde_json::to_value(&events).unwrap();
        assert_eq!(value[0]["id"], "support-incident-1:1");
        assert_eq!(value[0]["incident_id"], "support-incident-1");
        assert_eq!(value[0]["version"], 1);
        assert_eq!(value[0]["action"], "create");
        assert_eq!(value[0]["actor_id"], "creator");
        assert_eq!(value[0]["reason"], "Reported");
        assert_eq!(value[0]["created_at"], 1_793_757_000_000_i64);
        assert_eq!(value[0]["evidence_json"]["kind"], "outage");
        assert_eq!(value[0].as_object().unwrap().len(), 8);
    }

    #[test]
    fn general_report_events_keep_null_owner_and_due_date() {
        let mut general = ticket();
        general.owner_id = None;
        general.due_date = None;
        let events = ticket_events(vec![general]).unwrap();
        let value = serde_json::to_value(&events).unwrap();
        assert!(value[0]["owner_id"].is_null());
        assert!(value[0]["due_date"].is_null());
        assert_eq!(value[0].as_object().unwrap().len(), 11);
    }

    #[test]
    fn history_is_newest_first_and_rejects_corrupt_rows() {
        let mut older = ticket();
        older.id = "support-ticket-1:2".into();
        older.version = 2;
        let events = ticket_events(vec![ticket(), older]).unwrap();
        assert_eq!(events[0].version, 2);
        assert_eq!(events[1].version, 1);

        let mut corrupt = incident();
        corrupt.version = 0;
        assert_eq!(
            incident_events(vec![corrupt]).unwrap_err().code,
            "SUPPORT_INCIDENT_HISTORY_INVALID"
        );
        let mut corrupt = ticket();
        corrupt.id.clear();
        assert_eq!(
            ticket_events(vec![corrupt]).unwrap_err().code,
            "SUPPORT_TICKET_HISTORY_INVALID"
        );
    }
}
