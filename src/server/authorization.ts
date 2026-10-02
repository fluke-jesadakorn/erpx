import { measureReadPhase } from "./runtime/read-metrics";
import { refreshDirectoryMember } from "./iam-directory-refresh";
import type { PostgresDatabase } from "./runtime/postgres";
import { loadIamAuthority, iamRoles } from "./iam-store";
import type { PermissionActor } from "@/lib/iam";
import { readCompanyPreference, readWorkspacePreference } from "./workspace-preferences";
import {cache} from "react";
import { ApiError } from "./http";
import { initializeWorkSelection, type WorkAuthority } from "./work-role-store";
import type { SqlDatabase } from "./runtime/types";
import { betaAccess } from "./beta-access";
import { assertProductionFeatureRoute } from './hospitality-rollout';
import "server-only";
import { modulesForRole } from "@/lib/business-roles";
import { hasPermission } from "@/lib/permissions";

import { headers } from "next/headers";
import type { HoldingWorkspaceScope, WorkspaceScope } from "@/lib/workspace";
import {
  assignmentApplies,
  isRoleValidAtScope,
} from "@/lib/workspace-access";
export { assignmentApplies, isRoleValidAtScope } from "@/lib/workspace-access";
import { createAuth } from "./auth";
import {
  runtimeEnv,
  holdingBindingBySlug,
  holdingResources,
  type HoldingId,
} from "./runtime";

export type AssignmentScopeKind =
  | "platform"
  | "holding"
  | "branch"
  | "department";

export type ResolvedWorkspaceScope =
  | { kind: "platform" }
  | {
      kind: "holding";
      holdingId: HoldingId;
      holdingSlug: string;
      holdingName: string;
      companyId: string;
    }
  | {
      kind: "branch";
      holdingId: HoldingId;
      holdingSlug: string;
      holdingName: string;
      companyId: string;
      branchId: string;
      branchSlug: string;
      branchName: string;
    }
  | {
      kind: "department";
      holdingId: HoldingId;
      holdingSlug: string;
      holdingName: string;
      companyId: string;
      branchId: string | null;
      branchSlug?: string;
      branchName?: string;
      departmentId: string;
      departmentSlug: string;
      departmentName: string;
    };

export type WorkContext = {
  /** Trusted proxy evidence for permission recovery in the page render. */
  pageRequest?: true;
  inspection?: true;
  iam?: PermissionActor["iam"];
  authority?: WorkAuthority;
  availableAssignments?: WorkAssignment[];
  beta?: boolean;
  userId: string;
  name: string;
  email: string;
  assignmentId: string;
  role: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  holdingId: HoldingId;
  holdingSlug: string;
  holdingName: string;
  scope: Exclude<ResolvedWorkspaceScope, { kind: "platform" }>;
  assignmentScope: AssignmentScopeKind;
  inherited: boolean;
  modules: string[];
  assignments: WorkAssignment[];
  workspaces: AuthorizedWorkspace[];
};

export type AuthorizedWorkspace = {
  key: string;
  href: string;
  label: string;
  kind: "holding" | "branch" | "department";
  inherited: boolean;
};

export type PlatformContext = {
  iam?: PermissionActor["iam"];
  userId: string;
  name: string;
  email: string;
  assignmentId: string;
  role: string;
  scope: { kind: "platform" };
  modules: string[];
  assignments: WorkAssignment[];
};

export type WorkAssignment = {
  version?: number;
  label?: string;
  validUntil?: number;
  id: string;
  role: string;
  companyId: string | null;
  branchId: string | null;
  modules: string[];
  scopeKind: AssignmentScopeKind;
  scopeNodeId: string | null;
};

type OrganizationNode = {
  id: string;
  kind: "company" | "headquarters" | "branch" | "department" | "team";
  slug: string;
  display_name: string;
  parent_id: string | null;
  company_id: string;
  branch_id: string | null;
};

