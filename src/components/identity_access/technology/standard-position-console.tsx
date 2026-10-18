"use client";
import {useEffect,useLayoutEffect,useRef,useState,useSyncExternalStore,type FormEvent} from 'react';
import {BriefcaseBusiness,ArrowRight} from 'lucide-react';
import {Button,Field,Select,TextInput,FormLayout,FormActions,Badge,InlineNotice} from '@/components/ui/foundation';
import {DraftFormDrawer} from '@/components/ui/draft-form-drawer';
import {RecordLink as Link} from '@/components/records/record-link';
import {TimeText} from '@/components/people_payroll/time/time-preference-context';
import {positionGpoStatus} from '@/components/people_payroll/gpo-status';
import {canonicalUiHref} from '@/lib/work_projection/destination-registry';
import {useRouter} from '@/lib/shared/destination-router';
import {registerUnsavedWork} from '@/lib/work_projection/context-switch';
import {createPositionBindingCommands,positionBindingCanPrepare,positionBindingCanPreview,type PositionBindingSource,type PositionBindingInput,type PositionBindingPreview,type PositionBindingOutcome,type PositionBindingCommandStore} from '@/lib/identity_access/position-binding-command';
import type {PeopleLifecycleData,AccessPackageDefinition} from '@/lib/people_payroll/people-lifecycle';
import type {WorkCommandCompletion} from '@/lib/work_projection/work-fetch';
import styles from './standard-position-console.module.css';

type Position=PeopleLifecycleData['positions'][number];
type Readiness='all'|'unbound'|'pending'|'prepared';
function stateFor(position:Position,data:PeopleLifecycleData):Exclude<Readiness,'all'>{return !position.package_id?'unbound':data.tasks.some(task=>task.kind==='position'&&task.position_id===position.id&&task.status==='completed')?'prepared':'pending';}
const stateLabel={unbound:'No package',pending:'In preparation',prepared:'Prepared'};
function Recipe({definition}:{definition:AccessPackageDefinition}){
 return <div className={styles.recipe}>
  <dl className={styles.facts}><div><dt>Account</dt><dd>{definition.account==='local'?'Local ERPX account':definition.account==='univention'?'UCS account':'Active Directory account'}</dd></div><div><dt>Directory connection</dt><dd>{definition.connectionId??'Not required'}</dd></div></dl>
  <section><h4>ERPX roles · {definition.grants.length}</h4>{definition.grants.length?<ul>{definition.grants.map((grant,index)=><li key={`${grant.roleId}-${index}`}><strong>{grant.roleId}</strong> · {grant.scopeKind==='holding'?'Company':grant.scopeKind} · until <TimeText value={grant.validUntil}/></li>)}</ul>:<p>No ERPX role grants in this recipe.</p>}</section>
  <section><h4>ERPX groups · {definition.groupIds.length}</h4><p>{definition.groupIds.join(', ')||'No ERPX groups.'}</p></section>
  <section><h4>Directory groups · {definition.directoryGroupIds.length}</h4><p>{definition.directoryGroupIds.join(', ')||'No Directory groups.'}</p></section>
  <section><h4>Windows policies · {definition.windowsPolicies.length}</h4>{definition.windowsPolicies.length?<ul>{definition.windowsPolicies.map(policy=><li key={policy.id}><strong>{policy.name}</strong> · {policy.processor==='admx'?'Administrative policy':'Drive / printer preference'} · {policy.scope} · OU: {policy.targetOu}{policy.securityGroup?` · Security group: ${policy.securityGroup}`:''}{policy.computerTarget?` · Computers: ${policy.computerTarget}`:''} · IT verification required</li>)}</ul>:<p>No Windows policies.</p>}</section>
  <section><h4>Preparation tasks · {definition.tasks.length}</h4>{definition.tasks.length?<ul>{definition.tasks.map((task,index)=><li key={`${task.kind}-${index}`}>{task.label} · {task.kind}</li>)}</ul>:<p>No additional preparation tasks.</p>}</section>
 </div>;
}

