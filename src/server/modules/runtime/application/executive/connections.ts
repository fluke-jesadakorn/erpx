import type {WorkContext,WorkAssignment} from "../../../identity_access/public/authorization";
import type {loadIamAuthority} from "../../../identity_access/public/iam-store";
import {modulesForRole} from "@/lib/business-roles";
import {createConnectionSchema,type ConnectionScope,type ExternalConnection} from "@/lib/executive/connections";
import type {TokenRow,ExecutiveConnectionGrant} from "../../domain/executive/connections";
import type {Clock,IdentifierSource} from "../../../shared/application/ports";
type ConnectionRows<Row>={results:Row[]};
export type ExecutiveConnectionUnit={transactional:boolean;transaction<Result>(action:(tx:ExecutiveConnectionUnit)=>Promise<Result>):Promise<Result>;organizationScopes(facts:{value1:string;value2:string | null;value3:string | null;value4:string | null;value5:string | null;value6:string | null;}):Promise<ConnectionRows<{ id: string; kind: string; display_name: string; branch_id: string | null; branch_name: string | null; }>>;tokenByHash(facts:{value1:string;}):Promise<TokenRow | null>;activeToken(facts:{value1:string;value2:number;}):Promise<TokenRow | null>;holdingBinding(facts:{value1:string;}):Promise<{ slug: string; } | null>;activeGrant(facts:{value1:string;value2:string;value3:string;value4:string;value5:number;value6:"holding" | "branch" | "department";value7:string | null;value8:number;value9:number;value10:number;value11:number;}):Promise<ExecutiveConnectionGrant | null>;listTokens(facts:{value1:string;value2:string;value3:string;}):Promise<ConnectionRows<TokenRow>>;selectedGrant(facts:{value1:string;value2:string;value3:string;}):Promise<ExecutiveConnectionGrant | null>;lockPrincipal(facts:{value1:string;}):Promise<ConnectionRows<Record<string, unknown>>>;countActive(facts:{value1:string;value2:string;value3:number;}):Promise<{ count: number; } | null>;insertToken(facts:{value1:string;value2:string;value3:string;value4:string;value5:string;value6:string;value7:string;value8:number;value9:string;value10:"holding" | "branch" | "department";value11:string | null;value12:string | null;value13:string | null;value14:string;value15:number;value16:number;}):Promise<Record<string, unknown> | null>;auditCreate(facts:{value1:string;value2:string;value3:string;value4:string;value5:string;value6:string | null;value7:string | null;value8:number;}):Promise<ConnectionRows<Record<string, unknown>>>;revokeFailed(facts:{value1:number;value2:string;}):Promise<ConnectionRows<Record<string, unknown>>>;revokeOwned(facts:{value1:number;value2:string;value3:string;value4:string;value5:string;}):Promise<{ id: string; } | null>;consumeRate(facts:{value1:string;value2:string;value3:number;}):Promise<{ requests: number; } | null>;};
export function createExecutiveConnections(ports:{clock:Clock;identifiers:IdentifierSource;secret():string;hash(value:string):Promise<string>;error(status:number,code:string,message:string):Error;errorStatus(error:unknown):number|undefined;hasPermission(actor:WorkContext,permission:string):boolean;currentAuthority(db:ExecutiveConnectionUnit,actor:WorkContext):Promise<unknown>;executiveEligible(db:ExecutiveConnectionUnit,actor:WorkContext):Promise<boolean>;binding(slug:string):Promise<{id:WorkContext["holdingId"];slug:string;displayName:string}>;resource(id:WorkContext["holdingId"]):Promise<{write:ExecutiveConnectionUnit}>;authority(db:ExecutiveConnectionUnit,companyId:string,userId:string,role:string):ReturnType<typeof loadIamAuthority>;beta(db:ExecutiveConnectionUnit,companyId:string):Promise<boolean>}){
const invalidToken = () => ports.error(401, "INVALID_TOKEN", "This Executive token is invalid, expired or revoked.");

async function connectionEligible(db: ExecutiveConnectionUnit, actor: WorkContext) {
  return ports.hasPermission(actor, "executive.access") && await ports.executiveEligible(db, actor) && !actor.beta && !actor.iam?.emergencyUntil;
}
async function requireConnectionAccess(db: ExecutiveConnectionUnit, actor: WorkContext) {
  await ports.currentAuthority(db, actor);
  if (!await connectionEligible(db, actor)) throw ports.error(403, "EXECUTIVE_CONNECTION_FORBIDDEN", "An active Executive access grant and eligible executive work role are required.");
}

/** Only descendants of the selected browser workspace may be chosen. */
async function connectionScopes(db: ExecutiveConnectionUnit, actor: WorkContext): Promise<ConnectionScope[]> {
  const rows = (await db.organizationScopes({value1:actor.companyId,value2:actor.branchId,value3:actor.branchId,value4:actor.branchId,value5:actor.departmentId,value6:actor.departmentId})).results;
  return rows.map(n => ({ id: n.id, label: n.kind === "company" ? `${n.display_name} · Company-wide` : n.kind === "branch" ? n.display_name : [n.branch_name,n.display_name].filter(Boolean).join(" · "), branchId: n.kind === "branch" ? n.id : n.branch_id, departmentId: n.kind === "department" ? n.id : null }));
}

async function findExecutiveToken(control: ExecutiveConnectionUnit, authorization: string | null): Promise<TokenRow> {
  const match = /^Bearer (erpx_exec_[A-Za-z0-9_-]{43})$/i.exec(authorization ?? "");
  if (!match) throw invalidToken();
  const row = await control.tokenByHash({value1:await ports.hash(match[1])});
  if (!row) throw invalidToken();
  return row;
}

/** This principal has no browser/session fallback. Company, role and scope come only from persisted token bindings. */
async function resolveExecutivePrincipal(control: ExecutiveConnectionUnit, token: TokenRow) {
  const live = await control.activeToken({value1:token.id,value2:ports.clock.now()});
  if (!live) throw invalidToken();
  const binding = await control.holdingBinding({value1:live.holding_id});
  if (!binding) throw invalidToken();
  const holding = await ports.binding(binding.slug);
  if (holding.id !== live.holding_id) throw invalidToken();
  const { write: db } = await ports.resource(holding.id);
  const grant = await db.activeGrant({value1:live.assignment_id,value2:live.company_id,value3:live.user_id,value4:live.role,value5:live.assignment_version,value6:live.assignment_scope_kind,value7:live.assignment_scope_node_id,value8:ports.clock.now(),value9:ports.clock.now(),value10:ports.clock.now(),value11:ports.clock.now()});
  if (!grant) throw ports.error(403,"EXECUTIVE_ACCESS_CHANGED","The token's work assignment has changed or is no longer active. Create a new connection with an active role.");
  const iam = { ...await ports.authority(db,live.company_id,live.user_id,grant.role), queuedAssignmentVersion: Number(grant.version) };
  const base = {holdingId:holding.id,holdingSlug:holding.slug,holdingName:holding.displayName,companyId:live.company_id};
  const skeleton = {...base,branchId:live.branch_id,departmentId:live.department_id} as WorkContext;
  const scopes = await connectionScopes(db,skeleton);
  const selected = scopes.find(n=>n.id===(live.department_id??live.branch_id??live.company_id));
  if (!selected || selected.branchId!==live.branch_id || selected.departmentId!==live.department_id ||
    grant.scope_kind==="branch" && live.branch_id!==grant.scope_node_id ||
    grant.scope_kind==="department" && live.department_id!==grant.scope_node_id)
    throw ports.error(403,"EXECUTIVE_ACCESS_CHANGED","The token's organization scope is no longer active.");
  const scope = live.department_id ? {...base,kind:"department" as const,branchId:live.branch_id,departmentId:live.department_id,departmentSlug:"",departmentName:selected.label}
    : live.branch_id ? {...base,kind:"branch" as const,branchId:live.branch_id,branchSlug:"",branchName:selected.label}
    : {...base,kind:"holding" as const};
  const assignment: WorkAssignment = {id:grant.id,role:grant.role,version:Number(grant.version),companyId:live.company_id,branchId:grant.branch_id,scopeKind:grant.scope_kind,scopeNodeId:grant.scope_node_id,modules:JSON.parse(grant.modules_json),validUntil:Number(grant.valid_until)};
  const beta = await ports.beta(db,live.company_id);
  const assignedModules = beta ? [] : grant.role.startsWith("custom_") ? [...new Set(iam.permissions.filter(p=>p.startsWith("module.")).map(p=>p.split(".")[1]))] : modulesForRole(grant.role,assignment.modules);
  const modules = iam.permissions.includes("module.people_payroll.read") ? [...new Set([...assignedModules,"people_payroll"])] : assignedModules.filter(m=>m!=="people_payroll");
  const actor: WorkContext = {...base,iam,userId:live.user_id,name:"",email:"",assignmentId:grant.id,role:grant.role,branchId:live.branch_id,departmentId:live.department_id,scope,assignmentScope:grant.scope_kind,inherited:grant.scope_kind!==scope.kind,modules,beta,assignments:[assignment],workspaces:[]};
  await requireConnectionAccess(db,actor);
  return {token:live,actor,db};
}
type ExecutivePrincipal = Awaited<ReturnType<typeof resolveExecutivePrincipal>>;
async function recheckExecutivePrincipal(control: ExecutiveConnectionUnit, principal: ExecutivePrincipal) {
  const fresh = await resolveExecutivePrincipal(control,principal.token);
  // Permission/identity changes during a read must not release an earlier, broader result.
  if (JSON.stringify([fresh.actor.iam,fresh.actor.modules])!==JSON.stringify([principal.actor.iam,principal.actor.modules]))
    throw ports.error(403,"EXECUTIVE_ACCESS_CHANGED","Executive access changed during this request. Retry with current access.");
}

async function listExternalConnections(control: ExecutiveConnectionUnit, db: ExecutiveConnectionUnit, actor: WorkContext) {
  await ports.currentAuthority(db,actor);
  const canCreate = await connectionEligible(db,actor);
  const rows = (await control.listTokens({value1:actor.userId,value2:actor.holdingId,value3:actor.companyId})).results;
  const connections: ExternalConnection[] = [];
  for (const r of rows) {
    let status:ExternalConnection["status"] = r.revoked_at!==null ? "revoked" : Number(r.expires_at)<=ports.clock.now() ? "expired" : "active";
    if(status==="active")try{await resolveExecutivePrincipal(control,r);}catch(e){if([401,403].includes(ports.errorStatus(e)??0))status="access_changed";else throw e;}
    connections.push({id:r.id,name:r.name,role:r.role,scope:r.scope_label,status,createdAt:Number(r.created_at),expiresAt:Number(r.expires_at),lastUsedAt:r.last_used_at===null?null:Number(r.last_used_at)});
  }
  await ports.currentAuthority(db,actor);
  return {canCreate,scopes:canCreate?await connectionScopes(db,actor):[],connections};
}
async function createExternalConnection(control: ExecutiveConnectionUnit, db: ExecutiveConnectionUnit, actor: WorkContext, raw: unknown, commandKey: string) {
  const id = createCommandKey(commandKey);
  const input = createConnectionSchema.parse(raw);
  await requireConnectionAccess(db,actor);
  const selected = (await connectionScopes(db,actor)).find(s=>s.id===input.scopeId);
  if(!selected)throw ports.error(403,"CONNECTION_SCOPE_FORBIDDEN","Choose a scope covered by the current workspace.");
  const grant = await db.selectedGrant({value1:actor.assignmentId,value2:actor.companyId,value3:actor.userId});
  if(!grant)throw ports.error(403,"EXECUTIVE_ACCESS_CHANGED","Select an active work role.");
  const secret=`erpx_exec_${ports.secret()}`,now=ports.clock.now(),expiresAt=Math.min(now+input.expiresDays*86400000,Number(grant.valid_until));
  if(!control.transactional)throw ports.error(503,"EXECUTIVE_UNAVAILABLE","Connection storage is unavailable.");
  await control.transaction(async tx=>{
    await tx.lockPrincipal({value1:`${actor.userId}:${actor.companyId}`});
    const activeCount = await tx.countActive({value1:actor.userId,value2:actor.companyId,value3:now});
    if(Number(activeCount?.count)>=20)throw ports.error(409,"CONNECTION_LIMIT","Revoke an existing connection before creating another (maximum 20 active connections).");
    const inserted=await tx.insertToken({value1:id,value2:await ports.hash(secret),value3:input.name,value4:actor.userId,value5:actor.holdingId,value6:actor.companyId,value7:grant.id,value8:grant.version,value9:grant.role,value10:grant.scope_kind,value11:grant.scope_node_id,value12:selected.branchId,value13:selected.departmentId,value14:selected.label,value15:now,value16:expiresAt});
    if(!inserted)throw ports.error(409,"CONNECTION_ALREADY_CREATED","This request already created a connection. Its secret cannot be displayed again. Revoke it and create a new connection if the first response was lost.");
    await tx.auditCreate({value1:ports.identifiers.next(),value2:id,value3:actor.userId,value4:actor.companyId,value5:actor.assignmentId,value6:selected.branchId,value7:selected.departmentId,value8:now});
  });
  try{await requireConnectionAccess(db,actor);}catch(e){await control.revokeFailed({value1:ports.clock.now(),value2:id});throw e;}
  return {id,secret,expiresAt,scope:selected.label,role:grant.role};
}
function createCommandKey(value:string) {
  if(!/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/i.test(value))throw ports.error(422,"COMMAND_KEY_REQUIRED","Use a UUID Idempotency-Key.");
  return value.toLowerCase();
}
/** Revoke your own credential even after losing permission to create one. */
async function revokeExternalConnection(control: ExecutiveConnectionUnit, db: ExecutiveConnectionUnit, actor: WorkContext, id: string) {
  await ports.currentAuthority(db,actor);
  const result=await control.revokeOwned({value1:ports.clock.now(),value2:id,value3:actor.userId,value4:actor.holdingId,value5:actor.companyId});
  if(!result)throw ports.error(404,"CONNECTION_NOT_FOUND","This connection does not exist.");
  return result;
}
async function consumeExecutiveRateLimit(control: ExecutiveConnectionUnit, token: TokenRow) {
  const window=Math.floor(ports.clock.now()/60000)*60000;
  const row=await control.consumeRate({value1:token.user_id,value2:token.company_id,value3:window});
  if(!row || Number(row.requests)>60)throw ports.error(429,"RATE_LIMITED","Executive MCP allows 60 requests per minute per user and company. Retry after the current minute.");
}

return {connectionEligible,connectionScopes,findExecutiveToken,resolveExecutivePrincipal,recheckExecutivePrincipal,listExternalConnections,createExternalConnection,revokeExternalConnection,consumeExecutiveRateLimit};}
