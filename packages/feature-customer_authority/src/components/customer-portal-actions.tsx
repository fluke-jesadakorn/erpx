"use client";
import {holdingFetch} from '@erpx/contracts/holding-fetch';
import { InlineNotice } from "@erpx/ui/components/foundation";

import { Button, Field, TextArea } from "@erpx/ui/components/foundation";

import { useCallback,useEffect,useLayoutEffect,useRef,useState,type FormEvent } from "react";
import {registerUnsavedWork,contextIsSwitching} from '@erpx/feature-work_projection/lib/context-switch';
import {FormDisclosure} from '@erpx/ui/components/form-disclosure';
import {FormLayout} from '@erpx/ui/components/foundation';
import {createPortalActionAttempt,validPortalActionReceipt,type PortalActionAttempt,type PortalPurchaseControls} from '@erpx/feature-customer_authority/lib/portal-command-attempt';
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@erpx/feature-files/components/attachment-panel";

type SharedFile = { linkId: string; label: string; previewAvailable: true };
type Props = {
  holdingSlug: string;
  bindingId: string;
  bindingVersion: number;
  actions?: Record<string, unknown>[];
  sharedFiles?: SharedFile[];
  purchase?:PortalPurchaseControls;
};
export function CustomerPortalActions({
  holdingSlug,
  bindingId,
  bindingVersion,
  actions = [],
  sharedFiles = [],
  purchase,
}: Props) {
  const router = useRouter(),
    [note, setNote] = useState(""),
    [busy, setBusy] = useState<string | null>(null),
    [error, setError] = useState(""),
    [saved, setSaved] = useState(""),
    [evidence, setEvidence] = useState<AttachmentPanelChange | null>(null),[uncertain,setUncertain]=useState(false),[generation,setGeneration]=useState(0),pending=useRef(false),attempts=useRef(createPortalActionAttempt(undefined,{requireSource:true})),noteRef=useRef(note),evidenceRef=useRef(evidence),completions=useRef(new Map<string,{form?:HTMLFormElement;formEditVersion:string|null;note:string;evidence:string}>());
  const evidenceChanged=useCallback((value:AttachmentPanelChange)=>{evidenceRef.current=value;setEvidence(value);},[]);
  const [savedEvidence,setSavedEvidence]=useState<string|null>(null);
  const context=JSON.stringify([holdingSlug,bindingId,purchase?.identityRevision??'legacy-verified-binding']);
  const source=JSON.stringify([bindingVersion,actions,sharedFiles,purchase]);
  useLayoutEffect(()=>attempts.current.attach({context,image:source}),[context,source]);
  const acknowledged = actions.some((action) => action.action === "acknowledge_delivery"),
    supportRequested = actions.some((action) => action.action === "request_support");
  const activeEvidence =
      evidence?.links.filter((link) => !["withdrawn", "expired"].includes(link.linkState)) ?? [],
    evidencePending = activeEvidence.length > 0 && !evidence?.bindable;
  const evidenceStamp=(value:AttachmentPanelChange|null)=>JSON.stringify([value?.setId,value?.links.map(link=>[link.id,link.linkState]),value?.bindable]);
  const dirty=Boolean(note||activeEvidence.some(link=>link.linkState==='draft')&&evidenceStamp(evidence)!==savedEvidence||uncertain||busy);
  useEffect(()=>{
    const unregister=registerUnsavedWork(()=>dirty);
    if(!dirty)return unregister;
    // Public Portal links stay within this document or open a shared preview.
    // Native leave confirmation aggregates all dirty document cards without
    // rewriting their tenant URL/history or discarding an uncertain command.
    const unload=(event:BeforeUnloadEvent)=>{if(!contextIsSwitching())event.preventDefault();};
    window.addEventListener('beforeunload',unload);
    return()=>{unregister();window.removeEventListener('beforeunload',unload);};
  },[dirty,context]);
  async function execute(attempt:PortalActionAttempt){
    if(pending.current)return;if(attempt.context!==context){setError('Return to the original verified customer account before retrying.');return;}
    if(purchase?.history===false||purchase?.settleExisting===false&&!attempt.uncertain){setError('This retained document is currently available for history. Your message is retained.');return;}
    let fence:ReturnType<typeof attempts.current.capture>;try{fence=attempts.current.capture(attempt);}catch(cause){setError(cause instanceof Error?cause.message:'Return to the original verified customer account before retrying.');return;}
    const input=JSON.parse(attempt.body) as {action:'acknowledge_delivery'|'request_support'};pending.current=true;setBusy(input.action);
    setError("");
    setSaved("");
    let mayHaveCommitted=true;
    try {
      fence.assertCurrent();attempts.current.retain(attempt);
      const response = await holdingFetch(`/api/customer-portal/${encodeURIComponent(holdingSlug)}`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.key },
        body:attempt.body,
      });
      const body = await response.json();
      fence.assertCurrent();
      if (!response.ok){mayHaveCommitted=!attempts.current.firstRollback(attempt,response.status,body.error?.code,input.action);if(mayHaveCommitted){attempts.current.uncertain(attempt);setUncertain(true);}else{attempts.current.resolved(attempt);setUncertain(false);}
        throw new Error(body.error?.message ?? "The customer action could not be recorded.");
      }
      if(!validPortalActionReceipt(body.data,bindingId,input.action)){attempts.current.uncertain(attempt);setUncertain(true);throw Error('The acknowledgement is incomplete. Retry the exact submitted customer action.');}
      fence.assertCurrent();attempts.current.resolved(attempt);mayHaveCommitted=false;setUncertain(false);const completion=completions.current.get(attempt.key),unchanged=input.action!=='request_support'||!!completion&&(!completion.form||completion.form.isConnected&&completion.form.getAttribute('data-erpx-edit-version')===completion.formEditVersion)&&noteRef.current===completion.note&&evidenceStamp(evidenceRef.current)===completion.evidence;
      if(input.action==='request_support'&&completion)setSavedEvidence(completion.evidence);
      setSaved(
        !unchanged?'Submitted customer action recorded. Your newer message and evidence selection are retained.':input.action === "request_support"
          ? "Support request sent to the company."
          : "Delivery acknowledgement recorded.",
      );
      if(!unchanged)return;if(input.action==='request_support'){noteRef.current='';setNote('');if(completion?.form)window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:completion}));}
      router.refresh();
    } catch (error) {
      if(mayHaveCommitted)attempts.current.uncertain(attempt);
      if(fence.sameOwner()){
        setUncertain(Boolean(attempts.current.retry()));
        setError(fence.current()?(error instanceof Error?error.message:"The customer action could not be recorded."):'The customer document changed. Retry the original action in the current document.');
      }
    } finally {
      pending.current=false;if(fence.sameOwner())setBusy(null);
    }
  }
  function submit(action:'acknowledge_delivery'|'request_support',event?:FormEvent<HTMLFormElement>){
    event?.preventDefault();const body=JSON.stringify({bindingId,expectedBindingVersion:bindingVersion,action,note:action==='request_support'?note:'',attachmentSetIds:action==='request_support'&&evidence?.setId&&evidence.readyCount>0&&evidence.bindable?[evidence.setId]:[]});
    try{const attempt=attempts.current.select(body,context);if(!completions.current.has(attempt.key))completions.current.set(attempt.key,{form:event?.currentTarget,formEditVersion:event?.currentTarget.getAttribute('data-erpx-edit-version')??null,note,evidence:evidenceStamp(evidence)});void execute(attempt);}catch(error){setError(error instanceof Error?error.message:'Retry the exact submitted customer action.');}
  }
  return (
    <div className="customer-portal-actions" aria-label="Customer actions">
      <h3>Next action</h3>
      <FormDisclosure appearance="standard" dismissDisabled={Boolean(busy)} retainFormContents={Boolean(note||evidence||uncertain||busy)} confirmationTitle={uncertain?'Keep the submitted action?':undefined} confirmationDescription={uncertain?'The original action has no confirmed result. Closing keeps its exact request and your newer message and evidence for recovery.':undefined} discardLabel={uncertain?'Close and retain action':undefined} onDiscard={()=>{if(busy||attempts.current.retry())return;noteRef.current='';evidenceRef.current=null;setNote('');setEvidence(null);setGeneration(value=>value+1);}}><summary>Request support</summary><FormLayout key={generation} onSubmit={event=>submit('request_support',event)}>
      <Field appearance="standard">
        Message to the company (optional for support)
        <TextArea appearance="standard"
          value={note}
          onChange={(event) => {noteRef.current=event.target.value;setNote(event.target.value);}}
          maxLength={500}
          rows={3}
          placeholder="Describe what you need help with"
        />
      </Field>
      <AttachmentPanel appearance="standard"
        base={`/api/customer-portal/${encodeURIComponent(holdingSlug)}/attachments`}
        targetType="customer_access_binding"
        targetId={bindingId}
        expectedTargetVersion={bindingVersion}
        componentKind="action"
        componentRef="request_support"
        purposeKey="customer_support_evidence"
        title="Support request photos and documents"
        description="Optionally attach photos or documents that explain this support request. Every file is scanned before it can be submitted."
        visibility="Only your verified account and authorized company staff can access this evidence."
        readOnly={supportRequested||purchase?.settleExisting===false}
        showHistory
        allowOriginalDownload={false}
        onChange={evidenceChanged}
      />
      <Button appearance="standard" type="submit" variant="primary" disabled={Boolean(busy)||uncertain||supportRequested||evidencePending||purchase?.history===false||purchase?.settleExisting===false}>{supportRequested?'Support requested':busy==='request_support'?'Sending…':evidencePending?'Wait for file scan':'Send support request'}</Button>
      {uncertain&&<Button appearance="standard" type="button" disabled={Boolean(busy)} onClick={()=>{const attempt=attempts.current.retry();if(attempt)void execute(attempt);}}>Retry submitted customer action</Button>}
      {saved&&<InlineNotice tone="info" role="status">{saved}</InlineNotice>}
      {error&&<InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      </FormLayout></FormDisclosure>
      {sharedFiles.length > 0 && (
        <section aria-label="Files shared by the company">
          <h4>Files shared by the company</h4>
          {sharedFiles.map((file) => (
            <a
              key={file.linkId}
              className="ui-record-item"
              href={`/api/customer-portal/${encodeURIComponent(holdingSlug)}/attachments/shared/${encodeURIComponent(file.linkId)}/content`}
              target="_blank"
              rel="noreferrer"
            >
              {file.label}
            </a>
          ))}
        </section>
      )}
      <div className="actions">
        <Button appearance="standard"
          variant="primary"
          type="button"
          disabled={Boolean(busy) || uncertain||acknowledged||purchase?.history===false||purchase?.settleExisting===false}
          onClick={() => submit("acknowledge_delivery")}
        >
          {acknowledged
            ? "Delivery acknowledged"
            : busy === "acknowledge_delivery"
              ? "Recording…"
              : "Acknowledge delivery"}
        </Button>
        {uncertain&&<Button appearance="standard" type="button" disabled={Boolean(busy)} onClick={()=>{const attempt=attempts.current.retry();if(attempt)void execute(attempt);}}>Retry submitted customer action</Button>}
      </div>
      {saved && <InlineNotice tone="info" role="status">{saved}</InlineNotice>}
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
    </div>
  );
}
