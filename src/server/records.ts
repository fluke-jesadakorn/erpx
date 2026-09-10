import { requireBusinessRole } from "./responsibilities";
import "server-only";

import { z } from "zod";
import { hasModuleAccess } from "@/data/module-access";
import { findModule } from "@/data/module-registry";
import { runtimeEnv, holdingResources } from "@/server/runtime";
import type { WorkContext } from "@/server/authorization";
import { ApiError } from "@/server/http";

export type RecordField = {
  key: string;
  label: string;
  type: string;
  value: string | number | boolean;
  options?: string[];
};

export type WorkspaceRecord = {
  moduleId: string;
  id: string;
  sectionId: string;
  title: string;
  description: string;
  status: "draft" | "in_review" | "approved" | "completed" | "rejected";
  fields: RecordField[];
  version: number;
  createdBy: string;
  updatedAt: number;
};

type RecordRow = {
  module_id: string; id: string; section_id: string; title: string;
  description: string; status: WorkspaceRecord["status"]; fields_json: string;
  version: number; created_by: string; updated_at: number;
};

const fieldValue = z.union([z.string().max(4000), z.number().finite(), z.boolean()]);
export const updateRecordSchema = z.object({
  expectedVersion: z.number().int().positive(),
  fields: z.record(z.string().min(1).max(80), fieldValue),
});
const recordFieldSchema=z.object({key:z.string().min(1).max(80),label:z.string().min(1).max(120),type:z.string().min(1).max(30),value:fieldValue,options:z.array(z.string().max(120)).max(50).optional()});
export const createRecordSchema=z.object({sectionId:z.string().min(1).max(128),title:z.string().trim().min(1).max(160),description:z.string().trim().max(2000).default(""),fields:z.array(recordFieldSchema).min(1).max(64)});
export const transitionSchema = z.object({
  expectedVersion: z.number().int().positive(),
  action: z.enum(["submit", "approve", "reject", "complete", "reopen"]),
  note: z.string().max(1000).default(""),
});
export const addReferenceSchema = z.object({
  targetModule: z.string().min(1), targetId: z.string().min(1),
  kind: z.enum(["dependsOn", "blocks", "billedBy", "fulfills", "authoredWith"]),
});
export const suggestionSchema = z.object({ fieldKey: z.string().min(1).max(80) });
export const suggestionDecisionSchema = z.object({
  expectedVersion: z.number().int().positive(),
  action: z.enum(["apply", "reject"]),
});
export const addPrincipalMentionSchema = z.object({
  targetKind: z.literal("person"),
  targetId: z.string().min(1).max(128),
  targetLabel: z.string().min(1).max(160),
});

function assertModule(actor: WorkContext, moduleId: string) {
  if (!findModule(moduleId)) throw new ApiError(404, "NOT_FOUND", "Module not found.");
  if (!hasModuleAccess(actor.modules, moduleId))
    throw new ApiError(403, "MODULE_FORBIDDEN", "Change to a role with access to this module.");
}
function assertFinanceOwnerOperation(moduleId: string) {
  if (moduleId === "finance") throw new ApiError(409, "FINANCE_OWNER_REQUIRED", "Use the Finance API for accounting records; the generic workbench is read-only.");
}
function scopeSql(actor: WorkContext) {
  if (actor.departmentId) return { sql: "company_id=? AND department_id=?", args: [actor.companyId, actor.departmentId] };
  if (actor.branchId) return { sql: "company_id=? AND branch_id=?", args: [actor.companyId, actor.branchId] };
  return { sql: "company_id=?", args: [actor.companyId] };
}
function mapRecord(row: RecordRow): WorkspaceRecord {
  return { moduleId: row.module_id, id: row.id, sectionId: row.section_id, title: row.title,
    description: row.description, status: row.status, fields: JSON.parse(row.fields_json) as RecordField[],
    version: row.version, createdBy: row.created_by, updatedAt: row.updated_at };
}

export async function listRecords(actor: WorkContext, moduleId: string, sectionId: string) {
  assertModule(actor, moduleId);
  const { write } = await holdingResources(actor.holdingId);
  const scope = scopeSql(actor);
  const result = await write.prepare(`SELECT module_id,id,section_id,title,description,status,fields_json,version,created_by,updated_at
    FROM workspace_records WHERE archived_at IS NULL AND module_id=? AND section_id=? AND ${scope.sql} ORDER BY updated_at DESC,id`)
    .bind(moduleId, sectionId, ...scope.args).all<RecordRow>();
  return result.results.map(mapRecord);
}

