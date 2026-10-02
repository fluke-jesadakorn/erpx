"use client";
import { useCallback, useEffect, useRef, useState, type DragEvent } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import dynamic from "next/dynamic";
import { ArrowLeft, ArrowRight, Redo2, Undo2 } from "lucide-react";
import { workFetch } from "@/lib/work-fetch";
import { registerUnsavedWork } from "@/lib/context-switch";
import { retirementInstant, retirementLocalTime } from "@/lib/retirement";
import { draftKey, emptyDraft, organizationDraftDiff, organizationPath, projectOrganization, changedPeople, type DraftDefinition, type DraftItem, type OrganizationDraft, type DraftRequest } from "@/lib/organization-draft";
import type { OrganizationAccessData } from "@/lib/organization-access";
import type { OrganizationDraftPreview } from "@/server/organization-drafts";
import { PeopleStructureWorkspace } from "./people-structure-workspace";
import { mergeLegacyMoveTray } from "@/lib/people-draft-import";
import { moveTrayKey, readMoveTray, writeMoveTray, type MoveTrayItem } from "./people-move-tray-state";
import type { PeopleOrganizationProps } from "./people-organization-workspace";
import styles from "./organization-draft-workspace.module.css";

const PeopleMoveWorkImpact = dynamic(() => import("./people-team-schedule").then(module => module.PeopleMoveWorkImpact));

