//! Pure Work contribution for the Field invoice and spare-cost capabilities.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;

fn changed() -> BusinessError {
    BusinessError::new(FailureKind::Conflict,"FINANCE_WORK_SOURCE_CHANGED","The original financial workflow changed.")
}

/// Authority and purchase decisions come from the native owner Flow read.
/// This retains the existing Work participation and queue semantics.
pub fn field_financial_work_item(kind:&str,record:&Value,flow:&Value,actor:&str,href:String)->Result<Option<Value>,BusinessError>{
    let (title_key,at_key)=match kind {"invoice"=>("invoice_number","updated_at"),"journal"=>("journal_number","created_at"),_=>return Err(changed())};
    let id=record["id"].as_str().ok_or_else(changed)?;
    let version=record["version"].as_i64().ok_or_else(changed)?;
    let state=record["status"].as_str().ok_or_else(changed)?;
    if flow["kind"]!=kind||flow["recordId"]!=id||flow["recordVersion"]!=version||flow["state"]!=state{return Err(changed());}
    let can_act=flow["canAct"].as_bool().ok_or_else(changed)?;
    let can_manage=flow["canManage"].as_bool().ok_or_else(changed)?;
    let blocker=match &flow["blocker"]{Value::Null=>None,Value::String(s)=>Some(s.as_str()),_=>return Err(changed())};
    let steps=flow["steps"].as_array().ok_or_else(changed)?;
    let history=flow["history"].as_array().ok_or_else(changed)?;
    let own=record["created_by"].as_str()==Some(actor);
    let participant=steps.iter().any(|s|s["primary"]["userId"].as_str()==Some(actor))||history.iter().any(|e|e["actorName"].as_str()==Some(actor));
    if !own&&!participant&&!(can_manage&&blocker.is_some()){return Ok(None);}
    let current=&flow["current"];
    let action=if current.is_null(){"No pending action"}else{current["action"].as_str().ok_or_else(changed)?};
    let setup=blocker.is_some()&&current["primary"].is_null();
    let mut relationships=Vec::new();if own{relationships.push("requested");}if can_act{relationships.push("assigned");}
    let mut item = serde_json::Map::new();
    item.insert("id".into(), Value::String(id.into()));
    item.insert("type".into(), Value::String(kind.into()));
    item.insert("actionKind".into(), Value::String("work".into()));
    item.insert("title".into(), Value::String(record[title_key].as_str().ok_or_else(changed)?.into()));
    item.insert("detail".into(), Value::String(action.into()));
    item.insert("href".into(), Value::String(href));
    item.insert("status".into(), Value::String(state.into()));
    item.insert("updatedAt".into(), Value::from(record[at_key].as_i64().ok_or_else(changed)?));
    item.insert("queue".into(), Value::String(if can_act {"todo"} else if !current.is_null() {"waiting"} else {"completed"}.into()));
    item.insert("flow".into(), flow.clone());
    item.insert("canAct".into(), Value::Bool(can_act));
    item.insert("category".into(), Value::String(if setup {"setup"} else {"operational"}.into()));
    item.insert("setupReason".into(), blocker.map_or(Value::Null, |value| Value::String(value.into())));
    item.insert("relationships".into(), Value::Array(relationships.into_iter().map(|value| Value::String(value.into())).collect()));
    item.insert("nextAction".into(), Value::String(action.into()));
    item.insert("blocker".into(), blocker.map_or(Value::Null, |value| Value::String(value.into())));
    item.insert("responsibleQueue".into(), Value::String(if current.is_null() { String::new() } else if let Some(name) = current["primary"]["name"].as_str() { format!("{} / {name}", current["teamName"].as_str().ok_or_else(changed)?) } else { current["teamName"].as_str().ok_or_else(changed)?.into() }));
    Ok(Some(Value::Object(item)))
}