export async function getRecord(actor: WorkContext, moduleId: string, id: string) {
  assertModule(actor, moduleId);
  const { write } = await holdingResources(actor.holdingId);
  const scope = scopeSql(actor);
  const row = await write.prepare(`SELECT module_id,id,section_id,title,description,status,fields_json,version,created_by,updated_at
    FROM workspace_records WHERE archived_at IS NULL AND module_id=? AND id=? AND ${scope.sql}`).bind(moduleId, id, ...scope.args).first<RecordRow>();
  if (!row) throw new ApiError(404, "NOT_FOUND", "Record not found in this workspace.");
  return mapRecord(row);
}

export async function createRecord(actor:WorkContext,moduleId:string,input:z.infer<typeof createRecordSchema>){
  assertFinanceOwnerOperation(moduleId);
  assertModule(actor,moduleId);const definition=findModule(moduleId)!;if(!definition.sections.some(section=>section.id===input.sectionId))throw new ApiError(422,"INVALID_SECTION","Choose a valid module section.");
  if(new Set(input.fields.map(field=>field.key)).size!==input.fields.length)throw new ApiError(422,"DUPLICATE_FIELD","Field keys must be unique.");
  const {write}=await holdingResources(actor.holdingId);const now=Date.now();const id=`rec-${moduleId}-${crypto.randomUUID().slice(0,8)}`;
  await write.prepare(`INSERT INTO workspace_records(module_id,id,section_id,title,description,status,fields_json,company_id,branch_id,department_id,version,created_by,updated_by,created_at,updated_at) VALUES(?,?,?,?,?,'draft',?,?,?,?,1,?,?,?,?)`)
    .bind(moduleId,id,input.sectionId,input.title,input.description,JSON.stringify(input.fields),actor.companyId,actor.branchId,actor.departmentId,actor.userId,actor.userId,now,now).run();
  return getRecord(actor,moduleId,id);
}

export async function archiveRecord(actor:WorkContext,moduleId:string,id:string,expectedVersion:number){assertFinanceOwnerOperation(moduleId);const current=await getRecord(actor,moduleId,id);if(current.version!==expectedVersion)throw new ApiError(409,"VERSION_CONFLICT","This record changed. Refresh before archiving it.");const {write}=await holdingResources(actor.holdingId);const result=await write.prepare(`UPDATE workspace_records SET archived_at=?,version=version+1,updated_by=?,updated_at=? WHERE module_id=? AND id=? AND version=? AND archived_at IS NULL`).bind(Date.now(),actor.userId,Date.now(),moduleId,id,expectedVersion).run();if(result.meta.changes!==1)throw new ApiError(409,"VERSION_CONFLICT","This record changed. Refresh before archiving it.");return {id,moduleId,archived:true};}

export async function updateRecord(actor: WorkContext, moduleId: string, id: string, input: z.infer<typeof updateRecordSchema>) {
  assertFinanceOwnerOperation(moduleId);
  const current = await getRecord(actor, moduleId, id);
  if (current.version !== input.expectedVersion) throw new ApiError(409, "VERSION_CONFLICT", "This record changed. Refresh and review it again.");
  const allowed = new Map(current.fields.map((field) => [field.key, field]));
  for (const key of Object.keys(input.fields)) if (!allowed.has(key)) throw new ApiError(422, "UNKNOWN_FIELD", `Unknown field: ${key}`);
  const fields = current.fields.map((field) => keyValue(field, input.fields));
  const now = Date.now();
  const { write } = await holdingResources(actor.holdingId);
  const result = await write.prepare(`UPDATE workspace_records SET fields_json=?,version=version+1,updated_by=?,updated_at=?
    WHERE module_id=? AND id=? AND version=?`).bind(JSON.stringify(fields), actor.userId, now, moduleId, id, current.version).run();
  if (result.meta.changes !== 1) throw new ApiError(409, "VERSION_CONFLICT", "This record changed. Refresh and review it again.");
  return getRecord(actor, moduleId, id);
}
function keyValue(field: RecordField, values: Record<string, string | number | boolean>) {
  return Object.prototype.hasOwnProperty.call(values, field.key) ? { ...field, value: values[field.key] } : field;
}