/** Standard owner presentation. The protected legacy PositionConsole stays exact. */
export function StandardPositionConsole({base,data,ownerSnapshot,positionId,commandStore}:{base:string;data:PeopleLifecycleData;ownerSnapshot:PeopleLifecycleData;positionId?:string;commandStore:PositionBindingCommandStore}){
 const router=useRouter();
 const [selectedId,setSelectedId]=useState<string|null>(positionId??data.positions.find(position=>!!position.active)?.id??null);
 const [search,setSearch]=useState(''),[branchId,setBranchId]=useState(''),[readiness,setReadiness]=useState<Readiness>('all');
 const [choice,setChoice]=useState<{positionId:string;packageId:string}|null>(null),[preview,setPreview]=useState<PositionBindingPreview|null>(null),[draft,setDraft]=useState(0);
 const [editorOpen,setEditorOpen]=useState(false),[busy,setBusy]=useState(false),[error,setError]=useState(''),[notice,setNotice]=useState('');
 const pending=useSyncExternalStore(commandStore.subscribe,commandStore.pending,()=>null);
 const currentData=data,active=currentData.positions.filter(position=>!!position.active),nodes=new Map(currentData.nodes.map(node=>[node.id,node.name]));
 const normalized=search.trim().toLocaleLowerCase(),matching=active.filter(position=>(!branchId||position.branch_id===branchId)&&(readiness==='all'||stateFor(position,currentData)===readiness)&&`${position.title} ${nodes.get(position.branch_id??'')??'Company'} ${nodes.get(position.department_id??'')??'All departments'}`.toLocaleLowerCase().includes(normalized));
 const position=matching.find(row=>row.id===selectedId)??matching[0]??null,packages=currentData.packages.filter(pkg=>pkg.published_revision>0),packageId=position&&choice?.positionId===position.id?choice.packageId:position?.package_id??'',chosen=packages.find(pkg=>pkg.id===packageId);
 const selectedPositionId=position?.id??null,selectedPackageId=chosen?.id??null,selectedRevision=chosen?.published_revision??0;
 const binding:PositionBindingInput|null=position&&chosen?{positionId:position.id,expectedVersion:position.version,packageId:chosen.id,revision:chosen.published_revision}:null;
 const source=JSON.stringify([base,currentData,position?.id??null,packageId]),current=useRef<PositionBindingSource|null>(null),commands=useRef<ReturnType<typeof createPositionBindingCommands>|null>(null);
 useLayoutEffect(()=>{current.current={base,source,draft,data:currentData,ownerSnapshot,selection:selectedPositionId&&selectedPackageId?{positionId:selectedPositionId,packageId:selectedPackageId,revision:selectedRevision}:null};commands.current??=createPositionBindingCommands(()=>current.current,undefined,commandStore);return()=>{current.current=null;};},[base,source,draft,currentData,ownerSnapshot,commandStore,selectedPositionId,selectedPackageId,selectedRevision]);
 const dirty=!!pending||!!preview||!!position&&!!choice&&choice.positionId===position.id&&choice.packageId!==(position.package_id??'');
 useEffect(()=>registerUnsavedWork(()=>dirty),[dirty]);
 const validPreview=preview&&binding&&preview.source===source&&preview.draft===draft&&preview.input.positionId===binding.positionId&&preview.input.expectedVersion===binding.expectedVersion&&preview.input.packageId===binding.packageId&&preview.input.revision===binding.revision?preview:null;
 const canPrepare=positionBindingCanPrepare(currentData),canPreview=positionBindingCanPreview(currentData),occupants=currentData.roster.filter(person=>person.positionId===position?.id),task=currentData.tasks.find(row=>row.kind==='position'&&row.position_id===position?.id),assignedPackage=currentData.packages.find(pkg=>pkg.id===position?.package_id);
 const branches=[...new Set(active.map(row=>row.branch_id).filter((id):id is string=>!!id))].map(id=>({id,name:nodes.get(id)??'Branch'})).sort((a,b)=>a.name.localeCompare(b.name));
 const counts={prepared:active.filter(row=>stateFor(row,currentData)==='prepared').length,pending:active.filter(row=>stateFor(row,currentData)==='pending').length,unbound:active.filter(row=>stateFor(row,currentData)==='unbound').length};
 function select(id:string){setSelectedId(id);setPreview(null);setDraft(value=>value+1);setError('');}
 function completion(form:HTMLFormElement):WorkCommandCompletion{return {form,formEditVersion:form.getAttribute('data-erpx-edit-version')};}
 function isCurrent(invoked:PositionBindingSource){const now=current.current;return !!now&&now.base===invoked.base&&now.source===invoked.source&&now.ownerSnapshot===invoked.ownerSnapshot;}
 function confirmed(outcome:Extract<PositionBindingOutcome,{kind:'bind'}>,invoked:PositionBindingSource){
  if(!isCurrent(invoked))return;setError('');
  const target=outcome.data.positions.find(row=>row.id===outcome.receipt.id)?.title??'Original position';
  setNotice(outcome.changedSinceAssignment?`${target}: the original assignment is confirmed. It has a later change; current history is shown.`:`${target}: published package assigned. IT applies employee access separately; role reviews remain independent.`);
  if(outcome.unchanged&&current.current?.draft===invoked.draft){setPreview(null);setChoice(null);setEditorOpen(false);}
 }
 // A fresh SSR prop is the existing Admin owner read; no generic People GET.
 // The helper keeps the native receipt until that exact source confirms it.
 useEffect(()=>{
  if(!pending?.acknowledged||pending.running)return;
  const invoked=current.current,command=commands.current;if(!invoked||!command)return;
  void command.confirm().then(outcome=>{if(outcome)confirmed(outcome,invoked);}).catch(cause=>{if(isCurrent(invoked))setError(cause instanceof Error?cause.message:'The refreshed position has not confirmed the assignment.');});
 });
 async function run(effect:()=>Promise<PositionBindingOutcome>){
  const invoked=current.current;if(!invoked||!commands.current)return;setBusy(true);setError('');setNotice('');
  try{const outcome=await effect();if(!isCurrent(invoked))return;
   if(outcome.kind==='preview'){if(outcome.unchanged)setPreview(outcome.preview);else setNotice('The original preview was recovered. Review the current position and newer draft before previewing again.');}
   else if(outcome.kind==='bind-awaiting-source')router.refresh();
   else confirmed(outcome,invoked);
  }catch(cause){if(isCurrent(invoked))setError(cause instanceof Error?cause.message:'Unable to prepare this position.');}
  finally{if(current.current)setBusy(false);}
 }
 function submit(event:FormEvent<HTMLFormElement>){event.preventDefault();const command=commands.current;if(!command||!binding)return;const captured=completion(event.currentTarget);void run(()=>validPreview?command.bind(validPreview,captured):command.preview(binding,captured));}
 function discard(){if(pending)return;setChoice(null);setPreview(null);setEditorOpen(false);setDraft(value=>value+1);}
 const recovery=pending&&<InlineNotice tone="warning"><div><strong>{pending.acknowledged?'Assignment acknowledged; current history still needs confirmation.':'The original position outcome is not confirmed.'}</strong><p>{currentData.positions.find(row=>row.id===pending.positionId)?.title??'Original position'} · {pending.kind==='preview'?'Preview':'Package assignment'}. Recover the same request before preparing another; newer fields are retained.</p><Button appearance="standard" variant="secondary" type="button" disabled={busy||pending.running} onClick={()=>{const command=commands.current;if(command)void run(()=>command.retry());}}>{pending.acknowledged?'Refresh acknowledged assignment':'Recover original request'}</Button></div></InlineNotice>;
 return <section className={styles.workspace} data-appearance="standard" aria-label="Position access preparation">
  <header className={styles.heading}><div><h2>Position readiness</h2><p>{active.length} positions · {counts.prepared} prepared · {counts.pending} in preparation · {counts.unbound} without a package</p></div><Link href={canonicalUiHref(`${base}/work?type=people_lifecycle`)}>Open IT queue <ArrowRight size={16}/></Link></header>
  {recovery}{error&&<InlineNotice tone="danger">{error}</InlineNotice>}{notice&&<InlineNotice tone="success">{notice}</InlineNotice>}
  <div className={styles.layout}>
   <aside className={styles.list} aria-label="Positions"><div className={styles.filters}><Field appearance="standard">Search positions<TextInput appearance="standard" type="search" value={search} onChange={event=>setSearch(event.target.value)} placeholder="Title, branch or department"/></Field><Field appearance="standard">Branch<Select appearance="standard" value={branchId} onChange={event=>setBranchId(event.target.value)}><option value="">All branches</option>{branches.map(branch=><option key={branch.id} value={branch.id}>{branch.name}</option>)}</Select></Field><Field appearance="standard">Readiness<Select appearance="standard" value={readiness} onChange={event=>setReadiness(event.target.value as Readiness)}><option value="all">All positions</option><option value="unbound">No package</option><option value="pending">In preparation</option><option value="prepared">Prepared</option></Select></Field><p role="status">{matching.length} matching positions</p></div>
    {matching.length?<ul>{matching.map(row=><li key={row.id}><Button appearance="standard" type="button" variant="plain" aria-pressed={row.id===position?.id} className={styles.position} onClick={()=>select(row.id)}><BriefcaseBusiness size={18}/><span><strong>{row.title}</strong><small>{nodes.get(row.branch_id??'')??'Company'} · {nodes.get(row.department_id??'')??'All departments'}</small><Badge tone={stateFor(row,currentData)==='prepared'?'success':'neutral'}>{stateLabel[stateFor(row,currentData)]}</Badge></span></Button></li>)}</ul>:<div className={styles.empty}><p>No positions match these filters.</p><Button appearance="standard" type="button" variant="secondary" onClick={()=>{setSearch('');setBranchId('');setReadiness('all');}}>Clear filters</Button></div>}
   </aside>
   <section className={styles.detail} aria-label="Position details">{position?<>
    <header className={styles.detailHeading}><div><h3>{position.title}</h3><p>{nodes.get(position.branch_id??'')??'Company'} · {nodes.get(position.department_id??'')??'All departments'}</p></div><Badge tone={stateFor(position,currentData)==='prepared'?'success':'warning'}>{stateLabel[stateFor(position,currentData)]}</Badge></header>
    <section className={styles.block}><h4>Position and placement</h4><p>{position.requirements||'No additional work requirements recorded.'}</p><p>{positionGpoStatus(position.gpo)}</p><div className={styles.actions}>{currentData.canManage&&<Link href={canonicalUiHref(`${base}/people?new=person&position=${encodeURIComponent(position.id)}`)}>Onboard into this position</Link>}{currentData.canPrepare&&<Link href={canonicalUiHref(`${base}/technology?view=organization&position=${encodeURIComponent(position.id)}`)}>View Directory &amp; GPO</Link>}</div></section>
    <section className={styles.block}><h4>Access recipe</h4><p>{assignedPackage?`${assignedPackage.definition.name} · revision ${position.package_revision??assignedPackage.published_revision}`:'No published package is assigned.'}</p>{assignedPackage&&<p className={styles.muted}>Preview the published revision to inspect its exact recipe and affected people.</p>}<div className={styles.actions}>{canPreview&&<Button appearance="standard" type="button" variant="primary" disabled={busy||!!pending||!packages.length} onClick={()=>{setEditorOpen(true);setError('');}}>Review package assignment</Button>}<Link href={canonicalUiHref(`${base}/technology?view=resources&section=packages`)}>{packages.length?'Manage packages':'Create an access package'}</Link></div>{!canPreview?<p className={styles.muted}>Read-only preparation history. Use a current authorized IT role to assign a package.</p>:!canPrepare&&<p className={styles.muted}>Package assignment is paused. You can preview published recipes and recover an original request.</p>}</section>
    <section className={styles.block}><h4>People in this position</h4>{occupants.length?<ul>{occupants.map(person=><li key={person.engagementId}><Link href={canonicalUiHref(`${base}/administration/technology/organization/person/${encodeURIComponent(person.id)}`)}>{person.name}</Link> · {person.status}{person.itStatus?` · IT ${person.itStatus}`:''}</li>)}</ul>:<p>No current occupants.</p>}</section>
    <section className={styles.block}><h4>IT preparation</h4><p>{task?.title??(position.package_id?'Waiting for the preparation task.':'Starts when a published package is assigned.')}</p><p>{task?`${task.status.replaceAll('_',' ')} · ${task.assignee_id?'Assigned to IT owner':'Unassigned IT queue'}`:'No preparation task yet.'}</p><Link href={canonicalUiHref(`${base}/work?type=people_lifecycle`)}>Follow preparation in Work</Link></section>
   </>:<div className={styles.empty}><h3>Select a position</h3><p>Review its published recipe, people, Directory readiness and IT preparation.</p></div>}</section>
  </div>
  <DraftFormDrawer appearance="standard" open={editorOpen} title="Assign a published access package" dirty={dirty} onClose={()=>setEditorOpen(false)} onDiscard={discard} dismissDisabled={busy||!!pending} retainFormContents>
   <FormLayout layout="stack" className={styles.editor} onSubmit={submit} data-erpx-edit-version={String(draft)} data-work-dirty={dirty?'true':undefined}>
    <p>Review the original position and affected people before assigning. This changes the position recipe; employee access is applied separately and role grants retain independent review.</p>
    <dl className={styles.facts}><div><dt>Position</dt><dd>{position?.title??'Select a current position'}</dd></div><div><dt>Current version</dt><dd>{position?.version??'Unavailable'}</dd></div></dl>
    <Field appearance="standard">Published package<Select appearance="standard" required value={packageId} onChange={event=>{if(position)setChoice({positionId:position.id,packageId:event.target.value});setPreview(null);setDraft(value=>value+1);setError('');}}><option value="">Choose a published package</option>{packages.map(pkg=><option key={pkg.id} value={pkg.id}>{pkg.definition.name} · revision {pkg.published_revision}</option>)}</Select></Field>
    {validPreview&&<section className={styles.review}><h3>Preview before assigning</h3><p><strong>{validPreview.definition.name}</strong> · revision {validPreview.input.revision}</p><Recipe definition={validPreview.definition}/><h4>Affected people · {validPreview.people.length}</h4>{validPreview.people.length?<ul>{validPreview.people.map((name,index)=><li key={`${name}-${index}`}>{name}</li>)}</ul>:<p>This position has no current occupants.</p>}</section>}
    {recovery}{error&&<InlineNotice tone="danger">{error}</InlineNotice>}
    <FormActions><Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy||!!pending||!binding||(validPreview?!canPrepare:!canPreview)}>{busy?'Checking original request…':validPreview?'Assign published package':'Preview affected access'}</Button><Button appearance="standard" type="button" variant="secondary" disabled={busy||pending?.running} onClick={()=>setEditorOpen(false)}>{pending?'Close and retain request':'Keep draft'}</Button></FormActions>
   </FormLayout>
  </DraftFormDrawer>
 </section>;
}
