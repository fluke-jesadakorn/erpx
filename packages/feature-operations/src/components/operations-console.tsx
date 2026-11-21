"use client";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { FormDisclosure } from "@erpx/ui/components/form-disclosure";
import {useLayoutEffect,useRef,useState} from "react";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import {Button,Field,Select,TextArea,TextInput} from "@erpx/ui/components/foundation";
import {contextGeneration} from '@erpx/feature-work_projection/lib/context-switch';
import {workFetch} from "@erpx/feature-work_projection/lib/work-fetch";
import {matchesOperationsReceipt,type OperationsWorkspace as Workspace,type OperationsDetail as Detail} from "@erpx/feature-operations/lib/operations-work-model";
import {captureWorkCommandContext,captureWorkCommandCompletion} from "@erpx/feature-work_projection/lib/command-context";
import {announceWorkChange} from "@erpx/feature-work_projection/lib/work-refresh";
import { FormLayout, Card } from "@erpx/ui/components/foundation";
const defaultKinds=["commerce_order","support_ticket","staff_task","pos_sale","pms_record","asset"];
type Attempt={key:string;body:string;url:string;previousStatus?:string;assertCurrent:()=>void;complete:()=>void;uncertain:boolean};
type CommandStore={command:Attempt|null;busy:boolean;listeners:Set<()=>void>};
// The Work boundary survives component/RSC remounts; a different document,
// Company, role, account or context generation never shares its pending action.
const retained=new WeakMap<object,Map<string,CommandStore>>();
export function OperationsConsole({base,data,detail,commandUrl,sourceKinds}:{base:string;data?:Workspace;detail?:Detail;commandUrl?:string;sourceKinds?:string[]}){
 const kinds=sourceKinds??defaultKinds;
  const router = useRouter(),
    store = useRef<CommandStore|null>(null),
    [busy, setBusy] = useState(false),
    [pending, setPending] = useState(false),
    [error, setError] = useState("");
  const view=data??detail!;
  useLayoutEffect(()=>{
    const node=document.querySelector<HTMLElement>('[data-work-revision]');
    if(!node)return;
    const key=JSON.stringify([base,commandUrl??null,detail?.row.id??null,view.actorId,window.location.pathname,window.location.search,node.dataset.workUser,node.dataset.workCompany,node.dataset.workAssignment,node.dataset.workAssignmentVersion,node.dataset.workRevision,contextGeneration()]);
    let entries=retained.get(node);if(!entries){entries=new Map();retained.set(node,entries);}
    let current=entries.get(key);if(!current){current={command:null,busy:false,listeners:new Set()};entries.set(key,current);}
    store.current=current;
    const refresh=()=>{if(store.current===current){setBusy(current.busy);setPending(current.command!==null);}};
    current.listeners.add(refresh);refresh();
    return()=>{current.listeners.delete(refresh);if(store.current===current)store.current=null;};
  },[base,commandUrl,data,detail,view.actorId]);

  async function send(input: unknown, retry = false) {
    const current=store.current;if(!current||current.busy)return;
    const publish=()=>{for(const listener of current.listeners)listener();};
    const body = retry && current.command ? current.command.body : JSON.stringify(input);
    if (body === undefined) return;
    if (current.command && current.command.body !== body) {
      if(store.current===current)setError("Confirm the original action first. Your newer entries remain unsent.");
      return;
    }
    if (!current.command) { const assertWork=captureWorkCommandContext(),location=window.location.pathname+window.location.search;current.command = { key: crypto.randomUUID(), body, url:commandUrl??`/api/v1${base}/operations`, previousStatus:detail?.row.status??data?.exceptions.find(row=>row.id===(input as {id?:unknown})?.id)?.status, assertCurrent:()=>{assertWork();if(window.location.pathname+window.location.search!==location)throw Object.assign(Error("Return to the original Operations page to confirm this action."),{code:"WORK_CONTEXT_CHANGED"});}, complete:captureWorkCommandCompletion(), uncertain: false }; publish(); }
    const command = current.command, priorUncertain = command.uncertain;
    let sent = false;
    current.busy=true;publish();
    if(store.current===current)setError("");
    try {
      command.assertCurrent();
      sent = true;
      const response = await workFetch(command.url, {
        method: "POST", headers: { "Content-Type": "application/json", "Idempotency-Key": command.key }, body: command.body,
      }, undefined, "caller");
      const result = await response.json();
      command.assertCurrent();
      if (!response.ok) {
        if (!priorUncertain && response.status < 500 && result.error?.code !== 'WORK_CONTEXT_CHANGED' && current.command === command) { current.command = null; publish(); }
        throw new Error(result.error?.message ?? "Unable to update Operations.");
      }
      if (!matchesOperationsReceipt(command.body, command.key, result.data,command.previousStatus)) throw new Error("The acknowledgement is incomplete. Confirm the original action.");
      if (current.command === command) { current.command = null; publish(); command.complete(); announceWorkChange(); router.refresh(); }
    } catch (e) {
      if (current.command === command) {
        if (!sent && !priorUncertain) { current.command = null; publish(); }
        else command.uncertain = true;
      }
      try { command.assertCurrent(); if(store.current===current)setError(e instanceof Error ? e.message : "Confirm the original action."); } catch { /* The original attempt stays bound to its original work identity. */ }
    } finally {
      current.busy=false;publish();
    }
  }
 const values=(event:React.FormEvent<HTMLFormElement>)=>{event.preventDefault();return new FormData(event.currentTarget);};
 const people=(name:string,current?:string)=><Select appearance="standard" name={name} defaultValue={current??""} required><option value="">Choose a current responsible person</option>{view.people.map(person=><option value={String(person.id)} key={String(person.id)}>{String(person.name)}</option>)}</Select>;
 if(detail){const r=detail.row,own=r.owner_id===view.actorId,review=view.canReview&&r.owner_id!==view.actorId&&r.resolved_by!==view.actorId;
  const actions=[...(own&&r.status==="open"?["acknowledge"]:[]),...(own&&["acknowledged","escalated"].includes(r.status)?["start"]:[]),...(own&&r.status==="resolved"?["close"]:[]),...(review&&r.status==="awaiting_review"?["approve","reject"]:[]),...(view.canReview&&r.status==="closed"?["reopen"]:[]),...(view.canReview&&!["closed","cancelled","resolved"].includes(r.status)?["cancel"]:[])];
  return <>{error&&<InlineNotice tone="danger" role="alert">{error}</InlineNotice>}{pending&&<Button appearance="standard" clientManaged disabled={busy} onClick={()=>void send(undefined,true)}>Confirm original action</Button>}{actions.length>0&&<FormLayout className="ui-panel ui-form-grid" onSubmit={event=>{const f=values(event);void send({entity:"exception",id:r.id,action:f.get("action"),expectedVersion:r.version,reason:f.get("reason")});}}><h2>Next decision</h2><Field appearance="standard">Action<Select appearance="standard" name="action">{actions.map(action=><option key={action} value={action}>{action}</option>)}</Select></Field><Field appearance="standard">Reason<TextArea appearance="standard" name="reason" minLength={3} required/></Field><Button appearance="standard" type="submit" clientManaged disabled={busy}>Record decision</Button></FormLayout>}
   {own&&["in_progress","escalated"].includes(r.status)&&<FormLayout className="ui-panel ui-form-grid" onSubmit={event=>{const f=values(event);void send({entity:"exception",id:r.id,action:"resolve",expectedVersion:r.version,resolution:f.get("resolution"),reason:f.get("reason")});}}><h2>Propose resolution</h2><Field appearance="standard">Evidence and outcome<TextArea appearance="standard" name="resolution" minLength={3} required/></Field><Field appearance="standard">Reason<TextArea appearance="standard" name="reason" minLength={3} required/></Field><Button appearance="standard" type="submit" clientManaged disabled={busy}>Send for independent review</Button></FormLayout>}
   {own&&["open","acknowledged","in_progress"].includes(r.status)&&<FormLayout className="ui-panel ui-form-grid" onSubmit={event=>{const f=values(event);void send({entity:"exception",id:r.id,action:"escalate",expectedVersion:r.version,escalationOwnerId:f.get("escalationOwnerId"),reason:f.get("reason")});}}><h2>Escalate</h2><Field appearance="standard">Reviewer{people("escalationOwnerId")}</Field><Field appearance="standard">Escalation reason<TextArea appearance="standard" name="reason" minLength={3} required/></Field><Button appearance="standard" type="submit" clientManaged disabled={busy}>Escalate exception</Button></FormLayout>}
   {view.canReview&&["open","acknowledged","in_progress","escalated","awaiting_review"].includes(r.status)&&<FormLayout className="ui-panel ui-form-grid" onSubmit={event=>{const f=values(event);void send({entity:"exception",id:r.id,action:"assign",expectedVersion:r.version,ownerId:f.get("ownerId"),reason:f.get("reason")});}}><h2>Assign responsible owner</h2><Field appearance="standard">Owner{people("ownerId",r.owner_id)}</Field><Field appearance="standard">Reason<TextArea appearance="standard" name="reason" minLength={3} required/></Field><Button appearance="standard" type="submit" clientManaged disabled={busy}>Assign owner</Button></FormLayout>}
   {view.canReview&&["open","acknowledged","in_progress","escalated"].includes(r.status)&&<FormLayout className="ui-panel ui-form-grid" onSubmit={event=>{const f=values(event);void send({entity:"exception",id:r.id,action:"prioritize",expectedVersion:r.version,priority:f.get("priority"),reason:f.get("reason")});}}><h2>Set priority</h2><Field appearance="standard">Priority<Select appearance="standard" name="priority" defaultValue={r.priority}><option>normal</option><option>high</option><option>critical</option></Select></Field><Field appearance="standard">Reason<TextArea appearance="standard" name="reason" minLength={3} required/></Field><Button appearance="standard" type="submit" clientManaged disabled={busy}>Update priority</Button></FormLayout>}
  </>;
 }
 return <>{error&&<InlineNotice tone="danger" role="alert">{error}</InlineNotice>}{pending&&<Button appearance="standard" clientManaged disabled={busy} onClick={()=>void send(undefined,true)}>Confirm original action</Button>}{view.canWrite&&<FormDisclosure appearance="standard" className="ui-form-disclosure" dismissDisabled={busy}><summary>Open a source exception</summary><FormLayout className="ui-panel ui-form-grid" onSubmit={event=>{const f=values(event),policy=view.policies.find(p=>p.id===f.get("policyId"));void send({entity:"exception",source:{kind:f.get("sourceKind"),id:f.get("sourceId"),version:Number(f.get("sourceVersion"))},policyId:policy?.id,policyVersion:Number(policy?.version),title:f.get("title"),issue:f.get("issue"),priority:f.get("priority"),ownerId:f.get("ownerId")});}}><h2>Open a source exception</h2><Field appearance="standard">Source kind<Select appearance="standard" name="sourceKind">{kinds.map(kind=><option key={kind} value={kind}>{kind.replaceAll("_"," ")}</option>)}</Select></Field><Field appearance="standard">Source record<TextInput appearance="standard" name="sourceId" required/></Field><Field appearance="standard">Current source version<TextInput appearance="standard" type="number" min={1} name="sourceVersion" defaultValue={1} required/></Field><Field appearance="standard">Published queue policy<Select appearance="standard" name="policyId" required><option value="">Choose a published policy</option>{view.policies.filter(p=>p.status==="published").map(policy=><option key={String(policy.id)} value={String(policy.id)}>{String(policy.name)} · revision {String(policy.revision)}</option>)}</Select></Field><Field appearance="standard">Title<TextInput appearance="standard" name="title" minLength={3} required/></Field><Field appearance="standard">Exception details<TextArea appearance="standard" name="issue" minLength={3} required/></Field><Field appearance="standard">Priority<Select appearance="standard" name="priority"><option>normal</option><option>high</option><option>critical</option></Select></Field><Field appearance="standard">Owner{people("ownerId")}</Field><Button appearance="standard" type="submit" clientManaged disabled={busy}>Open exception</Button></FormLayout></FormDisclosure>}
  {view.canReview&&<FormLayout className="ui-panel ui-form-grid" onSubmit={event=>{const f=values(event);void send({entity:"policy",area:f.get("area"),name:f.get("name"),targetMinutes:Number(f.get("targetMinutes")),sourceKinds:f.getAll("sourceKinds")});}}><h2>Draft queue policy</h2><Field appearance="standard">Queue key<TextInput appearance="standard" name="area" pattern="[a-z][a-z0-9_-]{1,39}" placeholder="customer_service" required/></Field><Field appearance="standard">Policy name<TextInput appearance="standard" name="name" minLength={3} required/></Field><Field appearance="standard">Follow-up target in minutes<TextInput appearance="standard" type="number" min={1} max={10080} name="targetMinutes" defaultValue={60} required/></Field><fieldset><legend>Permitted sources</legend>{kinds.map(kind=><Field appearance="standard" key={kind} orientation="inline"><TextInput appearance="standard" type="checkbox" name="sourceKinds" value={kind}/>{kind.replaceAll("_"," ")}</Field>)}</fieldset><Button appearance="standard" type="submit" clientManaged disabled={busy}>Save policy draft</Button></FormLayout>}
  <Card as="section" appearance="standard" className="ui-panel"><h2>Queue policies and history</h2>{view.policies.map(policy=><section key={String(policy.id)}><h3>{String(policy.name)} · revision {String(policy.revision)}</h3><p>{String(policy.status)} · {String(policy.target_minutes)} minutes · {(JSON.parse(String(policy.source_kinds_json)) as string[]).join(", ")}</p>{view.canReview&&["draft","published"].includes(String(policy.status))&&<FormLayout className="ui-form-grid" onSubmit={event=>{const f=values(event);void send({entity:"policy",id:policy.id,action:f.get("action"),expectedVersion:Number(policy.version),reason:f.get("reason")});}}><Field appearance="standard">Decision<Select appearance="standard" name="action">{policy.status==="draft"?<><option value="publish">Publish reviewed policy</option><option value="reject">Reject draft</option></>:<option value="retire">Retire policy</option>}</Select></Field><Field appearance="standard">Review reason<TextArea appearance="standard" name="reason" minLength={3} required/></Field><Button appearance="standard" type="submit" clientManaged disabled={busy||policy.status==="draft"&&policy.created_by===view.actorId}>Record policy review</Button></FormLayout>}</section>)}</Card>
 </>;
}
