"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, TextArea, TextInput } from "@/components/ui/foundation";

import { useEffect,useLayoutEffect,useRef, useState } from "react";
import { Card, FormLayout } from "@/components/ui/foundation";
import {PublicMarketingDraftBoundary} from './public-form-draft-boundary';
import {createPublicMarketingCommand,publicMarketingAttemptCurrent,publicMarketingAcknowledgementCurrent,type PublicMarketingAttempt} from '@/lib/marketing/public-form-command';
import {contextGeneration,contextIsSwitching} from '@/lib/work_projection/context-switch';
export function PublicMarketingForm({
  holdingSlug,
  formId,
  formVersion,
}: {
  holdingSlug: string;
  formId: string;
  formVersion:number;
}) {
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [done, setDone] = useState(false),[dirty,setDirty]=useState(false),[uncertain,setUncertain]=useState(false),[saved,setSaved]=useState(''),
    command=useRef(createPublicMarketingCommand()),pending=useRef(false),alive=useRef(true),edits=useRef(0),form=useRef<HTMLFormElement>(null),completions=useRef(new Map<string,HTMLFormElement>()),context=JSON.stringify([holdingSlug,formId,formVersion]),currentContext=useRef(context);
  useLayoutEffect(()=>{currentContext.current=context;},[context]);
  useEffect(()=>{alive.current=true;return()=>{alive.current=false;};},[]);
  const current=(attempt:PublicMarketingAttempt)=>alive.current&&!contextIsSwitching()&&publicMarketingAttemptCurrent(attempt,currentContext.current,contextGeneration());
  async function execute(attempt:PublicMarketingAttempt){
    if(pending.current)return;
    if(!current(attempt)){setError('Return to the original form and account context before retrying.');return;}
    pending.current=true;setBusy(true);setError('');setSaved('');
    try{
      await command.current.execute(attempt,captured=>fetch(`/api/public/marketing/${encodeURIComponent(holdingSlug)}/${encodeURIComponent(formId)}`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':captured.key},body:captured.body}),()=>current(attempt));
      if(!current(attempt))return;
      setUncertain(false);const captured=completions.current.get(attempt.key),unchanged=publicMarketingAcknowledgementCurrent(attempt,currentContext.current,contextGeneration(),edits.current)&&captured===form.current&&captured?.isConnected;
      completions.current.delete(attempt.key);
      if(unchanged){setDirty(false);setDone(true);}else setSaved('Your submitted request is awaiting review. Your newer details are still here.');
    }catch(error){
      if(alive.current){setUncertain(Boolean(command.current.retry()));if(!command.current.retry())completions.current.delete(attempt.key);setError(current(attempt)?error instanceof Error?error.message:'Try again.':'The form or account context changed. Return to the original context to review the submitted request.');}
    }finally{pending.current=false;if(alive.current)setBusy(false);}
  }
  async function submit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (pending.current) return;
    const f = new FormData(e.currentTarget),
      query = new URLSearchParams(window.location.search);
    const body = {
      formVersion,
      contactName: f.get("name"),
      contactDetail: f.get("detail"),
      brief: f.get("brief"),
      consentAccepted: f.get("consent") === "on",
      source: (query.get("utm_source") ?? "Direct web form").slice(0, 120),
      campaignId: query.get("campaignId") || undefined,
    };
    try {
      const attempt=command.current.select(JSON.stringify(body),context,contextGeneration(),edits.current);if(!completions.current.has(attempt.key))completions.current.set(attempt.key,e.currentTarget);await execute(attempt);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Try again.");
    }
  }
  return <PublicMarketingDraftBoundary dirty={dirty||uncertain||busy}>{done?(
      <Card as="section" appearance="standard" className="ui-panel" role="status">
        <h2>Thank you</h2>
        <p>Your message is awaiting review.</p>
      </Card>
    ):(
    <FormLayout ref={form} data-marketing-public-draft className="ui-panel ui-form-grid" onInputCapture={()=>{edits.current++;setDirty(true);}} onChangeCapture={()=>{edits.current++;setDirty(true);}} onSubmit={submit}>
      <Field appearance="standard">
        Name
        <TextInput appearance="standard" name="name" required minLength={2} maxLength={160} />
      </Field>
      <Field appearance="standard">
        How can we contact you?
        <TextArea appearance="standard" name="detail" required minLength={3} maxLength={2000} />
      </Field>
      <Field appearance="standard">
        How can we help?
        <TextArea appearance="standard" name="brief" required minLength={3} maxLength={3000} />
      </Field>
      <Field appearance="standard" orientation="inline">
        <TextInput appearance="standard" name="consent" type="checkbox" required /> I agree to be contacted about this
        request.
      </Field>
      <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
        {busy ? "Submitting…" : "Send request"}
      </Button>
      {uncertain&&<Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={()=>{const attempt=command.current.retry();if(attempt)void execute(attempt);}}>Retry submitted request</Button>}
      {saved&&<InlineNotice tone="info" role="status">{saved}</InlineNotice>}
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
    </FormLayout>
  )}</PublicMarketingDraftBoundary>;
}
