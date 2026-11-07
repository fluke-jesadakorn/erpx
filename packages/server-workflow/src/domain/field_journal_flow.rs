//! Pure Field journal routing presentation. No authority or database access.
use serde_json::Value;
fn string(value: &str) -> Value { Value::String(value.to_owned()) }
// The renderer is the fixed public contract for the invoice/journal Flow.
#[allow(clippy::too_many_arguments)]
pub fn field_journal_flow_view(
    record:&Value,version:i64,revision:i64,history:&[Value],
    candidates:Vec<Value>,selected:Option<String>,actor:&str,
    actor_can_review:bool,can_manage:bool,
)->Value{
    let state=record["status"].as_str().unwrap_or("");
    let current=if state=="in_review"{"draft"}else{state};
    let terminal=matches!(state,"posted"|"reversed"|"voided"|"cancelled");
    let chosen=selected.or_else(||(candidates.len()==1).then(||candidates[0]["userId"].as_str().unwrap_or("").to_owned()));
    let primary=chosen.as_ref().and_then(|id|candidates.iter().find(|p|p["userId"].as_str()==Some(id)));
    let blocker=if primary.is_some(){Value::Null}
        else if chosen.is_some(){string("The primary staff member is no longer eligible. Reassign with a reason.")}
        else if !candidates.is_empty(){string("Select a primary staff member from the eligible team.")}
        else{string("No eligible staff member. Configure active staff and role coverage.")};
    let status=if !terminal&&current=="draft"{"current"}
        else if history.iter().any(|e|e["step"]=="draft"&&!matches!(e["action"].as_str(),Some("assign"|"configure"))){"recorded"}else{"upcoming"};
    let mut step=serde_json::Map::new();
    step.insert("key".into(),string("draft"));
    step.insert("action".into(),string("Review Debit / Credit and post GL"));
    step.insert("role".into(),string("finance_approver"));
    step.insert("allowedRoles".into(),Value::Array(vec![string("finance_approver"),string("holding_owner")]));
    step.insert("teamName".into(),string(primary.and_then(|p|p["teamName"].as_str()).unwrap_or("finance approver")));
    step.insert("primary".into(),primary.cloned().unwrap_or(Value::Null));
    step.insert("candidates".into(),Value::Array(candidates));
    step.insert("status".into(),string(status));
    step.insert("blocker".into(),blocker.clone());
    let step=Value::Object(step);
    let blocker=if terminal{Value::Null}else if let Some(reason)=record["blocked_reason"].as_str().filter(|s|!s.is_empty()){string(reason)}else if status=="current"{step["blocker"].clone()}else{string("This document requires routing repair before proceeding.")};
    let can_act=blocker.is_null()&&status=="current"&&actor_can_review&&step["primary"]["userId"].as_str()==Some(actor);
    let mut view=serde_json::Map::new();
    view.insert("kind".into(),string("journal"));
    view.insert("recordId".into(),record["id"].clone());
    view.insert("recordVersion".into(),record["version"].clone());
    view.insert("version".into(),Value::from(version));
    view.insert("defaultsRevision".into(),Value::from(revision));
    view.insert("state".into(),string(state));
    view.insert("steps".into(),Value::Array(vec![step.clone()]));
    view.insert("current".into(),if status=="current"{step}else{Value::Null});
    view.insert("next".into(),Value::Null);
    view.insert("canAct".into(),Value::Bool(can_act));
    view.insert("canManage".into(),Value::Bool(can_manage));
    view.insert("blocker".into(),blocker);
    view.insert("history".into(),Value::Array(history.to_vec()));
    Value::Object(view)
}
