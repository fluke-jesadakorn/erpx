import { workProjectionPresentationQueries } from "@/server/modules/work_projection/public/presentation-queries";

import { RecordScopeProvider } from "@/components/records/record-context";
import { MainContent } from "@/components/ui/main";
import { Button } from "@/components/ui/foundation";
import { measureReadPhase } from "@/server/modules/runtime/public/runtime/read-metrics";
import { financeWorkspaceV2Enabled } from "@/server/modules/finance/public/finance-presentation";
import { ProductBrand } from "@/components/brand/product-brand";
import { appearancePreference } from "@/lib/appearance-preference";
import { WorkspaceBrandMark } from "@/components/brand/workspace-brand";
import { FeatureEnvironment } from "@/components/features/feature-page";
import { NavigationCountsProvider } from "@/components/workplace/navigation-counts";

import { buildWorkCards, buildDestinationCards } from "@/server/modules/work_projection/public/work-cards";
import { localShortcutKey } from "@/lib/local-shortcuts";
import { CurrencyProvider } from "@/components/money/currency-provider";
import { RememberWorkspace } from "@/components/workplace/remember-workspace";
import styles from "@/components/workplace/shell.module.css";
import { WorkContextBoundary } from "@/components/workplace/work-context-boundary";
import { ReturnToFront } from "@/components/workplace/return-to-front";
import { ConsoleCommandSearch as CommandSearch, ConsoleWorkplaceNavigation as WorkplaceNavigation } from "@/components/workplace/console-ui-utilities";
import { WorkspaceAssistant } from "@/components/workplace/workspace-assistant";
import {
  WorkspaceBreadcrumb,
  type WorkspaceCrumb,
} from "@/components/workplace/workspace-breadcrumb";
import { WorkspaceMenu } from "@/components/workplace/workspace-menu";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { administrationAccess } from "@/server/modules/runtime/public/administration";
import {
  listWorkspaceCompanies,
  publicScope,
  requireWorkContext,
  canEnterManageErpx,
  requirePlatformContext,
} from "@/server/modules/identity_access/public/authorization";


import { pageError } from "@/server/modules/work_projection/public/page-error";

