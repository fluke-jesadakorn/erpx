import "server-only";

import { cookies } from "next/headers";
import { AccountSwitcher } from "@/app/holdings/[holdingSlug]/account-switcher";
import { ProductBrand } from "@/components/brand/product-brand";
import { appearancePreference } from "@/lib/appearance-preference";
import { parseHoldingWorkspace, workspaceBase } from "@/lib/workspace";
import type { CurrentWorkspaceChoice } from "@/lib/workspace-choice";
import {
  canEnterManageErpx,
  currentSession,
  type ResolvedWorkspaceScope,
} from "@/server/modules/identity_access/public/authorization";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { workRoleOptions } from "@/server/modules/work_projection/public/work-role-options";
import { workspaceChoices } from "@/server/modules/organization/public/workspace-choices";
import { accessUnavailable, requestedPage } from "./forbidden-context";
import { ForbiddenPanel } from "./forbidden-panel";
export { ForbiddenPanel } from "./forbidden-panel";
import { WorkspaceMenu } from "./workspace-menu";
import styles from "./forbidden-recovery.module.css";

function currentWorkspace(scope: Exclude<ResolvedWorkspaceScope, { kind: "platform" }>): CurrentWorkspaceChoice {
  const common = { holdingSlug: scope.holdingSlug, companyId: scope.companyId };
  if (scope.kind === "holding") return { kind: "holding", ...common };
  if (scope.kind === "branch") return { kind: "branch", ...common, branchId: scope.branchId };
  return { kind: "department", ...common, branchId: scope.branchId, departmentId: scope.departmentId };
}

/** A layout can fail before WorkContext exists; recovery depends only on the signed-in account. */
export async function ForbiddenRecoveryShell() {
  const from = await requestedPage();
  let parts: string[] = [];
  if (from) {
    try { parts = new URL(from, "https://erpx.invalid").pathname.split("/").filter(Boolean).map(decodeURIComponent); }
    catch { /* The account menu also remains available for malformed workspace links. */ }
  }
  const parsed = parts[0] === "holdings" ? parseHoldingWorkspace(parts[1], parts.slice(2)) : null;
  const { session } = await currentSession().catch(error => pageError(error, parts[1]));
  const [roles, choices, canManageErpx, cookieStore] = await Promise.all([
    parsed ? workRoleOptions(parsed.scope).catch(error => {
      if (accessUnavailable(error)) return null;
      return pageError(error, parts[1]);
    }) : Promise.resolve(null),
    workspaceChoices(),
    canEnterManageErpx().catch(() => false),
    cookies(),
  ]);
  const base = parsed ? workspaceBase(parsed.scope) : "/platform";
  const current: CurrentWorkspaceChoice = roles
    ? currentWorkspace(roles.access.scope)
    : parsed ? { kind: "holding", holdingSlug: parsed.scope.holdingSlug, companyId: "" } : { kind: "platform" };
  return <div className={styles.shell} data-work-user={session.user.id} data-work-forbidden>
    <a href="#workspace-forbidden-title" className="skip-link">Skip to access message</a>
    <header className={styles.header} data-app-chrome>
      <ProductBrand appearance="standard" className={styles.brand} />
      <div className={styles.controls}>
        <WorkspaceMenu appearance="standard" label={roles?.access.scope.holdingName ?? "Choose workspace"} choices={choices} current={current} canManageErpx={canManageErpx} />
        <AccountSwitcher
          workspaceBase={base}
          companyId={roles?.access.scope.companyId}
          activeUser={{ id: session.user.id, name: session.user.name, email: session.user.email }}
          assignments={roles?.options ?? []}
          preferredAssignmentId={roles?.selected?.id}
          contextRevision={roles?.selection?.revision ?? 0}
          theme={appearancePreference(cookieStore.get("erpx_appearance")?.value)}
        />
      </div>
    </header>
    <main><ForbiddenPanel /></main>
  </div>;
}