const nextStatus: Record<string, Partial<Record<z.infer<typeof transitionSchema>["action"], WorkspaceRecord["status"]>>> = {
  draft: { submit: "in_review" }, in_review: { approve: "approved", reject: "rejected" },
  approved: { complete: "completed", reopen: "draft" }, rejected: { reopen: "draft" }, completed: { reopen: "draft" },
};
export async function transitionRecord(actor: WorkContext, moduleId: string, id: string, input: z.infer<typeof transitionSchema>) {
  assertFinanceOwnerOperation(moduleId);
  if (["approve", "reject"].includes(input.action)) actor = requireBusinessRole(actor,["holding_owner","people_manager"]);
  const current = await getRecord(actor, moduleId, id);
  if (current.version !== input.expectedVersion) throw new ApiError(409, "VERSION_CONFLICT", "This record changed. Refresh and review it again.");
  const status = nextStatus[current.status]?.[input.action];
  if (!status) throw new ApiError(409, "INVALID_TRANSITION", `${input.action} is unavailable while this record is ${current.status}.`);
  if (["approve", "reject"].includes(input.action)) {
    if (!(["holding_owner", "people_manager"].includes(actor.role))) throw new ApiError(403, "REVIEW_FORBIDDEN", "An owner or manager assignment is required to review this record.");
    if (current.createdBy === actor.userId) throw new ApiError(409, "SELF_REVIEW_FORBIDDEN", "A different person must review this record.");
  }
  const now = Date.now(); const transitionId = crypto.randomUUID();
  const { write } = await holdingResources(actor.holdingId);
  const update = write.prepare(`UPDATE workspace_records SET status=?,version=version+1,updated_by=?,updated_at=? WHERE module_id=? AND id=? AND version=?`)
    .bind(status, actor.userId, now, moduleId, id, current.version);
  const history = write.prepare(`INSERT INTO workspace_record_transitions(id,module_id,record_id,from_status,to_status,action,note,actor_id,assignment_id,record_version,created_at)
    VALUES(?,?,?,?,?,?,?,?,?,?,?)`).bind(transitionId,moduleId,id,current.status,status,input.action,input.note,actor.userId,actor.assignmentId,current.version+1,now);
  const audit = write.prepare(`INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at)
    VALUES(?,?,?,?,?,?,?,?,?,?)`).bind(crypto.randomUUID(),transitionId,actor.userId,actor.assignmentId,`record.${input.action}`,moduleId,id,current.version+1,"workspace workflow",now);
  const results = await write.batch([update, history, audit]);
  if (results[0].meta.changes !== 1) throw new ApiError(409, "VERSION_CONFLICT", "This record changed. Refresh and review it again.");
  return getRecord(actor,moduleId,id);
}

export async function recordContext(actor: WorkContext, moduleId: string, id: string) {
  const record = await getRecord(actor,moduleId,id); const { write } = await holdingResources(actor.holdingId);
  const [edges, backlinks, mentions, suggestions, history] = await Promise.all([
    write.prepare(`SELECT target_module,target_id,kind,ordinal FROM record_reference_edges WHERE host_module=? AND host_id=? ORDER BY ordinal`).bind(moduleId,id).all<{target_module:string;target_id:string;kind:string;ordinal:number}>(),
    write.prepare(`SELECT host_module,host_id,kind FROM record_reference_edges WHERE target_module=? AND target_id=? ORDER BY created_at DESC LIMIT 50`).bind(moduleId,id).all<{host_module:string;host_id:string;kind:string}>(),
    write.prepare(`SELECT target_kind,target_id,target_label,created_at FROM record_principal_mentions WHERE host_module=? AND host_id=? ORDER BY created_at`).bind(moduleId,id).all(),
    write.prepare(`SELECT id,field_key,proposed_value_json,source_versions_json,provenance_json,state,created_at FROM record_field_suggestions WHERE module_id=? AND record_id=? ORDER BY created_at DESC LIMIT 20`).bind(moduleId,id).all(),
    write.prepare(`SELECT from_status,to_status,action,note,actor_id,record_version,created_at FROM workspace_record_transitions WHERE module_id=? AND record_id=? ORDER BY created_at DESC LIMIT 20`).bind(moduleId,id).all(),
  ]);
  const references = await Promise.all(edges.results.map(async edge => {
    try { const target=await getRecord(actor,edge.target_module,edge.target_id); return {...edge,title:target.title,version:target.version,access:"available"}; }
    catch { return {...edge,title:"Unavailable record",version:0,access:"denied"}; }
  }));
  const visibleBacklinks = (await Promise.all(backlinks.results.map(async edge => {
    try { const host=await getRecord(actor,edge.host_module,edge.host_id); return {...edge,title:host.title,version:host.version}; }
    catch { return null; }
  }))).filter(Boolean);
  return { record, references, backlinks: visibleBacklinks, mentions: mentions.results, suggestions: suggestions.results, history: history.results };
}

