import { iamMutationGuards } from "./iam-store";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import type { WorkContext } from "./authorization";
import { ApiError } from "./http";
import { flowCommandTarget, prepareFlowCommand } from "./flow-routing";

type Actor = Pick<WorkContext, "userId" | "assignmentId" | "role" | "companyId" | "branchId" | "departmentId" | "authority" | "iam">;
export function assertion(db: SqlDatabase, id: string, step: string, query: string, args: (string | number | null)[] = []) {
  return db.prepare(`INSERT INTO command_assertions(command_id,step,ok) VALUES(?,?,CASE WHEN (${query}) THEN 1 ELSE 0 END)`).bind(id, step, ...args);
}

async function fingerprint(actor:Actor,operation:string,recordId:string,input:unknown) {
  const bytes=await crypto.subtle.digest("SHA-256",new TextEncoder().encode(JSON.stringify({actor:actor.userId,assignment:actor.assignmentId,role:actor.role,authority:actor.authority,company:actor.companyId,branch:actor.branchId,department:actor.departmentId,operation,recordId,input})));
  return Array.from(new Uint8Array(bytes),b=>b.toString(16).padStart(2,"0")).join("");
}
export async function replayCommand<T=Record<string,unknown>>(db:SqlDatabase,actor:Actor,id:string,operation:string,recordId:string,input:unknown):Promise<T|null> {
  const row=await db.prepare("SELECT request_hash,result_json FROM command_receipts WHERE command_id=?").bind(id).first<{request_hash:string;result_json:string}>();
  if(!row)return null;
  if(row.request_hash!==await fingerprint(actor,operation,recordId,input))throw new ApiError(409,"IDEMPOTENCY_CONFLICT","This command key belongs to a different request.");
  await validateReplayAuthority(db,actor);
  return JSON.parse(row.result_json) as T;
}
/** Replays also revalidate their original role; they cannot recover privilege after a switch. */
async function validateReplayAuthority(db:SqlDatabase,actor:Actor) {
 if(!actor.authority)return;
 const key=crypto.randomUUID();
 try {
   await db.batch([...workContextGuards(db,actor,key),...iamMutationGuards(db,actor,key),db.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);
 } catch(error) {
   if((error as {code?:string}).code==='23514')throw new ApiError(409,"WORK_CONTEXT_CHANGED","Your access changed. Reload and review before retrying.");
   throw error;
 }
}

export function workContextGuards(db:SqlDatabase,actor:Actor,id:string):SqlStatement[]{
 return (actor.authority ? [
       db.prepare('SELECT session_id FROM active_work_contexts WHERE session_id=? AND company_id=? FOR UPDATE').bind(actor.authority.sessionId,actor.companyId),
       assertion(db,id,'work-context',`EXISTS(SELECT 1 FROM active_work_contexts WHERE session_id=? AND company_id=? AND user_id=? AND assignment_id=? AND revision=? AND assignment_version=? AND expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint)`,[actor.authority.sessionId,actor.companyId,actor.userId,actor.assignmentId,actor.authority.revision,actor.authority.assignmentVersion]),
       db.prepare('SELECT id FROM assignments WHERE id=? FOR SHARE').bind(actor.assignmentId),
       assertion(db,id,'assignment-version',`EXISTS(SELECT 1 FROM assignments a WHERE a.id=? AND a.version=? AND a.user_id=? AND a.role=? AND a.company_id=?
         AND a.revoked_at IS NULL AND a.valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND a.valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint
         AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=?) OR (a.scope_kind='department' AND a.scope_node_id=?))
         AND EXISTS(SELECT 1 FROM organization_nodes c WHERE c.id=a.company_id AND c.status='active')
         AND (a.scope_kind='holding' OR EXISTS(SELECT 1 FROM organization_nodes n WHERE n.id=a.scope_node_id AND n.company_id=a.company_id AND n.status='active'
           AND (n.branch_id IS NULL OR EXISTS(SELECT 1 FROM organization_nodes b WHERE b.id=n.branch_id AND b.company_id=a.company_id AND b.status='active')))))`,
         [actor.assignmentId,actor.authority.assignmentVersion,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId]),
      ]:[]);
}
/** Scope, optimistic guards, owner writes, receipt and evidence commit together. */
export async function executeCommand<T>(db: SqlDatabase, actor: Actor, id: string, operation: string, input: unknown, recordId: string, version: number, result: T, statements: SqlStatement[], eventScope: Pick<Actor,"companyId"|"branchId"|"departmentId"> = actor) {
  if (!/^[a-zA-Z0-9:_.-]{8,180}$/.test(id)) throw new ApiError(422, "COMMAND_KEY_REQUIRED", "Use a valid Idempotency-Key for this action.");
  const digest=await fingerprint(actor,operation,recordId,input);
  const prior=()=>replayCommand<T>(db,actor,id,operation,recordId,input);
  const existing=await prior();if(existing)return existing;
  const target = flowCommandTarget(operation,recordId,input);
  if (operation === "commerce.invoice.create") {
    const delivery = await db.prepare("SELECT f.order_id FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id WHERE f.id=? AND o.company_id=?").bind(recordId,actor.companyId).first<{order_id:string}>();
    if (delivery) { const flow=await prepareFlowCommand(db,actor,id,"order",delivery.order_id,"invoice",input,version); statements=[...flow.before,...statements,...flow.after]; }
  } else if (operation.startsWith("expense.payment.")) {
    const expenseId = operation === "expense.payment.prepare" ? recordId : (await db.prepare("SELECT expense_id FROM expense_payment_drafts WHERE id=? AND company_id=?").bind(recordId,actor.companyId).first<{expense_id:string}>())?.expense_id;
    if (expenseId) {
      const flow = await prepareFlowCommand(db,actor,id,"expense",expenseId,"payment",input,version);
      statements = [...flow.before,...statements,...flow.after];
    }
  } else if (target) {
    const flow = await prepareFlowCommand(db,actor,id,target.kind,target.id,target.action,input,version);
    statements = [...flow.before,...statements,...flow.after];
  }
  const now = Date.now();
  const state=typeof result==="object"&&result!==null ? ((result as Record<string,unknown>).state??(result as Record<string,unknown>).status) : undefined;
  try {
    await db.batch([
      ...workContextGuards(db,actor,id),
      ...iamMutationGuards(db,actor,id),
      db.prepare("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES(?,?,?,?,?,?,?,?)").bind(id, operation, actor.userId, digest, recordId, JSON.stringify(result), now,JSON.stringify({role:actor.role,assignmentId:actor.assignmentId,iam:actor.iam,...actor.authority,companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId})),
      assertion(db,id,"assignment", "EXISTS(SELECT 1 FROM assignments WHERE id=? AND user_id=? AND role=? AND company_id=? AND (scope_kind='holding' OR (scope_kind='branch' AND scope_node_id=?) OR (scope_kind='department' AND scope_node_id=?)) AND revoked_at IS NULL AND valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint))", [actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId]),
      db.prepare("UPDATE command_receipts SET authority_json=(authority_json::jsonb || (SELECT jsonb_build_object('scopeKind',scope_kind,'scopeNodeId',scope_node_id) FROM assignments WHERE id=?))::text WHERE command_id=?").bind(actor.assignmentId,id),
      ...statements,
      db.prepare("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) VALUES(?,?,?,?,?,?,?,?,?,?,(SELECT authority_json FROM command_receipts WHERE command_id=?))").bind(crypto.randomUUID(),id,actor.userId,actor.assignmentId,operation,operation.split(".").slice(0,2).join("_"),recordId,version,operation,now,id),
      db.prepare("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),id,operation,operation.split(".").slice(0,2).join("_"),recordId,version,JSON.stringify({authority:{role:actor.role,assignmentId:actor.assignmentId,iam:actor.iam,...actor.authority},id:recordId,...(typeof state==="string"?{state}:{}),companyId:eventScope.companyId,branchId:eventScope.branchId,departmentId:eventScope.departmentId,version}),now),
      db.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(id),
    ]);
    return result;
  } catch (error) {
    if(actor.authority){
     const active=await db.prepare('SELECT revision,assignment_id,assignment_version FROM active_work_contexts WHERE session_id=? AND company_id=?').bind(actor.authority.sessionId,actor.companyId).first<{revision:number;assignment_id:string;assignment_version:number}>();
     if(!active || active.revision!==actor.authority.revision || active.assignment_id!==actor.assignmentId || active.assignment_version!==actor.authority.assignmentVersion)throw new ApiError(409,'WORK_CONTEXT_CHANGED','Your work role changed. Reload and review before submitting.');
     const grant=await db.prepare("SELECT 1 FROM assignments a JOIN active_work_contexts c ON c.assignment_id=a.id WHERE c.session_id=? AND c.company_id=? AND c.user_id=? AND a.id=? AND a.user_id=? AND a.role=? AND a.version=? AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>? AND c.expires_at>?").bind(actor.authority.sessionId,actor.companyId,actor.userId,actor.assignmentId,actor.userId,actor.role,actor.authority.assignmentVersion,Date.now(),Date.now(),Date.now()).first();
     if(!grant)throw new ApiError(409,'WORK_ROLE_UNAVAILABLE','Your selected assignment is no longer valid. Choose a work role again.');
    }
    const committed = await prior();
    if (committed) return committed;
    if (/constraint|UNIQUE/i.test(String(error))) throw new ApiError(409,"CONFLICT","The record changed, a reference is unavailable, or this value already exists. Refresh and review again.");
    throw error;
  }
}
