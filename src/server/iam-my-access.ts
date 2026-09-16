import "server-only";
import { activeHoldingBindings, holdingResources, runtimeEnv } from "./runtime";
import { iamDb } from "./iam-db";
import { iamRole } from "./iam-store";
import { evaluateAccess, permissionCatalog } from "@/lib/permissions";
import type { AccessDecision } from "@/lib/iam";
export async function ownIamAccess(userId: string) {
  const rows: {
    id: string;
    company: string;
    role: string;
    scope: string;
    expiresAt: number;
    active: boolean;
    identity: string;
    decisions: AccessDecision[];
  }[] = [];
  for (const holding of await activeHoldingBindings()) {
    const db = iamDb((await holdingResources(holding.id)).write);
    const assignments = (
      await db
        .prepare(
          "SELECT a.*,n.display_name company_name,s.display_name scope_name,m.status identity_status,m.login_policy,iam_assignment_active(a.id,?) active FROM assignments a JOIN organization_nodes n ON n.id=a.company_id LEFT JOIN organization_nodes s ON s.id=a.scope_node_id LEFT JOIN iam_members m ON m.user_id=a.user_id AND m.company_id=a.company_id WHERE a.user_id=? ORDER BY n.display_name,a.role",
        )
        .bind(Date.now(), userId)
        .all<Record<string, unknown>>()
    ).results;
    for (const a of assignments) {
      const role = await iamRole(db, String(a.company_id), String(a.role));
      rows.push({
        id: String(a.id),
        company: `${holding.displayName} · ${a.company_name}`,
        role: role?.name ?? String(a.role),
        scope: String(a.scope_name ?? "Company"),
        expiresAt: Number(a.valid_until),
        active: a.active === true,
        identity: String(
          a.identity_status === "superseded"
            ? "Directory required"
            : (a.login_policy ?? "local"),
        ),
        decisions: permissionCatalog
          .filter((p) => p.namespace === "customer")
          .map((p) =>
            evaluateAccess(
              {
                userId,
                role: String(a.role),
                permissions: role?.permissions ?? [],
                policyRevision: role?.revision,
                assignmentId: String(a.id),
                assignmentScope: String(a.scope_kind),
              },
              p.id,
              {
                inspect: true,
                valid: a.active === true,
                expiresAt: Number(a.valid_until),
                source: String(a.source),
              },
            ),
          ),
      });
    }
  }
  const db = iamDb((await runtimeEnv()).CONTROL_DB),
    platform = (
      await db
        .prepare(
          "SELECT *,iam_assignment_active(id,?) active FROM platform_assignments WHERE user_id=?",
        )
        .bind(Date.now(), userId)
        .all<Record<string, unknown>>()
    ).results;
  for (const a of platform) {
    const role = await iamRole(db, "platform", String(a.role));
    rows.push({
      id: String(a.id),
      company: "Platform",
      role: role?.name ?? String(a.role),
      scope: "Platform",
      expiresAt: Number(a.valid_until),
      active: a.active === true,
      identity: "local",
      decisions: permissionCatalog
        .filter((p) => p.namespace === "platform")
        .map((p) =>
          evaluateAccess(
            {
              userId,
              role: String(a.role),
              permissions: role?.permissions ?? [],
              policyRevision: role?.revision,
            },
            p.id,
            {
              inspect: true,
              valid: a.active === true,
              expiresAt: Number(a.valid_until),
            },
          ),
        ),
    });
  }
  return rows;
}