export async function searchReferences(actor: WorkContext, query: string) {
  const { write } = await holdingResources(actor.holdingId); const scope = scopeSql(actor);
  const modules = actor.modules.filter((id) => findModule(id)); if (!modules.length) return [];
  const placeholders = modules.map(() => "?").join(","); const term = `%${query.replace(/^@/, "")}%`;
  const result = await write.prepare(`SELECT module_id,id,title,status,version FROM workspace_records WHERE archived_at IS NULL AND ${scope.sql}
    AND module_id IN (${placeholders}) AND (module_id||'/'||id ILIKE ? OR title ILIKE ?) ORDER BY updated_at DESC LIMIT 12`)
    .bind(...scope.args,...modules,term,term).all();
  const env=await runtimeEnv(); const needle=query.replace(/^@(?:user\/)?/,"").toLowerCase();
  const people=await env.CONTROL_DB.prepare(`SELECT user_id,display_name FROM principals WHERE status='active' AND lower(display_name) LIKE ? ORDER BY display_name LIMIT 8`).bind(`%${needle}%`).all<{user_id:string;display_name:string}>();
  const eligiblePeople=[];
  for(const person of people.results){const assignment=await write.prepare(`SELECT 1 AS ok FROM assignments WHERE user_id=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>? LIMIT 1`).bind(person.user_id,Date.now(),Date.now()).first();if(assignment)eligiblePeople.push({kind:"person",target_id:person.user_id,label:person.display_name});}
  return [...result.results.map(item=>({kind:"record",...item})),...eligiblePeople].slice(0,12);
}

export async function addPrincipalMention(actor:WorkContext,moduleId:string,id:string,input:z.infer<typeof addPrincipalMentionSchema>){
  await getRecord(actor,moduleId,id); const {write}=await holdingResources(actor.holdingId); const now=Date.now();
  const eligible=await write.prepare(`SELECT 1 AS ok FROM assignments WHERE user_id=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>? LIMIT 1`).bind(input.targetId,now,now).first();if(!eligible)throw new ApiError(404,"MENTION_TARGET_NOT_FOUND","That person is not available in this Holding.");
  const env=await runtimeEnv();const principal=await env.CONTROL_DB.prepare(`SELECT display_name FROM principals WHERE user_id=? AND status='active'`).bind(input.targetId).first<{display_name:string}>();if(!principal)throw new ApiError(404,"MENTION_TARGET_NOT_FOUND","That person is unavailable.");
  await write.prepare(`INSERT INTO record_principal_mentions(host_module,host_id,target_kind,target_id,target_label,created_by,created_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT DO NOTHING`).bind(moduleId,id,input.targetKind,input.targetId,principal.display_name,actor.userId,now).run();
  return recordContext(actor,moduleId,id);
}

export async function addReference(actor: WorkContext, moduleId: string, id: string, input: z.infer<typeof addReferenceSchema>) {
  await getRecord(actor,moduleId,id); await getRecord(actor,input.targetModule,input.targetId);
  if (moduleId === input.targetModule && id === input.targetId) throw new ApiError(422,"SELF_REFERENCE","A record cannot reference itself.");
  const { write } = await holdingResources(actor.holdingId);
  const count = await write.prepare(`SELECT COUNT(*) AS count FROM record_reference_edges WHERE host_module=? AND host_id=?`).bind(moduleId,id).first<{count:number}>();
  if ((count?.count ?? 0) >= 50) throw new ApiError(409,"REFERENCE_LIMIT","A record can contain up to 50 references.");
  await write.prepare(`INSERT INTO record_reference_edges(host_module,host_id,target_module,target_id,kind,ordinal,created_by,created_at)
    VALUES(?,?,?,?,?,COALESCE((SELECT MAX(ordinal)+1 FROM record_reference_edges WHERE host_module=? AND host_id=?),0),?,?) ON CONFLICT DO NOTHING`)
    .bind(moduleId,id,input.targetModule,input.targetId,input.kind,moduleId,id,actor.userId,Date.now()).run();
  return recordContext(actor,moduleId,id);
}

