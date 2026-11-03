//! Finite Field invoice routing, with current participants supplied by owners.
//! These rendering decisions grant no Finance write permission.
use serde_json::{Value, json};
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
        steps.push(json!({"key":keys[index],"action":actions[index],"role":role_names[index],"allowedRoles":if index==0{vec!["finance_approver","holding_owner"]}else{vec!["treasury"]},"teamName":primary.and_then(|p|p["teamName"].as_str()).map(str::to_owned).unwrap_or_else(||role_names[index].replace('_'," ")),"primary":primary,"candidates":pool,"status":status,"blocker":blocker}));
    }
    let index = steps.iter().position(|s| s["status"] == "current");
    let step = index.map(|i| &steps[i]);
    let blocker = if terminal {
        Value::Null
    } else if let Some(reason) = record["blocked_reason"].as_str().filter(|s| !s.is_empty()) {
        json!(reason)
    } else {
        step.map(|s| s["blocker"].clone())
            .unwrap_or_else(|| json!("This document requires routing repair before proceeding."))
    };
    let can_act = blocker.is_null()
        && index.is_some_and(|i| roles[i] && steps[i]["primary"]["userId"].as_str() == Some(actor));
    json!({"kind":"invoice","recordId":record["id"],"recordVersion":record["version"],"version":version,"defaultsRevision":revision,"state":state,"steps":steps,"current":step,"next":index.and_then(|i|steps.get(i+1)),"canAct":can_act,"canManage":can_manage,"blocker":blocker,"history":history})
}