function cookieValue(cookieHeader: string | null, name: string) {
  const item = cookieHeader
    ?.split(";")
    .map((part) => part.trim())
    .find((part) => part.startsWith(`${name}=`));
  if (!item) return null;
  try {
    return decodeURIComponent(item.slice(name.length + 1));
  } catch {
    return null;
  }
}

export const currentSession = cache(() => measureReadPhase("session", readCurrentSession));
async function readCurrentSession() {
  const requestHeaders = await headers();
  const host = requestHeaders.get("host");
  const localHost =
    host?.startsWith("localhost") ||
    host?.startsWith("127.0.0.1") ||
    host?.startsWith("[::1]");
  const protocol =
    requestHeaders.get("x-forwarded-proto") ?? (localHost ? "http" : "https");
  const auth = await createAuth(host ? `${protocol}://${host}` : undefined);
  const session = await auth.api.getSession({ headers: requestHeaders });
  if (!session) throw new Error("UNAUTHENTICATED");
  return { session, requestHeaders };
}

async function findNode(
  database: SqlDatabase,
  kind: OrganizationNode["kind"],
  slug: string,
  companyId?: string,
) {
  return database
    .prepare(
      `SELECT id,kind,slug,display_name,parent_id,company_id,branch_id
       FROM organization_nodes WHERE kind=? AND slug=? AND status='active'
         ${companyId ? "AND company_id=?" : ""}`,
    )
    .bind(kind, slug, ...(companyId ? [companyId] : []))
    .first<OrganizationNode>();
}

async function validateNodeLineage(
  database: SqlDatabase,
  node: OrganizationNode,
) {
  const seen = new Set<string>();
  let current: OrganizationNode | null = node;
  for (let depth = 0; current && depth < 16; depth += 1) {
    if (seen.has(current.id) || current.company_id !== node.company_id)
      throw new Error("INVALID_ORGANIZATION_LINEAGE");
    seen.add(current.id);
    if (current.kind === "company") {
      if (current.id !== node.company_id || current.parent_id)
        throw new Error("INVALID_ORGANIZATION_LINEAGE");
      return;
    }
    if (!current.parent_id) throw new Error("INVALID_ORGANIZATION_LINEAGE");
    current = await database
      .prepare(
        `SELECT id,kind,slug,display_name,parent_id,company_id,branch_id
         FROM organization_nodes WHERE id=? AND status='active'`,
      )
      .bind(current.parent_id)
      .first<OrganizationNode>();
  }
  throw new Error("INVALID_ORGANIZATION_LINEAGE");
}

export type WorkspaceCompany = { id: string; name: string };
export type WorkspaceResolutionOptions = { companyId?: string };

export const listWorkspaceCompanies = cache(async (holdingId: HoldingId) => {
  const { write } = await holdingResources(holdingId);
  const { session } = await currentSession();
  const now = Date.now();
  const rows = (await write.prepare(
    `SELECT n.id,n.display_name,a.role FROM organization_nodes n
     JOIN assignments a ON a.company_id=n.id
     WHERE n.kind='company' AND n.status='active' AND a.user_id=?
       AND a.scope_kind='holding' AND a.revoked_at IS NULL AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
       AND a.valid_from<=? AND a.valid_until>?
     ORDER BY n.display_name,n.id`,
  ).bind(session.user.id, now, now).all<{id:string;display_name:string;role:string}>()).results;
  return [...new Map(rows.filter(row => isRoleValidAtScope(row.role, "holding"))
    .map(row => [row.id, { id: row.id, name: row.display_name }])).values()];
});