async function digest(value: unknown) {
  const bytes = new TextEncoder().encode(JSON.stringify(value));
  return [...new Uint8Array(await crypto.subtle.digest("SHA-256", bytes))].map((item) => item.toString(16).padStart(2,"0")).join("");
}
export async function requestSuggestion(actor: WorkContext,moduleId:string,id:string,input:z.infer<typeof suggestionSchema>) {
  const context = await recordContext(actor,moduleId,id); const field = context.record.fields.find((item) => item.key === input.fieldKey);
  if (!field) throw new ApiError(422,"UNKNOWN_FIELD","Choose an editable field.");
  const refs = (context.references as Array<{target_module:string;target_id:string;title:string;version:number;access:string}>).filter(ref=>ref.access==="available");
  if (!refs.length) throw new ApiError(409,"EVIDENCE_REQUIRED","Add at least one readable @ reference before requesting a suggestion.");
  const proposed = field.type === "number" ? refs.length : `${refs.map((ref) => `@${ref.target_module}/${ref.target_id}`).join(", ")} — reviewed for ${field.label.toLowerCase()}`;
  const now=Date.now(), suggestionId=crypto.randomUUID(); const {write}=await holdingResources(actor.holdingId);
  const sources=refs.map((ref)=>({record:`${ref.target_module}/${ref.target_id}`,version:ref.version}));
  const provenance={provider_kind:"development",model_ref:"erpx-evidence-assistant-v1",prompt_digest:await digest({moduleId,field:field.key,sources})};
  await write.prepare(`INSERT INTO record_field_suggestions(id,module_id,record_id,field_key,proposed_value_json,previous_value_digest,source_versions_json,provenance_json,state,created_by,created_at,updated_at)
    VALUES(?,?,?,?,?,?,?,?,?,?,?,?)`).bind(suggestionId,moduleId,id,field.key,JSON.stringify(proposed),await digest(field.value),JSON.stringify(sources),JSON.stringify(provenance),"ready",actor.userId,now,now).run();
  return recordContext(actor,moduleId,id);
}

export async function decideSuggestion(actor:WorkContext,moduleId:string,id:string,suggestionId:string,input:z.infer<typeof suggestionDecisionSchema>) {
  const context=await recordContext(actor,moduleId,id); const suggestion=(context.suggestions as Array<Record<string,unknown>>).find((item)=>item.id===suggestionId);
  if (!suggestion || suggestion.state!=="ready") throw new ApiError(409,"SUGGESTION_UNAVAILABLE","This suggestion is no longer ready.");
  const {write}=await holdingResources(actor.holdingId); const now=Date.now();
  if(input.action==="reject") { await write.prepare(`UPDATE record_field_suggestions SET state='rejected',applied_by=?,updated_at=? WHERE id=? AND state='ready'`).bind(actor.userId,now,suggestionId).run(); return recordContext(actor,moduleId,id); }
  if(context.record.version!==input.expectedVersion) { await write.prepare(`UPDATE record_field_suggestions SET state='conflicted',updated_at=? WHERE id=?`).bind(now,suggestionId).run(); throw new ApiError(409,"SUGGESTION_CONFLICT","The record changed. Request a new suggestion."); }
  const sourceVersions=JSON.parse(String(suggestion.source_versions_json)) as Array<{record:string;version:number}>;
  for(const source of sourceVersions){const [sourceModule,...rest]=source.record.split("/"); const current=await getRecord(actor,sourceModule,rest.join("/")); if(current.version!==source.version){await write.prepare(`UPDATE record_field_suggestions SET state='conflicted',updated_at=? WHERE id=?`).bind(now,suggestionId).run(); throw new ApiError(409,"SUGGESTION_CONFLICT","Referenced evidence changed. Request a new suggestion.");}}
  const value=JSON.parse(String(suggestion.proposed_value_json)) as string|number|boolean;
  const updated=await updateRecord(actor,moduleId,id,{expectedVersion:context.record.version,fields:{[String(suggestion.field_key)]:value}});
  await write.prepare(`UPDATE record_field_suggestions SET state='applied',applied_by=?,updated_at=? WHERE id=? AND state='ready'`).bind(actor.userId,now,suggestionId).run();
  return {...await recordContext(actor,moduleId,id),record:updated};
}
