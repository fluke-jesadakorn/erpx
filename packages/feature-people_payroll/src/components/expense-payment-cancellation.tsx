'use client';
import {useEffect,useLayoutEffect,useRef,useState} from 'react';
import {Button,Field,TextArea,TextInput,InlineNotice} from '@erpx/ui/components/foundation';
import {DraftFormDrawer} from '@erpx/ui/components/draft-form-drawer';
import {registerUnsavedWork} from '@erpx/feature-work_projection/lib/context-switch';
import WorkspaceLink from '@erpx/workplace/components/workspace-link';
import {canonicalUiHref} from '@erpx/feature-work_projection/lib/destination-registry';
import {useRouter} from '@erpx/feature-shared/lib/destination-router';
import {useExpensePurchase} from '@erpx/feature-people_payroll/components/expense-purchase-context';
import {createExpensePaymentCancellationCommands,expensePaymentCancellationOwner,type ExpensePaymentCancellationSource} from '@erpx/feature-people_payroll/lib/expense-payment-cancellation';
import type {ExpenseRow} from '@erpx/server-people_payroll/expense';
export function ExpensePaymentCancellation({base,expense,draft}:{base:string;expense:ExpenseRow;draft:ExpenseRow}){
 const purchase=useExpensePurchase(),router=useRouter(),[open,setOpen]=useState(false),[reason,setReason]=useState(''),[notPaid,setNotPaid]=useState(false),[busy,setBusy]=useState(false),[uncertain,setUncertain]=useState(false),[error,setError]=useState(''),[saved,setSaved]=useState<{owner:string;newer:boolean}|null>(null),revision=useRef(0),live=useRef<ExpensePaymentCancellationSource|null>(null),busyRef=useRef(false);
 const source:ExpensePaymentCancellationSource={base,companyId:String(expense.company_id),expenseId:String(expense.id),draftId:String(draft.id),expenseVersion:Number(expense.version),draftVersion:Number(draft.version),canCancel:draft.can_cancel_payment===1&&purchase?.settleExisting!==false,history:purchase?.history!==false,contextKey:purchase?.contextKey??''};
 useLayoutEffect(()=>{live.current=source;return()=>{live.current=null;};});
 const commands=useRef<ReturnType<typeof createExpensePaymentCancellationCommands>|null>(null);commands.current??=createExpensePaymentCancellationCommands(()=>live.current);
 const owner=expensePaymentCancellationOwner(source),dirty=reason!==''||notPaid,notice=saved?.owner===owner?saved:null;
 useEffect(()=>registerUnsavedWork(()=>dirty||uncertain),[dirty,uncertain]);
 async function submit(form:HTMLFormElement,retry=false){if(busyRef.current)return;busyRef.current=true;setBusy(true);setError('');const edit=revision.current,captured=owner;try{if(!retry&&!notPaid)throw Error('Verify that no bank payment was executed before cancelling.');
  const outcome=await(retry?commands.current!.retry():commands.current!.submit({expectedVersion:Number(draft.version),expenseVersion:Number(expense.version),reason,paymentNotExecuted:true},{form,formEditVersion:form.getAttribute('data-erpx-edit-version')})),unchanged=outcome.unchanged()&&revision.current===edit;
  if(!outcome.acknowledge())throw Error('Confirm the original cancellation receipt.');setUncertain(false);setSaved({owner:captured,newer:!unchanged});if(unchanged){setReason('');setNotPaid(false);setOpen(false);}router.refresh();
 }catch(cause){if(live.current&&expensePaymentCancellationOwner(live.current)===captured){setUncertain(commands.current!.pending());setError(cause instanceof Error?cause.message:'Cancellation is uncertain. Retry its original request.');}}finally{busyRef.current=false;if(live.current){setBusy(false);setUncertain(commands.current!.pending());}}}
 if(!source.canCancel&&!open&&!dirty&&!uncertain)return null;
 const savedNotice=notice&&<InlineNotice tone="info">The original payment preparation was cancelled.{notice.newer?' Your newer note remains unsent.':''} <WorkspaceLink href={canonicalUiHref(`${base}/expenses/${encodeURIComponent(String(expense.id))}`)}>View the claim and payment history</WorkspaceLink></InlineNotice>;
 return <div>{savedNotice}<Button appearance="standard" type="button" variant="secondary" disabled={!source.canCancel||busy||uncertain} onClick={()=>setOpen(true)}>Cancel unused payment preparation</Button>
 <DraftFormDrawer appearance="standard" open={open} dirty={dirty||uncertain} title="Cancel unused payment preparation" dismissDisabled={busy||uncertain} retainFormContents={dirty||uncertain} onClose={()=>{if(!commands.current!.pending())setOpen(false);}} onDiscard={()=>{if(commands.current!.pending())return;setReason('');setNotPaid(false);revision.current++;setOpen(false);}}>
  {error&&<InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
  <p>Cancel only a preparation for which no bank payment was executed. The claim, uploaded evidence and any actual recorded payment remain in their original history.</p>
  {savedNotice}
  <form data-finance-draft="expense-payment-cancellation" onChangeCapture={event=>event.currentTarget.setAttribute('data-erpx-edit-version',crypto.randomUUID())} onSubmit={event=>{event.preventDefault();void submit(event.currentTarget);}}>
   <fieldset disabled={busy}>
    <Field appearance="standard">Reason<TextArea appearance="standard" value={reason} minLength={3} maxLength={2000} required onChange={event=>{revision.current++;setReason(event.target.value);}}/></Field>
    <Field appearance="standard" orientation="inline"><TextInput appearance="standard" type="checkbox" checked={notPaid} required onChange={event=>{revision.current++;setNotPaid(event.target.checked);}}/>I checked that no bank payment was executed through this preparation.</Field>
    <Button appearance="standard" type="submit" variant="danger" disabled={!source.canCancel||busy||uncertain||!notPaid||reason.trim().length<3}>Cancel this preparation</Button>
   </fieldset>
   {uncertain&&<InlineNotice tone="info">The original request is unconfirmed. Keep this drawer open and retry that exact request. Your newer note will remain unsent.<Button appearance="standard" type="button" disabled={busy} onClick={event=>{const form=event.currentTarget.closest('form');if(form)void submit(form,true);}}>Retry original cancellation</Button></InlineNotice>}
  </form>
 </DraftFormDrawer></div>;
}
