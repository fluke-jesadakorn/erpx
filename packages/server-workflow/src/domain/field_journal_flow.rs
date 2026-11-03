//! Pure Field journal routing presentation. No authority or database access.
use serde_json::{Value,json};
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
        else if chosen.is_some(){json!("The primary staff member is no longer eligible. Reassign with a reason.")}
        else if !candidates.is_empty(){json!("Select a primary staff member from the eligible team.")}
        else{json!("No eligible staff member. Configure active staff and role coverage.")};
    let status=if !terminal&&current=="draft"{"current"}
        else if history.iter().any(|e|e["step"]=="draft"&&!matches!(e["action"].as_str(),Some("assign"|"configure"))){"recorded"}else{"upcoming"};
    let step=json!({"key":"draft","action":"Review Debit / Credit and post GL","role":"finance_approver","allowedRoles":["finance_approver","holding_owner"],"teamName":primary.and_then(|p|p["teamName"].as_str()).unwrap_or("finance approver"),"primary":primary,"candidates":candidates,"status":status,"blocker":blocker});
    let blocker=if terminal{Value::Null}else if let Some(reason)=record["blocked_reason"].as_str().filter(|s|!s.is_empty()){json!(reason)}else if status=="current"{step["blocker"].clone()}else{json!("This document requires routing repair before proceeding.")};
    let can_act=blocker.is_null()&&status=="current"&&actor_can_review&&step["primary"]["userId"].as_str()==Some(actor);
    json!({"kind":"journal","recordId":record["id"],"recordVersion":record["version"],"version":version,"defaultsRevision":revision,"state":state,"steps":[step.clone()],"current":if status=="current"{step}else{Value::Null},"next":null,"canAct":can_act,"canManage":can_manage,"blocker":blocker,"history":history})
}