type Source = Pick<DraftItem,"kind"|"id">;
type Target = { kind:"position"|"department"|"level"|"branch"; id:string };
const mime="application/erpx-organization-draft";
const titles={person:"Employee",position:"Position",department:"Department"};
type Props={base:string;companyName:string;organization:OrganizationAccessData;draftId?:string;review?:boolean;people?:PeopleOrganizationProps};
export function OrganizationDraftWorkspace({base,companyName,organization,draftId:providedDraftId,review=false,people}:Props) {
  const router=useRouter(), query=useSearchParams();
  review=review||query.get("panel")==="review";
  const draftId=providedDraftId??query.get("draft")??undefined;
  const entryKey=["kind","id","targetKind","targetId"].map(key=>query.get(key)??"").join(":");
  const [draft,setDraft]=useState<OrganizationDraft|null>(null), [definition,setDefinition]=useState<DraftDefinition>(emptyDraft);
  const [recent,setRecent]=useState<{id:string;updated_at:number;count:number}[]>([]);
  const [requests,setRequests]=useState<DraftRequest[]>([]), [status,setStatus]=useState("Loading draft…"), [error,setError]=useState("");
  const [loaded,setLoaded]=useState(false), [dirty,setDirty]=useState(false), [busy,setBusy]=useState(false);
  const [selected,setSelected]=useState<Source[]>([]), [dragging,setDragging]=useState<Source[]>([]), [over,setOver]=useState("");
  const draggingRef=useRef<Source[]>([]);
  const [preview,setPreview]=useState<OrganizationDraftPreview|null>(null);
  const [workImpactReady,setWorkImpactReady]=useState(false);
  const [impactRevision,setImpactRevision]=useState(0);
  const [undo,setUndo]=useState<DraftDefinition[]>([]), [redo,setRedo]=useState<DraftDefinition[]>([]);
  const draftRef=useRef(draft), definitionRef=useRef(definition), savedRef=useRef(JSON.stringify(definition)), savingRef=useRef<Promise<OrganizationDraft>|null>(null), submitKey=useRef<string|null>(null);
  const importedTray=useRef<MoveTrayItem[]>([]);
  const [legacyRemaining,setLegacyRemaining]=useState<MoveTrayItem[]>([]);
  const trayKey=people?moveTrayKey(base,people.currentCompanyId,people.actorId):null;
  const clearImportedTray=useCallback((snapshot:DraftDefinition)=> {
    if(!trayKey||!importedTray.current.length)return;
    try {
      const saved=importedTray.current.filter(item=>snapshot.items.some(i=>i.kind==='person'&&i.id===item.engagementId&&(item.positionId===null?i.parked:!i.parked&&i.positionId===item.positionId)));
      const remaining=readMoveTray(trayKey).filter(item=>!saved.some(old=>old.engagementId===item.engagementId&&old.positionId===item.positionId));
      writeMoveTray(trayKey,remaining);setLegacyRemaining(remaining);
      importedTray.current=importedTray.current.filter(item=>!saved.includes(item));
    }catch { /* The persisted draft is retained even if browser storage is unavailable. */ }
  },[trayKey]);
  const api=useCallback(async <T,>(suffix:string,body?:unknown,key?:string):Promise<T>=>{
    const response=await workFetch(`/api/v1${base}/people/organization/drafts${suffix}`,{method:body?"POST":"GET",headers:body?{"Content-Type":"application/json","Idempotency-Key":key??crypto.randomUUID()}:undefined,body:body?JSON.stringify(body):undefined});
    const result=await response.json(); if(!response.ok)throw new Error(result.error?.message??"Unable to save this draft."); return result.data as T;
  },[base]);
  useEffect(()=>{
    if(loaded&&draftRef.current&&(draftId===draftRef.current.id||!draftId&&draftRef.current.state==='draft')&&!query.get("kind"))return;
    if(loaded&&JSON.stringify(definitionRef.current)!==savedRef.current&&draftId&&draftRef.current?.id!==draftId){setError('Save the current draft before opening another draft.');return;}
    let alive=true;
    void api<{draft:OrganizationDraft|null;requests:DraftRequest[];recent:{id:string;updated_at:number;count:number}[]}>(draftId?`/${draftId}`:"").then(result=>{
      if(!alive)return;
      if(draftRef.current?.id!==result.draft?.id){setUndo([]);setRedo([]);}
      draftRef.current=result.draft; setDraft(result.draft); setRequests(result.requests);setRecent(result.recent); let initial=result.draft?.definition??emptyDraft();
      savedRef.current=JSON.stringify(initial);
      if(trayKey&&(!result.draft||result.draft.state==='draft'))try {
        const merged=mergeLegacyMoveTray(initial,readMoveTray(trayKey));
        initial=merged.definition;importedTray.current=merged.importedItems;setLegacyRemaining(merged.remainingItems);
      }catch { /* Keep any legacy browser draft when storage cannot be read. */ }
      const kind=query.get("kind"), id=query.get("id"), targetKind=query.get("targetKind"), targetId=query.get("targetId");
      if(id && (kind==="person"||kind==="position"||kind==="department") && (!result.draft||result.draft.state==='draft') && !initial.items.some(i=>i.kind===kind&&i.id===id) && initial.items.length<20) {
        const item:DraftItem={kind,id,parked:!targetId};
        if(targetId&&targetKind==='position'&&kind==='person')item.positionId=targetId;
        else if(targetId&&targetKind==='department'&&kind==='position')item.departmentId=targetId;
        else if(targetId&&targetKind==='level'&&kind==='position')item.levelId=targetId;
        else if(targetId&&targetKind==='branch'&&kind==='department')item.branchId=targetId;
        else item.parked=true;
        initial={...initial,items:[...initial.items,item]};
      }
      definitionRef.current=initial;setDefinition(initial);setLoaded(true);setDirty(JSON.stringify(initial)!==savedRef.current);setStatus(result.draft?"Draft saved":"Ready to explore");
      if(query.get("kind") && result.draft){
        const url=new URL(window.location.href);url.searchParams.set("draft",result.draft.id);
        if(kind&&id&&url.searchParams.get('panel')==='move')url.searchParams.set(kind,id);
        for(const key of ["kind","id","targetKind","targetId"])url.searchParams.delete(key);
        window.history.replaceState(null,"",url.pathname+url.search);
      }
    }).catch(e=>{if(alive){setError(e.message);setStatus("Unable to open draft");}});
    return()=>{alive=false;};
    // Entry parameters are consumed once; only the durable draft ID stays in the URL.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  },[api,draftId,review,entryKey]);
  const save=useCallback(async function saveCurrent():Promise<OrganizationDraft>{
    if(savingRef.current) { await savingRef.current; return saveCurrent(); }
    const current=draftRef.current;
    if(!current){
      const opening=api<{draft:OrganizationDraft}>("",{}).then(result=>{
        if(result.draft.definition.items.length)throw new Error("A draft was opened in another tab. Reload the saved draft before editing.");
        draftRef.current=result.draft;setDraft(result.draft);savedRef.current=JSON.stringify(result.draft.definition);
        const url=new URL(window.location.href);url.searchParams.set('draft',result.draft.id);const kind=url.searchParams.get('kind'),id=url.searchParams.get('id');if(url.searchParams.get('panel')==='move'&&id&&['person','position','department'].includes(kind??''))url.searchParams.set(kind!,id);for(const key of ['kind','id','targetKind','targetId'])url.searchParams.delete(key);
        window.history.replaceState(null,"",url.pathname+url.search);return result.draft;
      }).finally(()=>{savingRef.current=null;});
      savingRef.current=opening;try{await opening;}catch(e){setStatus("Draft not saved");setError((e as Error).message);throw e;}return saveCurrent();
    }
    if(current.state!=='draft'||JSON.stringify(definitionRef.current)===savedRef.current){if(current.state==='draft')clearImportedTray(current.definition);return current;}
    const snapshot=definitionRef.current; setStatus("Saving draft…");
    const promise=api<{draft:OrganizationDraft}>(`/${current.id}/save`,{expectedVersion:current.version,definition:snapshot}).then(result=>{
      draftRef.current=result.draft;setDraft(result.draft);savedRef.current=JSON.stringify(snapshot);setDirty(JSON.stringify(definitionRef.current)!==savedRef.current);setStatus("Draft saved");setError("");clearImportedTray(snapshot);return result.draft;
    }).catch(e=>{setStatus("Draft not saved");setError(e.message);throw e;}).finally(()=>{savingRef.current=null;});
    savingRef.current=promise;return promise;
  },[api,clearImportedTray]);
  useEffect(()=>{if(!loaded||!dirty||draft?.state&&draft.state!=='draft')return;const timer=setTimeout(()=>void save().catch(()=>{}),700);return()=>clearTimeout(timer);},[loaded,dirty,definition,draft?.state,save]);
  useEffect(()=>{const check=()=>JSON.stringify(definitionRef.current)!==savedRef.current;const unregister=registerUnsavedWork(check);const warn=(event:BeforeUnloadEvent)=>{if(check()){event.preventDefault();event.returnValue="";}};window.addEventListener("beforeunload",warn);return()=>{unregister();window.removeEventListener("beforeunload",warn);};},[]);
  useEffect(()=>{
    const guard=(event:MouseEvent)=>{const link=(event.target as Element).closest?.('a[href]') as HTMLAnchorElement|null;if(!link||event.metaKey||event.ctrlKey||event.button!==0||link.target==='_blank'||link.origin!==location.origin||JSON.stringify(definitionRef.current)===savedRef.current)return;event.preventDefault();event.stopPropagation();void save().then(()=>{if(JSON.stringify(definitionRef.current)===savedRef.current)router.push(link.pathname+link.search+link.hash);}).catch(()=>{});};
    document.addEventListener('click',guard,true);return()=>document.removeEventListener('click',guard,true);
  },[router,save]);
  function edit(next:DraftDefinition,history=true){
    const impacted=new Set(changedPeople(organization,projectOrganization(organization,{...next,managers:{},leave:{}})).map(person=>person.engagement_id));
    for(const item of next.items)if(item.kind==='person'&&!item.parked)impacted.add(item.id);
    next={...next,managers:Object.fromEntries(Object.entries(next.managers).filter(([id])=>impacted.has(id))),leave:Object.fromEntries(Object.entries(next.leave).filter(([id])=>impacted.has(id)))};
    if(history){const previous=definitionRef.current;setUndo(old=>[...old.slice(-49),previous]);setRedo([]);}definitionRef.current=next;setDefinition(next);setDirty(JSON.stringify(next)!==savedRef.current);setPreview(null);submitKey.current=null;setError("");}
  const graph=projectOrganization(organization,definition), diff=organizationDraftDiff(organization,definition), affected=changedPeople(organization,graph);
  const name=(source:Source)=>source.kind==='person'?organization.people.find(p=>p.engagement_id===source.id)?.legal_name??"Unavailable employee":source.kind==='position'?organization.positions.find(p=>p.id===source.id)?.title??"Unavailable position":organization.nodes.find(n=>n.id===source.id)?.display_name??"Unavailable department";
  function select(source:Source,multiple=false){setSelected(old=>multiple?old.some(i=>draftKey(i)===draftKey(source))?old.filter(i=>draftKey(i)!==draftKey(source)):[...old.filter(i=>i.kind===source.kind),source]:[source]);}
  function denial(sources:Source[],target:Target){
    if(!sources.length)return "Select an item first.";
    if(sources.some(s=>s.kind==='person'?target.kind!=='position':s.kind==='position'?!['department','level'].includes(target.kind):target.kind!=='branch'))return "This destination cannot receive the selected item type.";
    if(target.kind==='department'&&!graph.nodes.some(n=>n.id===target.id&&n.status==='active'&&graph.nodes.some(b=>b.id===n.branch_id&&b.kind==='branch'&&b.status==='active')))return "Choose a department in an active branch.";
    if(target.kind==='position'&&!graph.positions.some(p=>p.id===target.id&&p.active))return "Choose an active position in this Company.";
    if(target.kind==='branch'&&!graph.nodes.some(n=>n.id===target.id&&n.kind==='branch'&&n.status==='active'))return "Choose an active branch in this Company.";
    if(target.kind==='level'&&!graph.levels.some(l=>l.id===target.id&&l.level_number>=1&&l.level_number<=6))return "Choose one of the six active company Levels.";
    for(const source of sources){
      if(source.kind==='person'&&organization.people.find(p=>p.engagement_id===source.id)?.position_id===target.id)return "Already holds this position. Use Undo change to return to the source.";
      if(source.kind==='department'){
        const department=organization.nodes.find(n=>n.id===source.id);
        if(department?.branch_id===target.id)return "This department already belongs to this branch.";
        if(organization.nodes.some(n=>n.kind==='department'&&n.id!==source.id&&n.branch_id===target.id&&n.slug===department?.slug))return "A department with this URL code already exists in that branch.";
      }
    }
    return "";
  }
  function place(sources:Source[],target?:Target){
    if(!sources.length)return;
    if(sources.some(s=>s.kind==='person'?!organization.people.some(p=>p.engagement_id===s.id):s.kind==='position'?!organization.positions.some(p=>p.id===s.id):!organization.nodes.some(n=>n.id===s.id&&n.kind==='department'))){setError("Choose a current item in this Company.");return;}
    if(target&&denial(sources,target)){setError(denial(sources,target));return;}
    let items=[...definitionRef.current.items];
    for(const source of sources){
      const old=items.find(i=>draftKey(i)===draftKey(source));const item:DraftItem={...old,...source,parked:!target};
      if(target?.kind==='position')item.positionId=target.id;
      if(target?.kind==='department')item.departmentId=target.id;
      if(target?.kind==='level')item.levelId=target.id;
      if(target?.kind==='branch')item.branchId=target.id;
      items=items.filter(i=>draftKey(i)!==draftKey(source));items.push(item);
    }
    if(items.length>20){setError("A draft can contain up to 20 directly arranged items.");return;}
    edit({...definitionRef.current,items});setSelected([]);setDragging([]);setOver("");
  }
  function restore(sources:Source[]){const keys=new Set(sources.map(draftKey));edit({...definitionRef.current,items:definitionRef.current.items.filter(i=>!keys.has(draftKey(i)))});setSelected([]);setDragging([]);setOver("");}
  function start(event:DragEvent,source:Source){event.stopPropagation();const sources=selected.some(i=>draftKey(i)===draftKey(source))?selected:[source];draggingRef.current=sources;setDragging(sources);event.dataTransfer.effectAllowed='move';event.dataTransfer.setData(mime,JSON.stringify(sources));}
  function receive(event:DragEvent,target?:Target,returnToSource=false){event.preventDefault();event.stopPropagation();try{const sources=JSON.parse(event.dataTransfer.getData(mime)) as Source[];if(!Array.isArray(sources)||!sources.every(s=>['person','position','department'].includes(s.kind)&&typeof s.id==='string'))return;if(returnToSource)restore(sources);else place(sources,target);}catch{setError("Select an item and use Move here.");}setOver("");}
  function dragOver(event:DragEvent,key:string,target?:Target){event.stopPropagation();const sources=draggingRef.current;if(!sources.length||target&&denial(sources,target))return;event.preventDefault();event.dataTransfer.dropEffect='move';setOver(key);}
  function endDrag(){draggingRef.current=[];setDragging([]);setOver("");}
  async function navigate(url:string){setBusy(true);try{const saved=await save();if(JSON.stringify(definitionRef.current)!==savedRef.current)await save();router.push(url.replace('{id}',saved.id));}catch{}finally{setBusy(false);}}
  async function check(){setBusy(true);setError("");try{let saved=await save();if(JSON.stringify(definitionRef.current)!==savedRef.current)saved=await save();const checked=await api<OrganizationDraftPreview>(`/${saved.id}/preview`,{expectedVersion:saved.version});if(draftRef.current?.version===saved.version&&JSON.stringify(definitionRef.current)===savedRef.current){setWorkImpactReady(false);setImpactRevision(revision=>revision+1);setPreview(checked);}}catch(e){setError((e as Error).message);}finally{setBusy(false);}}
  async function confirm(){if(!preview?.ready||!workImpactReady||!draft||dirty||preview.version!==draft.version)return;setBusy(true);try{submitKey.current??=crypto.randomUUID();await api(`/${draft.id}/submit`,{expectedVersion:draft.version,previewToken:preview.token},submitKey.current);const result=await api<{draft:OrganizationDraft;requests:DraftRequest[]}>(`/${draft.id}`);draftRef.current=result.draft;setDraft(result.draft);setRequests(result.requests);setPreview(null);setStatus("Changes submitted");}catch(e){setError((e as Error).message);setPreview(null);}finally{setBusy(false);}}
  async function discard(){if(!draft)return;setBusy(true);try{await api(`/${draft.id}/discard`,{expectedVersion:draft.version,definition:definitionRef.current});const next=emptyDraft();draftRef.current=null;setDraft(null);definitionRef.current=next;setDefinition(next);savedRef.current=JSON.stringify(next);setDirty(false);setUndo([]);setRedo([]);setPreview(null);setStatus('Ready to explore');const url=new URL(window.location.href);url.searchParams.delete('draft');url.searchParams.delete('panel');window.history.replaceState(null,"",url.pathname+url.search);}catch(e){setError((e as Error).message);}finally{setBusy(false);}}
  if(draft?.state==='discarded')return <section className={styles.workspace}><h2>This draft was discarded</h2><button type="button" onClick={()=>router.push(`${base}/people`)}>Back to People</button></section>;
  if(!loaded)return <section className={styles.workspace}><p role="status">{status}</p>{error&&<p role="alert">{error}</p>}</section>;
  const toolbar=<header className={styles.toolbar}><div><span className={styles.eyebrow}>PEOPLE & ORGANIZATION</span><h2>{review?'Review changes':'Organization workspace'} · {companyName}</h2><p role="status" aria-live="polite"><span className={dirty?styles.unsaved:styles.saved}/>{dirty&&status==='Draft saved'?'Unsaved changes':status}</p></div><div className={styles.actions}>{review&&<button type="button" onClick={()=>void navigate(`${base}/people`)} disabled={busy}><ArrowLeft size={16}/>People</button>}{(!draft||draft.state==='draft')&&<><button type="button" onClick={()=>void save().catch(()=>{})} disabled={busy||!dirty}>Save draft</button>{!review&&<button type="button" className={styles.primary} disabled={busy||!definition.items.length||definition.items.some(i=>i.parked)} onClick={()=>void navigate(`${base}/people/organization/drafts/{id}/review`)}>Review changes · {definition.items.length}<ArrowRight size={16}/></button>}</>}</div></header>;
  const history=recent.length>0&&<details className={styles.submissions}><summary>Previous submissions</summary>{recent.map(item=><button key={item.id} type="button" onClick={()=>void navigate(`${base}/people/organization/drafts/${item.id}/review`)}>{new Date(item.updated_at).toLocaleDateString()} · {item.count} changes</button>)}</details>;
  const alert=error&&<div className={styles.error} role="alert">{error}<button type="button" onClick={()=>window.location.reload()}>Reload saved draft</button></div>;
  if(draft?.state==='submitted')return <section className={styles.workspace}>{toolbar}{alert}<h3>Submitted changes</h3><p>Each item follows its approval and effective date. IT preparation is tracked separately.</p>{requests.map(request=><article className={styles.result} key={request.id}><strong>{request.item_keys.map(k=>{const item=definition.items.find(i=>draftKey(i)===k);return item?name(item):k;}).join(', ')}</strong><span>{request.state.replaceAll('_',' ')}</span>{request.last_error&&<p>{request.last_error}</p>}</article>)}<button type="button" onClick={async()=>{const result=await api<{draft:OrganizationDraft;requests:DraftRequest[]}>(`/${draft.id}`);setRequests(result.requests);}}>Refresh status</button>{requests.some(r=>r.state==='needs_review')&&<button type="button" onClick={async()=>{try{const result=await api<{draft:OrganizationDraft}>(`/${draft.id}/continue`,{});router.push(`${base}/people?draft=${result.draft.id}`);}catch(e){setError((e as Error).message);}}}>Continue unresolved changes</button>}</section>;
  if(review)return <section className={styles.workspace} data-testid="organization-draft-review">{toolbar}{alert}<h3>{definition.items.length} direct changes · {affected.length} employees affected</h3><p>Approved salaries stay unchanged. Department moves require independent approval; dependent changes wait for that approval.</p><div className={styles.diff}>{(preview?.diff??diff).map(row=><article key={row.key}><div><strong>{row.label}</strong><small>{titles[row.kind]} · {row.affected.length} employees</small></div><div><span>Before</span><p>{row.before}</p></div><div><span>After</span><p>{row.after}</p></div>{row.kind==='position'&&<label>Reporting position<select value={definition.items.find(i=>draftKey(i)===row.key)?.reportsToPositionId??organization.positions.find(p=>p.id===row.id)?.reports_to_position_id??''} onChange={e=>edit({...definition,items:definition.items.map(i=>draftKey(i)===row.key?{...i,reportsToPositionId:e.target.value||null}:i)})}><option value="">No reporting position</option>{graph.positions.filter(p=>p.id!==row.id).map(p=><option key={p.id} value={p.id}>{p.title}</option>)}</select></label>}{row.blockers.map(message=><p className={styles.error} key={message}>{message}</p>)}</article>)}</div>
    <h3>Employee impact</h3><PeopleMoveWorkImpact key={`${draft?.version}:${preview?.token??""}:${impactRevision}`} base={base} engagementIds={affected.map(p=>p.engagement_id)} onReadyChange={setWorkImpactReady}/>{affected.map(person=>{const before=organization.people.find(p=>p.engagement_id===person.engagement_id)!;const legacy=preview?.legacy.find(l=>l.engagementId===person.engagement_id);const explicit=definition.items.some(i=>i.kind==='person'&&i.id===person.engagement_id);return <article key={person.engagement_id} className={styles.impact}><div><strong>{person.legal_name}</strong><small>{explicit?'Direct move':'Moves with structure'}</small><p>{organizationPath(organization,'person',person.engagement_id)} → {organizationPath(graph,'person',person.engagement_id)}</p><small>Manager: {organization.people.find(p=>p.person_id===before.manager_person_id)?.legal_name??'None'} → {graph.people.find(p=>p.person_id===person.manager_person_id)?.legal_name??'None'}</small></div><label>Reporting manager<select value={person.manager_person_id??''} onChange={e=>edit({...definition,managers:{...definition.managers,[person.engagement_id]:e.target.value||null}})}><option value="">No manager</option>{graph.people.filter(p=>p.person_id!==person.person_id&&p.status==='active').map(p=><option key={p.engagement_id} value={p.person_id}>{p.legal_name}</option>)}</select></label><label><input type="checkbox" checked={definition.leave[person.engagement_id]??false} onChange={e=>edit({...definition,leave:{...definition.leave,[person.engagement_id]:e.target.checked}})}/>Apply Level leave default</label>{preview?.leaveImpact.find(l=>l.engagementId===person.engagement_id)?.note}{legacy&&<label><input type="checkbox" checked={definition.items.find(i=>i.id===person.engagement_id)?.supersedeLegacyCase?.id===legacy.id} onChange={e=>edit({...definition,items:definition.items.map(i=>i.kind==='person'&&i.id===person.engagement_id?{...i,supersedeLegacyCase:e.target.checked?{id:legacy.id,expectedVersion:legacy.version}:undefined}:i)})}/>Replace older {legacy.status.replaceAll('_',' ')} request; preview again</label>}</article>;})}
    <div className={styles.settings}><label>Timezone<select value={definition.timezone} onChange={e=>edit({...definition,timezone:e.target.value})}>{[...new Set([definition.timezone,'Asia/Bangkok','UTC','Asia/Singapore'])].map(zone=><option key={zone}>{zone}</option>)}</select></label><label>Effective date and time<input type="datetime-local" value={retirementLocalTime(definition.effectiveAt,definition.timezone)} onChange={e=>{if(e.target.value)edit({...definition,effectiveAt:retirementInstant(e.target.value,definition.timezone)});}}/></label><label className={styles.reason}>Reason<textarea minLength={3} maxLength={1000} value={definition.reason} onChange={e=>edit({...definition,reason:e.target.value})}/></label></div>{!!preview?.staleSources.length&&<button type="button" onClick={async()=>{try{await api(`/${draft!.id}/rebase`,{expectedVersion:draft!.version,definition});savedRef.current=JSON.stringify(definition);window.location.reload();}catch(e){setError((e as Error).message);}}}>Use current structure and review again</button>}{preview&&<p>IT policy assignments: {preview.policyImpact.added} added · {preview.policyImpact.removed} removed · {preview.policyImpact.retained} retained. Delivery follows the HR change.</p>}{preview&&<div className={preview.ready?styles.ready:styles.error} role="status">{preview.ready?'All changes are ready to submit.':preview.blockers.map(b=><p key={b}>{b}</p>)}</div>}<footer className={styles.footer}><button type="button" onClick={()=>{const url=new URL(window.location.href);url.searchParams.delete('panel');url.searchParams.set('draft','{id}');void navigate(url.pathname+url.search.replace('%7Bid%7D','{id}'));}}>Back to arrange</button><button type="button" disabled={busy} onClick={()=>void check()}>Preview all changes</button><button className={styles.primary} type="button" disabled={busy||dirty||!preview?.ready||!workImpactReady||preview.version!==draft?.version} onClick={()=>void confirm()}>Confirm and submit all</button></footer></section>;
  const changeBar=!!definition.items.length&&<footer className={styles.changeBar}><strong>{definition.items.length} changes</strong><span role="status" aria-live="polite">{dirty&&status==='Draft saved'?'Unsaved changes':status}</span><div className={styles.actions}>
      <button type="button" aria-label="Undo" disabled={!undo.length} onClick={()=>{const prior=undo.at(-1)!;setUndo(undo.slice(0,-1));setRedo([...redo,definition]);edit(prior,false);}}><Undo2 size={17}/></button>
      <button type="button" aria-label="Redo" disabled={!redo.length} onClick={()=>{const next=redo.at(-1)!;setRedo(redo.slice(0,-1));setUndo([...undo,definition]);edit(next,false);}}><Redo2 size={17}/></button>
      <button type="button" onClick={()=>void save().catch(()=>{})} disabled={busy||!dirty}>Save draft</button>
      <button type="button" className={styles.primary} disabled={busy||definition.items.some(i=>i.parked)} onClick={()=>{const url=new URL(window.location.href);url.searchParams.set('panel','review');url.searchParams.set('draft','{id}');void navigate(url.pathname+url.search.replace('%7Bid%7D','{id}'));}}>Review changes · {definition.items.length}<ArrowRight size={16}/></button>
      {draft&&<button type="button" disabled={busy} onClick={()=>void discard()}>Discard draft</button>}
    </div></footer>;
  const extraChanges=legacyRemaining.length>0&&<details className={styles.submissions}><summary>{legacyRemaining.length} older pending moves need attention</summary><p>Saved destinations were preserved where they conflict with this draft or exceed its 20-item limit.</p>{legacyRemaining.map(item=><div key={item.engagementId}><strong>{name({kind:'person',id:item.engagementId})}</strong><span> · {item.positionId?organization.positions.find(p=>p.id===item.positionId)?.title??'Unavailable position':'Destination not chosen'}</span><button type="button" onClick={()=>{if(!definition.items.some(i=>i.kind==='person'&&i.id===item.engagementId)&&definition.items.length>=20){setError('Remove a draft item before importing another move.');return;}const target=item.positionId?{kind:'position' as const,id:item.positionId}:undefined;if(target&&denial([{kind:'person',id:item.engagementId}],target)){setError(denial([{kind:'person',id:item.engagementId}],target));return;}importedTray.current=[...importedTray.current.filter(old=>old.engagementId!==item.engagementId),item];place([{kind:'person',id:item.engagementId}],target);}}>Use saved destination</button></div>)}</details>;
  return <section className={styles.workspace} data-testid="organization-draft-workspace" data-panel-workspace>{alert}{history}
    {people&&<PeopleStructureWorkspace people={people} definition={definition} changeBar={changeBar} extraChanges={extraChanges} controls={{selected,dragging,over,select,place,restore,start,end:endDrag,receive,dragOver,denial}}/>}
    {!definition.items.length&&redo.length>0&&<div className={styles.tools}><button type="button" aria-label="Redo" onClick={()=>{const next=redo.at(-1)!;setRedo(redo.slice(0,-1));setUndo([...undo,definition]);edit(next,false);}}><Redo2 size={17}/>Redo</button></div>}
  </section>;
}
