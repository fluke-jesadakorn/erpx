import "server-only";
import { randomBytes, randomUUID } from "node:crypto";
import { hasPermission } from "@/lib/permissions";
import { modulesForRole } from "@/lib/business-roles";
import { createConnectionSchema, type ConnectionScope, type ExternalConnection } from "@/lib/executive/connections";
import type { WorkContext, WorkAssignment } from "../authorization";
import { holdingBindingBySlug, holdingResources } from "../runtime";
import type { SqlDatabase } from "../runtime/types";
import { isTransactionalSqlDatabase } from "../runtime/types";
import { loadIamAuthority } from "../iam-store";
import { ApiError } from "../http";
import { sha256 } from "../crypto";
import { betaAccess } from "../beta-access";
import { currentAuthority, executiveEligible } from "./access";

export type TokenRow = {
  id: string; name: string; user_id: string; holding_id: string; company_id: string;
  assignment_id: string; assignment_version: number; role: string;
  assignment_scope_kind: "holding" | "branch" | "department"; assignment_scope_node_id: string | null;
  branch_id: string | null; department_id: string | null; scope_label: string;
  created_at: number; expires_at: number; revoked_at: number | null; last_used_at: number | null;
};
const tokenColumns = "id,name,user_id,holding_id,company_id,assignment_id,assignment_version,role,assignment_scope_kind,assignment_scope_node_id,branch_id,department_id,scope_label,created_at,expires_at,revoked_at,last_used_at";
type Grant = { id: string; role: string; version: number; scope_kind: TokenRow["assignment_scope_kind"]; scope_node_id: string | null; branch_id: string | null; modules_json: string; valid_until: number };
const invalidToken = () => new ApiError(401, "INVALID_TOKEN", "This Executive token is invalid, expired or revoked.");

export async function connectionEligible(db: SqlDatabase, actor: WorkContext) {
  return hasPermission(actor, "executive.access") && await executiveEligible(db, actor) && !actor.beta && !actor.iam?.emergencyUntil;
}
async function requireConnectionAccess(db: SqlDatabase, actor: WorkContext) {
  await currentAuthority(db, actor);
  if (!await connectionEligible(db, actor)) throw new ApiError(403, "EXECUTIVE_CONNECTION_FORBIDDEN", "An active Executive access grant and eligible executive work role are required.");
}

/** Only descendants of the selected browser workspace may be chosen. */
export async function connectionScopes(db: SqlDatabase, actor: WorkContext): Promise<ConnectionScope[]> {
  const rows = (await db.prepare(`SELECT n.id,n.kind,n.display_name,n.branch_id,b.display_name branch_name
    FROM organization_nodes n LEFT JOIN organization_nodes b ON b.id=n.branch_id AND b.company_id=n.company_id AND b.status='active'
    WHERE n.company_id=? AND n.status='active' AND n.kind IN ('company','branch','department')
      AND (n.branch_id IS NULL OR b.id IS NOT NULL)
      AND (?::text IS NULL OR n.id=? OR n.branch_id=?)
      AND (?::text IS NULL OR n.id=?) ORDER BY n.kind,n.display_name,n.id`)
    .bind(actor.companyId, actor.branchId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId)
    .all<{id:string;kind:string;display_name:string;branch_id:string|null;branch_name:string|null}>()).results;
  return rows.map(n => ({ id: n.id, label: n.kind === "company" ? `${n.display_name} · Company-wide` : n.kind === "branch" ? n.display_name : [n.branch_name,n.display_name].filter(Boolean).join(" · "), branchId: n.kind === "branch" ? n.id : n.branch_id, departmentId: n.kind === "department" ? n.id : null }));
}

export async function findExecutiveToken(control: SqlDatabase, authorization: string | null): Promise<TokenRow> {
  const match = /^Bearer (erpx_exec_[A-Za-z0-9_-]{43})$/i.exec(authorization ?? "");
  if (!match) throw invalidToken();
  const row = await control.prepare(`SELECT ${tokenColumns} FROM executive_external_tokens WHERE secret_hash=?`).bind(await sha256(match[1])).first<TokenRow>();
  if (!row) throw invalidToken();
  return row;
}

