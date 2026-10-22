"use client";
import {useRef,useState} from 'react';
import {FormDisclosure} from '@erpx/ui/components/form-disclosure';
import {Button,Field,FormLayout,TextArea} from '@erpx/ui/components/foundation';
import {workFetch} from '@erpx/feature-work_projection/lib/work-fetch';
import {captureWorkCommandContext} from '@erpx/feature-work_projection/lib/command-context';
import {useRouter} from '@erpx/feature-shared/lib/destination-router';
import {announceWorkChange} from '@erpx/feature-work_projection/lib/work-refresh';
import {financeDocumentCommandContext,financeDocumentFormUnchanged} from '@erpx/feature-finance/lib/document-post-control';
import {captureServiceNoChargeAttempt,serviceNoChargeAcknowledged,serviceNoChargeView,type ServiceNoChargeAttempt,type ServiceNoChargeControls} from '@erpx/server-finance/public/rules/service-no-charge';
import {captureFinanceForm,acknowledgeFinanceForm,useFinanceDraftScope} from '@erpx/feature-finance/components/finance-draft-boundary';
/** A no-charge confirmation fulfills one retained covered billing source. It
 * creates neither a payment nor a ledger posting or invoice cancellation. */
export function ServiceNoChargeConfirmation({base,controls}:{base:string;controls:ServiceNoChargeControls}){
 const router=useRouter(),scope=useFinanceDraftScope(),pending=useRef(false),attempt=useRef<{command:ServiceNoChargeAttempt;completion:ReturnType<typeof captureFinanceForm>}|null>(null);
 const [reason,setReason]=useState(''),[busy,setBusy]=useState(false),[uncertain,setUncertain]=useState(false),[error,setError]=useState(''),[notice,setNotice]=useState('');
 async function submit(){
  if(pending.current)return;const currentScope=financeDocumentCommandContext();
  if(attempt.current&&attempt.current.command.scope!==currentScope){setError('Review the original selected Finance context before retrying. Your draft is retained.');return;}
  if(!attempt.current){if(!controls.canConfirm){setError('This source can no longer accept a new confirmation. Your draft is retained.');return;}attempt.current={command:captureServiceNoChargeAttempt(controls,reason,crypto.randomUUID(),currentScope),completion:captureFinanceForm()};}
  const sent=attempt.current;pending.current=true;setBusy(true);setUncertain(true);setError('');setNotice('');
  try{const assertCurrent=captureWorkCommandContext(),response=await workFetch(`/api/v1${base}/finance/invoices/${encodeURIComponent(sent.command.invoiceId)}/confirm-no-charge`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':sent.command.key},body:sent.command.body},sent.completion,'caller');
   const value=await response.json() as {data?:unknown;error?:{message?:string}};if(!response.ok||!serviceNoChargeAcknowledged(value.data,sent.command.invoiceId,sent.command.version,sent.command.key))throw Error(value.error?.message??'The exact Service No-Charge Confirmation was not acknowledged. Retry the captured review.');
   assertCurrent();if(sent.command.scope!==financeDocumentCommandContext())throw Error('Return to the original selected Finance context before reviewing this captured confirmation. Your draft is retained.');
   acknowledgeFinanceForm(sent.completion);if(attempt.current===sent){attempt.current=null;setUncertain(false);}if(financeDocumentFormUnchanged(sent.completion))setReason('');else setNotice('The captured confirmation was saved. Your newer draft is retained.');announceWorkChange();router.refresh();
  }catch(cause){setError(cause instanceof Error?cause.message:'Connection interrupted. Retry the exact captured confirmation.');}finally{pending.current=false;setBusy(false);}
 }
 return <ServiceNoChargeConfirmationPresentation controls={controls} scope={scope} reason={reason} busy={busy} uncertain={uncertain} error={error} notice={notice} onSubmit={()=>void submit()} onReason={setReason} onDiscard={()=>{setReason('');setError('');setNotice('');}}/>;
}
/** Controlled rendering shared by the live editor and its retained-draft tests. */
export function ServiceNoChargeConfirmationPresentation({controls,scope,reason,busy,uncertain,error,notice,onSubmit,onReason,onDiscard}:{controls:ServiceNoChargeControls;scope:string|undefined;reason:string;busy:boolean;uncertain:boolean;error:string;notice:string;onSubmit:()=>void;onReason:(reason:string)=>void;onDiscard:()=>void}){
 const view=serviceNoChargeView(controls,{reason,notice,uncertain});
 return <>
  {controls.confirmed&&<p role="status">Service No-Charge Confirmation retained. There is no customer amount to collect.</p>}
  {notice&&<p role="status">{notice}</p>}
  {view.showReview&&<FormDisclosure appearance="standard" drawerTitle="Service No-Charge Confirmation" dismissDisabled={busy||uncertain} error={error} onDiscard={onDiscard}><summary>Review Service no-charge fulfillment</summary><FormLayout data-finance-draft={scope} onSubmit={event=>{event.preventDefault();onSubmit();}}>
   <p>Confirm that the original completed visit is fully covered by warranty and has no customer amount to collect. The original billing handoff and evidence stay retained.</p>
   <Field appearance="standard">Independent review reason<TextArea appearance="standard" name="reason" minLength={10} maxLength={3000} required value={reason} onChange={event=>onReason(event.target.value)}/></Field>
   <Button appearance="standard" clientManaged variant="primary" type="submit" disabled={busy||!view.canSubmit}>{busy?'Confirming…':uncertain?'Retry exact confirmation':'Confirm no customer amount'}</Button>
  </FormLayout></FormDisclosure>}
 </>;
}
