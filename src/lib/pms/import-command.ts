import {z} from "zod";
import {workFetch, type WorkCommandCompletion} from "@/lib/work_projection/work-fetch";
import {captureWorkCommandContext} from "@/lib/work_projection/command-context";
import {retainWorkCommandCompletion} from "@/lib/work_projection/command-completion";
import {announceWorkChange} from "@/lib/work_projection/work-refresh";
import {samePmsCommandView} from "@/lib/pms/pending-navigation";
import {pmsImportSchema} from "@/server/modules/pms/public/rules/pms-import";

const positive=z.number().int().positive(),minor=z.number().int().nonnegative();
const report=z.object({
  errors:z.array(z.object({row:z.string(),code:z.string(),message:z.string()})),
  counts:z.record(z.string(),minor),
  totals:z.object({due:minor,advance:minor,advanceTax:minor,deposit:minor}),
  canApprove:z.boolean(),
});
const identity={id:z.string().min(1),version:positive};
const preview=z.object({...identity,version:z.literal(1),state:z.literal("preview"),report});
const approved=z.object({...identity,state:z.literal("approved")});
const applied=z.object({...identity,state:z.literal("applied"),propertyId:z.string().min(1),propertyVersion:positive});
const batch=z.object({
  ...identity,state:z.enum(["preview","approved","applied"]),created_by:z.string(),
  approved_by:z.string().nullable(),opening_journal_id:z.string().nullable(),report,
});
export type PmsImportBatch=z.infer<typeof batch>;
export type PmsImportAction="preview"|"approve"|"apply";
export type PmsImportSource={api:string;propertyId:string;companyId?:string;propertyVersion?:number;href?:string};
type Attempt={source:PmsImportSource;scope:string;body:string;key:string;sends:number;completion:ReturnType<typeof retainWorkCommandCompletion>;ownerCompletion?:()=>void};

function scope(source:PmsImportSource|null){
  if(!source)return "disposed";
  const node=typeof document==="undefined"?null:document.querySelector<HTMLElement>("[data-work-revision]");
  if(!node||node.isConnected===false||!node.dataset.workCompany||!node.dataset.workAssignment||!node.dataset.workRevision)throw changed();
  if(source.companyId&&node?.dataset.workCompany&&source.companyId!==node.dataset.workCompany)throw changed();
  return JSON.stringify([source.api,source.propertyId,source.companyId??node?.dataset.workCompany,node?[node.dataset.workUser,node.dataset.workRevision,node.dataset.workCompany,node.dataset.workAssignment]:null]);
}
function changed(){return Object.assign(new Error("Your account, Company, role or hotel changed. Return to the original work context before retrying."),{code:"WORK_CONTEXT_CHANGED"});}
function captureForm():WorkCommandCompletion|undefined{
  const form=typeof document==="undefined"?null:document.activeElement?.closest<HTMLFormElement>("form");
  return form?{form,formEditVersion:form.getAttribute("data-erpx-edit-version")}:undefined;
}
async function responseData(response:Response):Promise<unknown>{
  const body:unknown=await response.json();
  if(!response.ok){
    const error=body&&typeof body==="object"&&"error" in body?body.error:null;
    const message=error&&typeof error==="object"&&"message" in error&&typeof error.message==="string"?error.message:"Please try again.";
    const code=error&&typeof error==="object"&&"code" in error?error.code:undefined,reference=response.headers.get("X-Request-ID");
    throw Object.assign(new Error(reference?`${message} (Reference: ${reference})`:message),{status:response.status,code});
  }
  return body&&typeof body==="object"&&"data" in body?body.data:undefined;
}
async function validReceipt(attempt:Attempt,data:unknown){
  const {action,input}=JSON.parse(attempt.body) as {action:PmsImportAction;input:unknown};
  if(action==="preview"){
    const parsed=preview.safeParse(data),definition=pmsImportSchema.safeParse(input);
    if(!parsed.success||!definition.success||!attempt.source.companyId)return false;
    const digest=await crypto.subtle.digest('SHA-256',new TextEncoder().encode(JSON.stringify([attempt.source.companyId,attempt.key])));
    const id='pms-import-'+Array.from(new Uint8Array(digest),byte=>byte.toString(16).padStart(2,'0')).join('').slice(0,40);
    const counts={roomTypes:definition.data.roomTypes.length,rooms:definition.data.rooms.length,rates:definition.data.rates.length,guests:definition.data.guests.length,reservations:definition.data.reservations.length};
    const totals={due:0,advance:0,advanceTax:0,deposit:0};
    for(const row of definition.data.reservations)for(const key of ['due','advance','advanceTax','deposit'] as const)totals[key]+=row.opening[key];
    return parsed.data.id===id&&Object.entries(counts).every(([key,value])=>parsed.data.report.counts[key]===value)&&
      (['due','advance','advanceTax','deposit'] as const).every(key=>parsed.data.report.totals[key]===totals[key])&&
      parsed.data.report.canApprove===(parsed.data.report.errors.length===0);
  }
  const parsed=(action==="approve"?approved:applied).safeParse(data);
  if(!parsed.success||!input||typeof input!=="object"||!("id" in input)||!("expectedVersion" in input)||typeof input.expectedVersion!=="number")return false;
  return parsed.data.id===input.id&&parsed.data.version===input.expectedVersion+1&&
    (action!=="apply"||"propertyId" in parsed.data&&parsed.data.propertyId===attempt.source.propertyId&&
      Number.isSafeInteger(attempt.source.propertyVersion)&&parsed.data.propertyVersion===Number(attempt.source.propertyVersion)+1);
}