/** This principal has no browser/session fallback. Company, role and scope come only from persisted token bindings. */
export async function resolveExecutivePrincipal(control: SqlDatabase, token: TokenRow) {
  const live = await control.prepare(`SELECT ${tokenColumns} FROM executive_external_tokens WHERE id=? AND revoked_at IS NULL AND expires_at>?`).bind(token.id, Date.now()).first<TokenRow>();
  if (!live) throw invalidToken();
  const binding = await control.prepare("SELECT slug FROM holding_bindings WHERE holding_id=? AND status='active'").bind(live.holding_id).first<{slug:string}>();
  if (!binding) throw invalidToken();
  const holding = await holdingBindingBySlug(binding.slug);
  if (holding.id !== live.holding_id) throw invalidToken();
  const { write: db } = await holdingResources(holding.id);
  const grant = await db.prepare(`SELECT a.id,a.role,a.version,a.scope_kind,a.scope_node_id,a.branch_id,a.modules_json,a.valid_until
    FROM assignments a JOIN organization_nodes c ON c.id=a.company_id AND c.status='active' AND c.kind='company'
    WHERE a.id=? AND a.company_id=? AND a.user_id=? AND a.role=? AND a.version=? AND a.scope_kind=?
      AND a.scope_node_id IS NOT DISTINCT FROM ?::text AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>?
      AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?)
      AND EXISTS(SELECT 1 FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision
        WHERE r.id=a.role AND r.company_id IN ('*',a.company_id) AND r.status='active' AND v.scopes_json @> jsonb_build_array(a.scope_kind))`)
    .bind(live.assignment_id,live.company_id,live.user_id,live.role,live.assignment_version,live.assignment_scope_kind,live.assignment_scope_node_id,Date.now(),Date.now(),Date.now(),Date.now()).first<Grant>();
  if (!grant) throw new ApiError(403,"EXECUTIVE_ACCESS_CHANGED","The token's work assignment has changed or is no longer active. Create a new connection with an active role.");
  const iam = { ...await loadIamAuthority(db,live.company_id,live.user_id,grant.role), queuedAssignmentVersion: Number(grant.version) };
  const base = {holdingId:holding.id,holdingSlug:holding.slug,holdingName:holding.displayName,companyId:live.company_id};
  const skeleton = {...base,branchId:live.branch_id,departmentId:live.department_id} as WorkContext;
  const scopes = await connectionScopes(db,skeleton);
  const selected = scopes.find(n=>n.id===(live.department_id??live.branch_id??live.company_id));
  if (!selected || selected.branchId!==live.branch_id || selected.departmentId!==live.department_id ||
    grant.scope_kind==="branch" && live.branch_id!==grant.scope_node_id ||
    grant.scope_kind==="department" && live.department_id!==grant.scope_node_id)
    throw new ApiError(403,"EXECUTIVE_ACCESS_CHANGED","The token's organization scope is no longer active.");
  const scope = live.department_id ? {...base,kind:"department" as const,branchId:live.branch_id,departmentId:live.department_id,departmentSlug:"",departmentName:selected.label}
    : live.branch_id ? {...base,kind:"branch" as const,branchId:live.branch_id,branchSlug:"",branchName:selected.label}
    : {...base,kind:"holding" as const};
  const assignment: WorkAssignment = {id:grant.id,role:grant.role,version:Number(grant.version),companyId:live.company_id,branchId:grant.branch_id,scopeKind:grant.scope_kind,scopeNodeId:grant.scope_node_id,modules:JSON.parse(grant.modules_json),validUntil:Number(grant.valid_until)};
  const beta = await betaAccess(db,live.company_id,new Headers());
  const assignedModules = beta ? [] : grant.role.startsWith("custom_") ? [...new Set(iam.permissions.filter(p=>p.startsWith("module.")).map(p=>p.split(".")[1]))] : modulesForRole(grant.role,assignment.modules);
  const modules = iam.permissions.includes("module.people_payroll.read") ? [...new Set([...assignedModules,"people_payroll"])] : assignedModules.filter(m=>m!=="people_payroll");
  const actor: WorkContext = {...base,iam,userId:live.user_id,name:"",email:"",assignmentId:grant.id,role:grant.role,branchId:live.branch_id,departmentId:live.department_id,scope,assignmentScope:grant.scope_kind,inherited:grant.scope_kind!==scope.kind,modules,beta,assignments:[assignment],workspaces:[]};
  await requireConnectionAccess(db,actor);
  return {token:live,actor,db};
}
export type ExecutivePrincipal = Awaited<ReturnType<typeof resolveExecutivePrincipal>>;
export async function recheckExecutivePrincipal(control: SqlDatabase, principal: ExecutivePrincipal) {
  const fresh = await resolveExecutivePrincipal(control,principal.token);
  // Permission/identity changes during a read must not release an earlier, broader result.
  if (JSON.stringify([fresh.actor.iam,fresh.actor.modules])!==JSON.stringify([principal.actor.iam,principal.actor.modules]))
    throw new ApiError(403,"EXECUTIVE_ACCESS_CHANGED","Executive access changed during this request. Retry with current access.");
}

