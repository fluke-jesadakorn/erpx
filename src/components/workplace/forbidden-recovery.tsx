import "server-only";

import { cookies, headers } from "next/headers";
import { unstable_rethrow } from "next/navigation";
import { AccountSwitcher } from "@/app/holdings/[holdingSlug]/account-switcher";
import { ProductBrand } from "@/components/brand/product-brand";
import { appearancePreference } from "@/lib/appearance-preference";
import { safeReturnPath } from "@/lib/access-problem";
import { parseHoldingWorkspace, workspaceBase } from "@/lib/workspace";
import type { CurrentWorkspaceChoice } from "@/lib/workspace-choice";
import {
  canEnterManageErpx,
  currentSession,
  requirePlatformContext,
  type ResolvedWorkspaceScope,
} from "@/server/authorization";
import { ApiError } from "@/server/http";
import { pageError } from "@/server/page-error";
import { workRoleOptions } from "@/server/work-role-options";
import { workspaceChoices } from "@/server/workspace-choices";
import { PlatformRoleSwitcher } from "./platform-role-switcher";
import { WorkspaceMenu } from "./workspace-menu";
import styles from "./forbidden-recovery.module.css";

function accessUnavailable(error: unknown) {
  unstable_rethrow(error);
  if (error instanceof ApiError && ["EMERGENCY_ACCESS_REQUIRED", "FRESH_MFA_REQUIRED"].includes(error.code)) return false;
  if (error instanceof ApiError && [403, 404, 409].includes(error.status)) return true;
  return error instanceof Error && [
    "FORBIDDEN", "HOLDING_NOT_FOUND", "WORKSPACE_NOT_FOUND",
    "COMPANY_REQUIRED", "INVALID_ORGANIZATION_LINEAGE",
  ].includes(error.message);
}

async function requestedPage() {
  const path = (await headers()).get("x-erpx-page-path");
  if (!path) return null;
  const url = new URL(path, "https://erpx.invalid");
  return safeReturnPath(url.pathname === "/work-role" ? url.searchParams.get("from") : path);
}

function currentWorkspace(scope: Exclude<ResolvedWorkspaceScope, { kind: "platform" }>): CurrentWorkspaceChoice {
  const common = { holdingSlug: scope.holdingSlug, companyId: scope.companyId };
  if (scope.kind === "holding") return { kind: "holding", ...common };
  if (scope.kind === "branch") return { kind: "branch", ...common, branchId: scope.branchId };
  return { kind: "department", ...common, branchId: scope.branchId, departmentId: scope.departmentId };
}

async function PlatformRoleRecovery({ from }: { from: string | null }) {
  if (!from || !/^\/holdings\/erpx\/manage-erpx(?:[/?#]|$)/.test(from)) return null;
  const platform = await requirePlatformContext().catch(error => {
    if (accessUnavailable(error)) return null;
    return pageError(error, "erpx");
  });
  if (!platform) return null;
  return <div className={styles.platformRoles}>
    <PlatformRoleSwitcher activeAssignmentId={platform.assignmentId} assignments={platform.assignments} />
  </div>;
}

/** Page failures retain the already-authorized workspace shell and its account menu. */
export async function ForbiddenPanel() {
  const from = await requestedPage();
  return <section className={styles.panel} aria-labelledby="workspace-forbidden-title" data-work-forbidden>
    <span className="eyebrow">Access required</span>
    <h1 id="workspace-forbidden-title">Your role cannot open this view</h1>
    <p>Your selected account and role do not have access to this page. Choose another role or account to continue here, or select an available workspace.</p>
    <PlatformRoleRecovery from={from} />
  </section>;
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
      <ProductBrand className={styles.brand} />
      <div className={styles.controls}>
        <WorkspaceMenu label={roles?.access.scope.holdingName ?? "Choose workspace"} choices={choices} current={current} canManageErpx={canManageErpx} />
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
