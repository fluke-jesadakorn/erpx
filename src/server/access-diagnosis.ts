import "server-only";
import { ApiError } from "./http";
import { redirect } from "next/navigation";
import { currentSession, requireWorkContext, requirePlatformContext, resolveWorkspaceScope, assignmentApplies, isRoleValidAtScope, type WorkAssignment } from "./authorization";
import { holdingResources, runtimeEnv } from "./runtime";
import { parseHoldingWorkspace } from "@/lib/workspace";
import { assignmentProblem, safeReturnPath, type AccessProblem } from "@/lib/access-problem";

type AssignmentRow = { role: string; company_id: string; branch_id: string | null; scope_kind: "holding" | "branch" | "department"; scope_node_id: string | null; valid_from: number; valid_until: number; revoked_at: number | null };
const asAssignment = (r: AssignmentRow): WorkAssignment => ({ id: "diagnostic", role: r.role, companyId: r.company_id, branchId: r.branch_id, scopeKind: r.scope_kind, scopeNodeId: r.scope_node_id, modules: [] });

export async function accessDiagnosis(from: string | null, hint?: AccessProblem): Promise<{ problem: AccessProblem; email?: string }> {
  let identity;
  try { identity = await currentSession(); } catch (error) {
    if (error instanceof Error && error.message === "UNAUTHENTICATED") return { problem: "sign_in" };
    throw error;
  }
  const email = identity.session.user.email, userId = identity.session.user.id;
  if (!safeReturnPath(from)) return { problem: "invalid", email };
  if (from!.startsWith("/platform/")) {
    try { await requirePlatformContext(); return { problem: hint ?? "available", email }; }
    catch (error) { if (!(error instanceof Error) || error.message !== "FORBIDDEN") throw error; }
    const env = await runtimeEnv();
    const rows = (await env.CONTROL_DB.prepare("SELECT role,valid_from,valid_until,revoked_at FROM platform_assignments WHERE user_id=?").bind(userId).all<{ role: string; valid_from: number; valid_until: number; revoked_at: number | null }>()).results;
    const reason = assignmentProblem(rows.map(r => ({ ...r, applies: true, validRole: isRoleValidAtScope(r.role, "platform") })), Date.now());
    return { problem: reason === "no_access" ? "platform" : reason, email };
  }
  let parts: string[];
  try { parts = new URL(from!, "https://erpx.invalid").pathname.split("/").filter(Boolean).map(decodeURIComponent); }
  catch { return { problem: "invalid", email }; }
  const parsed = parseHoldingWorkspace(parts[1], parts.slice(2));
  if (!parsed) return { problem: "invalid", email };
  try { await requireWorkContext(parsed.scope); return { problem: hint ?? "available", email }; }
  catch (error) {
    if(error instanceof ApiError && ["WORK_ROLE_REQUIRED","WORK_ROLE_UNAVAILABLE","WORK_CONTEXT_CHANGED"].includes(error.code)) redirect(`/work-role?${new URLSearchParams({from:from!})}`);
    const code = error instanceof Error ? error.message : "";
    if (code === "COMPANY_REQUIRED") return { problem: "company", email };
    if (["HOLDING_NOT_FOUND", "WORKSPACE_NOT_FOUND", "INVALID_ORGANIZATION_LINEAGE"].includes(code)) {
      // Inactive status is disclosed only to an existing member, never to outsiders.
      const env = await runtimeEnv();
      const binding = await env.CONTROL_DB.prepare("SELECT holding_id,status FROM holding_bindings WHERE slug=?").bind(parts[1]).first<{ holding_id: string; status: string }>();
      if (binding) {
        const { write } = await holdingResources(binding.holding_id);
        const member = await write.prepare("SELECT 1 FROM assignments WHERE user_id=? LIMIT 1").bind(userId).first();
        if (binding.status !== "active" && member) return { problem: "suspended", email };
        if (parsed.scope.kind !== "holding") {
          const branchSlug = parsed.scope.branchSlug ?? null;
          const kind = parsed.scope.kind;
          const slug = kind === "branch" ? branchSlug : parsed.scope.kind === "department" ? parsed.scope.departmentSlug : null;
          type Node = { id: string; parent_id: string | null; company_id: string; status: string };
          let node = await write.prepare(`SELECT n.id,n.parent_id,n.company_id,n.status FROM organization_nodes n WHERE n.kind=? AND n.slug=?
            AND (?::text IS NULL OR n.kind='branch' OR EXISTS(SELECT 1 FROM organization_nodes b WHERE b.id=n.branch_id AND b.slug=?))
            AND EXISTS(SELECT 1 FROM assignments a WHERE a.user_id=? AND a.company_id=n.company_id AND (a.scope_kind='holding' OR a.scope_node_id=n.id OR a.scope_node_id=n.branch_id OR (n.kind='branch' AND a.branch_id=n.id))) LIMIT 1`)
            .bind(kind, slug, branchSlug, branchSlug, userId).first<Node>();
          const seen = new Set<string>();
          while (node && !seen.has(node.id) && seen.size < 16) {
            if (node.status !== "active") return { problem: "suspended", email };
            seen.add(node.id);
            node = node.parent_id ? await write.prepare("SELECT id,parent_id,company_id,status FROM organization_nodes WHERE id=? AND company_id=?").bind(node.parent_id, node.company_id).first<Node>() : null;
          }
        }
      }
      return { problem: code === "INVALID_ORGANIZATION_LINEAGE" ? "setup" : "workspace", email };
    }
    if (code !== "FORBIDDEN") throw error;
    const scope = await resolveWorkspaceScope(parsed.scope);
    const { write } = await holdingResources(scope.holdingId);
    const rows = (await write.prepare("SELECT role,company_id,branch_id,scope_kind,scope_node_id,valid_from,valid_until,revoked_at FROM assignments WHERE user_id=?").bind(userId).all<AssignmentRow>()).results;
    return { problem: assignmentProblem(rows.map(r => ({ ...r, applies: assignmentApplies(asAssignment(r), scope), validRole: isRoleValidAtScope(r.role, r.scope_kind) })), Date.now()), email };
  }
}
