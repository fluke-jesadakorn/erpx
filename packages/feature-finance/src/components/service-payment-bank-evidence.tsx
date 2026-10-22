"use client";
import {serviceBankImportAcknowledged} from "@erpx/feature-finance/lib/service-payment-controls";
import {useRef,useState} from "react";
import {FormDisclosure} from "@erpx/ui/components/form-disclosure";
import {Button,Field,FormLayout,TextInput} from "@erpx/ui/components/foundation";
import {workFetch} from "@erpx/feature-work_projection/lib/work-fetch";
import {useRouter} from "@erpx/feature-shared/lib/destination-router";
import {minorToDecimal} from "@erpx/feature-finance/lib/finance-money";
import {captureFinanceForm,acknowledgeFinanceForm,useFinanceDraftScope} from "@erpx/feature-finance/components/finance-draft-boundary";
type Row=Record<string,string|number|null>;
/** Exact owner source only. Original bank importer validates the account,
 * currency/date/amount; this surface never opens a bank statement collection. */
export function ServicePaymentBankEvidence({base,payment,sourcePurchase}:{base:string;payment:Row;sourcePurchase:{settleExisting:boolean}}){
 const router=useRouter(),scope=useFinanceDraftScope(),attempt=useRef<{body:string;key:string;completion:ReturnType<typeof captureFinanceForm>}|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState(''),[uncertain,setUncertain]=useState(false),[generation,setGeneration]=useState(0);
 async function submit(form:HTMLFormElement){
  if(busy)return;
  const values=new FormData(form),date=String(values.get('transactionDate')),body=JSON.stringify({settlementPaymentId:String(payment.id),bankAccount:String(payment.bank_account),statementNumber:String(values.get('statementNumber')),statementDate:date,opening:String(values.get('opening')),closing:String(values.get('closing')),sourceHash:String(values.get('sourceHash')),rows:[{externalId:String(values.get('externalId')),transactionDate:date,description:`Service receipt ${payment.payment_number}`,currency:String(payment.currency),amount:minorToDecimal(Number(payment.amount_minor))}]});
  if(!sourcePurchase.settleExisting&&!uncertain&&attempt.current?.body!==body){setError('This receipt can no longer import new evidence. Your draft is retained.');return;}
  if(!uncertain&&attempt.current?.body!==body)attempt.current={body,key:crypto.randomUUID(),completion:captureFinanceForm()};const sent=attempt.current!;setBusy(true);setError('');setUncertain(true);
  try{const response=await workFetch(`/api/v1${base}/finance/bank/statements/import`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':sent.key},body:sent.body}),result=await response.json() as {data?:{id?:string;settlementPaymentId?:string;imported?:number};error?:{message?:string}};
   if(!response.ok||!serviceBankImportAcknowledged(result.data,sent.key,String(payment.id)))throw Error(result.error?.message??'Bank evidence was not acknowledged for this exact Service receipt. Retry safely.');
   acknowledgeFinanceForm(sent.completion);if(attempt.current===sent){attempt.current=null;setUncertain(false);}router.refresh();
  }catch(cause){setError(cause instanceof Error?cause.message:'Connection interrupted. Retry the original evidence safely.');}finally{setBusy(false);}
 }
 return <FormDisclosure appearance="standard" drawerTitle="Import exact Service receipt bank evidence" error={error} dismissDisabled={busy||uncertain} onDiscard={()=>{setGeneration(value=>value+1);setError('');}}><summary>Import bank evidence for this receipt</summary><FormLayout key={generation} data-finance-draft={scope} onSubmit={event=>{event.preventDefault();void submit(event.currentTarget);}}>
  <p>The original bank record must show {String(payment.currency)} {minorToDecimal(Number(payment.amount_minor))} received into this payment’s selected account. Treasury confirms it independently.</p>
  <Field appearance="standard">Statement reference<TextInput appearance="standard" name="statementNumber" required/></Field>
  <Field appearance="standard">Bank transaction date<TextInput appearance="standard" name="transactionDate" type="date" min={String(payment.payment_date)} required/></Field>
  <Field appearance="standard">Opening balance<TextInput appearance="standard" name="opening" inputMode="decimal" required/></Field>
  <Field appearance="standard">Closing balance<TextInput appearance="standard" name="closing" inputMode="decimal" required/></Field>
  <Field appearance="standard">Bank transaction reference<TextInput appearance="standard" name="externalId" required/></Field>
  <Field appearance="standard">Source evidence hash<TextInput appearance="standard" name="sourceHash" minLength={8} required/></Field>
  <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy||(!sourcePurchase.settleExisting&&!uncertain)}>{busy?'Importing…':uncertain?'Retry exact evidence':'Import exact evidence'}</Button>
 </FormLayout></FormDisclosure>;
}
