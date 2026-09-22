"use client";
import { FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { SetupChecklist, type SetupItem } from "@/components/ui/setup-checklist";


import { PositionPreparation } from "@/components/people/lifecycle-workspace";
import { staffLevelTitles,type OrganizationNode,type OrganizationNodeChange,type OrganizationOverview,type OrganizationPosition,type OrganizationTemplatePosition } from "@/lib/organization";
import type { PeopleLifecycleData } from "@/lib/people-lifecycle";
import { Network,UserPlus,Users } from "lucide-react";
import Link from "next/link";
import { useState } from "react";
import { OrganizationChart } from "./organization-chart";
import styles from "./organization.module.css";
import { SpendingReadiness } from "./spending-readiness";
import { SpendingTemplateEditor } from "./spending-template-editor";
import { useStaffCommand } from "./staff-work-controls";

export function OrganizationWorkspace({base,data,lifecycle,initialPositionId,initialView}:{base:string;data:OrganizationOverview;lifecycle?:PeopleLifecycleData;initialPositionId?:string;initialView?:"people"|"positions"|"templates"|"spending"}) {
  const issues=data.people.filter(p=>p.issues.length),activePositions=data.positions.filter(p=>p.active===1);
  const ceoPosition=activePositions.find(position=>position.is_ceo===1),ceoReady=!!ceoPosition&&data.people.some(person=>person.position_id===ceoPosition.id&&person.level_number===1&&!person.issues.length);
  const approvalIssues=data.canManage?data.people.filter(person=>person.issues.some(issue=>issue.includes("People Manager access is missing"))).length:0;
  const setupItems:SetupItem[]=[
    {id:"staff-levels",label:"Six staff levels",ready:data.levels.length>=6,owner:"Company HR administrator",href:`${base}/people/organization?view=positions`,detail:data.levels.length>=6?"The level catalog is installed.":"Install the level catalog before creating positions."},
    {id:"business-units",label:"Branch and department structure",ready:data.nodes.some(node=>node.kind==="branch"&&node.status==="active"),owner:"Company HR administrator",href:`${base}/people/organization`,detail:data.nodes.some(node=>node.kind==="branch"&&node.status==="active")?"At least one active branch is available.":"Create and independently approve the first active branch."},
    {id:"leadership",label:"Active CEO and reporting anchor",ready:ceoReady,owner:"People administrator",href:`${base}/people/organization?view=positions`,detail:ceoReady?"An active Level 1 position has an assigned person.":"Create a Level 1 CEO position and assign an active person."},
    {id:"staff-profiles",label:"Staff profiles ready for work",ready:issues.length===0,owner:"People manager",href:`${base}/people`,detail:issues.length===0?"All visible staff have valid placement and reporting lines.":`${issues.length} profile${issues.length===1?"":"s"} need organization setup.`},
    {id:"approval-coverage",label:"Approval coverage for reporting teams",ready:approvalIssues===0,owner:"Organization administrator",href:`${base}/settings?section=approvals`,detail:approvalIssues===0?"No missing People Manager coverage is reported.":`${approvalIssues} reporting team${approvalIssues===1?"":"s"} need a scoped approver grant.`},
  ];
  return <WidgetGrid pageKey="people-organization">
    <div className={styles.stats}><div><Users size={20}/><strong>{data.people.length}</strong><span>Staff profiles</span></div><div><Network size={20}/><strong>{activePositions.length}</strong><span>Active positions</span></div><div><UserPlus size={20}/><strong>{activePositions.filter(p=>!data.people.some(s=>s.position_id===p.id)).length}</strong><span>Vacant positions</span></div><div><strong>{issues.length}</strong><span>Profiles needing setup</span></div></div>
    <div className={styles.toolbar}><Link className="button secondary" href={`${base}/people`}>People directory</Link><Link className="button secondary" href={`${base}/inbox?tab=teams`}>Organization channels</Link></div>
    <SetupChecklist title="Organization readiness" items={setupItems}/>
    {data.canManageUnits&&<FeatureWidget id="organization-units" title="Branches & departments"><OrganizationUnits base={base} data={data}/></FeatureWidget>}
    {data.levels.length<6&&data.canManage&&<InstallLevels base={base}/>}
    {(!initialView || initialView === "people")&&<FeatureWidget id="organization-organizationchart" title="Teams"><OrganizationChart key={base} base={base} data={data}/></FeatureWidget>}
    {(!initialView || initialView === "positions")&&lifecycle&&<FeatureWidget id="organization-positionpreparation" title="Position preparation"><PositionPreparation base={base} data={{...lifecycle,canPrepare:false}} positionId={initialPositionId} hrDetails={id=>{const position=data.positions.find(p=>p.id===id);return position&&<><p>{data.people.filter(p=>p.position_id===id).length} staff · Level {position.level_number} · Reports to {data.positions.find(p=>p.id===position.reports_to_position_id)?.title??"Company leadership"}</p>{lifecycle.canPrepare&&<Link href={`${base}/technology?view=positions&position=${encodeURIComponent(id)}`}>Open IT preparation in Technology</Link>}{data.canManage&&<details><summary>Edit HR position details</summary><PositionEditor base={base} data={data} position={position}/></details>}</>;}}/></FeatureWidget>}
    {(!initialView || initialView === "positions")&&(!lifecycle||data.canManage)&&<FeatureWidget id="organization-positions" title={lifecycle?"Create a position":"Positions and levels"}><p className="muted">Positions stay when staff move or leave. HR defines the work and reporting line. IT prepares resources separately in Technology.</p>
      {!lifecycle&&data.positions.map(position=><details key={position.id} className={styles.position}><summary><strong>{position.title}</strong> · Level {position.level_number} · {position.active?data.people.filter(p=>p.position_id===position.id).length+" staff":"Archived"}</summary><p>{data.nodes.find(n=>n.id===position.branch_id)?.display_name??"Company"} / {data.nodes.find(n=>n.id===position.department_id)?.display_name??"All departments"} · Reports to {data.positions.find(p=>p.id===position.reports_to_position_id)?.title??"Company leadership"}</p>{data.canManage&&<><Link className="button secondary" href={`${base}/people?position=${encodeURIComponent(position.id)}`}>Add staff to this position</Link><PositionEditor base={base} data={data} position={position}/></>}</details>)}
      {data.canManage&&<details className={styles.position}><summary>Add a position</summary><PositionEditor base={base} data={data}/></details>}
    </FeatureWidget>}
    <SpendingReadiness base={base}/>
    {(!initialView || initialView === "spending")&&data.canManageSpending&&data.spendingTemplate&&<FeatureWidget id="organization-spendingtemplateeditor" title="Spending approvals"><SpendingTemplateEditor base={base} data={data}/></FeatureWidget> }
    {(!initialView || initialView === "templates")&&data.canManage&&<FeatureWidget id="organization-templateeditor" title="Organization templates"><TemplateEditor base={base} data={data}/></FeatureWidget>}
  </WidgetGrid>;
}
function OrganizationUnits({base,data}:{base:string;data:OrganizationOverview}){
  const active=data.nodes.filter(n=>n.kind!=="company"&&n.status==="active"),suspended=data.nodes.filter(n=>n.kind!=="company"&&n.status==="suspended"),archived=data.nodes.filter(n=>n.kind!=="company"&&n.status==="archived");
  return <div className="content-stack">
    <p className="muted">Create or change a business unit through an independent review. Active staff, roles, positions, child units, or warehouses must be moved before a unit can be suspended or archived. Relocating a department moves its current staff, positions and scoped assignments with it.</p>
    <details className={styles.position}><summary><strong>Propose a new branch or department</strong></summary><NodeChangeForm base={base} data={data}/></details>
    <div className={styles.unitGrid}>{[...active,...suspended,...archived].map(node=><section className="card" key={node.id}><h3>{node.display_name}</h3><p>{node.kind} · {node.status} · revision {node.hierarchy_revision}</p>{node.kind==="department"&&<p className="muted">{data.nodes.find(n=>n.id===node.branch_id)?.display_name??"Unavailable branch"}</p>}<details><summary>Propose a change</summary><NodeChangeForm key={node.hierarchy_revision} base={base} data={data} node={node}/></details></section>)}</div>
    <OrganizationChangeQueue base={base} data={data}/>
  </div>;
}
function NodeChangeForm({base,data,node,change}:{base:string;data:OrganizationOverview;node?:OrganizationNode;change?:OrganizationNodeChange}){
  const target=node??data.nodes.find(n=>n.id===change?.target_node_id),initialKind=change?.node_kind??(target?.kind==="department"?"department":"branch"),initialAction=change?.action??(target?(target.status==="active"?"rename":"reactivate"):"create");
  const {send,busy,error}=useStaffCommand(base),[kind,setKind]=useState<"branch"|"department">(initialKind as "branch"|"department"),[action,setAction]=useState<"create"|"rename"|"suspend"|"reactivate"|"archive"|"relocate">(initialAction);
  const branchId=action==="relocate"?(change?.branch_id??""):change?.branch_id??target?.branch_id??"";
  return <form className="business-form" onSubmit={async e=>{e.preventDefault();const form=e.currentTarget,f=new FormData(form,(e.nativeEvent as SubmitEvent).submitter),mode=f.get("mode")==="submit"?"submit":"draft";const saved=await send(`people/organization/changes/${change?`${change.id}/`:""}${mode}`,{action,targetNodeId:change?.target_node_id??target?.id??null,kind:target?(target.kind as "branch"|"department"):kind,slug:action==="create"?f.get("slug"):null,displayName:f.get("displayName"),branchId:target?(action==="relocate"?f.get("branchId"):branchId||null):(kind==="department"?f.get("branchId")||null:null),expectedNodeVersion:change?.expected_node_version??target?.hierarchy_revision??null,expectedVersion:change?.version??0,reason:f.get("reason")});if(saved&&!change){form.reset();setKind(initialKind as "branch"|"department");}}}>
    {!target&&<label>Unit type<select value={kind} onChange={e=>setKind(e.target.value as "branch"|"department")}><option value="branch">Branch</option><option value="department">Department</option></select></label>}
    {target&&<label>Change<select value={action} disabled={Boolean(change)} onChange={e=>setAction(e.target.value as typeof action)}><option value="rename">Rename</option>{target.status==="active"?<><option value="suspend">Suspend</option><option value="archive">Archive</option>{target.kind==="department"&&<option value="relocate">Relocate to another branch</option>}</>:<option value="reactivate">Reactivate</option>}</select></label>}
    {!target&&<label>Stable URL code<input name="slug" required pattern="[a-z0-9]+(?:-[a-z0-9]+)*" maxLength={80} placeholder="chiang-mai" defaultValue={change?.slug??""}/></label>}
    <label>Display name<input name="displayName" required minLength={2} maxLength={120} defaultValue={change?.display_name??target?.display_name}/></label>
    {(!target&&kind==="department"||target&&action==="relocate")&&<label>{action==="relocate"?"Destination branch":"Branch"}<select name="branchId" required defaultValue={branchId}><option value="">Choose an active branch</option>{data.nodes.filter(n=>n.kind==="branch"&&n.status==="active"&&n.id!==(action==="relocate"?target?.branch_id:null)).map(n=><option key={n.id} value={n.id}>{n.display_name}</option>)}</select></label>}
    <label>Business reason<input name="reason" required minLength={3} maxLength={1000} defaultValue={change?.reason}/></label>
    <div className={styles.formActions}><button className="button secondary" name="mode" value="draft" disabled={busy}>Save draft</button><button className="button" name="mode" value="submit" disabled={busy}>Submit for review</button></div>{error&&<p role="alert" className="error">{error}</p>}
  </form>;
}
function OrganizationChangeQueue({base,data}:{base:string;data:OrganizationOverview}){
  const {send,busy,error}=useStaffCommand(base),[reason,setReason]=useState<Record<string,string>>({});
  const queue=data.changeQueue;
  const pageLink=(page:number)=>`${base}/people/organization?${new URLSearchParams({changeState:queue.state,changeQuery:queue.query,changePage:String(page)})}#organization-change-queue`;
  const timestamp=(value:number)=>new Date(value).toISOString().replace("T"," ").slice(0,16)+" UTC";
  return <section id="organization-change-queue" className={styles.changeRequests} aria-label="Organization change requests">
    <h3>Organization change requests</h3>
    {queue.recordId&&<p>Showing the linked request. <Link href={`${base}/people/organization#organization-change-queue`}>View all open requests</Link></p>}
    <form className="business-form" action={`${base}/people/organization#organization-change-queue`}>
      <label>Change status<select name="changeState" defaultValue={queue.state} key={queue.state}>
        <option value="open">Open requests</option><option value="all">All requests</option>
        <option value="draft">Draft</option><option value="submitted">Awaiting review</option>
        <option value="rejected">Returned for correction</option><option value="applied">Applied</option><option value="cancelled">Cancelled</option>
      </select></label>
      <label>Find organization changes<input type="search" name="changeQuery" maxLength={120} defaultValue={queue.query} key={queue.query}/></label>
      <button className="button secondary">Filter changes</button>
    </form>
    {!data.nodeChanges.length&&<p className="muted">No organization changes match these filters.</p>}
    <div className={styles.changeQueue}>{data.nodeChanges.map(change=><article className={`card ${styles.changeRequests}`} id={`organization-change-${change.id}`} key={change.id} aria-label={`${change.action} ${change.node_kind}: ${change.display_name}`}>
      <strong>{change.action} {change.node_kind}: {change.display_name}</strong>
      <p>{change.state} · revision {change.version}</p><p>{change.reason}</p>
      {change.review_reason&&<p className="muted">Review: {change.review_reason}</p>}
      {change.state==="submitted"&&!change.canReview&&<p className="notice">Awaiting an independent Company HR administrator. You cannot review your own request.</p>}
      {change.canReview&&<>
        <label>Review reason<input value={reason[change.id]??""} onChange={e=>setReason(r=>({...r,[change.id]:e.target.value}))} minLength={3} maxLength={1000}/></label>
        <div className={styles.formActions}>
          <button className="button" disabled={busy||(reason[change.id]?.trim().length??0)<3} onClick={()=>void send(`people/organization/changes/${change.id}/review`,{expectedVersion:change.version,decision:"approve",reason:reason[change.id]})}>Approve and apply</button>
          <button className="button secondary" disabled={busy||(reason[change.id]?.trim().length??0)<3} onClick={()=>void send(`people/organization/changes/${change.id}/review`,{expectedVersion:change.version,decision:"reject",reason:reason[change.id]})}>Return for correction</button>
        </div>
      </>}
      {change.requested_by===data.actorId&&['draft','rejected'].includes(change.state)&&<details><summary>Correct and resubmit</summary><NodeChangeForm key={change.version} base={base} data={data} change={change}/></details>}
      {change.requested_by===data.actorId&&['draft','submitted','rejected'].includes(change.state)&&<button className="button secondary" disabled={busy} onClick={()=>void send(`people/organization/changes/${change.id}/cancel`,{expectedVersion:change.version})}>Cancel request</button>}
      <details><summary>Decision history</summary><ol>{change.events?.map(event=><li key={event.version}>
        <p>Revision {event.version} · {event.action} · {event.actor_name} · {timestamp(event.created_at)}</p>
        {event.reason&&<p>{event.reason}</p>}
      </li>)}</ol></details>
    </article>)}</div>
    <nav aria-label="Organization change pagination" className={styles.formActions}>
      <span>{queue.total} matching requests · Page {queue.page} of {queue.pages}</span>
      {queue.page>1&&<Link href={pageLink(queue.page-1)}>Previous changes</Link>}
      {queue.page<queue.pages&&<Link href={pageLink(queue.page+1)}>Next changes</Link>}
    </nav>
    {error&&<p role="alert" className="error">{error}</p>}
  </section>;
}
function InstallLevels({base}:{base:string}){const {send,busy,error}=useStaffCommand(base);return <section className="card"><h2>Set up six staff levels</h2><p>Install the numbered catalog, then explicitly assign each staff member. Existing levels and history are retained.</p><button className="button" disabled={busy} onClick={()=>void send("people/organization/levels",{})}>Install staff levels</button>{error&&<p role="alert">{error}</p>}</section>;}
function PositionEditor({base,data,position}:{base:string;data:OrganizationOverview;position?:OrganizationPosition}){
  const {send,busy,error}=useStaffCommand(base),[branch,setBranch]=useState(position?.branch_id??""),[ceo,setCeo]=useState(Boolean(position?.is_ceo));
  return <form className="business-form" onSubmit={async e=>{e.preventDefault();const f=new FormData(e.currentTarget);await send(`people/organization/positions${position?`/${position.id}`:""}`,{title:f.get("title"),branchId:branch||null,departmentId:f.get("department")||null,levelId:f.get("level"),reportsToPositionId:ceo?null:f.get("manager")||null,isCeo:ceo,active:f.get("active")==="on",expectedVersion:position?.version??0,requirements:f.get("requirements"),reason:f.get("reason")});}}>
    <label>Work requirements<textarea name="requirements" defaultValue={position?.requirements??""} maxLength={3000} placeholder="Accounting system, branch printer, laptop…"/></label>
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
    {template&&<form className="business-form" onSubmit={async e=>{e.preventDefault();const f=new FormData(e.currentTarget);await send(`people/organization/templates/${template.id}/apply`,{expectedVersion:template.version,branchId:branch||null,departmentId:f.get("department")||null,requirements:f.get("requirements"),reason:f.get("reason")});}}>
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