export async function resolveWorkspaceScope(
  requested: HoldingWorkspaceScope,
  options: WorkspaceResolutionOptions = {},
): Promise<Exclude<ResolvedWorkspaceScope, { kind: "platform" }>> {
  const binding = await holdingBindingBySlug(requested.holdingSlug);
  const { write } = await holdingResources(binding.id);
  if (requested.kind === "holding") {
    const requestHeaders = await headers();
    const contextCompany = requestHeaders.get("x-erpx-context-company");
    const frontCompanySlug = requestHeaders.get("x-erpx-front-company");
    const { session } = await currentSession();
    const savedCompany = await readCompanyPreference((await runtimeEnv()).CONTROL_DB,session.user.id,binding.id);
    // A switch keeps the company of the open browser page. Account preferences
    // initialize browsers without a choice; they cannot move an existing page.
    const preferred = options.companyId ?? contextCompany ?? cookieValue(
      requestHeaders.get("cookie"),
      `erpx_company_${binding.id}`,
    ) ?? savedCompany?.company_id;
    const available = !frontCompanySlug && !preferred
      ? await listWorkspaceCompanies(binding.id)
      : [];
    const selectedCompany = preferred ?? (available.length === 1 ? available[0].id : undefined);
    const company = frontCompanySlug
      ? await write
          .prepare(
            `SELECT id,kind,slug,display_name,parent_id,company_id,branch_id
             FROM organization_nodes WHERE kind='company' AND slug=? AND status='active'`,
          )
          .bind(frontCompanySlug)
          .first<OrganizationNode>()
      : selectedCompany
      ? await write
          .prepare(
            `SELECT id,kind,slug,display_name,parent_id,company_id,branch_id
             FROM organization_nodes WHERE kind='company' AND id=? AND status='active'`,
          )
          .bind(selectedCompany)
          .first<OrganizationNode>()
      : null;
    if (!company) throw new Error("COMPANY_REQUIRED");
    await assertFrontCompany(write, company.id);
    return {
      kind: "holding",
      holdingId: binding.id,
      holdingSlug: binding.slug,
      holdingName: binding.displayName,
      companyId: company.id,
    };
  }
  if (requested.kind === "branch") {
    const frontCompanySlug = (await headers()).get("x-erpx-front-company");
    const branch = frontCompanySlug
      ? await write.prepare(
          "SELECT n.id,n.kind,n.slug,n.display_name,n.parent_id,n.company_id,n.branch_id FROM organization_nodes n " +
          "JOIN organization_nodes c ON c.id=n.company_id AND c.kind='company' AND c.slug=? AND c.status='active' " +
          "WHERE n.kind='branch' AND n.slug=? AND n.status='active'",
        ).bind(frontCompanySlug, requested.branchSlug).first<OrganizationNode>()
      : await findNode(write, "branch", requested.branchSlug, options.companyId);
    if (!branch) throw new Error("WORKSPACE_NOT_FOUND");
    if (options.companyId && branch.company_id !== options.companyId)
      throw new Error("WORKSPACE_NOT_FOUND");
    await validateNodeLineage(write, branch);
    await assertFrontCompany(write, branch.company_id);
    return {
      kind: "branch",
      holdingId: binding.id,
      holdingSlug: binding.slug,
      holdingName: binding.displayName,
      companyId: branch.company_id,
      branchId: branch.id,
      branchSlug: branch.slug,
      branchName: branch.display_name,
    };
  }
  let branch: OrganizationNode | null = null;
  if (requested.branchSlug) {
    const frontCompanySlug = (await headers()).get("x-erpx-front-company");
    branch = frontCompanySlug
      ? await write.prepare(
          "SELECT n.id,n.kind,n.slug,n.display_name,n.parent_id,n.company_id,n.branch_id FROM organization_nodes n " +
          "JOIN organization_nodes c ON c.id=n.company_id AND c.kind='company' AND c.slug=? AND c.status='active' " +
          "WHERE n.kind='branch' AND n.slug=? AND n.status='active'",
        ).bind(frontCompanySlug, requested.branchSlug).first<OrganizationNode>()
      : await findNode(write, "branch", requested.branchSlug, options.companyId);
    if (!branch) throw new Error("WORKSPACE_NOT_FOUND");
    if (options.companyId && branch.company_id !== options.companyId)
      throw new Error("WORKSPACE_NOT_FOUND");
  }
  const department = await write
    .prepare(
      branch
        ? `SELECT id,kind,slug,display_name,parent_id,company_id,branch_id
           FROM organization_nodes WHERE kind='department' AND slug=? AND company_id=?
           AND status='active' AND branch_id=? AND parent_id=?`
        : options.companyId
          ? `SELECT id,kind,slug,display_name,parent_id,company_id,branch_id
             FROM organization_nodes WHERE kind='department' AND slug=? AND company_id=?
             AND status='active' AND branch_id IS NULL`
        : (await headers()).get("x-erpx-front-company")
          ? `SELECT n.id,n.kind,n.slug,n.display_name,n.parent_id,n.company_id,n.branch_id
             FROM organization_nodes n JOIN organization_nodes c ON c.id=n.company_id AND c.kind='company' AND c.slug=? AND c.status='active'
             WHERE n.kind='department' AND n.slug=? AND n.status='active' AND n.branch_id IS NULL`
          : `SELECT id,kind,slug,display_name,parent_id,company_id,branch_id
             FROM organization_nodes WHERE kind='department' AND slug=?
             AND status='active' AND branch_id IS NULL`,
    )
    .bind(
      ...(options.companyId && !branch ? [requested.departmentSlug, options.companyId] : (await headers()).get("x-erpx-front-company") && !branch ? [(await headers()).get("x-erpx-front-company")!, requested.departmentSlug] : [requested.departmentSlug]),
      ...(branch ? [branch.company_id, branch.id, branch.id] : []),
    )
    .first<OrganizationNode>();
  if (!department) throw new Error("WORKSPACE_NOT_FOUND");
  await validateNodeLineage(write, department);
  await assertFrontCompany(write, department.company_id);
  return {
    kind: "department",
    holdingId: binding.id,
    holdingSlug: binding.slug,
    holdingName: binding.displayName,
    companyId: department.company_id,
    branchId: department.branch_id,
    branchSlug: branch?.slug,
    branchName: branch?.display_name,
    departmentId: department.id,
    departmentSlug: department.slug,
    departmentName: department.display_name,
  };
}

