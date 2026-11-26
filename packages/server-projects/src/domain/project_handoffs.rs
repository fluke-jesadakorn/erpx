//! Pure Projects handoff-view facts and projection.
//!
//! The retained operation is the read-side projection of a stored handoff
//! snapshot. A snapshot was written by the native structure command as a
//! complete project graph; its recipient review applies the normal time/cost
//! visibility policy when the snapshot is read. This module owns that policy:
//! it filters snapshot actuals, recomputes per-package minutes/cost and the
//! graph totals, and shapes the complete `ProjectHandoffView` payload. No SQL,
//! HTTP or I/O appears here.

use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

/// The owner's stored handoff row. Field names and nullability match the
/// Projects-owned `staff_project_handoffs` row selected by the native reader.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HandoffRow {
    pub id: String,
    pub company_id: String,
    pub project_id: String,
    pub source_project_version: i64,
    pub recipient_id: String,
    pub status: String,
    pub version: i64,
    pub snapshot_json: String,
    pub snapshot_sha256: String,
    pub evidence: String,
    pub prepared_by: String,
    pub prepared_at: i64,
    pub decided_by: Option<String>,
    pub decided_at: Option<i64>,
    pub decision_reason: Option<String>,
}

/// Current actor facts needed by the handoff view policy. Composition supplies
/// the purchase-qualified cost-history capability and the same IAM
/// person-equivalence predicate used by the retained read policy.
pub struct HandoffViewer {
    pub is_owner: bool,
    pub can_read_costs: bool,
    pub same_person: Arc<dyn Fn(&str) -> bool + Send + Sync>,
}

pub fn snapshot_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROJECT_HANDOFF_SNAPSHOT_INVALID",
        "The retained handoff snapshot could not be projected.",
    )
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, BusinessError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(snapshot_invalid)
}

fn integer(value: &Value, key: &str) -> Result<i64, BusinessError> {
    value
        .get(key)
        .and_then(Value::as_i64)
        .ok_or_else(snapshot_invalid)
}

/// The retained `sourceCurrent` rule: the bound source is still its own current
/// version, independently approved, and either a work log or a posted journal.
fn source_current(source: &Value) -> Result<bool, BusinessError> {
    let kind = text(source, "source_kind")?;
    let state = text(source, "source_state")?;
    let posted = match source.get("journal_status") {
        Some(Value::String(status)) => status == "posted",
        Some(Value::Null) | None => false,
        _ => return Err(snapshot_invalid()),
    };
    Ok(integer(source, "source_version")? == integer(source, "current_source_version")?
        && state == "approved"
        && (kind == "work_log" || posted))
}

fn add_cost(
    costs: &mut BTreeMap<String, i128>,
    currency: &str,
    amount: i128,
) -> Result<(), BusinessError> {
    let entry = costs.entry(currency.to_owned()).or_insert(0);
    *entry = entry.checked_add(amount).ok_or_else(snapshot_invalid)?;
    Ok(())
}

fn cost_strings(costs: &BTreeMap<String, i128>) -> Value {
    let strings: BTreeMap<&str, String> = costs
        .iter()
        .map(|(currency, amount)| (currency.as_str(), amount.to_string()))
        .collect();
    serde_json::to_value(strings).unwrap_or(Value::Null)
}

