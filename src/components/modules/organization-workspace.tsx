"use client";

import { SpendingReadiness } from "./spending-readiness";
import { SpendingTemplateEditor } from "./spending-template-editor";
import Link from "next/link";
import { useState } from "react";
import { Network, Users, UserPlus } from "lucide-react";
import { OrganizationChart } from "./organization-chart";
import { useStaffCommand } from "./staff-work-controls";
import { staffLevelTitles, type OrganizationOverview, type OrganizationPosition, type OrganizationTemplatePosition } from "@/lib/organization";
import styles from "./organization.module.css";

export function OrganizationWorkspace({base,data}:{base:string;data:OrganizationOverview}) {
  const [view,setView]=useState<"people"|"positions"|"templates"|"spending">("people");
  const issues=data.people.filter(p=>p.issues.length),activePositions=data.positions.filter(p=>p.active===1);
  return <div className={styles.workspace}>
    <div className={styles.stats}><div><Users size={20}/><strong>{data.people.length}</strong><span>Staff profiles</span></div><div><Network size={20}/><strong>{activePositions.length}</strong><span>Active positions</span></div><div><UserPlus size={20}/><strong>{activePositions.filter(p=>!data.people.some(s=>s.position_id===p.id)).length}</strong><span>Vacant positions</span></div><div><strong>{issues.length}</strong><span>Profiles needing setup</span></div></div>
    <div className={styles.toolbar}><nav aria-label="Organization views">{([['people','Reporting chart'],['positions','Positions'],...(data.canManage?[['templates','Templates']]:[]),...(data.canManageSpending?[['spending','Spending approvals']]:[])] as const).map(([id,label])=><button key={id} className={`button ${view===id?"":"secondary"}`} aria-pressed={view===id} onClick={()=>setView(id as typeof view)}>{label}</button>)}</nav><Link className="button secondary" href={`${base}/people`}>People directory</Link><Link className="button secondary" href={`${base}/inbox?tab=teams`}>Organization channels</Link></div>
    {data.levels.length<6&&data.canManage&&<InstallLevels base={base}/>}
    {view==="people"&&<OrganizationChart key={base} base={base} data={data}/>}
    {view==="positions"&&<section className="card"><h2>Positions and levels</h2><p className="muted">Positions stay in the organization when staff move or leave. Assign system roles separately in Login & access.</p>
      {data.positions.map(position=><details key={position.id} className={styles.position}><summary><strong>{position.title}</strong> · Level {position.level_number} · {position.active?data.people.filter(p=>p.position_id===position.id).length+" staff":"Archived"}</summary><p>{data.nodes.find(n=>n.id===position.branch_id)?.display_name??"Company"} / {data.nodes.find(n=>n.id===position.department_id)?.display_name??"All departments"} · Reports to {data.positions.find(p=>p.id===position.reports_to_position_id)?.title??"Company leadership"}</p>{data.canManage&&<><Link className="button secondary" href={`${base}/people?position=${encodeURIComponent(position.id)}`}>Add staff to this position</Link><PositionEditor base={base} data={data} position={position}/></>}</details>)}
      {data.canManage&&<details className={styles.position}><summary>Add a position</summary><PositionEditor base={base} data={data}/></details>}
    </section>}
    <SpendingReadiness base={base}/>
    {view==="spending"&&data.canManageSpending&&data.spendingTemplate&&<SpendingTemplateEditor base={base} data={data}/> }
    {view==="templates"&&data.canManage&&<TemplateEditor base={base} data={data}/>}
  </div>;
}
function InstallLevels({base}:{base:string}){const {send,busy,error}=useStaffCommand(base);return <section className="card"><h2>Set up six staff levels</h2><p>Install the numbered catalog, then explicitly assign each staff member. Existing levels and history are retained.</p><button className="button" disabled={busy} onClick={()=>void send("people/organization/levels",{})}>Install staff levels</button>{error&&<p role="alert">{error}</p>}</section>;}
function PositionEditor({base,data,position}:{base:string;data:OrganizationOverview;position?:OrganizationPosition}){
  const {send,busy,error}=useStaffCommand(base),[branch,setBranch]=useState(position?.branch_id??""),[ceo,setCeo]=useState(Boolean(position?.is_ceo));
  return <form className="business-form" onSubmit={async e=>{e.preventDefault();const f=new FormData(e.currentTarget);await send(`people/organization/positions${position?`/${position.id}`:""}`,{title:f.get("title"),branchId:branch||null,departmentId:f.get("department")||null,levelId:f.get("level"),reportsToPositionId:ceo?null:f.get("manager")||null,isCeo:ceo,active:f.get("active")==="on",expectedVersion:position?.version??0,reason:f.get("reason")});}}>
    <label>Position title<input name="title" defaultValue={position?.title} required minLength={2} maxLength={160}/></label>
    <label>Branch<select value={branch} onChange={e=>setBranch(e.target.value)}><option value="">Company-wide</option>{data.nodes.filter(n=>n.kind==="branch").map(n=><option key={n.id} value={n.id}>{n.display_name}</option>)}</select></label>
    <label>Department<select name="department" key={branch} defaultValue={position?.department_id??""}><option value="">All departments</option>{data.nodes.filter(n=>n.kind==="department"&&(n.branch_id??"")===branch).map(n=><option key={n.id} value={n.id}>{n.display_name}</option>)}</select></label>
    <label>Default level<select name="level" required defaultValue={position?.level_id??""}><option value="">Choose a level</option>{data.levels.map(l=><option key={l.id} value={l.id}>Level {l.level_number} · {l.name}</option>)}</select></label>
    <label><input type="checkbox" checked={ceo} onChange={e=>setCeo(e.target.checked)}/> Designated CEO position (Level 1)</label>
    {!ceo&&<label>Expected reporting position<select name="manager" required defaultValue={position?.reports_to_position_id??""}><option value="">Choose a reporting position</option>{data.positions.filter(p=>p.active===1&&p.id!==position?.id).map(p=><option key={p.id} value={p.id}>{p.title} · Level {p.level_number}</option>)}</select></label>}
    <label><input type="checkbox" name="active" defaultChecked={position?.active!==0}/> Active position</label>
    <label>Reason<input name="reason" required minLength={3} maxLength={1000} placeholder="Describe this organization change"/></label>
    <button className="button" disabled={busy||!data.levels.length}>{position?"Save position":"Create position"}</button>{error&&<p role="alert">{error}</p>}
  </form>;
}
function TemplateEditor({base,data}:{base:string;data:OrganizationOverview}){
  const {send,busy,error}=useStaffCommand(base);
  const starter:OrganizationTemplatePosition[]=staffLevelTitles.map((title,i)=>({key:`level-${i+1}`,title,levelNumber:i+1,reportsToKey:i?`level-${i}`:null,isCeo:i===0}));
  const [draft,setDraft]=useState(starter),[name,setName]=useState("Standard organization"),[selected,setSelected]=useState(""),[branch,setBranch]=useState("");
  const template=data.templates.find(t=>t.id===selected);
  return <div className="content-stack"><section className="card"><h2>Create a reusable organization template</h2><p className="muted">Review the positions and reporting structure. Templates create positions; they do not assign people or grant roles.</p>
    <label>Template name<input value={name} onChange={e=>setName(e.target.value)} maxLength={120}/></label>
    <div className={styles.templateGrid}>{draft.map((p,i)=><div key={p.key}><label>Default level<select value={p.levelNumber} disabled={p.isCeo} onChange={e=>setDraft(d=>d.map((v,j)=>j===i?{...v,levelNumber:Number(e.target.value),reportsToKey:"level-1"}:v))}>{staffLevelTitles.map((t,j)=><option key={j} value={j+1}>Level {j+1} · {t}</option>)}</select></label><label>Position title<input value={p.title} onChange={e=>setDraft(d=>d.map((v,j)=>j===i?{...v,title:e.target.value}:v))}/></label><label>Reports to<select disabled={p.isCeo} value={p.reportsToKey??""} onChange={e=>setDraft(d=>d.map((v,j)=>j===i?{...v,reportsToKey:e.target.value||null}:v))}><option value="">No superior (CEO)</option>{draft.filter(v=>v.levelNumber<p.levelNumber).map(v=><option key={v.key} value={v.key}>{v.title}</option>)}</select></label>{!p.isCeo&&<button className="button secondary" onClick={()=>setDraft(d=>d.filter((_,j)=>i!==j))}>Remove position</button>}</div>)}</div>
    <button className="button secondary" onClick={()=>setDraft(d=>[...d,{key:crypto.randomUUID(),title:"New position",levelNumber:6,reportsToKey:"level-1",isCeo:false}])}>Add template position</button>
    <button className="button" disabled={busy||name.trim().length<2} onClick={()=>void send("people/organization/templates",{name,positions:draft,expectedVersion:0})}>Save template</button>
  </section><section className="card"><h2>Apply a reviewed template</h2><label>Saved template<select value={selected} onChange={e=>setSelected(e.target.value)}><option value="">Choose a template</option>{data.templates.map(t=><option key={t.id} value={t.id}>{t.name} · revision {t.version}</option>)}</select></label>
    {template&&<form className="business-form" onSubmit={async e=>{e.preventDefault();const f=new FormData(e.currentTarget);await send(`people/organization/templates/${template.id}/apply`,{expectedVersion:template.version,branchId:branch||null,departmentId:f.get("department")||null,reason:f.get("reason")});}}>
      <label>Target branch<select value={branch} onChange={e=>setBranch(e.target.value)}><option value="">Company-wide</option>{data.nodes.filter(n=>n.kind==="branch").map(n=><option key={n.id} value={n.id}>{n.display_name}</option>)}</select></label>
      <label>Target department<select key={branch} name="department"><option value="">All departments</option>{data.nodes.filter(n=>n.kind==="department"&&(n.branch_id??"")===branch).map(n=><option key={n.id} value={n.id}>{n.display_name}</option>)}</select></label>
      <ul>{template.positions.map(p=><li key={p.key}>{p.title} · Level {p.levelNumber} → {template.positions.find(v=>v.key===p.reportsToKey)?.title??"Company leadership"}</li>)}</ul>
      <p>The existing CEO position is reused when present. Other positions are added as vacancies.</p>
      <label>Reason<input name="reason" required minLength={3} maxLength={1000}/></label>
      <label><input type="checkbox" required/> I have reviewed the positions and target placement.</label>
      <button className="button" disabled={busy}>Apply reviewed template</button>
    </form>}
  </section>{error&&<p role="alert" className="error">{error}</p>}</div>;
}