async function assertFrontCompany(database: SqlDatabase, companyId: string) {
  const requestedSlug = (await headers()).get("x-erpx-front-company");
  if (!requestedSlug) return;
  const company = await database
    .prepare("SELECT 1 FROM organization_nodes WHERE id=? AND slug=? AND kind='company' AND status='active'")
    .bind(companyId, requestedSlug)
    .first();
  if (!company) throw new ApiError(404, "COMPANY_NOT_FOUND", "This storefront does not match the selected Company.");
}

export async function requestedHoldingScope(
  holdingSlug: string,
): Promise<HoldingWorkspaceScope> {
  const requestHeaders = await headers();
  const kind = requestHeaders.get("x-erpx-workspace-kind");
  const branchSlug = requestHeaders.get("x-erpx-branch-slug") ?? undefined;
  const departmentSlug =
    requestHeaders.get("x-erpx-department-slug") ?? undefined;
  if (kind === "department" && departmentSlug)
    return { kind, holdingSlug, branchSlug, departmentSlug };
  if (kind === "branch" && branchSlug)
    return { kind, holdingSlug, branchSlug };
  return { kind: "holding", holdingSlug };
}

async function listAuthorizedWorkspaces(
  database: SqlDatabase,
  holdingSlug: string,
  assignments: WorkAssignment[],
): Promise<AuthorizedWorkspace[]> {
  const holdingGrant = assignments.some(
    (assignment) => assignment.scopeKind === "holding",
  );
  const result = await database
    .prepare(
      `SELECT n.id,n.kind,n.slug,n.display_name,n.parent_id,n.branch_id,b.slug AS branch_slug
       FROM organization_nodes n
       LEFT JOIN organization_nodes b ON n.branch_id=b.id AND b.kind='branch'
       WHERE n.kind IN ('branch','department') AND n.status='active' AND n.company_id=?
       ORDER BY CASE n.kind WHEN 'branch' THEN 1 ELSE 2 END,n.display_name,n.id`,
    )
    .bind(assignments[0]?.companyId ?? "")
    .all<{
      id: string;
      kind: "branch" | "department";
      slug: string;
      display_name: string;
      parent_id: string | null;
      branch_id: string | null;
      branch_slug: string | null;
    }>();
  const workspaces: AuthorizedWorkspace[] = [];
  if (holdingGrant)
    workspaces.push({
      key: `holding:${holdingSlug}`,
      href: `/holdings/${encodeURIComponent(holdingSlug)}/home`,
      label: "Holding",
      kind: "holding",
      inherited: false,
    });
  for (const node of result.results) {
    const exact = assignments.some(
      (assignment) =>
        assignment.scopeKind === node.kind && assignment.scopeNodeId === node.id,
    );
    const branchGrant =
      node.kind === "department" &&
      assignments.some(
        (assignment) =>
          assignment.scopeKind === "branch" &&
          assignment.scopeNodeId === node.branch_id,
      );
    if (!holdingGrant && !exact && !branchGrant) continue;
    const prefix = `/holdings/${encodeURIComponent(holdingSlug)}`;
    const href =
      node.kind === "branch"
        ? `${prefix}/branches/${encodeURIComponent(node.slug)}/home`
        : node.branch_slug
          ? `${prefix}/branches/${encodeURIComponent(node.branch_slug)}/departments/${encodeURIComponent(node.slug)}/home`
          : `${prefix}/departments/${encodeURIComponent(node.slug)}/home`;
    workspaces.push({
      key: `${node.kind}:${node.id}`,
      href,
      label: node.display_name,
      kind: node.kind,
      inherited: !exact,
    });
  }
  return workspaces;
}

