"use client";
import { useState } from "react";

type Person={person_id:string;legal_name:string;position_id:string|null};
type Position={id:string;title:string;department_id:string|null;is_ceo:number};
type Seat={kind:"ceo"|"hr"|"head";scope_id:string;version:number;person_id:string|null;position_id:string|null;person_name?:string|null;position_title?:string|null;department_name?:string|null};
type Selector={kind:"person";personId:string}|{kind:"position";positionId:string};
type Policy={department_id:string;version:number;steps_json:Selector[]|string};
type Data={seats:Seat[];history:{id:string;kind:string;scope_id:string;reason:string;provenance:string;appointed_by:string|null;created_at:number;version:number}[];policies:Policy[];departments:{id:string;display_name:string}[];canAppointKinds:("ceo"|"hr"|"head")[];canPublishDepartmentIds:string[]};
type Props={holdingSlug:string;initial:Data;people:Person[];positions:Position[]};
const label={ceo:"CEO",hr:"HR",head:"Department head"};
function validateFlowSteps(steps:Selector[]){
  if(steps.length<1||steps.length>20)return "Choose between 1 and 20 approval steps.";
  const unique=new Set(steps.map(step=>step.kind==="person"?`person:${step.personId}`:`position:${step.positionId}`));
  return unique.size===steps.length?null:"Choose each person or position only once.";
}
function FormContext({version,effect}:{version:number;effect:string}){
  return <div className="notice"><strong>Current version {version}</strong><p>{effect}</p><p className="muted">Saving checks the current version and the selected people or positions. If it changed, reload and review before saving again.</p></div>;
}
async function post(url:string,body:unknown){
  const response=await fetch(url,{method:"POST",headers:{"Content-Type":"application/json","Idempotency-Key":crypto.randomUUID()},body:JSON.stringify(body)});
  const result=await response.json();
  if(!response.ok)throw new Error(result?.error?.message??result?.message??"Could not save this change.");
  return result;
}
function AppointmentEditor({seat,people,positions,url,onSaved}:{seat:Seat;people:Person[];positions:Position[];url:string;onSaved:()=>Promise<void>}){
  const [personId,setPersonId]=useState(seat.person_id??"");
  const [positionId,setPositionId]=useState(seat.position_id??"");
  const [reason,setReason]=useState("");
  const [message,setMessage]=useState("");
  const [busy,setBusy]=useState(false);
  const choices=positions.filter(p=>seat.kind==="ceo"?!!p.is_ceo:seat.kind==="head"?p.department_id===seat.scope_id:true);
  return <form className="card" onSubmit={async event=>{event.preventDefault();setBusy(true);setMessage("");try{await post(url,{kind:seat.kind,departmentId:seat.kind==="head"?seat.scope_id:null,positionId:positionId||null,personId:personId||null,expectedVersion:seat.version,reason});await onSaved();setReason("");setMessage("Appointment saved.");}catch(error){setMessage((error as Error).message);}finally{setBusy(false);}}}>
    <h3>{label[seat.kind]}{seat.department_name?` · ${seat.department_name}`:""}</h3>
    <p className="muted">Current: {seat.person_name??"Vacant"}</p><FormContext version={seat.version} effect="This appointment applies to new decisions immediately. Existing submitted routes retain their recorded approvers."/>
    <label>Position <select value={positionId} onChange={event=>setPositionId(event.target.value)}><option value="">No position</option>{choices.map(p=><option key={p.id} value={p.id}>{p.title}</option>)}</select></label>
    <label>Holder <select value={personId} onChange={event=>setPersonId(event.target.value)}><option value="">Vacant</option>{people.map(p=><option key={p.person_id} value={p.person_id}>{p.legal_name}</option>)}</select></label>
    <label>Reason <input required minLength={3} maxLength={1000} value={reason} onChange={event=>setReason(event.target.value)}/></label>
    <button type="submit" disabled={busy}>{busy?"Saving…":"Record appointment"}</button>{message&&<p role="status">{message}</p>}
  </form>;
}
function FlowEditor({department,policy,people,positions,url,onSaved}:{department:{id:string;display_name:string};policy:Policy|undefined;people:Person[];positions:Position[];url:string;onSaved:()=>Promise<void>}){
  const original=policy?(typeof policy.steps_json==="string"?JSON.parse(policy.steps_json) as Selector[]:policy.steps_json):[];
  const [steps,setSteps]=useState<Selector[]>(original);
  const [reason,setReason]=useState("");
  const [message,setMessage]=useState("");
  const [busy,setBusy]=useState(false);
  const options=[...people.map(p=>({value:`person:${p.person_id}`,name:`Person · ${p.legal_name}`})),...positions.map(p=>({value:`position:${p.id}`,name:`Position · ${p.title}`}))];
  const toValue=(step:Selector)=>step.kind==="person"?`person:${step.personId}`:`position:${step.positionId}`;
  const fromValue=(value:string):Selector=>value.startsWith("person:")?{kind:"person",personId:value.slice(7)}:{kind:"position",positionId:value.slice(9)};
  return <form className="card" onSubmit={async event=>{event.preventDefault();const invalid=validateFlowSteps(steps);if(invalid){setMessage(invalid);return;}setBusy(true);setMessage("");try{await post(url,{departmentId:department.id,expectedVersion:policy?.version??0,steps,reason});await onSaved();setReason("");setMessage("Flow published for new requests.");}catch(error){setMessage((error as Error).message);}finally{setBusy(false);}}}>
    <h3>{department.display_name}</h3><FormContext version={policy?.version??0} effect="Publishing applies to new requests. Requests already submitted keep their saved route and decision history."/>
    {steps.map((step,index)=><div key={index} style={{display:"flex",gap:8,alignItems:"center",marginBottom:8}}><span>{index+1}.</span><select aria-label={`Approver ${index+1}`} value={toValue(step)} onChange={event=>setSteps(values=>values.map((s,i)=>i===index?fromValue(event.target.value):s))}>{options.map(o=><option key={o.value} value={o.value}>{o.name}</option>)}</select><button type="button" disabled={index===0} onClick={()=>setSteps(values=>{const next=[...values];[next[index-1],next[index]]=[next[index],next[index-1]];return next;})}>↑</button><button type="button" disabled={index===steps.length-1} onClick={()=>setSteps(values=>{const next=[...values];[next[index],next[index+1]]=[next[index+1],next[index]];return next;})}>↓</button><button type="button" onClick={()=>setSteps(values=>values.filter((_,i)=>i!==index))}>Remove</button></div>)}
    <button type="button" disabled={steps.length>=20||!options.length} onClick={()=>setSteps(values=>[...values,fromValue(options[0].value)])}>Add step</button>
    <label>Reason <input required minLength={3} maxLength={1000} value={reason} onChange={event=>setReason(event.target.value)}/></label>
    <button type="submit" disabled={busy||!steps.length}>{busy?"Publishing…":"Publish flow"}</button>{message&&<p role="status">{message}</p>}
  </form>;
}
export function DepartmentAuthorityWorkspace({holdingSlug,initial,people,positions}:Props){
  const [data,setData]=useState(initial);
  const root=`/api/v1/holdings/${encodeURIComponent(holdingSlug)}/authority`;
  const refresh=async()=>{const response=await fetch(root,{cache:"no-store"});if(!response.ok)throw new Error("Could not reload authority data.");setData((await response.json()).data as Data);};
  const seats:Seat[]=[...data.seats,...data.departments.filter(d=>!data.seats.some(s=>s.kind==="head"&&s.scope_id===d.id)).map(d=>({kind:"head" as const,scope_id:d.id,version:0,person_id:null,position_id:null,person_name:null,department_name:d.display_name}))];
  return <section className="content-stack" aria-label="Appointments and department approval flows">
    <div><h2>Appointments</h2><p className="muted">Appointment authority applies immediately. Directory work has a separate delivery status.</p></div>
    {data.canAppointKinds.length>0&&<div className="grid-two">{seats.filter(seat=>data.canAppointKinds.includes(seat.kind)).map(seat=><AppointmentEditor key={`${seat.kind}:${seat.scope_id}:${seat.version}`} seat={seat} people={people} positions={positions} url={`${root}/appointments`} onSaved={refresh}/>)}</div>}
    <div className="card">{seats.filter(seat=>!data.canAppointKinds.includes(seat.kind)).map(s=><p key={`${s.kind}:${s.scope_id}`}>{label[s.kind]}{s.department_name?` · ${s.department_name}`:""}: {s.person_name??"Vacant"}</p>)}</div>
    <div><h2>Department approval flows</h2><p className="muted">Choose people or positions in order. A position must have one active holder when a request is sent.</p></div>
    {data.canPublishDepartmentIds.map(departmentId=>{const department=data.departments.find(d=>d.id===departmentId)!;const policy=data.policies.find(p=>p.department_id===departmentId);return <FlowEditor key={`${departmentId}:${policy?.version??0}`} department={department} policy={policy} people={people} positions={positions} url={`${root}/flows`} onSaved={refresh}/>;})}
    <details className="card"><summary>Appointment history</summary>{data.history.map(item=><p key={item.id}>{label[item.kind as keyof typeof label]} · v{item.version} · {item.provenance==="legacy"?"Imported legacy record":"Appointed by "+item.appointed_by} · {item.reason}</p>)}</details>
  </section>;
}
