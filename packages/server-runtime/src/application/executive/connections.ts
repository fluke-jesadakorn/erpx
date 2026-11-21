import type {WorkContext,WorkAssignment} from "@erpx/server-identity_access/public/authorization";
import type {loadIamAuthority} from "@erpx/server-identity_access/public/iam-store";
import {modulesForRole} from "@erpx/feature-identity_access/lib/business-roles";
import type {ConnectionScope} from "@erpx/feature-shared/lib/executive/connections";
import type {TokenRow,ExecutiveConnectionGrant} from "@erpx/server-runtime/domain/executive/connections";
import type {Clock} from "@erpx/server-shared/application/ports";
type ConnectionRows<Row>={results:Row[]};
/**
 * Retained MCP token resolution. Connection creation, listing and revocation are
 * owned by the native Runtime producer; this unit never writes a connection,
 * audit or rate-limit row.
 */
export type ExecutiveConnectionUnit={organizationScopes(facts:{value1:string;value2:string | null;value3:string | null;value4:string | null;value5:string | null;value6:string | null;}):Promise<ConnectionRows<{ id: string; kind: string; display_name: string; branch_id: string | null; branch_name: string | null; }>>;tokenByHash(facts:{value1:string;}):Promise<TokenRow | null>;activeToken(facts:{value1:string;value2:number;}):Promise<TokenRow | null>;holdingBinding(facts:{value1:string;}):Promise<{ slug: string; } | null>;activeGrant(facts:{value1:string;value2:string;value3:string;value4:string;value5:number;value6:"holding" | "branch" | "department";value7:string | null;value8:number;value9:number;value10:number;value11:number;}):Promise<ExecutiveConnectionGrant | null>;};
export function createExecutiveConnections(ports:{clock:Clock;hash(value:string):Promise<string>;error(status:number,code:string,message:string):Error;errorStatus(error:unknown):number|undefined;hasPermission(actor:WorkContext,permission:string):boolean;currentAuthority(db:ExecutiveConnectionUnit,actor:WorkContext):Promise<unknown>;executiveEligible(db:ExecutiveConnectionUnit,actor:WorkContext):Promise<boolean>;binding(slug:string):Promise<{id:WorkContext["holdingId"];slug:string;displayName:string}>;resource(id:WorkContext["holdingId"]):Promise<{write:ExecutiveConnectionUnit}>;authority(db:ExecutiveConnectionUnit,companyId:string,userId:string,role:string):ReturnType<typeof loadIamAuthority>;beta(db:ExecutiveConnectionUnit,companyId:string):Promise<boolean>}){
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

return {connectionEligible,connectionScopes,findExecutiveToken,resolveExecutivePrincipal,recheckExecutivePrincipal};}