export type AccountAccessContext = {
 sessionId:string; expiresAt:number; userId:string; name:string; email:string;
 scope:Exclude<ResolvedWorkspaceScope,{kind:"platform"}>; assignments:WorkAssignment[]; requestHeaders:Headers;
};
export async function requireAccountAccess(requested:HoldingWorkspaceScope|string,options:WorkspaceResolutionOptions={}):Promise<AccountAccessContext> {
 const {session,requestHeaders}=await currentSession();
 const scope=await resolveWorkspaceScope(typeof requested==="string"?await requestedHoldingScope(requested):requested,options);
 const {write}=await holdingResources(scope.holdingId),now=Date.now();
 if(await write.prepare('SELECT 1 FROM iam_revoked_sessions WHERE session_id=? AND company_id=? AND user_id=?').bind(session.session.id,scope.companyId,session.user.id).first())throw new ApiError(401,'SESSION_REVOKED','Sign in again to access this company.');
 await refreshDirectoryMember(write as PostgresDatabase,scope.companyId,session.user.id);
 const rows=(await write.prepare(`SELECT a.*,c.display_name company_name,n.display_name scope_name,b.display_name branch_name FROM assignments a
 JOIN organization_nodes c ON c.id=a.company_id AND c.kind='company' AND c.status='active'
 LEFT JOIN organization_nodes n ON n.id=a.scope_node_id LEFT JOIN organization_nodes b ON b.id=n.branch_id AND b.id!=n.id
 WHERE a.user_id=? AND a.company_id=? AND a.revoked_at IS NULL AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND a.valid_from<=? AND a.valid_until>?
 AND (a.scope_kind='holding' OR (n.company_id=a.company_id AND n.status='active' AND (n.branch_id IS NULL OR EXISTS(SELECT 1 FROM organization_nodes b WHERE b.id=n.branch_id AND b.company_id=a.company_id AND b.status='active')))) ORDER BY a.role,a.id`).bind(session.user.id,scope.companyId,now,now).all<{
 id:string;role:string;company_id:string;branch_id:string|null;modules_json:string;scope_kind:"holding"|"branch"|"department";scope_node_id:string|null;version:number;valid_until:number;company_name:string;scope_name:string|null;branch_name:string|null;
 }>()).results;
 const roles=await iamRoles(write,scope.companyId,rows.map(a=>a.role));
 const validRows=[]; for(const a of rows){const definition=roles.get(a.role);if(definition?.scopes.includes(a.scope_kind))validRows.push({...a,roleName:definition.name});}
 const assignments=validRows.map(a=>({id:a.id,role:a.role,companyId:a.company_id,branchId:a.branch_id,modules:JSON.parse(a.modules_json) as string[],scopeKind:a.scope_kind,scopeNodeId:a.scope_node_id,version:a.version,validUntil:a.valid_until,label:[a.roleName,a.company_name,a.branch_name,a.scope_name].filter(Boolean).join(" · ")}));
 return {sessionId:session.session.id,expiresAt:new Date(session.session.expiresAt).getTime(),userId:session.user.id,name:session.user.name,email:session.user.email,scope,assignments,requestHeaders};
}
export async function requireWorkContext(requested:HoldingWorkspaceScope|string,options:WorkspaceResolutionOptions={}):Promise<WorkContext> {
 try {
  return await cachedWorkContext(JSON.stringify({requested:typeof requested==="string"?await requestedHoldingScope(requested):requested,options}));
 } catch (error) {
  if ((await headers()).get("x-erpx-api") === "false") {
   const { pageError } = await import("./page-error");
   return pageError(error, typeof requested === "string" ? requested : requested.holdingSlug);
  }
  throw error;
 }
}
const cachedWorkContext=cache(async (key:string):Promise<WorkContext>=>{
 const {requested,options}=JSON.parse(key) as {requested:HoldingWorkspaceScope;options:WorkspaceResolutionOptions};
 const access=await measureReadPhase("account_access",()=>requireAccountAccess(requested,options)),{scope}=access;
 const {write}=await holdingResources(scope.holdingId);
 const eligible=access.assignments.filter(a=>assignmentApplies(a,scope));
 const selection=await initializeWorkSelection(write,{sessionId:access.sessionId,userId:access.userId,companyId:scope.companyId,expiresAt:access.expiresAt},access.assignments,eligible);
 if(!selection) throw new ApiError(409,"WORK_ROLE_REQUIRED","Choose a work role to continue.");
 const assignment=eligible.find(a=>a.id===selection.assignment_id && a.version===selection.assignment_version);
 if(!assignment || selection.expires_at<=Date.now()) throw new ApiError(409,"WORK_ROLE_UNAVAILABLE","Your selected role no longer covers this workspace. Choose an available role.");
 const method=access.requestHeaders.get("x-erpx-method");
 if((access.requestHeaders.has("x-erpx-context-revision") || method && !["GET","HEAD","OPTIONS"].includes(method)) && (access.requestHeaders.get("x-erpx-context-revision")!==String(selection.revision) || access.requestHeaders.get("x-erpx-context-company")!==scope.companyId || access.requestHeaders.get("x-erpx-context-assignment")!==assignment.id))
  throw new ApiError(409,"WORK_CONTEXT_CHANGED","Your work role changed. Reload and review before submitting.");
 const beta=await betaAccess(write,scope.companyId,access.requestHeaders);
 const iam=await loadIamAuthority(write,scope.companyId,access.userId,assignment.role,access.sessionId);
 const assignedModules=beta?[]:assignment.role.startsWith("custom_")?[...new Set(iam.permissions.filter(p=>p.startsWith("module.")).map(p=>p.split(".")[1]))]:modulesForRole(assignment.role,assignment.modules);
 const modules=iam.permissions.includes("module.people_payroll.read")?[...new Set([...assignedModules,"people_payroll"])]:assignedModules.filter(module=>module!=="people_payroll");
 await assertProductionFeatureRoute(write,scope.companyId,access.requestHeaders.get('x-erpx-destination')??'',method??'GET');
 return {...(access.requestHeaders.get("x-erpx-api") === "false" ? { pageRequest: true as const } : {}),iam,userId:access.userId,name:access.name,email:access.email,assignmentId:assignment.id,role:assignment.role,
 companyId:scope.companyId,branchId:scope.kind==="holding"?null:scope.branchId,departmentId:scope.kind==="department"?scope.departmentId:null,
 holdingId:scope.holdingId,holdingSlug:scope.holdingSlug,holdingName:scope.holdingName,beta,scope,assignmentScope:assignment.scopeKind,
 inherited:assignment.scopeKind!==scope.kind,modules,assignments:[assignment],availableAssignments:access.assignments,
 authority:{sessionId:access.sessionId,revision:selection.revision,assignmentVersion:assignment.version!},
 workspaces:await listAuthorizedWorkspaces(write,scope.holdingSlug,access.assignments)};
});

