"use client";
import {useEffect,useLayoutEffect,useRef,useState,useTransition} from 'react';
import {useRouter} from '@erpx/feature-shared/lib/destination-router';
import {canonicalUiHref} from '@erpx/feature-work_projection/lib/destination-registry';
import {recordCanonicalHref} from '@erpx/feature-records/lib/record-drawer';
import {RecordLink} from '@erpx/feature-records/components/record-link';
import {announceWorkChange} from '@erpx/feature-work_projection/lib/work-refresh';
import {captureWorkCommandContext} from '@erpx/feature-work_projection/lib/command-context';
import {contextIsSwitching,registerUnsavedWork} from '@erpx/feature-work_projection/lib/context-switch';
import {Button,Card,Field,InlineNotice,Select,TextArea} from '@erpx/ui/components/foundation';
import {DraftFormDrawer} from '@erpx/ui/components/draft-form-drawer';
import {ManufacturingForm} from '@erpx/feature-manufacturing/components/draft-boundary';
import {createPlanningReviewCommands,planningReviewRow,type PlanningReviewActor,type PlanningReviewCurrent,type PlanningReviewReceipt,type PlanningReviewRow} from '@erpx/feature-manufacturing/lib/planning-review-command';
import type {ManufacturingPurchaseControls} from '@erpx/feature-manufacturing/lib/purchase-controls';

type Props={base:string;actor:PlanningReviewActor;rows:readonly Record<string,unknown>[];canReview:boolean;purchase?:ManufacturingPurchaseControls;recordPurchases?:Record<string,ManufacturingPurchaseControls>;blocked:boolean;onRetentionChange:(retained:boolean,working:boolean)=>void};
type Editor={owner:string;row:PlanningReviewRow;action:'approve'|'reject';reason:string;instance:string};
const owner=(value:Pick<Props,'base'|'actor'>)=>JSON.stringify([value.base,value.actor]);
const sourceImage=(value:PlanningReviewCurrent)=>JSON.stringify([value.base,value.actor,value.rows,value.canReview,value.purchase,value.recordPurchases,value.blocked]);
/** Actual Planning review, retaining its editor independently from the mutable
 * proposed-row list so an approved source still has exact retry/history. */
