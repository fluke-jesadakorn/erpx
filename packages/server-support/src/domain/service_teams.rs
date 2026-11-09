//! Pure service-team input normalization and locally decidable validation.
//! The caller supplies the current People-owned category catalogue. This does
//! not establish owner authority, membership, scope, version, or persistence.
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;

const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServiceTeamInput {
    name: String,
    active: bool,
    expected_version: i64,
    members: Vec<Member>,
    coverage: Vec<Coverage>,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Member { user_id: String, responsibility: String }
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Coverage { category: String, scope_kind: String, scope_node_id: String }

fn js_trim(s: &str) -> &str {
    s.trim_matches(|c| matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'))
}
fn bounded(s: &str, min: usize, max: usize) -> bool { (min..=max).contains(&s.chars().count()) }
fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, &'static str> {
    value.get(key).and_then(Value::as_str).ok_or("VALIDATION_FAILED")
}

/// Mirrors Zod parsing and checks that require no database facts. The caller
/// must perform owner/current-version checks first, before the returned local
/// decisions, matching `createValidateServiceTeam`'s ordering.
pub(crate) fn normalize_service_team(raw: &Value, categories: &[&str]) -> Result<ServiceTeamInput, &'static str> {
    let object=raw.as_object().ok_or("VALIDATION_FAILED")?;
    let name=js_trim(string(raw,"name")?);
    if !bounded(name,2,160) {return Err("VALIDATION_FAILED");}
    let active=match object.get("active") {None=>true,Some(v)=>v.as_bool().ok_or("VALIDATION_FAILED")?};
    let n=match object.get("expectedVersion") {None=>1.0,Some(v)=>v.as_f64().ok_or("VALIDATION_FAILED")?};
    if !n.is_finite() || n.fract()!=0.0 || !(1.0..=MAX_SAFE_INTEGER).contains(&n) {return Err("VALIDATION_FAILED");}
    let member_values=object.get("members").and_then(Value::as_array).ok_or("VALIDATION_FAILED")?;
    if member_values.len()>100 {return Err("VALIDATION_FAILED");}
    let mut members=Vec::with_capacity(member_values.len());
    for item in member_values {
        let id=string(item,"userId")?;let responsibility=string(item,"responsibility")?;
        if !bounded(id,1,160) || !matches!(responsibility,"lead"|"member") {return Err("VALIDATION_FAILED");}
        members.push(Member{user_id:id.into(),responsibility:responsibility.into()});
    }
    let coverage_values=object.get("coverage").and_then(Value::as_array).ok_or("VALIDATION_FAILED")?;
    if !(1..=50).contains(&coverage_values.len()) {return Err("VALIDATION_FAILED");}
    let mut coverage=Vec::with_capacity(coverage_values.len());
    for item in coverage_values {
        let category=string(item,"category")?;let kind=string(item,"scopeKind")?;let node=string(item,"scopeNodeId")?;
        if !categories.contains(&category) || !matches!(kind,"holding"|"branch"|"department") || !bounded(node,1,160) {return Err("VALIDATION_FAILED");}
        coverage.push(Coverage{category:category.into(),scope_kind:kind.into(),scope_node_id:node.into()});
    }
    Ok(ServiceTeamInput{name:name.into(),active,expected_version:n as i64,members,coverage})
}