export async function requirePlatformContext(): Promise<PlatformContext> {
  const { session, requestHeaders } = await currentSession();
  const env = await runtimeEnv();
  const now = Date.now();
  const result = await env.CONTROL_DB.prepare(
    `SELECT id,role,modules_json FROM platform_assignments
     WHERE user_id=? AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,'platform',floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND revoked_at IS NULL AND valid_from<=? AND valid_until>?
     ORDER BY valid_until ASC,id ASC`,
  )
    .bind(session.user.id, now, now)
    .all<{ id: string; role: string; modules_json: string }>();
  const assignments: WorkAssignment[] = result.results.map((assignment) => ({
    id: assignment.id,
    role: assignment.role,
    companyId: null,
    branchId: null,
    modules: JSON.parse(assignment.modules_json) as string[],
    scopeKind: "platform" as const,
    scopeNodeId: null,
  })).filter((assignment) => isRoleValidAtScope(assignment.role, "platform"));
  if(await env.CONTROL_DB.prepare("SELECT 1 FROM iam_revoked_sessions WHERE session_id=? AND company_id='platform' AND user_id=?").bind(session.session.id,session.user.id).first())throw new ApiError(401,'SESSION_REVOKED','Sign in again to continue.');
  const savedPlatform=await readWorkspacePreference(env.CONTROL_DB,session.user.id);
  const selectedId = cookieValue(
    requestHeaders.get("cookie"),
    "erpx_assignment_platform",
  ) ?? savedPlatform?.platform_assignment_id;
  const assignment =
    assignments.find((item) => item.id === selectedId) ?? assignments[0];
  if (!assignment) throw new Error("FORBIDDEN");
  return {
    iam: await loadIamAuthority(env.CONTROL_DB,"platform",session.user.id,assignment.role,session.session.id),
    userId: session.user.id,
    name: session.user.name,
    email: session.user.email,
    assignmentId: assignment.id,
    role: assignment.role,
    scope: { kind: "platform" },
    modules: modulesForRole(assignment.role,assignment.modules),
    assignments,
  };
}

