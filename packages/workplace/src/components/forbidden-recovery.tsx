import {workRoleDisplayChoices} from "@erpx/feature-identity_access/lib/work-role-display";
import "server-only";

import { cookies, headers } from "next/headers";
import { configuredProductHandoff, configuredProductOrigin, productHostForHost } from "@erpx/feature-runtime/lib/product-hosts";
import { product } from "@erpx/server-commercial/public/product-registry";
import { preferredAccountLanguage } from "@erpx/server-identity_access/public/account-preference-snapshot";
import { AppearanceSwitcher } from "@erpx/feature-brand/components/appearance-switcher";
import { AccountSwitcher } from "@erpx/workplace/routes/holdings/[holdingSlug]/account-switcher";
import { ProductBrand } from "@erpx/feature-brand/components/product-brand";
import { preferredAccountAppearance } from "@erpx/server-identity_access/public/account-preference-snapshot";
import { parseHoldingWorkspace, workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import type { CurrentWorkspaceChoice } from "@erpx/feature-work_projection/lib/workspace-choice";
import {
  canEnterManageErpx,
  currentSession,
  type ResolvedWorkspaceScope,
} from "@erpx/server-identity_access/public/authorization";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { workRoleOptions } from "@erpx/server-work_projection/public/work-role-options";
import { workspaceChoices } from "@erpx/server-organization/public/workspace-choices";
import { accessUnavailable, requestedPage } from "@erpx/workplace/components/forbidden-context";
import { ForbiddenPanel } from "@erpx/workplace/components/forbidden-panel";
export { ForbiddenPanel } from "@erpx/workplace/components/forbidden-panel";
import { WorkspaceMenu } from "@erpx/workplace/components/workspace-menu";
import styles from "@erpx/workplace/components/forbidden-recovery.module.css";

function currentWorkspace(scope: Exclude<ResolvedWorkspaceScope, { kind: "platform" }>): CurrentWorkspaceChoice {
  const common = { holdingSlug: scope.holdingSlug, companyId: scope.companyId };
  if (scope.kind === "holding") return { kind: "holding", ...common };
  if (scope.kind === "branch") return { kind: "branch", ...common, branchId: scope.branchId };
  return { kind: "department", ...common, branchId: scope.branchId, departmentId: scope.departmentId };
}

/** A layout can fail before WorkContext exists; recovery depends only on the signed-in account. */
export async function ForbiddenRecoveryShell() {
  const from = await requestedPage();
  const audience = productHostForHost((await headers()).get("host")), definition = audience && product(audience);
  if (definition?.shell === "office") {
    const { session } = await currentSession();
    const cookieStore = await cookies(), language = await preferredAccountLanguage(cookieStore.get("erpx_language")?.value === "th" ? "th" : "en");
    const th = language === "th";
    return <div className={styles.shell}>
      <header className={styles.header} data-app-chrome>
        <ProductBrand appearance="standard" className={styles.brand} /><strong>{definition.label[language]}</strong>
        <nav className={styles.controls} aria-label={th ? "บัญชีและผลิตภัณฑ์" : "Account and products"}>
          <span>{session.user.name}</span><a data-product-switch href={configuredProductHandoff("apps")!}>Apps</a>
          <a data-product-switch href={configuredProductHandoff("account")!}>{th ? "บัญชี" : "Account"}</a>
          <AppearanceSwitcher appearance="standard" initialTheme={await preferredAccountAppearance(cookieStore.get("erpx_appearance")?.value)} />
        </nav>
      </header><main><ForbiddenPanel productLabel={definition.label[language]} language={language} /></main>
    </div>;
  }
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
          accountNavigation={configuredProductOrigin('account')!==null}
          workspaceBase={base}
          companyId={roles?.access.scope.companyId}
          activeUser={{ id: session.user.id, name: session.user.name, email: session.user.email }}
          assignments={workRoleDisplayChoices(roles?.options ?? [])}
          preferredAssignmentId={roles?.selected?.id}
          contextRevision={roles?.selection?.revision ?? 0}
          theme={await preferredAccountAppearance(cookieStore.get("erpx_appearance")?.value)}
        />
      </div>
    </header>
    <main><ForbiddenPanel /></main>
  </div>;
}
