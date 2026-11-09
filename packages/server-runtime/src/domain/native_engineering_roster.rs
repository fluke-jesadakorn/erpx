//! JSON-input parity with the Runtime-owned TypeScript roster validator.
//! Validation is not a filesystem seal, database identity or authorization.
use serde_json::Value;
use std::collections::HashSet;
const BINDINGS:[&str;8]=["CONTROL_DB","CONTROL_READ_DB","H001_DB","H001_READ_DB","H002_DB","H002_READ_DB","H003_DB","H003_READ_DB"];
const MAX_SAFE:f64=9_007_199_254_740_991.0;
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct NativeEngineeringEntry{binding:&'static str,token:String,oid:u32}
impl NativeEngineeringEntry{pub fn binding(&self)->&'static str{self.binding}pub fn token(&self)->&str{&self.token}pub fn oid(&self)->u32{self.oid}}
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct NativeEngineeringRoster{postmaster_started_ms:i64,entries:Vec<NativeEngineeringEntry>}
impl NativeEngineeringRoster{pub fn postmaster_started_ms(&self)->i64{self.postmaster_started_ms}pub fn entries(&self)->&[NativeEngineeringEntry]{&self.entries}}
fn positive_safe(v:&Value,max:f64)->Option<u64>{let n=v.as_f64()?;(n.is_finite()&&n.fract()==0.0&&n>0.0&&n<=max).then_some(n as u64)}
// RegExp.test applies JS ToString: for JSON values, a token that can match a
// 32-character lowercase-hex regex is a string or arrays nested around one.
fn token_text(v:&Value)->Option<&str>{match v{Value::String(s)=>Some(s),Value::Array(a) if a.len()==1=>token_text(&a[0]),_=>None}}
/// Checks an already parsed JSON value against the actual TS validator. Expected
/// fields are supplied by trusted owner context and are never an authority grant.
pub fn validate_native_engineering_roster(value:&Value,run:&str,source:&str,manifest:&str)->Option<NativeEngineeringRoster>{
 let root=value.as_object()?;
 if root.get("version")?.as_f64()?!=1.0||root.get("run")?.as_str()?!=run||root.get("sourceFingerprint")?.as_str()?!=source||root.get("fileManifestSha256")?.as_str()?!=manifest{return None;}
 let start=positive_safe(root.get("postmasterStartedMs")?,MAX_SAFE)?;let rows=root.get("entries")?.as_array()?;if rows.len()!=BINDINGS.len(){return None;}
 let mut string_tokens=HashSet::new();let mut oids=HashSet::new();let mut entries=Vec::with_capacity(8);
 for (i,row) in rows.iter().enumerate(){let row=row.as_object()?;if row.get("binding")?.as_str()?!=BINDINGS[i]{return None;}let raw=row.get("token")?;let token=token_text(raw)?;if token.len()!=32||!token.bytes().all(|v|v.is_ascii_digit()||(b'a'..=b'f').contains(&v)){return None;}if raw.is_string()&&!string_tokens.insert(token.to_owned()){return None;}let oid=positive_safe(row.get("oid")?,u32::MAX as f64)? as u32;if !oids.insert(oid){return None;}entries.push(NativeEngineeringEntry{binding:BINDINGS[i],token:token.to_owned(),oid});}
 Some(NativeEngineeringRoster{postmaster_started_ms:start as i64,entries})
}