export function PlanningRecommendationReviews(props:Props){
 const router=useRouter(),form=useRef<HTMLFormElement>(null),[runtime]=useState(()=>{
  let current:PlanningReviewCurrent|null=null,epoch=0,image='',dirty=false,listener:(()=>void)|null=null;
  const commands=createPlanningReviewCommands();commands.bind(()=>current);
  function lease(){const captured=current,generation=epoch,assertWork=captureWorkCommandContext(),href=typeof window==='undefined'?null:window.location.href;let cancelled=false;return{current(){try{assertWork();if(cancelled||!captured||!current||generation!==epoch||window.location.href!==href)return false;const node=document.querySelector<HTMLElement>('[data-work-revision]');return !!node?.isConnected&&node.dataset.workStale!=='true'&&node.dataset.workUser===current.actor.userId&&node.dataset.workCompany===current.actor.companyId&&node.dataset.workAssignment===current.actor.assignmentId;}catch{return false;}},cancel(){cancelled=true;}};}
  return{commands,lease,commit(value:PlanningReviewCurrent,edited:boolean){const next=sourceImage(value);if(next!==image){image=next;++epoch;}current=value;dirty=edited;},dispose(){current=null;image='';++epoch;},dirty:()=>dirty,setDirty(value:boolean){dirty=value;},subscribe(next:()=>void){listener=next;return()=>{if(listener===next)listener=null;};},sync(){listener?.();}};
 });
 const {commands}=runtime,[editor,setEditor]=useState<Editor|null>(null),[open,setOpen]=useState(false),[dirty,setDirty]=useState(false),[draftVersion,setDraftVersion]=useState(0),[busy,setBusy]=useState(false),[pending,setPending]=useState(false),[refreshing,startRefresh]=useTransition(),[error,setError]=useState<{owner:string;text:string}|null>(null),[notice,setNotice]=useState<{owner:string;text:string}|null>(null),[confirmed,setConfirmed]=useState<{owner:string;data:PlanningReviewReceipt}|null>(null);
 const current:PlanningReviewCurrent={...props,draftVersion},stamp=sourceImage(current),ownerStamp=owner(props),working=busy||refreshing,visibleEditor=editor?.owner===ownerStamp?editor:null;
 useLayoutEffect(()=>{runtime.commit(current,dirty);});
 useLayoutEffect(()=>()=>runtime.dispose(),[runtime]);
 useEffect(()=>{
  let cancelled=false,sequence=0;
  const sync=()=>{const token=++sequence,lease=runtime.lease();queueMicrotask(()=>{if(cancelled||token!==sequence||!lease.current())return;setBusy(commands.busy());setPending(commands.hasPending());});};
  const unsubscribe=runtime.subscribe(sync),retained=()=>commands.hasPending()||commands.busy()||runtime.dirty(),unregister=registerUnsavedWork(retained),unload=(event:BeforeUnloadEvent)=>{if(retained()&&!contextIsSwitching())event.preventDefault();};
  sync();window.addEventListener('beforeunload',unload);for(const name of ['focus','erpx:work-context-changed','erpx:context-switch-started'])window.addEventListener(name,sync);
  return()=>{cancelled=true;++sequence;unsubscribe();unregister();window.removeEventListener('beforeunload',unload);for(const name of ['focus','erpx:work-context-changed','erpx:context-switch-started'])window.removeEventListener(name,sync);};
 },[commands,runtime,stamp]);
 const {onRetentionChange}=props;
 useEffect(()=>{onRetentionChange(pending||working,working);},[pending,working,onRetentionChange]);
 function begin(row:PlanningReviewRow){
  if(working)return;if(commands.hasPending()){setOpen(true);return;}
  if(visibleEditor?.row.id===row.id){setOpen(true);return;}
  if(dirty&&!window.confirm('Discard the unsubmitted Planning review draft?'))return;
  form.current?.reset();runtime.setDirty(false);setDirty(false);setDraftVersion(value=>value+1);setEditor({owner:ownerStamp,row,action:'approve',reason:'',instance:crypto.randomUUID()});setOpen(true);setError(null);setNotice(null);
 }
 function discard(){if(commands.hasPending()||commands.busy()){setOpen(false);return;}form.current?.reset();runtime.setDirty(false);setDirty(false);setEditor(null);setOpen(false);setDraftVersion(value=>value+1);}
 async function save(retry=false){
  if(working||!visibleEditor&&!retry)return;const lease=runtime.lease();if(!lease.current())return;props.onRetentionChange(true,true);setBusy(true);setError(null);setNotice(null);
  try{
   const submitted=retry?commands.retry():commands.send({entity:'recommendation',id:visibleEditor!.row.id,expectedVersion:visibleEditor!.row.version,action:visibleEditor!.action,reason:visibleEditor!.reason},form.current?{form:form.current,formEditVersion:form.current.getAttribute('data-erpx-edit-version')}:undefined);runtime.sync();
   const result=await submitted;result.assertCurrent();if(!lease.current())return;const unchanged=result.unchanged();if(!result.acknowledge())throw Error('The original Planning acknowledgement was already handled.');
   result.assertCurrent();if(!lease.current())return;setConfirmed({owner:ownerStamp,data:result.data});
   if(unchanged){runtime.setDirty(false);setDirty(false);setEditor(null);setOpen(false);const completion=result.completion();if(completion){result.assertCurrent();window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{...completion,manufacturingAcknowledged:true}}));}}
   else setNotice({owner:ownerStamp,text:'The original Planning review was saved. Your newer decision or reason remains unsubmitted.'});
   result.assertCurrent();if(!lease.current())return;announceWorkChange();result.assertCurrent();if(!lease.current())return;
   startRefresh(()=>{result.assertCurrent();if(!lease.current())return;if(unchanged&&result.data.orderId)router.push(canonicalUiHref(`${props.base}/manufacturing/${result.data.orderId}`));result.assertCurrent();if(lease.current())router.refresh();});
  }catch(cause){if(lease.current())setError({owner:ownerStamp,text:cause instanceof Error?cause.message:'Recover the original Planning review before submitting again.'});}
  finally{if(lease.current()){props.onRetentionChange(commands.hasPending(),commands.busy());setBusy(commands.busy());setPending(commands.hasPending());}lease.cancel();runtime.sync();}
 }
 const rows=props.rows.map(planningReviewRow).filter((row):row is PlanningReviewRow=>!!row&&row.companyId===props.actor.companyId),original=commands.pendingForCurrent(),latest=visibleEditor?rows.find(row=>row.id===visibleEditor.row.id):null,sourceChanged=!!visibleEditor&&(!latest||latest.version!==visibleEditor.row.version||latest.status!=='proposed'),purchase=visibleEditor?(props.recordPurchases?.[visibleEditor.row.id]??props.purchase):props.purchase,canSend=!!visibleEditor&&props.canReview&&!props.blocked&&visibleEditor.row.createdBy!==props.actor.userId&&!sourceChanged&&purchase?.[visibleEditor.action==='approve'?'newWork':'settleExisting']!==false,visibleError=error?.owner===ownerStamp?error.text:'',visibleNotice=notice?.owner===ownerStamp?notice.text:'',confirmation=confirmed?.owner===ownerStamp?confirmed.data:null;
 const retry=<Button appearance="standard" type="button" disabled={working||props.blocked} onClick={()=>void save(true)}>Retry original Planning review</Button>;
 return <section aria-label="Planning recommendation reviews" data-planning-review-retained={pending?'true':undefined}>
  {visibleError&&<InlineNotice tone="danger" role="alert">{visibleError}</InlineNotice>}{visibleNotice&&<InlineNotice role="status">{visibleNotice}</InlineNotice>}
  {pending&&<Card appearance="standard"><h2>Retained original Planning review</h2><p>{original?'The original recommendation, action, reason, work revision and key remain fixed. Current native review authority is required for Retry.':'Return to the original Planning Company and work scope before recovering its retained request.'}</p>{original&&<>{retry}<Button appearance="standard" type="button" variant="secondary" onClick={()=>setOpen(true)}>Resume retained review</Button></>}</Card>}
  {confirmation&&<p role="status">Recommendation {confirmation.status} · v{confirmation.version}{confirmation.orderId&&<> · <RecordLink href={canonicalUiHref(`${props.base}/manufacturing/${confirmation.orderId}`)}>Open confirmed Manufacturing order</RecordLink></>}</p>}
  {rows.filter(row=>row.status==='proposed').map(row=>{const controls=props.recordPurchases?.[row.id]??props.purchase;return <Card appearance="standard" as="section" key={row.id}><h2>Review recommendation · feasible {row.feasibleDate}</h2><p><RecordLink href={recordCanonicalHref(props.base,{kind:'planning_scenario',id:row.scenarioId})}>Review native planning sources</RecordLink> · v{row.version}</p><Button appearance="standard" type="button" disabled={working||pending||props.blocked||!props.canReview||row.createdBy===props.actor.userId||controls?.newWork===false&&controls.settleExisting===false} onClick={()=>begin(row)}>Review recommendation</Button></Card>;})}
  {visibleEditor&&!open&&<Button appearance="standard" type="button" variant="secondary" onClick={()=>setOpen(true)}>Resume Planning review draft</Button>}
  <DraftFormDrawer appearance="standard" retainFormContents open={open&&!!visibleEditor} title="Review Planning recommendation" dirty={dirty&&!pending} onClose={()=>setOpen(false)} onDiscard={discard}>
   {visibleEditor&&<><ManufacturingForm key={visibleEditor.instance} ref={form} sourceVersion={visibleEditor.row.version} onInputCapture={()=>{runtime.setDirty(true);setDirty(true);setDraftVersion(value=>value+1);}} onChangeCapture={()=>{runtime.setDirty(true);setDirty(true);setDraftVersion(value=>value+1);}} onSubmit={event=>{event.preventDefault();if(canSend&&!pending)void save();}}>
    <p>Original recommendation {visibleEditor.row.id} · v{visibleEditor.row.version} · feasible {visibleEditor.row.feasibleDate}</p>
    {visibleError&&<InlineNotice tone="danger" role="alert">{visibleError}</InlineNotice>}{visibleNotice&&<InlineNotice role="status">{visibleNotice}</InlineNotice>}
    {sourceChanged&&<InlineNotice tone="warning">The native recommendation changed. Your reviewed source and draft are retained; no new version or key has been substituted.</InlineNotice>}
    <Field appearance="standard">Decision<Select appearance="standard" value={visibleEditor.action} onChange={event=>{const action=event.currentTarget.value;if(action==='approve'||action==='reject')setEditor(value=>value?{...value,action}:value);}}><option value="approve" disabled={purchase?.newWork===false}>Approve and release order</option><option value="reject" disabled={purchase?.settleExisting===false}>Reject recommendation</option></Select></Field>
    <Field appearance="standard">Reason<TextArea appearance="standard" value={visibleEditor.reason} minLength={3} maxLength={3000} required onChange={event=>{const reason=event.currentTarget.value;setEditor(value=>value?{...value,reason}:value);}}/></Field>
    <div className="ui-form-actions"><Button appearance="standard" clientManaged type="submit" disabled={working||pending||!canSend}>{visibleEditor.action==='approve'?'Approve and release order':'Reject recommendation'}</Button></div>
   </ManufacturingForm>
   <div className="ui-form-actions"><Button appearance="standard" type="button" variant="secondary" onClick={()=>setOpen(false)}>Close and retain draft</Button>{pending&&retry}{!pending&&<Button appearance="standard" type="button" variant="secondary" disabled={working} onClick={discard}>Discard this review draft</Button>}</div></>}
  </DraftFormDrawer>
 </section>;
}