/// Project one stored handoff row into the complete owner view. Every field the
/// retained TypeScript read returned is present, with `snapshot_json` replaced
/// by the visibility-projected `snapshot`.
pub fn project_handoff_view(
    row: &HandoffRow,
    snapshot: Value,
    viewer: &HandoffViewer,
) -> Result<Value, BusinessError> {
    let mut graph = snapshot.as_object().cloned().ok_or_else(snapshot_invalid)?;
    let actuals = graph
        .get("actuals")
        .and_then(Value::as_array)
        .ok_or_else(snapshot_invalid)?
        .clone();
    let packages_in = graph
        .get("packages")
        .and_then(Value::as_array)
        .ok_or_else(snapshot_invalid)?
        .clone();

    // Cost sources require the purchase-qualified cost capability; work-log
    // sources are visible to the project owner or to the actor named on the
    // source. This is the retained handoff review policy, not a new rule.
    let visible: Vec<Value> = actuals
        .into_iter()
        .filter(|source| {
            if text(source, "source_kind") == Ok("cost_allocation") {
                viewer.can_read_costs
            } else {
                let user = source
                    .get("user_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                viewer.is_owner || (viewer.same_person)(user)
            }
        })
        .collect();

    let mut packages = Vec::with_capacity(packages_in.len());
    let mut total_minutes = 0_i64;
    let mut total_costs: BTreeMap<String, i128> = BTreeMap::new();
    for package in packages_in {
        let mut projected = package.as_object().cloned().ok_or_else(snapshot_invalid)?;
        let id = text(&package, "id")?.to_owned();
        let mut minutes = 0_i64;
        let mut costs: BTreeMap<String, i128> = BTreeMap::new();
        for source in visible
            .iter()
            .filter(|row| source_matches(row, &id) && source_current(row).unwrap_or(false))
        {
            match text(source, "source_kind")? {
                "work_log" => {
                    minutes = minutes
                        .checked_add(integer(source, "minutes")?)
                        .ok_or_else(snapshot_invalid)?;
                }
                "cost_allocation" => {
                    let currency = text(source, "currency")?.to_owned();
                    let amount = i128::from(integer(source, "amount_minor")?);
                    add_cost(&mut costs, &currency, amount)?;
                }
                _ => return Err(snapshot_invalid()),
            }
        }
        total_minutes = total_minutes
            .checked_add(minutes)
            .ok_or_else(snapshot_invalid)?;
        for (currency, amount) in &costs {
            add_cost(&mut total_costs, currency, *amount)?;
        }
        projected.insert("approvedMinutes".into(), serde_json::json!(minutes));
        projected.insert("costByCurrency".into(), cost_strings(&costs));
        packages.push(Value::Object(projected));
    }

    let mut graph_costs: BTreeMap<String, i128> = BTreeMap::new();
    for source in visible
        .iter()
        .filter(|row| text(row, "source_kind") == Ok("cost_allocation") && source_current(row).unwrap_or(false))
    {
        let currency = text(source, "currency")?.to_owned();
        add_cost(
            &mut graph_costs,
            &currency,
            i128::from(integer(source, "amount_minor")?),
        )?;
    }

    graph.insert("actuals".into(), Value::Array(visible));
    graph.insert("packages".into(), Value::Array(packages));
    graph.insert("canReadCosts".into(), Value::Bool(viewer.can_read_costs));
    graph.insert("isProjectOwner".into(), Value::Bool(viewer.is_owner));
    graph.insert(
        "totals".into(),
        serde_json::json!({
            "approvedMinutes": total_minutes,
            "costByCurrency": cost_strings(&graph_costs),
        }),
    );

    let mut view = Map::new();
    view.insert("id".into(), Value::String(row.id.clone()));
    view.insert("company_id".into(), Value::String(row.company_id.clone()));
    view.insert("project_id".into(), Value::String(row.project_id.clone()));
    view.insert(
        "source_project_version".into(),
        serde_json::json!(row.source_project_version),
    );
    view.insert("recipient_id".into(), Value::String(row.recipient_id.clone()));
    view.insert("status".into(), Value::String(row.status.clone()));
    view.insert("version".into(), serde_json::json!(row.version));
    view.insert("snapshot_sha256".into(), Value::String(row.snapshot_sha256.clone()));
    view.insert("evidence".into(), Value::String(row.evidence.clone()));
    view.insert("prepared_by".into(), Value::String(row.prepared_by.clone()));
    view.insert("prepared_at".into(), serde_json::json!(row.prepared_at));
    view.insert(
        "decided_by".into(),
        row.decided_by
            .as_ref()
            .map_or(Value::Null, |value| Value::String(value.clone())),
    );
    view.insert(
        "decided_at".into(),
        row.decided_at
            .map_or(Value::Null, |value| serde_json::json!(value)),
    );
    view.insert(
        "decision_reason".into(),
        row.decision_reason
            .as_ref()
            .map_or(Value::Null, |value| Value::String(value.clone())),
    );
    view.insert("snapshot".into(), Value::Object(graph));
    Ok(Value::Object(view))
}

fn source_matches(source: &Value, milestone_id: &str) -> bool {
    source
        .get("milestone_id")
        .and_then(Value::as_str)
        == Some(milestone_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> HandoffRow {
        HandoffRow {
            id: "handoff".into(),
            company_id: "company".into(),
            project_id: "project".into(),
            source_project_version: 4,
            recipient_id: "member".into(),
            status: "pending".into(),
            version: 2,
            snapshot_json: "{}".into(),
            snapshot_sha256: "a".repeat(64),
            evidence: "All work is complete.".into(),
            prepared_by: "owner".into(),
            prepared_at: 100,
            decided_by: None,
            decided_at: None,
            decision_reason: None,
        }
    }

    fn viewer(is_owner: bool, can_read_costs: bool) -> HandoffViewer {
        HandoffViewer {
            is_owner,
            can_read_costs,
            same_person: Arc::new(|value| value == "member"),
        }
    }

    fn snapshot() -> Value {
        serde_json::json!({
            "project": {"id": "project"},
            "packages": [{
                "id": "m-root",
                "approvedMinutes": 999,
                "costByCurrency": {},
                "tasks": []
            }],
            "tasks": [],
            "actuals": [
                {
                    "id": "time", "milestone_id": "m-root", "source_kind": "work_log",
                    "source_id": "log", "source_version": 1, "minutes": 60,
                    "amount_minor": null, "currency": null, "user_id": "member",
                    "current_source_version": 1, "source_state": "approved",
                    "journal_status": null, "journal_id": null
                },
                {
                    "id": "cost", "milestone_id": "m-root", "source_kind": "cost_allocation",
                    "source_id": "allocation", "source_version": 1, "minutes": null,
                    "amount_minor": 1000, "currency": "USD", "user_id": null,
                    "current_source_version": 1, "source_state": "approved",
                    "journal_status": "posted", "journal_id": "journal"
                },
                {
                    "id": "stale", "milestone_id": "m-root", "source_kind": "work_log",
                    "source_id": "old", "source_version": 1, "minutes": 30,
                    "amount_minor": null, "currency": null, "user_id": "member",
                    "current_source_version": 2, "source_state": "approved",
                    "journal_status": null, "journal_id": null
                }
            ],
            "edges": [],
            "unassignedTasks": [],
            "canReadCosts": true,
            "isProjectOwner": true,
            "totals": {"approvedMinutes": 999, "costByCurrency": {}}
        })
    }

    #[test]
    fn view_omits_snapshot_json_and_keeps_every_row_field() {
        let view = project_handoff_view(&row(), snapshot(), &viewer(true, true)).unwrap();
        let object = view.as_object().unwrap();
        assert!(!object.contains_key("snapshot_json"));
        for key in [
            "id", "company_id", "project_id", "source_project_version", "recipient_id",
            "status", "version", "snapshot_sha256", "evidence", "prepared_by", "prepared_at",
            "decided_by", "decided_at", "decision_reason", "snapshot",
        ] {
            assert!(object.contains_key(key), "missing {key}");
        }
        assert_eq!(object.len(), 15);
    }

    #[test]
    fn owner_with_cost_access_sees_current_sources_and_recomputed_totals() {
        let view = project_handoff_view(&row(), snapshot(), &viewer(true, true)).unwrap();
        let snapshot = view.get("snapshot").unwrap();
        assert_eq!(snapshot.get("actuals").unwrap().as_array().unwrap().len(), 2);
        let package = &snapshot.get("packages").unwrap()[0];
        assert_eq!(package.get("approvedMinutes").unwrap(), 60);
        assert_eq!(package.get("costByCurrency").unwrap().get("USD").unwrap(), "1000");
        let totals = snapshot.get("totals").unwrap();
        assert_eq!(totals.get("approvedMinutes").unwrap(), 60);
        assert_eq!(totals.get("costByCurrency").unwrap().get("USD").unwrap(), "1000");
        assert_eq!(snapshot.get("canReadCosts").unwrap(), true);
        assert_eq!(snapshot.get("isProjectOwner").unwrap(), true);
    }

    #[test]
    fn member_without_cost_access_sees_only_own_work() {
        let view = project_handoff_view(&row(), snapshot(), &viewer(false, false)).unwrap();
        let snapshot = view.get("snapshot").unwrap();
        let actuals = snapshot.get("actuals").unwrap().as_array().unwrap();
        assert_eq!(actuals.len(), 1);
        assert_eq!(actuals[0].get("id").unwrap(), "time");
        let package = &snapshot.get("packages").unwrap()[0];
        assert_eq!(package.get("approvedMinutes").unwrap(), 60);
        assert_eq!(package.get("costByCurrency").unwrap().as_object().unwrap().len(), 0);
        assert_eq!(snapshot.get("canReadCosts").unwrap(), false);
        assert_eq!(snapshot.get("isProjectOwner").unwrap(), false);
    }

    #[test]
    fn malformed_snapshot_is_rejected() {
        assert_eq!(
            project_handoff_view(&row(), serde_json::json!({"actuals": 1}), &viewer(true, true))
                .unwrap_err()
                .code,
            "PROJECT_HANDOFF_SNAPSHOT_INVALID"
        );
    }
}