import { moneyContext } from "@/server/modules/finance/public/company-money";
import { companyFeatureCatalogue } from "@/server/modules/pms/public/hospitality-rollout";
import { availableNavigationAppIds } from "@/lib/workplace-catalogue";
import { workspaceChoices } from "@/server/modules/organization/public/workspace-choices";
import { Building2 } from "lucide-react";
import { cookies } from "next/headers";
import Link from "next/link";
import { Suspense, type ComponentProps } from "react";
import { ConsoleAccountSwitcher as AccountSwitcher, ConsoleCurrencySelector as CurrencySelector } from "@/components/workplace/console-ui-utilities";
async function DeferredWorkspaceMenu(
  props: Omit<ComponentProps<typeof WorkspaceMenu>, "choices" | "canManageErpx">,
) {
  const [choices, canManageErpx] = await Promise.all([
    workspaceChoices(),
    canEnterManageErpx().catch(() => false),
  ]);
  return <WorkspaceMenu appearance="standard" {...props} choices={choices} canManageErpx={canManageErpx} />;
}
export const dynamic = "force-dynamic";
export default async function WorkplaceLayout({
  children,
  record,
  params,
}: LayoutProps<"/holdings/[holdingSlug]"> & { record?: React.ReactNode }) {
  const { holdingSlug } = await params;
  let context;
  try {
    context = await measureReadPhase("work_context", () => requireWorkContext(holdingSlug));
  } catch (error) {
    return pageError(error, holdingSlug);
  }
  const theme = appearancePreference((await cookies()).get("erpx_appearance")?.value);
  const holdingName = context.holdingName;
  const base = workspaceBase(publicScope(context));
  const scopeLabel =
    context.scope.kind === "department"
      ? context.scope.departmentName
      : context.scope.kind === "branch"
        ? context.scope.branchName
        : "Holding";
  const presentation = await workProjectionPresentationQueries(context.holdingId);
  const companySlug = (
    await presentation.activeCompanySlug(context.companyId)
  )?.slug;
  const frontLocation =
    context.scope.kind === "holding"
      ? "holding"
      : context.scope.kind === "branch"
        ? `branches/${context.scope.branchSlug}`
        : context.scope.branchSlug
          ? `branches/${context.scope.branchSlug}/departments/${context.scope.departmentSlug}`
          : `departments/${context.scope.departmentSlug}`;
  const frontBase = companySlug
    ? `/front/${encodeURIComponent(holdingSlug)}/${encodeURIComponent(companySlug)}`
    : undefined;
  const frontHref = (app: "pos" | "pms") =>
    frontBase ? `${frontBase}/${app}/${frontLocation}/` : undefined;
  const [workspaceBrand, companies, admin, shortcutCapabilities] = await measureReadPhase("layout", () =>
    Promise.all([
      presentation.resolveWorkspaceBrand(context),
      context.scope.kind === "holding"
        ? listWorkspaceCompanies(context.holdingId)
        : Promise.resolve([]),
      administrationAccess(context),
      presentation.getWorkCapabilities(context),
    ]),
  );
  const money = await moneyContext(context);
  const featureCatalogue =
    process.env.NODE_ENV === "production" ? await companyFeatureCatalogue(context) : [];
  const navigation = {
    financeWorkspaceV2: financeWorkspaceV2Enabled(),
    manageErpxVisible:
      context.holdingId === "h003" &&
      context.scope.kind === "holding" &&
      hasPermission(context, "manage_erpx.access") &&
      (await requirePlatformContext()
        .then((platform) => platform.userId === context.userId)
        .catch(() => false)),
    executiveVisible: await presentation.executiveVisible(context),
    workspaceBase: base,
    frontPosHref: frontHref("pos"),
    frontPmsHref: frontHref("pms"),
    scopeKind: context.scope.kind,
    beta: context.beta,
    role: context.role,
    moduleIds: context.modules,
    permissions: context.iam?.permissions,
    adminSections: admin.sections,
    canManageApprovals: admin.manageOrganization,
    ...(process.env.NODE_ENV === "production"
      ? {
          availableAppIds: availableNavigationAppIds(featureCatalogue),
        }
      : {}),
  };
  const companyName =
    companies.find((company) => company.id === context.companyId)?.name ??
    (
      await presentation.activeCompanyName(context.companyId)
    )?.display_name ??
    "Company unavailable";
  const workspaceLabel =
    context.scope.kind === "holding" ? companyName : `${companyName} · ${scopeLabel}`;
  const holdingBase = workspaceBase({ kind: "holding", holdingSlug });
  const ancestor = (label: string, path: string): WorkspaceCrumb => ({
    label,
    // Scoped routes share an internal layout after proxy rewriting. A document
    // navigation refreshes its authorization context when changing workspace.
    fullNavigation: path !== base,
    ...(context.workspaces.some((workspace) => workspace.href === `${path}/home`)
      ? { href: `${path}/home` }
      : {}),
  });
  const ancestors = [ancestor(holdingName, holdingBase)];
  if (context.scope.kind !== "holding" && context.scope.branchSlug) {
    ancestors.push(
      ancestor(
        context.scope.branchName ?? context.scope.branchSlug,
        workspaceBase({ kind: "branch", holdingSlug, branchSlug: context.scope.branchSlug }),
      ),
    );
  }
  if (context.scope.kind === "department")
    ancestors.push(ancestor(context.scope.departmentName, base));
  return (
    <NavigationCountsProvider
      key={`${base}:${context.userId}:${context.companyId}:${context.assignmentId}:${context.authority!.revision}`}
      base={base}
      shortcutStorageKey={localShortcutKey(context.userId, context.companyId, base)}
      initialShortcutCards={[
        ...buildWorkCards(base, { items: [], unavailableSources: [] }, shortcutCapabilities),
        ...buildDestinationCards(navigation),
      ]}
    >
      <CurrencyProvider initial={money} apiBase={`/api/v1${base}`}>
        <div className={styles.shell}>
          <RememberWorkspace
            base={base}
            companyId={context.companyId}
            assignmentId={context.assignmentId}
            revision={context.authority!.revision}
          />
          <a href="#workspace-content" className="skip-link">
            Skip to content
          </a>
          <aside className={styles.sidebar} data-app-chrome>
            <Link className={styles.brand} href={`${base}/home`}>
              <ProductBrand appearance="standard" className={`${styles.productBrand} ${styles.productBrandWordmark}`} />
              <ProductBrand appearance="standard"
                variant="symbol"
                className={`${styles.productBrand} ${styles.productBrandSymbol}`}
              />
              <small>WORKSPACE</small>
            </Link>
            <Suspense>
              <WorkplaceNavigation {...navigation} />
            </Suspense>
            <div className={styles.sidebarBottom}>
              <div>
                <Building2 size={17} />
                <div>
                  <strong>{holdingName}</strong>
                  <small>Connected work, shared progress</small>
                </div>
              </div>
            </div>
          </aside>
          <div className={styles.main} data-app-main>
            <header className={styles.topbar} data-app-chrome>
              <div className={styles.workspace}>
                <WorkspaceBrandMark appearance="standard" brand={workspaceBrand} compact />
                <Suspense
                  fallback={
                    <Button appearance="standard"
                      variant="ghost"
                      className={styles.workspaceTrigger}
                      type="button"
                      aria-label="Switch workspace"
                      title={`Change workspace (currently ${workspaceLabel})`}
                      disabled
                    >
                      <Building2 size={18} aria-hidden="true" />
                      <span className={styles.workspaceTriggerLabel}>{workspaceLabel}</span>
                    </Button>
                  }
                >
                  <DeferredWorkspaceMenu
                    label={workspaceLabel}
                    current={
                      context.scope.kind === "holding"
                        ? {
                            kind: "holding",
                            holdingSlug,
                            companyId: context.companyId,
                          }
                        : context.scope.kind === "branch"
                          ? {
                              kind: "branch",
                              holdingSlug,
                              companyId: context.companyId,
                              branchId: context.scope.branchId,
                            }
                          : {
                              kind: "department",
                              holdingSlug,
                              companyId: context.companyId,
                              branchId: context.scope.branchId,
                              departmentId: context.scope.departmentId,
                            }
                    }
                  />
                </Suspense>

                <Suspense
                  fallback={
                    <span className={styles.breadcrumbFallback}>
                      {scopeLabel === "Holding" ? holdingName : scopeLabel}
                    </span>
                  }
                >
                  <WorkspaceBreadcrumb
                    workspaceBase={base}
                    ancestors={ancestors}
                    navigation={navigation}
                  />
                </Suspense>
              </div>
              <div className={styles.actions}>
                <CurrencySelector />
                <CommandSearch {...navigation} />
                <AccountSwitcher
                  workspaceBase={base}
                  assignments={context.availableAssignments ?? context.assignments}
                  preferredAssignmentId={context.assignmentId}
                  contextRevision={context.authority!.revision}
                  companyId={context.companyId}
                  activeUser={{
                    id: context.userId,
                    name: context.name,
                    email: context.email,
                  }}
                  theme={theme}
                />
              </div>
            </header>
            <div>
              <WorkContextBoundary appearance="standard"
                key={`${context.userId}:${context.companyId}:${context.assignmentId}:${context.authority!.revision}`}
                base={base}
                userId={context.userId}
                companyId={context.companyId}
                assignmentId={context.assignmentId}
                revision={context.authority!.revision}
              >
                <div className={styles.workspaceBody}>
                  <MainContent
                    id="workspace-content"
                    className={styles.workspacePage}
                    tabIndex={-1}
                  >
                    <Suspense>
                      <ReturnToFront
                        base={base}
                        holdingSlug={holdingSlug}
                        companySlug={companySlug}
                        companyId={context.companyId}
                        userId={context.userId}
                        assignmentId={context.assignmentId}
                      />
                    </Suspense>
                    <FeatureEnvironment
                      navigation={navigation}
                      productionCatalogue={
                        process.env.NODE_ENV === "production"
                          ? featureCatalogue.map(({ id, name, state }) => ({ id, name, state }))
                          : undefined
                      }
                    >
                      <RecordScopeProvider base={base}>{children}{record}</RecordScopeProvider>
                    </FeatureEnvironment>
                  </MainContent>
                </div>
                <Suspense>
                  <WorkspaceAssistant
                    base={base}
                    userId={context.userId}
                    companyId={context.companyId}
                    assignmentId={context.assignmentId}
                    crossContext={hasPermission(context, "ai.context.cross")}
                  />
                </Suspense>
              </WorkContextBoundary>
            </div>
          </div>
          <div data-app-chrome>
            <Suspense>
              <WorkplaceNavigation {...navigation} mobile />
            </Suspense>
          </div>
        </div>
      </CurrencyProvider>
    </NavigationCountsProvider>
  );
}
