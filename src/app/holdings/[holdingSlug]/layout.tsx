import { measureReadPhase } from "@/server/runtime/read-metrics";
import { ProductBrand } from "@/components/brand/product-brand";
import { appearancePreference } from "@/lib/appearance-preference";
import { WorkspaceBrandMark } from "@/components/brand/workspace-brand";
import { FeatureEnvironment } from "@/components/features/feature-page";
import { NavigationCountsProvider } from "@/components/workplace/navigation-counts";
import { RememberWorkspace } from "@/components/workplace/remember-workspace";
import styles from "@/components/workplace/shell.module.css";
import { WorkContextBoundary } from "@/components/workplace/work-context-boundary";
import {
CommandSearch,
WorkplaceNavigation,
} from "@/components/workplace/workplace-controls";
import { WorkspaceAssistant } from "@/components/workplace/workspace-assistant";
import {
WorkspaceBreadcrumb,
type WorkspaceCrumb,
} from "@/components/workplace/workspace-breadcrumb";
import { WorkspaceMenu } from "@/components/workplace/workspace-menu";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { administrationAccess } from "@/server/administration";
import {
listWorkspaceCompanies,
publicScope,
requireWorkContext,
canEnterManageErpx,
requirePlatformContext,
} from "@/server/authorization";
import { resolveWorkspaceBrand } from "@/server/branding";
import { executiveVisible } from "@/server/executive/access";
import { pageError } from "@/server/page-error";
import { holdingResources } from "@/server/runtime";
import { companyFeatureCatalogue } from "@/server/hospitality-rollout";
import { workspaceChoices } from "@/server/workspace-choices";
import { Building2 } from "lucide-react";
import { cookies } from "next/headers";
import Link from "next/link";
import { Suspense,type ComponentProps } from "react";
import { AccountSwitcher } from "./account-switcher";
async function DeferredWorkspaceMenu(props: Omit<ComponentProps<typeof WorkspaceMenu>, "choices" | "canManageErpx">) {
  const [choices, canManageErpx] = await Promise.all([
    workspaceChoices(),
    canEnterManageErpx().catch(() => false),
  ]);
  return <WorkspaceMenu {...props} choices={choices} canManageErpx={canManageErpx}/>;
}
export const dynamic = "force-dynamic";
export default async function WorkplaceLayout({
  children,
  params,
}: LayoutProps<"/holdings/[holdingSlug]">) {
  const { holdingSlug } = await params;
  let context;
  try {
    context = await measureReadPhase("work_context",()=>requireWorkContext(holdingSlug));
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
  const { write } = await holdingResources(context.holdingId);
  const companySlug = (await write.prepare("SELECT slug FROM organization_nodes WHERE id=? AND kind='company' AND status='active'")
    .bind(context.companyId).first<{ slug: string }>())?.slug;
  const frontLocation = context.scope.kind === "holding"
    ? "holding"
    : context.scope.kind === "branch"
      ? `branches/${context.scope.branchSlug}`
      : context.scope.branchSlug
        ? `branches/${context.scope.branchSlug}/departments/${context.scope.departmentSlug}`
        : `departments/${context.scope.departmentSlug}`;
  const frontBase = companySlug
    ? `/front/${encodeURIComponent(holdingSlug)}/${encodeURIComponent(companySlug)}`
    : undefined;
  const frontHref = (app: "pos" | "pms") => frontBase ? `${frontBase}/${app}/${frontLocation}/` : undefined;
  const [workspaceBrand, companies, admin] = await measureReadPhase("layout",()=>Promise.all([
    resolveWorkspaceBrand(write, context),
    context.scope.kind === "holding" ? listWorkspaceCompanies(context.holdingId) : Promise.resolve([]),
    administrationAccess(context),
  ]));
  const featureCatalogue = process.env.NODE_ENV === 'production'
    ? await companyFeatureCatalogue(context) : [];
  const navigation = {
    manageErpxVisible: context.holdingId === "h003" && context.scope.kind === "holding" &&
      hasPermission(context, "manage_erpx.access") &&
      await requirePlatformContext().then((platform) => platform.userId === context.userId).catch(() => false),
    executiveVisible: await executiveVisible(write,context),
    workspaceBase: base,
    frontPosHref: frontHref("pos"),
    frontPmsHref: frontHref("pms"),
    scopeKind: context.scope.kind,
    beta: context.beta,
    role: context.role,
    permissions:context.iam?.permissions,
    adminSections: admin.sections,
    canManageApprovals: admin.manageOrganization,
    ...(process.env.NODE_ENV === 'production' ? {
      availableAppIds: (() => {
        const enabled = new Set(featureCatalogue.filter((entry) => ['Available','Pilot','Setup required'].includes(entry.state)).map((entry) => entry.id));
        const apps: string[] = [];
        if (['organization','identity_access','people_payroll'].some((id) => enabled.has(id))) apps.push('people');
        if (enabled.has('procurement')) apps.push('procurement');
        if (enabled.has('sales')) apps.push('sales');
        if (enabled.has('crm')) apps.push('crm');
        if (enabled.has('support')) apps.push('support');
        if (enabled.has('quality')) apps.push('quality');
        if (enabled.has('projects')) apps.push('projects');
        if (enabled.has('inventory')) apps.push('inventory');
        if (enabled.has('pos')) apps.push('pos');
        if (enabled.has('pms')) apps.push('pms');
        if (enabled.has('finance')) apps.push('finance');
        if (enabled.has('identity_access')) apps.push('technology');
        if (context.holdingId === 'h003') apps.push('manage-erpx');
        return apps;
      })(),
    } : {}),
  };
  const companyName =
    companies.find((company) => company.id === context.companyId)?.name ??
    (await write.prepare("SELECT display_name FROM organization_nodes WHERE id=? AND kind='company' AND status='active'")
      .bind(context.companyId).first<{display_name:string}>())?.display_name ?? "Company unavailable";
  const workspaceLabel =
    context.scope.kind === "holding"
      ? companyName
      : `${companyName} · ${scopeLabel}`;
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
    ancestors.push(ancestor(
      context.scope.branchName ?? context.scope.branchSlug,
      workspaceBase({ kind: "branch", holdingSlug, branchSlug: context.scope.branchSlug }),
    ));
  }
  if (context.scope.kind === "department")
    ancestors.push(ancestor(context.scope.departmentName, base));
  return (
    <NavigationCountsProvider key={`${base}:${context.userId}:${context.companyId}:${context.authority!.revision}`} base={base}>
    <div className={styles.shell}>
      <RememberWorkspace base={base} companyId={context.companyId} assignmentId={context.assignmentId} revision={context.authority!.revision} />
      <a href="#workspace-content" className="skip-link">
        Skip to content
      </a>
      <aside className={styles.sidebar} data-app-chrome>
        <Link className={styles.brand} href={`${base}/home`}>
          <ProductBrand className={`${styles.productBrand} ${styles.productBrandWordmark}`} />
          <ProductBrand variant="symbol" className={`${styles.productBrand} ${styles.productBrandSymbol}`} />
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
            <WorkspaceBrandMark brand={workspaceBrand} compact />
            <Suspense fallback={<button className={styles.workspaceTrigger} type="button" aria-label="Switch workspace" title={`Change workspace (currently ${workspaceLabel})`} disabled><Building2 size={18} aria-hidden="true"/><span className={styles.workspaceTriggerLabel}>{workspaceLabel}</span></button>}>
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
            <CommandSearch {...navigation} />
            <AccountSwitcher
              workspaceBase={base}
              assignments={context.availableAssignments ?? context.assignments}
              preferredAssignmentId={context.assignmentId}
              contextRevision={context.authority!.revision}
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
          <WorkContextBoundary
            key={`${context.companyId}:${context.assignmentId}:${context.authority!.revision}`}
            base={base}
            companyId={context.companyId}
            assignmentId={context.assignmentId}
            revision={context.authority!.revision}
          >
            <div className={styles.workspaceBody}>
              <div id="workspace-content" className={styles.workspacePage} tabIndex={-1}>
                <FeatureEnvironment navigation={navigation} productionCatalogue={process.env.NODE_ENV === 'production' ? featureCatalogue.map(({id,name,state})=>({id,name,state})) : undefined}>{children}</FeatureEnvironment>
              </div>
            </div>
            <Suspense><WorkspaceAssistant base={base} userId={context.userId} companyId={context.companyId} assignmentId={context.assignmentId} crossContext={hasPermission(context,"ai.context.cross")}/></Suspense>
          </WorkContextBoundary>
        </div>
      </div>
      <div data-app-chrome><Suspense>
        <WorkplaceNavigation {...navigation} mobile />
      </Suspense></div>
    </div>
    </NavigationCountsProvider>
  );
}
