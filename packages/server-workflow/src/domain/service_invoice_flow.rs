//! Finite Field invoice routing, with current participants supplied by owners.
//! These rendering decisions grant no Finance write permission.
use serde_json::Value;
fn string(value: &str) -> Value {
    Value::String(value.to_owned())
}
// The renderer is the fixed public contract for the invoice/journal Flow.
#[allow(clippy::too_many_arguments)]
pub fn service_invoice_flow_view(
    record: &Value,
    version: i64,
    revision: i64,
    history: &[Value],
    candidates: [Vec<Value>; 2],
    selected: [Option<String>; 2],
    actor: &str,
    roles: [bool; 2],
    can_manage: bool,
) -> Value {
    let state = record["status"].as_str().unwrap_or("");
    let current = match state {
        "in_review" => "draft",
        "partially_paid" => "posted",
        s => s,
    };
    let terminal = matches!(
        state,
        "paid"
            | "settled"
            | "voided"
            | "no_charge_confirmed"
            | "cancelled"
            | "completed"
            | "closed"
            | "reimbursed"
            | "reversed"
            | "void"
    ) || (current == "posted" && record["outstanding_minor"].as_i64() == Some(0))
        || !record["cancelled_at"].is_null();
    let keys = ["draft", "posted"];
    let actions = [
        "Review invoice Debit / Credit and post GL",
        "Record receipt and settle receivable",
    ];
    let role_names = ["finance_approver", "treasury"];
    let mut steps = Vec::new();
    for index in 0..2 {
        let pool = &candidates[index];
        let chosen = selected[index]
            .clone()
            .or_else(|| (pool.len() == 1).then(|| pool[0]["userId"].as_str().unwrap_or("").into()));
        let primary = chosen
            .as_ref()
            .and_then(|id| pool.iter().find(|p| p["userId"].as_str() == Some(id)));
        let blocker = if primary.is_some() {
            None
        } else if chosen.is_some() {
            Some("The primary staff member is no longer eligible. Reassign with a reason.")
        } else if !pool.is_empty() {
            Some("Select a primary staff member from the eligible team.")
        } else {
            Some("No eligible staff member. Configure active staff and role coverage.")
        };
        let status = if !terminal && current == keys[index] {
            "current"
        } else if history.iter().any(|e| {
            e["step"] == keys[index]
                && !matches!(e["action"].as_str(), Some("assign" | "configure"))
        }) {
            "recorded"
        } else {
            "upcoming"
        };
        let team_name = primary
            .and_then(|p| p["teamName"].as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| role_names[index].replace('_', " "));
        let mut step = serde_json::Map::new();
        step.insert("key".into(), string(keys[index]));
        step.insert("action".into(), string(actions[index]));
        step.insert("role".into(), string(role_names[index]));
        step.insert(
            "allowedRoles".into(),
            Value::Array(if index == 0 {
                vec![string("finance_approver"), string("holding_owner")]
            } else {
                vec![string("treasury")]
            }),
        );
        step.insert("teamName".into(), Value::String(team_name));
        step.insert("primary".into(), primary.cloned().unwrap_or(Value::Null));
        step.insert("candidates".into(), Value::Array(pool.clone()));
        step.insert("status".into(), string(status));
        step.insert("blocker".into(), blocker.map_or(Value::Null, string));
        steps.push(Value::Object(step));
    }
    let index = steps.iter().position(|s| s["status"] == "current");
    let step = index.map(|i| &steps[i]);
    let blocker = if terminal {
        Value::Null
    } else if let Some(reason) = record["blocked_reason"].as_str().filter(|s| !s.is_empty()) {
        string(reason)
    } else {
        step.map(|s| s["blocker"].clone())
            .unwrap_or_else(|| string("This document requires routing repair before proceeding."))
    };
    let can_act = blocker.is_null()
        && index.is_some_and(|i| roles[i] && steps[i]["primary"]["userId"].as_str() == Some(actor));
    let next = index
        .and_then(|i| steps.get(i + 1))
        .cloned()
        .unwrap_or(Value::Null);
    let current = step.cloned().unwrap_or(Value::Null);
    let mut view = serde_json::Map::new();
    view.insert("kind".into(), string("invoice"));
    view.insert("recordId".into(), record["id"].clone());
    view.insert("recordVersion".into(), record["version"].clone());
    view.insert("version".into(), Value::from(version));
    view.insert("defaultsRevision".into(), Value::from(revision));
    view.insert("state".into(), string(state));
    view.insert("steps".into(), Value::Array(steps));
    view.insert("current".into(), current);
    view.insert("next".into(), next);
    view.insert("canAct".into(), Value::Bool(can_act));
    view.insert("canManage".into(), Value::Bool(can_manage));
    view.insert("blocker".into(), blocker);
    view.insert("history".into(), Value::Array(history.to_vec()));
    Value::Object(view)
}