/** Show the ERPX destination when an eligible Holding role and Platform role exist. */
export async function canEnterManageErpx(): Promise<boolean> {
  const [{ session }, platform] = await Promise.all([
    currentSession(),
    requirePlatformContext(),
  ]);
  if (session.user.id !== platform.userId) return false;
  const { write } = await holdingResources("h003");
  const now = Date.now();
  const rows = (await write.prepare(`SELECT a.role,a.company_id FROM assignments a
    JOIN organization_nodes c ON c.id=a.company_id AND c.kind='company' AND c.status='active'
    WHERE a.user_id=? AND a.scope_kind='holding' AND a.revoked_at IS NULL
      AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?)
      AND a.valid_from<=? AND a.valid_until>?`)
    .bind(session.user.id, now, now, now, now).all<{role:string;company_id:string}>()).results;
  for (const companyId of new Set(rows.map((row) => row.company_id))) {
    const roles = await iamRoles(write, companyId, rows.filter((row) => row.company_id === companyId).map((row) => row.role));
    if (rows.some((row) => row.company_id === companyId && roles.get(row.role)?.scopes.includes("holding") && roles.get(row.role)?.permissions.includes("manage_erpx.access"))) return true;
  }
  return false;
}

export async function requireManageErpxContext(): Promise<{ work: WorkContext; platform: PlatformContext }> {
  const requestHeaders = await headers();
  const method = requestHeaders.get("x-erpx-method") ?? requestHeaders.get("x-erpx-request-method");
  if (method && !["GET", "HEAD", "OPTIONS"].includes(method) &&
    (!requestHeaders.has("x-erpx-context-revision") || !requestHeaders.has("x-erpx-platform-assignment")))
    throw new ApiError(409, "WORK_CONTEXT_CHANGED", "Reload Manage ERPX before submitting.");
  let work: WorkContext;
  try {
    work = await requireWorkContext("erpx");
  } catch (error) {
    if (error instanceof ApiError && ["WORK_ROLE_REQUIRED", "WORK_ROLE_UNAVAILABLE"].includes(error.code)) {
      const access = await requireAccountAccess("erpx");
      if (!access.assignments.some((assignment) => assignment.scopeKind === "holding"))
        throw new ApiError(403, "MANAGE_ERPX_FORBIDDEN", "An ERPX Holding assignment is required.");
    }
    throw error;
  }
  if (work.holdingId !== "h003" || work.scope.kind !== "holding" || !hasPermission(work, "manage_erpx.access"))
    throw new ApiError(403, "MANAGE_ERPX_FORBIDDEN", "The selected ERPX Holding role cannot access Manage ERPX.");
  const platform = await requirePlatformContext();
  if (platform.userId !== work.userId)
    throw new ApiError(403, "MANAGE_ERPX_FORBIDDEN", "The Platform and Holding identities must match.");
  if (requestHeaders.has("x-erpx-platform-assignment") &&
    requestHeaders.get("x-erpx-platform-assignment") !== platform.assignmentId)
    throw new ApiError(409, "PLATFORM_CONTEXT_CHANGED", "Your Platform role changed. Reload and review.");
  return { work, platform };
}