pub(crate) fn validate_local_team_rules(input:&ServiceTeamInput) -> Result<(), &'static str> {
    let mut members=HashSet::new();
    if input.members.iter().any(|member|!members.insert(&member.user_id)) {return Err("DUPLICATE_MEMBER");}
    let mut coverage=HashSet::new();
    // JS builds exactly this colon-delimited string; do not replace its
    // equality with tuple equality when source identifiers contain ':'.
    if input.coverage.iter().any(|c|!coverage.insert(format!("{}:{}:{}",c.category,c.scope_kind,c.scope_node_id))) {return Err("DUPLICATE_COVERAGE");}
    if input.active && !input.members.iter().any(|member|member.responsibility=="lead") {return Err("TEAM_LEAD_REQUIRED");}
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schema_and_local_rules_match_actual_typescript() {
        let cases=serde_json::json!([
            {"name":" \u{feff}Team 😀 ","members":[{"userId":"a","responsibility":"lead","extra":1}],"coverage":[{"category":"it","scopeKind":"holding","scopeNodeId":"c","extra":1}],"extra":true},
            {"name":" T ","members":[],"coverage":[{"category":"it","scopeKind":"holding","scopeNodeId":"c"}]},
            {"name":"Team","active":false,"expectedVersion":2,"members":[],"coverage":[{"category":"it","scopeKind":"branch","scopeNodeId":"b"}]},
            {"name":"Team","members":[{"userId":"a","responsibility":"lead"},{"userId":"a","responsibility":"member"}],"coverage":[{"category":"it","scopeKind":"holding","scopeNodeId":"c"}]},
            {"name":"Team","members":[{"userId":"a","responsibility":"lead"}],"coverage":[{"category":"it","scopeKind":"holding","scopeNodeId":"c"},{"category":"it","scopeKind":"holding","scopeNodeId":"c"}]},
            {"name":"Team","members":[{"userId":"a","responsibility":"member"}],"coverage":[{"category":"it","scopeKind":"holding","scopeNodeId":"c"}]},
            {"name":"Team","expectedVersion":9007199254740992_u64,"members":[],"coverage":[{"category":"it","scopeKind":"holding","scopeNodeId":"c"}]},
            {"name":"Team","members":[],"coverage":[{"category":"unknown","scopeKind":"holding","scopeNodeId":"c"}]},
            {"name":"Team","active":null,"members":[],"coverage":[{"category":"it","scopeKind":"holding","scopeNodeId":"c"}]},
            {"name":"Team","members":[{"userId":"😀".repeat(81),"responsibility":"lead"}],"coverage":[{"category":"it","scopeKind":"holding","scopeNodeId":"c"}]},
            {"name":"Team","members":[{"userId":"😀".repeat(161),"responsibility":"lead"}],"coverage":[{"category":"it","scopeKind":"holding","scopeNodeId":"c"}]},
            {"name":"😀","members":[{"userId":"a","responsibility":"lead"}],"coverage":[{"category":"it","scopeKind":"holding","scopeNodeId":"c"}]}
        ]);
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let schema=root.join("packages/server-support/src/domain/service-teams.ts");
        let application=root.join("packages/server-support/src/application/service-teams.ts");
        let script=r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const s=await import(process.argv[1]),a=await import(process.argv[2]);const cases=JSON.parse(process.argv[3]);const out=[];for(const raw of cases){const p=s.serviceTeamSchema.safeParse(raw);if(!p.success){out.push({error:'VALIDATION_FAILED'});continue}const validate=a.createValidateServiceTeam({error:x=>Object.assign(new Error(x.message),{code:x.code}),owner:async()=>{},team:async()=>null,companyMember:async()=>true,node:async(_c,id)=>({kind:id==='b'?'branch':'company',branch_id:null}),validateScope:async()=>{},scopeMember:async()=>true});try{await validate({companyId:'company'},null,p.data);out.push({input:p.data})}catch(e){out.push({error:e.code})}}process.stdout.write(JSON.stringify(out));"#;
        let output=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).current_dir(root).args(["--conditions=react-server","-e",script]).arg(schema).arg(application).arg(cases.to_string()).output().unwrap();
        assert!(output.status.success(),"actual TS oracle: {}",String::from_utf8_lossy(&output.stderr));
        let expected:Vec<Value>=serde_json::from_slice(&output.stdout).unwrap();
        let actual:Vec<Value>=cases.as_array().unwrap().iter().map(|v|match normalize_service_team(v,&["it","engineering","maintenance","facilities","housekeeping","people","finance","purchasing","operations","sales"]) {Ok(input)=>match validate_local_team_rules(&input){Ok(())=>serde_json::json!({"input":input}),Err(error)=>serde_json::json!({"error":error})},Err(error)=>serde_json::json!({"error":error})}).collect();
        assert_eq!(actual,expected);
    }
}
