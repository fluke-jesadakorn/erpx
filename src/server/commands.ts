import type { SqlDatabase, SqlStatement } from "./runtime/types";
import type { WorkContext } from "./authorization";
import { ApiError } from "./http";

type Actor = Pick<WorkContext, "userId" | "assignmentId" | "role" | "companyId" | "branchId" | "departmentId">;
export function assertion(db: SqlDatabase, id: string, step: string, query: string, args: (string | number | null)[] = []) {
  return db.prepare(`INSERT INTO command_assertions(command_id,step,ok) VALUES(?,?,CASE WHEN (${query}) THEN 1 ELSE 0 END)`).bind(id, step, ...args);
}

async function fingerprint(actor:Actor,operation:string,recordId:string,input:unknown) {
  const bytes=await crypto.subtle.digest("SHA-256",new TextEncoder().encode(JSON.stringify({actor:actor.userId,company:actor.companyId,branch:actor.branchId,department:actor.departmentId,operation,recordId,input})));
  return Array.from(new Uint8Array(bytes),b=>b.toString(16).padStart(2,"0")).join("");
}
export async function replayCommand<T=Record<string,unknown>>(db:SqlDatabase,actor:Actor,id:string,operation:string,recordId:string,input:unknown):Promise<T|null> {
  const row=await db.prepare("SELECT request_hash,result_json FROM command_receipts WHERE command_id=?").bind(id).first<{request_hash:string;result_json:string}>();
  if(!row)return null;
  if(row.request_hash!==await fingerprint(actor,operation,recordId,input))throw new ApiError(409,"IDEMPOTENCY_CONFLICT","This command key belongs to a different request.");
  return JSON.parse(row.result_json) as T;
}
/** Scope, optimistic guards, owner writes, receipt and evidence commit together. */
export async function executeCommand<T>(db: SqlDatabase, actor: Actor, id: string, operation: string, input: unknown, recordId: string, version: number, result: T, statements: SqlStatement[], eventScope: Pick<Actor,"companyId"|"branchId"|"departmentId"> = actor) {
  if (!/^[a-zA-Z0-9:_.-]{8,180}$/.test(id)) throw new ApiError(422, "COMMAND_KEY_REQUIRED", "Use a valid Idempotency-Key for this action.");
  const digest=await fingerprint(actor,operation,recordId,input);
  const prior=()=>replayCommand<T>(db,actor,id,operation,recordId,input);
  const existing=await prior();if(existing)return existing;
  const now = Date.now();
  const state=typeof result==="object"&&result!==null ? ((result as Record<string,unknown>).state??(result as Record<string,unknown>).status) : undefined;
  try {
    await db.batch([
      db.prepare("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES(?,?,?,?,?,?,?)").bind(id, operation, actor.userId, digest, recordId, JSON.stringify(result), now),
      assertion(db,id,"assignment", "EXISTS(SELECT 1 FROM assignments WHERE id=? AND user_id=? AND role=? AND company_id=? AND (scope_kind='holding' OR (scope_kind='branch' AND scope_node_id=?) OR (scope_kind='department' AND scope_node_id=?)) AND revoked_at IS NULL AND valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint))", [actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId]),
      ...statements,
      db.prepare("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),id,actor.userId,actor.assignmentId,operation,operation.split(".").slice(0,2).join("_"),recordId,version,operation,now),
      db.prepare("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),id,operation,operation.split(".").slice(0,2).join("_"),recordId,version,JSON.stringify({id:recordId,...(typeof state==="string"?{state}:{}),companyId:eventScope.companyId,branchId:eventScope.branchId,departmentId:eventScope.departmentId,version}),now),
      db.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(id),
    ]);
    return result;
  } catch (error) {
    const committed = await prior();
    if (committed) return committed;
    if (/constraint|UNIQUE/i.test(String(error))) throw new ApiError(409,"CONFLICT","The record changed, a reference is unavailable, or this value already exists. Refresh and review again.");
    throw error;
  }
}