export function publicScope(context: WorkContext | PlatformContext): WorkspaceScope {
  const scope = context.scope;
  if (scope.kind === "platform") return scope;
  if (scope.kind === "holding")
    return { kind: "holding", holdingSlug: scope.holdingSlug };
  if (scope.kind === "branch")
    return {
      kind: "branch",
      holdingSlug: scope.holdingSlug,
      branchSlug: scope.branchSlug,
    };
  return {
    kind: "department",
    holdingSlug: scope.holdingSlug,
    branchSlug: scope.branchSlug,
    departmentSlug: scope.departmentSlug,
  };
}

export async function firstAuthorizedWorkspace(
  holdingSlug: string,
): Promise<HoldingWorkspaceScope | null> {
  const binding = await holdingBindingBySlug(holdingSlug);
  const { write } = await holdingResources(binding.id);
  const nodes = await write
    .prepare(
      `SELECT n.id,n.kind,n.slug,n.branch_id,b.slug AS branch_slug
       FROM organization_nodes n
       LEFT JOIN organization_nodes b ON n.branch_id=b.id AND b.kind='branch'
       WHERE n.kind IN ('branch','department') AND n.status='active'
       ORDER BY CASE n.kind WHEN 'branch' THEN 1 ELSE 2 END,n.display_name,n.id`,
    )
    .all<{
      id: string;
      kind: "branch" | "department";
      slug: string;
      branch_id: string | null;
      branch_slug: string | null;
    }>();
  const candidates: HoldingWorkspaceScope[] = [
    { kind: "holding", holdingSlug },
    ...nodes.results.map((node): HoldingWorkspaceScope =>
      node.kind === "branch"
        ? { kind: "branch", holdingSlug, branchSlug: node.slug }
        : {
            kind: "department",
            holdingSlug,
            branchSlug: node.branch_slug ?? undefined,
            departmentSlug: node.slug,
          },
    ),
  ];
  for (const candidate of candidates) {
    try {
      const account=await requireAccountAccess(candidate);
      if(account.assignments.some(assignment=>assignmentApplies(assignment,account.scope)))return candidate;
    } catch {
      // Continue to the next active workspace candidate.
    }
  }
  return null;
}
