//! Legacy Zod normalization only; no source, authority, or command effect.
//! CommandJson retains lone UTF-16 units for Workflow identity, not storage.
use crate::domain::order::valid_date;
use erpx_server_shared::public::BusinessError;
use erpx_server_workflow::public::commands::CommandJson;

fn invalid()->BusinessError { BusinessError::invalid("COMMERCE_FULFILLMENT_INPUT_INVALID","Use a valid fulfillment input.") }
fn prop(o:&CommandJson,key:&str)->Result<Option<CommandJson>,BusinessError> { o.property(key).map_err(|_|invalid()) }
fn units(v:&CommandJson)->Result<Vec<u16>,BusinessError> { v.string_utf16_units().map_err(|_|invalid())?.ok_or_else(invalid) }
fn encode(v:&[u16])->Result<String,BusinessError> { Ok(CommandJson::from_utf16_string(v).map_err(|_|invalid())?.as_str().to_owned()) }
fn ascii(v:&[u16])->Option<String> { if v.iter().all(|c|*c<=0x7f) {Some(v.iter().map(|c|*c as u8 as char).collect())}else{None} }
fn trimmed(v:&[u16])->&[u16] {
    fn ws(c:u16)->bool { matches!(c,0x0009..=0x000d|0x0020|0x00a0|0x1680|0x2000..=0x200a|0x2028|0x2029|0x202f|0x205f|0x3000|0xfeff) }
    let start=v.iter().position(|c|!ws(*c)).unwrap_or(v.len());
    let end=v.iter().rposition(|c|!ws(*c)).map_or(start,|i|i+1);
    &v[start..end]
}
fn code_points(v:&[u16])->usize {
    let mut count=0;let mut i=0;
    while i<v.len(){if (0xd800..=0xdbff).contains(&v[i]) && i+1<v.len() && (0xdc00..=0xdfff).contains(&v[i+1]){i+=1;} i+=1;count+=1;}
    count
}
fn field(o:&CommandJson,key:&str,default:Option<&str>)->Result<Vec<u16>,BusinessError> {
    match prop(o,key)? {Some(v)=>units(&v),None=>default.map(|v|v.encode_utf16().collect()).ok_or_else(invalid)}
}
fn text(o:&CommandJson,key:&str,default:Option<&str>,trim:bool,min:usize,max:usize)->Result<String,BusinessError> {
    let value=field(o,key,default)?;let value=if trim {trimmed(&value)}else{&value};
    if !(min..=max).contains(&code_points(value)){return Err(invalid());}encode(value)
}
fn quantity(v:&[u16])->bool {
    let Some(v)=ascii(v) else{return false};
    let (whole,fraction)=v.split_once('.').map_or((v.as_str(),None),|(a,b)|(a,Some(b)));
    !whole.is_empty() && whole.bytes().all(|c|c.is_ascii_digit()) && fraction.is_none_or(|f|!f.is_empty()&&f.len()<=3&&f.bytes().all(|c|c.is_ascii_digit()))
}
fn date(o:&CommandJson,key:&str,default:Option<&str>,empty:bool)->Result<String,BusinessError> {
    let v=field(o,key,default)?;let value=ascii(&v).ok_or_else(invalid)?;
    if !(valid_date(&value)||(empty&&value.is_empty())){return Err(invalid());}encode(&v)
}
fn strings(o:&CommandJson,key:&str)->Result<String,BusinessError> {
    let Some(value)=prop(o,key)? else{return Ok("[]".into())};
    let values=value.array_elements().map_err(|_|invalid())?.ok_or_else(invalid)?;
    if values.len()>10{return Err(invalid());}
    let mut encoded=Vec::with_capacity(values.len());
    for value in values {let value=units(&value)?;if value.is_empty(){return Err(invalid());}encoded.push(encode(&value)?);}
    Ok(format!("[{}]",encoded.join(",")))
}
fn allocation(o:&CommandJson)->Result<String,BusinessError> {
    let mut fields=Vec::new();
    for (key,trim,max) in [("lotId",false,usize::MAX),("lotCode",true,100),("serialNumber",true,160)] {
        if prop(o,key)?.is_some(){fields.push(format!("\"{key}\":{}",text(o,key,None,trim,1,max)?));}
    }
    let q=field(o,"quantity",None)?;if !quantity(&q){return Err(invalid());}
    fields.push(format!("\"quantity\":{}",encode(&q)?));
    fields.push(format!("\"manufacturedDate\":{}",date(o,"manufacturedDate",Some(""),true)?));
    fields.push(format!("\"expiryDate\":{}",date(o,"expiryDate",Some(""),true)?));
    let disposition=field(o,"disposition",Some("available"))?;
    if !matches!(ascii(&disposition).as_deref(),Some("available"|"quarantine")){return Err(invalid());}
    fields.push(format!("\"disposition\":{}",encode(&disposition)?));
    fields.push(format!("\"quarantineReason\":{}",text(o,"quarantineReason",Some(""),true,0,2000)?));
    fields.push(format!("\"quarantineDueDate\":{}",date(o,"quarantineDueDate",Some(""),true)?));
    Ok(format!("{{{}}}",fields.join(",")))
}
fn line(o:&CommandJson)->Result<String,BusinessError> {
    let id=text(o,"lineId",None,false,1,usize::MAX)?;
    let q=field(o,"quantity",None)?;if !quantity(&q){return Err(invalid());}
    let allocations=match prop(o,"allocations")? {None=>Vec::new(),Some(v)=>{let list=v.array_elements().map_err(|_|invalid())?.ok_or_else(invalid)?;if list.len()>200{return Err(invalid());}list.iter().map(allocation).collect::<Result<Vec<_>,_>>()?}};
    Ok(format!("{{\"lineId\":{id},\"quantity\":{},\"allocations\":[{}]}}",encode(&q)?,allocations.join(",")))
}
/// Preserve JSON.stringify ordering and UTF-16 units within Workflow's 2 MiB
/// codec bound. This is not an authority or storage conversion.
pub fn normalize_legacy_fulfillment_input(raw:&str)->Result<String,BusinessError> {
    let o=CommandJson::parse(raw).map_err(|_|invalid())?;
    let mode=field(&o,"completionMode",Some("goods"))?;
    if !matches!(ascii(&mode).as_deref(),Some("goods"|"service"|"mixed")){return Err(invalid());}
    let version=prop(&o,"expectedVersion")?.ok_or_else(invalid)?.as_str().parse::<f64>().map_err(|_|invalid())?;
    if !version.is_finite() || version.fract()!=0.0 || !(1.0..=9_007_199_254_740_991.0).contains(&version){return Err(invalid());}
    let lines=prop(&o,"lines")?.ok_or_else(invalid)?.array_elements().map_err(|_|invalid())?.ok_or_else(invalid)?;
    if lines.is_empty()||lines.len()>20{return Err(invalid());}
    let lines=lines.iter().map(line).collect::<Result<Vec<_>,_>>()?;
    let out=format!("{{\"completionMode\":{},\"expectedVersion\":{},\"attachmentSetIds\":{},\"date\":{},\"evidenceIds\":{},\"evidenceNote\":{},\"lines\":[{}]}}",encode(&mode)?,version as i64,strings(&o,"attachmentSetIds")?,date(&o,"date",None,false)?,strings(&o,"evidenceIds")?,text(&o,"evidenceNote",Some(""),true,0,1000)?,lines.join(","));
    Ok(CommandJson::parse(&out).map_err(|_|invalid())?.as_str().to_owned())
}

/// Ordered Workflow input only; caller obtains current selected actor separately.
pub fn legacy_fulfillment_workflow_input(raw:&str)->Result<CommandJson,BusinessError> {
    CommandJson::parse(&normalize_legacy_fulfillment_input(raw)?)
}
