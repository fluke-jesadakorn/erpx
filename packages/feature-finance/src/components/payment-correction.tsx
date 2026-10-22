"use client";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { Button, Field, Select, TextInput } from "@erpx/ui/components/foundation";

import {createPaymentCorrectionCommands} from "@erpx/feature-finance/lib/sales-command";
import { useLayoutEffect,useRef, useState } from "react";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import { Card, FormLayout } from "@erpx/ui/components/foundation";
import {FormDisclosure} from "@erpx/ui/components/form-disclosure";
import {useFinanceDraftScope,captureFinanceForm} from "@erpx/feature-finance/components/finance-draft-boundary";
import {useFinanceActionAllowed} from "@erpx/feature-finance/components/purchase-context";
import {controlledPaymentAllowed,type ServicePaymentPurchase} from "@erpx/feature-finance/lib/service-payment-controls";


type Row = Record<string, string | number | null>;

export function PaymentCorrection({
  base,
  payment,
  periods,
  sourcePurchase,
}: {
  base: string;
  payment: Row;
  periods: Row[];
  sourcePurchase?:ServicePaymentPurchase;
}) {
  const scope=useFinanceDraftScope(),banking=useFinanceActionAllowed("settle-existing","finance.banking"),allowed=controlledPaymentAllowed(banking,sourcePurchase);
  const router=useRouter(),source=useRef<{base:string;paymentId:string}|null>({base,paymentId:String(payment.id)}),commands=useRef<ReturnType<typeof createPaymentCorrectionCommands>|null>(null),busyRef=useRef(false),formRef=useRef<HTMLFormElement>(null);
  useLayoutEffect(()=>{source.current={base,paymentId:String(payment.id)};return()=>{source.current=null;};},[base,payment.id]);commands.current??=createPaymentCorrectionCommands(()=>source.current);
  const [busy,setBusy]=useState(false),[error,setError]=useState(""),[hasAttempt,setHasAttempt]=useState(false),[generation,setGeneration]=useState(0),[saved,setSaved]=useState(false);
  async function submit(form:HTMLFormElement,retryOriginal=false){
    if(busyRef.current)return;busyRef.current=true;setBusy(true);setError("");
    try{
      if(!allowed&&!commands.current!.pending())throw Error("This Finance correction is unavailable. Your draft is retained.");
      const values=new FormData(form),input={expectedVersion:Number(payment.version),periodId:values.get("periodId"),entryDate:values.get("entryDate"),reason:values.get("reason")};
      const outcome=await(retryOriginal?commands.current!.retry():commands.current!.submit(input,captureFinanceForm())),unchanged=outcome.unchanged();
      if(!outcome.acknowledge())throw Error("Confirm the original correction again.");setHasAttempt(false);
      if(unchanged)router.refresh();else setSaved(true);
    }catch(cause){setHasAttempt(commands.current!.pending());setError(cause instanceof Error?cause.message:"Unable to confirm the payment correction.");}
    finally{busyRef.current=false;setBusy(false);}
  }
  return (
    <FormDisclosure appearance="standard" drawerTitle="Correct posted payment" error={error} dismissDisabled={busy||hasAttempt} onDiscard={()=>{if(commands.current!.pending()){setError("The original correction is unresolved. Retry its confirmation before discarding.");return;}setGeneration(value=>value+1);setHasAttempt(false);setError("");if(saved)router.refresh();}}><summary>Review a payment correction</summary><Card as="section" appearance="standard" className="ui-panel">
      <h2>Correct posted payment</h2>
      <p className="muted">
        Creates a posted reversing journal, restores every invoice allocation, and preserves the
        original payment as voided evidence. Reconciled bank items must be unmatched first.
      </p>
      {saved&&<InlineNotice tone="info">The original payment was reversed. Your later edits remain unsaved.</InlineNotice>}
      {error && (
        <InlineNotice tone="danger" className="error" role="alert">
          {error}
        </InlineNotice>
      )}
      {hasAttempt&&<Button appearance="standard" type="button" disabled={busy} onClick={()=>{if(formRef.current)void submit(formRef.current,true);}}>Retry original confirmation</Button>}
      <FormLayout ref={formRef}
        key={generation} data-finance-draft={scope}
        className="ui-form-grid"
        onSubmit={(event) => {
          event.preventDefault();
          void submit(event.currentTarget);
        }}
      >
        <Field appearance="standard">
          Correction period
          <Select appearance="standard" name="periodId" required defaultValue="">
            <option value="">Select open period</option>
            {periods.map((period) => (
              <option value={String(period.id)} key={String(period.id)}>
                {String(period.name)}
              </option>
            ))}
          </Select>
        </Field>
        <Field appearance="standard">
          Correction date
          <TextInput appearance="standard"
            name="entryDate"
            type="date"
            required
            defaultValue={new Date().toISOString().slice(0, 10)}
          />
        </Field>
        <Field appearance="standard">
          Correction evidence
          <TextInput appearance="standard" name="reason" required minLength={3} maxLength={500} />
        </Field>
        <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={busy||(!allowed&&!hasAttempt)}>
          {busy ? "Reversing…" : "Reverse payment and allocations"}
        </Button>
      </FormLayout>
    </Card></FormDisclosure>
  );
}