export async function listExternalConnections(control: SqlDatabase, db: SqlDatabase, actor: WorkContext) {
  await currentAuthority(db,actor);
  const canCreate = await connectionEligible(db,actor);
  const rows = (await control.prepare(`SELECT ${tokenColumns} FROM executive_external_tokens WHERE user_id=? AND holding_id=? AND company_id=? ORDER BY created_at DESC LIMIT 100`).bind(actor.userId,actor.holdingId,actor.companyId).all<TokenRow>()).results;
  const connections: ExternalConnection[] = [];
  for (const r of rows) {
    let status:ExternalConnection["status"] = r.revoked_at!==null ? "revoked" : Number(r.expires_at)<=Date.now() ? "expired" : "active";
    if(status==="active")try{await resolveExecutivePrincipal(control,r);}catch(e){if(e instanceof ApiError && [401,403].includes(e.status))status="access_changed";else throw e;}
    connections.push({id:r.id,name:r.name,role:r.role,scope:r.scope_label,status,createdAt:Number(r.created_at),expiresAt:Number(r.expires_at),lastUsedAt:r.last_used_at===null?null:Number(r.last_used_at)});
  }
  await currentAuthority(db,actor);
  return {canCreate,scopes:canCreate?await connectionScopes(db,actor):[],connections};
}
export async function createExternalConnection(control: SqlDatabase, db: SqlDatabase, actor: WorkContext, raw: unknown, commandKey: string) {
  const id = createCommandKey(commandKey);
  const input = createConnectionSchema.parse(raw);
  await requireConnectionAccess(db,actor);
  const selected = (await connectionScopes(db,actor)).find(s=>s.id===input.scopeId);
  if(!selected)throw new ApiError(403,"CONNECTION_SCOPE_FORBIDDEN","Choose a scope covered by the current workspace.");
  const grant = await db.prepare("SELECT id,role,version,scope_kind,scope_node_id,branch_id,modules_json,valid_until FROM assignments WHERE id=? AND company_id=? AND user_id=?").bind(actor.assignmentId,actor.companyId,actor.userId).first<Grant>();
  if(!grant)throw new ApiError(403,"EXECUTIVE_ACCESS_CHANGED","Select an active work role.");
  const secret=`erpx_exec_${randomBytes(32).toString("base64url")}`,now=Date.now(),expiresAt=Math.min(now+input.expiresDays*86400000,Number(grant.valid_until));
  if(!isTransactionalSqlDatabase(control))throw new ApiError(503,"EXECUTIVE_UNAVAILABLE","Connection storage is unavailable.");
  await control.transaction(async tx=>{
    await tx.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,7092))").bind(`${actor.userId}:${actor.companyId}`).run();
    const activeCount = await tx.prepare("SELECT count(*)::integer count FROM executive_external_tokens WHERE user_id=? AND company_id=? AND revoked_at IS NULL AND expires_at>?").bind(actor.userId,actor.companyId,now).first<{count:number}>();
    if(Number(activeCount?.count)>=20)throw new ApiError(409,"CONNECTION_LIMIT","Revoke an existing connection before creating another (maximum 20 active connections).");
    const inserted=await tx.prepare(`INSERT INTO executive_external_tokens(id,secret_hash,name,user_id,holding_id,company_id,assignment_id,assignment_version,role,assignment_scope_kind,assignment_scope_node_id,branch_id,department_id,scope_label,created_at,expires_at)
      VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO NOTHING RETURNING id`).bind(id,await sha256(secret),input.name,actor.userId,actor.holdingId,actor.companyId,grant.id,grant.version,grant.role,grant.scope_kind,grant.scope_node_id,selected.branchId,selected.departmentId,selected.label,now,expiresAt).first();
    if(!inserted)throw new ApiError(409,"CONNECTION_ALREADY_CREATED","This request already created a connection. Its secret cannot be displayed again. Revoke it and create a new connection if the first response was lost.");
    await tx.prepare(`INSERT INTO executive_mcp_audit(id,token_id,user_id,company_id,assignment_id,branch_id,department_id,action,outcome,code,started_at,duration_ms)
      VALUES(?,?,?,?,?,?,?,'connection.create','success','OK',?,0)`).bind(randomUUID(),id,actor.userId,actor.companyId,actor.assignmentId,selected.branchId,selected.departmentId,now).run();
  });
  try{await requireConnectionAccess(db,actor);}catch(e){await control.prepare("UPDATE executive_external_tokens SET revoked_at=? WHERE id=?").bind(Date.now(),id).run();throw e;}
  return {id,secret,expiresAt,scope:selected.label,role:grant.role};
}
function createCommandKey(value:string) {
  if(!/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/i.test(value))throw new ApiError(422,"COMMAND_KEY_REQUIRED","Use a UUID Idempotency-Key.");
  return value.toLowerCase();
}
/** Revoke your own credential even after losing permission to create one. */
export async function revokeExternalConnection(control: SqlDatabase, db: SqlDatabase, actor: WorkContext, id: string) {
  await currentAuthority(db,actor);
  const result=await control.prepare("UPDATE executive_external_tokens SET revoked_at=COALESCE(revoked_at,?) WHERE id=? AND user_id=? AND holding_id=? AND company_id=? RETURNING id").bind(Date.now(),id,actor.userId,actor.holdingId,actor.companyId).first<{id:string}>();
  if(!result)throw new ApiError(404,"CONNECTION_NOT_FOUND","This connection does not exist.");
  return result;
}
export async function consumeExecutiveRateLimit(control: SqlDatabase, token: TokenRow) {
  const window=Math.floor(Date.now()/60000)*60000;
  const row=await control.prepare(`INSERT INTO executive_mcp_rate_limits(user_id,company_id,window_start,requests) VALUES(?,?,?,1)
    ON CONFLICT(user_id,company_id) DO UPDATE SET window_start=excluded.window_start,
      requests=CASE WHEN executive_mcp_rate_limits.window_start=excluded.window_start THEN executive_mcp_rate_limits.requests+1 ELSE 1 END
    RETURNING requests`).bind(token.user_id,token.company_id,window).first<{requests:number}>();
  if(!row || Number(row.requests)>60)throw new ApiError(429,"RATE_LIMITED","Executive MCP allows 60 requests per minute per user and company. Retry after the current minute.");
}
