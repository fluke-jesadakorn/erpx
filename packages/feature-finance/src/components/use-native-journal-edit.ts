'use client';
import { useEffect, useRef, useState } from 'react';
import { holdingFetch } from '@erpx/contracts/holding-fetch';
import { canonicalUiHref } from '@erpx/feature-work_projection/lib/destination-registry';
import { contextGeneration, contextIsSwitching } from '@erpx/feature-work_projection/lib/context-switch';
import { announceWorkChange } from '@erpx/feature-work_projection/lib/work-refresh';
import { useRouter } from '@erpx/feature-shared/lib/destination-router';
import { acknowledgeFinanceForm, useFinanceDraftCommandPending } from '@erpx/feature-finance/components/finance-draft-boundary';
import { financeDocumentFormUnchanged } from '@erpx/feature-finance/lib/document-post-control';
import { matchingReadbackContext, type PostContext } from '../application/native-journal-post';
import type { ManualJournalEditInput, ManualJournalEditResult } from '@erpx/contracts/finance-journal-edit';
import type { FinanceJournalWorkspaceReadResponse } from '@erpx/contracts/finance-journal-read';

type Receipt = ManualJournalEditResult & {status:'draft'};
function isEditInput(value:unknown): value is ManualJournalEditInput {
 if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
 const v=value as Record<string,unknown>;
 return Object.keys(v).every(k=>['chartId','periodId','entryDate','currency','memo','lines','expectedVersion','reason'].includes(k)) && ['chartId','periodId','entryDate','currency','reason'].every(k=>typeof v[k]==='string') && (v.memo===undefined || typeof v.memo==='string') && typeof v.expectedVersion==='number' && Number.isSafeInteger(v.expectedVersion) && v.expectedVersion>0 && v.expectedVersion<Number.MAX_SAFE_INTEGER && Array.isArray(v.lines) && v.lines.every(line=>line && typeof line==='object' && !Array.isArray(line) && ['accountId','debit','credit'].every(k=>typeof line[k]==='string') && (line.description===undefined || typeof line.description==='string') && (line.dimensions===undefined || line.dimensions && typeof line.dimensions==='object' && !Array.isArray(line.dimensions) && Object.values(line.dimensions).every(v=>typeof v==='string')));
}
type Held = {url:string; id:string; expectedVersion:number; reason:string; body:string; key:string; context:PostContext; node:HTMLElement; generation:number; token:object; completion:{form:HTMLFormElement;formEditVersion:string|null}; drafts:{form:HTMLFormElement;version:string|null}[]; uncertain:boolean; receipt?:Receipt};
const unresolved = new Map<string,Held>();
const sameSet=(a:readonly string[],b:readonly string[])=>a.length===b.length&&new Set(a).size===new Set(b).size&&a.every(v=>b.includes(v));
const sameContext=(a:PostContext,b:PostContext)=>Object.entries(a).every(([key,value])=>key==='modules'||key==='permissions'||b[key as keyof PostContext]===value)&&sameSet(a.modules,b.modules)&&sameSet(a.permissions,b.permissions);
const matches=(node:HTMLElement,c:PostContext)=>JSON.stringify([node.dataset.workUser,node.dataset.workCompany,node.dataset.workAssignment,node.dataset.workRevision,node.dataset.workAssignmentVersion])===JSON.stringify([c.userId,c.companyId,c.assignmentId,String(c.revision),String(c.assignmentVersion)]);
function receipt(held:Held,value:unknown):Receipt|undefined {
 if(!value||typeof value!=='object'||Array.isArray(value))return;
 const r=value as Record<string,unknown>;
 if(Object.keys(r).sort().join(',')!=='id,status,version'||r.id!==held.id||r.status!=='draft'||r.version!==held.expectedVersion+1)return;
 return r as Receipt;
}
export function useNativeJournalEdit(base:string, id:string, context:PostContext|undefined, allowed:boolean) {
 const url=`${base.replace(canonicalUiHref('/holdings/'),'/api/v1/holdings/')}/finance/journals/${encodeURIComponent(id)}`;
 const [initial]=useState(()=>unresolved.get(url)??null), held=useRef<Held|null>(initial), running=useRef(false);
 const [busy,setBusy]=useState(false),[message,setMessage]=useState(''),[saved,setSaved]=useState(!!initial?.receipt),[hasAttempt,setHasAttempt]=useState(!!initial);
 const track=useFinanceDraftCommandPending(),router=useRouter();
 useEffect(()=>{const current=held.current;if(current)track(current.token,current.completion,true);},[track]);
 async function send(form:HTMLFormElement,input:unknown) {
  if(running.current)return;
  let current=held.current;
  if(!current) {
   const node=document.querySelector<HTMLElement>('[data-work-revision]');
   if(!allowed||!context||!node||node.dataset.workStale==='true'||contextIsSwitching()||!matches(node,context)||!isEditInput(input)){setMessage('This Finance action is unavailable. Your draft is retained.');return;}
   current={url,id,expectedVersion:input.expectedVersion,reason:input.reason.trim().toWellFormed(),body:JSON.stringify(input),key:crypto.randomUUID(),context:{...context,modules:[...context.modules],permissions:[...context.permissions]},node,generation:contextGeneration(),token:{},uncertain:false,completion:{form,formEditVersion:form.getAttribute('data-erpx-edit-version')},drafts:[...document.querySelectorAll<HTMLFormElement>('form[data-finance-draft]')].map(item=>({form:item,version:item.getAttribute('data-erpx-edit-version')}))};
   held.current=current;unresolved.set(url,current);track(current.token,current.completion,true);setHasAttempt(true);
  }
  if(!current.node.isConnected&&context&&current.generation===contextGeneration()&&!contextIsSwitching()&&sameContext(current.context,context)) {const node=document.querySelector<HTMLElement>('[data-work-revision]');if(node?.isConnected&&node.dataset.workStale!=='true'&&matches(node,current.context))current.node=node;}
  const {node,generation,completion}=current;
  const stillCurrent=()=>!!context&&generation===contextGeneration()&&sameContext(current!.context,context)&&!contextIsSwitching()&&node.isConnected&&node.dataset.workStale!=='true'&&matches(node,current!.context);
  if(!stillCurrent()){setMessage('Your work context changed. The original request is retained.');return;}
  running.current=true;setBusy(true);setMessage('');
  const c=current.context,headers=new Headers({'x-erpx-context-company':c.companyId,'x-erpx-context-assignment':c.assignmentId,'x-erpx-context-revision':String(c.revision),'x-erpx-context-assignment-version':String(c.assignmentVersion)});
  if(c.branchSlug)headers.set('x-erpx-workspace-branch',c.branchSlug);if(c.departmentSlug)headers.set('x-erpx-workspace-department',c.departmentSlug);
  try {
   if(!current.receipt){const postHeaders=new Headers(headers);postHeaders.set('content-type','application/json');postHeaders.set('idempotency-key',current.key);const response=await holdingFetch(`${current.url}/edit`,{method:'POST',headers:postHeaders,body:current.body});const value:unknown=await response.json();
    const failure=value && typeof value==='object' ? (value as {error?:unknown}).error : undefined;
    const validation=failure && typeof failure==='object' && !Array.isArray(failure) ? failure as {code?:unknown;message?:unknown} : undefined;
    if(!current.uncertain && response.status===422 && validation?.code==='VALIDATION_FAILED' && validation.message==='Invalid journal.' && stillCurrent()) {
     track(current.token,completion,false);unresolved.delete(current.url);held.current=null;setHasAttempt(false);setMessage('Invalid journal. Correct the form and try again.');return;
    }
    const result=response.status===200&&value&&typeof value==='object'?receipt(current,(value as {data?:unknown}).data):undefined;if(!result)throw Error('Unable to confirm journal edit. Retry the original request.');current.receipt=result;setSaved(true);}
   if(!stillCurrent())throw Error('Journal saved; readback is pending.');
   const response=await holdingFetch(`${current.url}/edit-workspace`,{headers,cache:'no-store'});const value=await response.json() as FinanceJournalWorkspaceReadResponse;const journal=value?.data?.journal;
   const editions=value?.data?.history;
   const latest=Array.isArray(editions) ? editions.filter(edition=>edition.journal_version===current!.receipt!.version) : [];
   let snapshot:unknown;try { snapshot = typeof latest[0]?.snapshot_json === 'string' ? JSON.parse(latest[0].snapshot_json) : undefined; } catch { snapshot = undefined; }
   const image=snapshot && typeof snapshot==='object' && !Array.isArray(snapshot) ? (snapshot as {journal?:unknown}).journal : undefined;
   const saved=image && typeof image==='object' && !Array.isArray(image) ? image as Record<string,unknown> : undefined;
   if(!response.ok||!matchingReadbackContext(c,value)||!journal||journal.id!==current.receipt.id||!['draft','posted','reversed'].includes(String(journal.status))||typeof journal.version!=='number'||!Number.isSafeInteger(journal.version)||journal.version<current.receipt.version||journal.version===current.receipt.version&&journal.created_by!==c.userId||journal.source_type!=='manual'||latest.length!==1||latest[0]?.prepared_by!==c.userId||latest[0]?.reason!==current.reason||!saved||saved.id!==current.id||saved.version!==current.receipt.version||saved.created_by!==c.userId||!stillCurrent())throw Error('Journal saved; readback is pending.');
   acknowledgeFinanceForm(completion);announceWorkChange();track(current.token,completion,false);unresolved.delete(current.url);held.current=null;setHasAttempt(false);setSaved(false);
   const forms=[...document.querySelectorAll<HTMLFormElement>('form[data-finance-draft]')];
   if(!financeDocumentFormUnchanged(completion)||current.drafts.some(d=>d.version!==null&&d.form!==completion.form||!d.form.isConnected||d.form.getAttribute('data-erpx-edit-version')!==d.version)||forms.some(f=>!current!.drafts.some(d=>d.form===f)&&f.getAttribute('data-erpx-edit-version')!==null)){setMessage('Journal saved. Your newer draft is retained.');return;}
   router.refresh();
  }catch(error){current.uncertain=true;setMessage(current.receipt?'Journal saved; readback is pending. Retry confirmation.':error instanceof Error?error.message:'Unable to confirm journal edit. Retry the original request.');}
  finally{running.current=false;setBusy(false);}
 }
 return {send,busy,message,hasAttempt,saved};
}
