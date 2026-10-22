"use client";
import {useRef,useState} from "react";
import {Button,Field,FormLayout,InlineNotice,Select,TextInput} from "@erpx/ui/components/foundation";
import {FormDisclosure} from "@erpx/ui/components/form-disclosure";
import {useRouter} from "@erpx/feature-shared/lib/destination-router";
import {canonicalUiHref} from "@erpx/feature-work_projection/lib/destination-registry";
import {workFetch} from "@erpx/feature-work_projection/lib/work-fetch";
import {financeDocumentCommandContext,financeDocumentFormUnchanged} from "@erpx/feature-finance/lib/document-post-control";
import {minorToDecimal} from "@erpx/feature-finance/lib/finance-money";
import {supplierCorrectionSource,supplierCorrectionInput,sendSupplierCorrection,type SupplierInvoiceCorrections,type SupplierCorrectionInput} from "@erpx/feature-finance/lib/supplier-payment-correction";
import {controlledPaymentAllowed} from "@erpx/feature-finance/lib/service-payment-controls";
import {useFinanceActionAllowed} from "@erpx/feature-finance/components/purchase-context";
import {useFinanceDraftScope,captureFinanceForm,acknowledgeFinanceForm} from "@erpx/feature-finance/components/finance-draft-boundary";
type Row=Record<string,string|number|null>;
type Attempt={input:SupplierCorrectionInput;body:string;key:string;context:string;completion:ReturnType<typeof captureFinanceForm>};

export function SupplierPaymentCorrectionPreparation({base,invoice,corrections}:{base:string;invoice:Row;corrections:SupplierInvoiceCorrections}){
 const scope=useFinanceDraftScope(),router=useRouter(),banking=useFinanceActionAllowed("settle-existing","finance.banking"),allowed=controlledPaymentAllowed(banking,corrections),attempt=useRef<Attempt|null>(null);
 const [selectedId,setSelectedId]=useState(""),[busy,setBusy]=useState(false),[pending,setPending]=useState<{reference:string}|null>(null),[error,setError]=useState(""),[generation,setGeneration]=useState(0),[preparedId,setPreparedId]=useState<string|null>(null);
 const selected=supplierCorrectionSource(corrections.sources,selectedId);
 async function perform(captured:Attempt){
  if(busy)return;
  if(captured.context!==financeDocumentCommandContext()){setError("Return to the original Company and Finance assignment before retrying this captured correction.");return;}
  setBusy(true);setError("");
  try{
   const id=await sendSupplierCorrection(workFetch,`/api/v1${base}/finance/payments`,captured);
   if(captured.context!==financeDocumentCommandContext())throw Error("The correction was confirmed in the original context. Return there to review its result.");
   acknowledgeFinanceForm(captured.completion);attempt.current=null;setPending(null);
   if(financeDocumentFormUnchanged(captured.completion))router.push(canonicalUiHref(`${base}/finance/payments/${id}`));else setPreparedId(id);
  }catch(cause){setError(cause instanceof Error?cause.message:"The correction was not confirmed. Retry its captured request safely.");}
  finally{setBusy(false);}
 }
 function submit(form:HTMLFormElement){
  if(attempt.current){setError("Retry the captured request before preparing another correction. Your later edits are retained.");return;}
  if(!allowed||!selected){setError("Choose an available original supplier payment. Your draft is retained.");return;}
  const values=new FormData(form),input=supplierCorrectionInput(selected,{id:String(invoice.id),partyId:String(invoice.party_id),currency:String(invoice.currency)},{paymentNumber:String(values.get("paymentNumber")??""),paymentDate:String(values.get("paymentDate")??"")}),captured:Attempt={input,body:JSON.stringify(input),key:crypto.randomUUID(),context:financeDocumentCommandContext(),completion:captureFinanceForm()};
  attempt.current=captured;setPending({reference:input.paymentNumber});void perform(captured);
 }
 return <FormDisclosure appearance="standard" drawerTitle="Correct an original supplier payment" error={error} dismissDisabled={busy} onDiscard={()=>{setGeneration(value=>value+1);setSelectedId("");attempt.current=null;setPending(null);setError("");setPreparedId(null);}}><summary>Correct an original supplier payment</summary>
  <p>Choose the independently reversed payment to replace. Its original amount and bank account stay fixed. A different approver and Treasury payer review the new draft.</p>
  {preparedId&&<InlineNotice tone="info">The correction was prepared. Your later edits remain unsaved. <a href={canonicalUiHref(`${base}/finance/payments/${preparedId}`)}>Open prepared payment</a></InlineNotice>}
  {pending&&<InlineNotice tone="info">The captured request for {pending.reference} has not been acknowledged. Retrying keeps its original payment, amount, bank account and command key.</InlineNotice>}
  {!corrections.sources.length&&!pending&&<InlineNotice tone="info">The original payments already have corrections or their bank evidence needs review. Open the retained correction records to continue their independent workflow.</InlineNotice>}
  {(corrections.prepared?.length??0)>0&&<ul aria-label="Prepared supplier corrections">{corrections.prepared!.map(payment=><li key={payment.paymentId}><a href={canonicalUiHref(`${base}/finance/payments/${payment.paymentId}`)}>{payment.paymentNumber} · {payment.status}</a></li>)}</ul>}
  <FormLayout key={generation} data-finance-draft={scope} className="ui-form-grid" onSubmit={event=>{event.preventDefault();submit(event.currentTarget);}}>
   <Field appearance="standard">Original reversed payment<Select appearance="standard" name="correctionPaymentId" required value={selectedId} onChange={event=>setSelectedId(event.currentTarget.value)}><option value="">Select the original payment</option>{corrections.sources.map(source=><option key={source.paymentId} value={source.paymentId}>{source.paymentNumber} · {String(invoice.currency)} {minorToDecimal(source.amountMinor)} · {source.bankReference}</option>)}</Select></Field>
   {selected&&<InlineNotice tone="info">Original amount: {String(invoice.currency)} {minorToDecimal(selected.amountMinor)}. Bank account: {selected.bankAccountLabel}. Retained bank evidence: {selected.bankReference}.</InlineNotice>}
   <Field appearance="standard">Correction reference<TextInput appearance="standard" name="paymentNumber" required maxLength={100}/></Field>
   <Field appearance="standard">Planned date<TextInput appearance="standard" name="paymentDate" type="date" required defaultValue={new Date().toISOString().slice(0,10)}/></Field>
   <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy||pending!==null||!allowed||!selected}>Prepare selected correction</Button>
   {pending&&<Button appearance="standard" clientManaged type="button" disabled={busy} onClick={()=>{if(attempt.current)void perform(attempt.current);}}>Retry captured request</Button>}
  </FormLayout>
 </FormDisclosure>;
}