/** Exact first native schema rejection or complete owner-transaction rollback.
 * None of these codes is emitted after its native transaction commits. */
function firstRollback(attempt:Attempt,status:number,code:unknown){
  const {action}=JSON.parse(attempt.body) as {action:PmsImportAction};
  if(status===422&&code==='VALIDATION_FAILED')return true;
  if(status!==409)return false;
  if(['PMS_IMPORT_STALE','PMS_IMPORT_CUTOVER','PMS_IMPORT_ACCOUNTING'].includes(String(code)))return true;
  if(action==='preview')return false;
  if(['PMS_IMPORT_ERRORS','PMS_IMPORT_CUSTOMER','PMS_IMPORT_JOURNAL','PMS_IMPORT_RECEIVABLE','PMS_IMPORT_GL_DIFFERENCE'].includes(String(code)))return true;
  return action==='apply'&&code==='PMS_IMPORT_APPROVAL';
}

/** Only this Import owner opts out of header-time acknowledgement. The original
 * command remains pending until both typed responses are safe to present. */
export function createPmsImportCommands(currentSource:()=>PmsImportSource|null,send:typeof workFetch=workFetch){
  let pending:Attempt|undefined;
  function assertSource(attempt:Attempt){
    const current=currentSource();
    if(attempt.scope!==scope(current)||attempt.source.href&&(!current?.href||!samePmsCommandView(attempt.source.href,current.href)))throw changed();
  }
  async function execute(attempt:Attempt){
    assertSource(attempt);
    const assertCurrent=captureWorkCommandContext(),completion=attempt.completion();
    assertCurrent();
    ++attempt.sends;
    const response=await send(attempt.source.api,{method:"POST",headers:{"Content-Type":"application/json","Idempotency-Key":attempt.key},body:attempt.body},completion,"caller");
    let data:unknown;
    try{data=await responseData(response);}catch(error){
      assertCurrent();assertSource(attempt);
      // A later refusal cannot disprove an earlier uncertain submission.
      const code=error&&typeof error==="object"&&"code" in error?error.code:undefined;
      if(attempt.sends===1&&firstRollback(attempt,response.status,code)&&pending===attempt)pending=undefined;
      throw error;
    }
    const valid=await validReceipt(attempt,data);
    assertCurrent();assertSource(attempt);
    if(!valid)throw Error("The import acknowledgement is incomplete. Retry the submitted import action.");
    const list=await send(`${attempt.source.api}?${new URLSearchParams({property:attempt.source.propertyId})}`,{method:"GET",cache:"no-store"},undefined,"caller");
    let rows:unknown;
    try{rows=await responseData(list);}catch(error){assertCurrent();assertSource(attempt);throw error;}
    const parsed=z.array(batch).safeParse(rows);
    if(!parsed.success)throw Error("The import batch list is incomplete. Retry the submitted import action.");
    assertCurrent();assertSource(attempt);
    return {batches:parsed.data,acknowledge(){
      assertCurrent();assertSource(attempt);
      if(pending!==attempt)return false;
      attempt.ownerCompletion?.();
      pending=undefined;
      if(typeof window!=="undefined"){
        const original=attempt.completion();
        window.dispatchEvent(new CustomEvent("erpx:record-command-saved",{detail:original??{form:null,formEditVersion:null}}));
        announceWorkChange();
      }
      return true;
    }};
  }
  return {
    run(action:PmsImportAction,input:unknown,ownerCompletion?:()=>void){
      const current=currentSource();if(!current)throw changed();
      const node=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');
      const source={...current,companyId:current.companyId??node?.dataset.workCompany},body=JSON.stringify({action,input});
      if(action==='preview'&&!source.companyId)throw changed();
      if(action==='apply'&&(!Number.isSafeInteger(source.propertyVersion)||Number(source.propertyVersion)<1))throw changed();
      if(action==='preview'&&input&&typeof input==='object'&&'propertyId' in input&&input.propertyId!==source.propertyId)throw Error("Use the selected hotel's propertyId in the import JSON.");
      if(pending){
        assertSource(pending);
        if(pending.body!==body)throw Error("Retry the submitted import action first. Your newer JSON and journal draft are retained.");
      }else pending={source,scope:scope(source),body,key:crypto.randomUUID(),sends:0,completion:retainWorkCommandCompletion(captureForm()),ownerCompletion};
      return execute(pending);
    },
    retry(){return pending?execute(pending):Promise.resolve(null);},
    hasPending(){return !!pending;},
  };
}
