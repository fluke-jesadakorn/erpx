"use client";
import {useEffect,useLayoutEffect,useRef,useState} from 'react';
import {Button,Field,FormLayout,InlineNotice,Select,TextArea,TextInput} from '@/components/ui/foundation';
import {DraftFormDrawer} from '@/components/ui/draft-form-drawer';
import {HirePayGroupSetup} from './hire-pay-group-setup';
import {useRouter} from '@/lib/shared/destination-router';
import {contextIsSwitching,registerUnsavedWork} from '@/lib/work_projection/context-switch';
import {createHireConsumptionCommands,type HireConsumptionReceipt,type HireConsumptionSource,type HireHandoffData,type HireHandoffOwner} from '@/lib/talent/hire-consumption-command';
import type {HireConsumptionInput} from '@/server/modules/people_payroll/public/rules/hire-consumption';

type Props={base:string;data:HireHandoffData;owner?:HireHandoffOwner};
type ReviewedHire=Pick<HireHandoffData,'handoff'|'payGroups'|'people'>;
const reviewHire=(data:HireHandoffData):ReviewedHire=>({handoff:{...data.handoff},payGroups:data.payGroups.map(group=>({...group})),people:data.people.map(person=>({...person}))});
export function HireHandoffEditor(props:Props){
 return <HireHandoffOwnerEditor key={JSON.stringify([props.base,props.owner?.userId,props.owner?.companyId,props.data.handoff.id])} {...props}/>;
}
function HireHandoffOwnerEditor({base,data,owner}:Props){
 const router=useRouter(),formRef=useRef<HTMLFormElement>(null),trigger=useRef<HTMLButtonElement>(null);
 const [runtime]=useState(()=>{
  let current:HireConsumptionSource|null=null,listener:(()=>void)|null=null;
  const commands=createHireConsumptionCommands(()=>current);
  return {commands,commit(source:HireConsumptionSource){current=source;},dispose(){current=null;},observe(next:()=>void){listener=next;return()=>{if(listener===next)listener=null;};},synchronize(){listener?.();}};
 });
 const [payGroupActive,setPayGroupActive]=useState(false),[busy,setBusy]=useState(false),[error,setError]=useState(''),[notice,setNotice]=useState(''),[uncertain,setUncertain]=useState(false),[open,setOpen]=useState(false),[dirty,setDirty]=useState(false),[recorded,setRecorded]=useState<HireConsumptionReceipt|null>(null),[reviewed,setReviewed]=useState<ReviewedHire|null>(null);
 useLayoutEffect(()=>{runtime.commit({base,owner,data});});
 useLayoutEffect(()=>()=>runtime.dispose(),[runtime]);
 // Only current live state is synchronized here; an old invocation may not
 // publish its error or completion into a later Work/source image.
 useEffect(()=>{
  let cancelled=false;
  const synchronize=()=>{
   let assertCurrent:()=>void;try{assertCurrent=runtime.commands.captureCurrent();}catch{return;}
   queueMicrotask(()=>{if(cancelled)return;try{assertCurrent();setUncertain(runtime.commands.pending());setBusy(runtime.commands.busy());}catch{/* The next lawful owner/source lease will synchronize recovery. */}});
  };
  const unobserve=runtime.observe(synchronize);synchronize();
  window.addEventListener('erpx:work-changed',synchronize);window.addEventListener('erpx:work-context-changed',synchronize);
  return()=>{cancelled=true;unobserve();window.removeEventListener('erpx:work-changed',synchronize);window.removeEventListener('erpx:work-context-changed',synchronize);};
 },[runtime,base,owner,data]);
 useEffect(()=>{
  const unsaved=()=>dirty||runtime.commands.pending()||runtime.commands.busy(),unregister=registerUnsavedWork(unsaved),unload=(event:BeforeUnloadEvent)=>{if(unsaved()&&!contextIsSwitching())event.preventDefault();};
  window.addEventListener('beforeunload',unload);return()=>{unregister();window.removeEventListener('beforeunload',unload);};
 },[dirty,runtime]);
 function openDrawer(){
  if(!open&&!dirty&&!runtime.commands.pending())setReviewed(reviewHire(data));
  setOpen(true);
 }
 async function execute(form?:HTMLFormElement,retry=false){
  if(runtime.commands.busy())return;
  let assertCurrent:(()=>void)|undefined;
  try{
   assertCurrent=runtime.commands.captureCurrent();assertCurrent();setBusy(true);setError('');setNotice('');
   let response:ReturnType<typeof runtime.commands.retry>;
   if(retry)response=runtime.commands.retry();
   else{
    if(!form||!reviewed)throw Error('Open and review the original HR intake form before submitting.');
    const input=Object.fromEntries(new FormData(form)),person=reviewed.people.find(value=>value.id===input.reconcilePersonId),body:HireConsumptionInput={handoffId:reviewed.handoff.id,expectedOfferVersion:reviewed.handoff.offerVersion,expectedCandidateVersion:reviewed.handoff.candidateVersion,payGroupId:String(input.payGroupId),countryCode:String(input.countryCode),evidenceRef:String(input.evidenceRef),...(person?{reconcilePersonId:person.id,expectedPersonVersion:person.version}:{})};
    response=runtime.commands.submit(body,{form,formEditVersion:form.getAttribute('data-erpx-edit-version')});
   }
   assertCurrent();setUncertain(runtime.commands.pending());
   const result=await response;assertCurrent();result.assertCurrent();const acknowledged=result.acknowledge();if(!acknowledged)return;assertCurrent();result.assertCurrent();
   // Native identity is recorded independently of draft reset. Newer fields
   // remain visible, but cannot create a second intake for the consumed source.
   setRecorded(result.receipt);setUncertain(false);
   if(acknowledged.newer)setNotice('The accepted hire was recorded. Your newer draft is retained and has not been submitted.');
   else{setDirty(false);setOpen(false);setReviewed(null);setNotice('The accepted hire was recorded.');}
   assertCurrent();result.assertCurrent();router.refresh();
  }catch(cause){try{if(assertCurrent){assertCurrent();setError(cause instanceof Error?cause.message:'Connection interrupted. Retry the submitted hire handoff.');}}catch{/* Retain the original attempt without publishing a stale failure. */}}
  finally{
   try{if(assertCurrent){assertCurrent();setUncertain(runtime.commands.pending());setBusy(runtime.commands.busy());}}catch{/* A fresh live lease, not this invocation, owns recovery state. */}
   runtime.synchronize();
  }
 }

 function discard(){
  if(runtime.commands.busy())return;
  if(runtime.commands.pending()){setOpen(false);setNotice('The original hire request and your draft are retained. Recover its result before submitting another intake.');return;}
  formRef.current?.reset();setDirty(false);setOpen(false);setReviewed(null);setError('');
 }
 const eligiblePeople=data.people.filter(person=>person.status==='active'&&!person.merged_into_id),consumed=!!data.receipt||!!recorded,visible=!payGroupActive&&!consumed&&data.canConsume&&(data.purchase?.settleExisting??true)&&data.payGroups.length>0&&(data.people.length===0||eligiblePeople.length===1&&data.people.length===1),retain=dirty||uncertain,reviewedGroups=reviewed?.payGroups??data.payGroups,reviewedPeople=reviewed?.people??data.people,reviewedEligible=reviewedPeople.filter(person=>person.status==='active'&&!person.merged_into_id);
 const retryControl=<Button appearance="standard" type="button" disabled={busy} onClick={()=>void execute(undefined,true)}>Retry submitted hire handoff</Button>;
 return <>
  {error&&<InlineNotice tone="danger" role="alert">{error}</InlineNotice>}{notice&&<InlineNotice role="status">{notice}</InlineNotice>}
  {uncertain&&<div role="status"><p>The original hire request is still unresolved. Retry its original body and key; newer fields are not submitted.</p>{retryControl}<Button appearance="standard" type="button" disabled={busy} onClick={openDrawer}>Open retained hire draft</Button></div>}
  {data.receipt?<p>Consumed · receipt {data.receipt.id} · person {data.receipt.person_id} · onboarding engagement {data.receipt.engagement_id} · draft compensation {data.receipt.compensation_id}</p>:recorded?<p>Consumed · receipt {recorded.id} · person {recorded.personId} · onboarding engagement {recorded.engagementId} · draft compensation {recorded.compensationId}</p>:<>
   <p>{data.handoff.legalName} · {data.handoff.email} · {data.handoff.jobTitle} · {data.handoff.currency} {(data.handoff.baseAmountMinor/100).toFixed(2)} · starts {data.handoff.startDate}</p>
   {!data.canConsume&&<InlineNotice>Review the accepted offer revision and current candidate consent before HR intake.</InlineNotice>}
   {!data.payGroups.length&&<InlineNotice>An active same-currency Company pay group is required.</InlineNotice>}
   {data.people.length>0&&<InlineNotice>Explicit reconciliation of the existing Company identity is required. This handoff does not merge identities.</InlineNotice>}
   {data.people.length>0&&!visible&&eligiblePeople.length!==1&&<InlineNotice>Resolve the existing identity state using an authorized HR assignment before intake.</InlineNotice>}
  </>}
  {data.companyId&&<HirePayGroupSetup source={{base,companyId:data.companyId,handoffId:data.handoff.id,offerVersion:data.handoff.offerVersion,candidateVersion:data.handoff.candidateVersion,currency:data.handoff.currency}} allowed={data.canCreatePayGroup===true} disabled={busy||uncertain||dirty||open} onActiveChange={setPayGroupActive} onChanged={()=>router.refresh()}/>}
  {(visible||dirty||open||uncertain)&&<>
   <Button ref={trigger} appearance="standard" type="button" disabled={busy||!visible&&!retain} onClick={openDrawer}>Consume accepted hire</Button>
   <DraftFormDrawer appearance="standard" title="Consume accepted hire" open={open} dirty={dirty} retainFormContents={retain} dismissDisabled={busy} finalFocus={()=>trigger.current} onClose={()=>setOpen(false)} onDiscard={discard} discardLabel={uncertain?'Close and retain original request':'Discard draft'} confirmationTitle={uncertain?'Keep the original hire request':'Discard changes?'} confirmationDescription={uncertain?'Closing retains the original body, key and newer draft. Use Retry to recover the native result.':'Your HR intake draft has not been submitted.'}>
    {error&&<InlineNotice tone="danger" role="alert">{error}</InlineNotice>}{notice&&<InlineNotice role="status">{notice}</InlineNotice>}
    {uncertain&&<div><p>Retry uses the original handoff, versions, reason and identity reconciliation.</p>{retryControl}</div>}
    {!visible&&<InlineNotice>Your draft is retained. A new HR intake is currently unavailable.</InlineNotice>}
    <FormLayout ref={formRef} className="ui-form-grid" onChangeCapture={event=>{const form=(event.target as HTMLElement).closest('form');if(form)form.setAttribute('data-erpx-edit-version',crypto.randomUUID());setDirty(true);}} onSubmit={event=>{event.preventDefault();void execute(event.currentTarget);}}>
     <fieldset disabled={busy||!visible&&!uncertain} className="ui-form-grid">
      <p>The accepted salary and placement are retained from the original offer. HR creates an onboarding engagement and a draft compensation revision for separate review.</p>
      <Field appearance="standard">Company pay group<Select appearance="standard" name="payGroupId" required>{reviewedGroups.map(group=><option key={group.id} value={group.id}>{group.name} · {group.currency} · {group.frequency}</option>)}</Select></Field>
      <Field appearance="standard">Country code<TextInput appearance="standard" name="countryCode" defaultValue="TH" minLength={2} maxLength={2} required/></Field>
      {reviewedPeople.length>0&&<Field appearance="standard">Existing matching identity<Select appearance="standard" name="reconcilePersonId" required>{reviewedEligible.map(person=><option key={person.id} value={person.id}>{person.legal_name} · revision {person.version}</option>)}</Select></Field>}
      <Field appearance="standard">HR intake evidence<TextArea appearance="standard" name="evidenceRef" required minLength={8} maxLength={2000}/></Field>
      <Button appearance="standard" clientManaged variant="primary" type="submit" disabled={busy||uncertain||!visible}>Create onboarding and draft compensation</Button>
     </fieldset>
    </FormLayout>
    <Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={()=>setOpen(false)}>Close and retain draft</Button>
   </DraftFormDrawer>
  </>}
 </>;
}
