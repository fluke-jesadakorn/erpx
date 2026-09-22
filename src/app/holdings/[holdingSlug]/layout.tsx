import { measureReadPhase } from "@/server/runtime/read-metrics";
import { ProductBrand } from "@/components/brand/product-brand";
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
} from "@/server/authorization";
import { resolveWorkspaceBrand } from "@/server/branding";
import { executiveVisible } from "@/server/executive/access";
import { pageError } from "@/server/page-error";
import { holdingResources } from "@/server/runtime";
import { workspaceChoices } from "@/server/workspace-choices";
import { Building2 } from "lucide-react";
import { cookies } from "next/headers";
import Link from "next/link";
import { Suspense,type ComponentProps } from "react";
import { AccountSwitcher } from "./account-switcher";
async function DeferredWorkspaceMenu(props: Omit<ComponentProps<typeof WorkspaceMenu>, "choices">) {
  return <WorkspaceMenu {...props} choices={await workspaceChoices()}/>;
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
  const theme = (await cookies()).get("erpx_appearance")?.value ?? "light";
  const holdingName = context.holdingName;
  const base = workspaceBase(publicScope(context));
  const scopeLabel =
    context.scope.kind === "department"
      ? context.scope.departmentName
      : context.scope.kind === "branch"
        ? context.scope.branchName
        : "Holding";
  const { write } = await holdingResources(context.holdingId);
  const [workspaceBrand, companies, admin] = await measureReadPhase("layout",()=>Promise.all([
    resolveWorkspaceBrand(write, context),
    context.scope.kind === "holding" ? listWorkspaceCompanies(context.holdingId) : Promise.resolve([]),
    administrationAccess(context),
  ]));
  const navigation = {
    executiveVisible: await executiveVisible(write,context),
    workspaceBase: base,
    scopeKind: context.scope.kind,
    beta: context.beta,
    role: context.role,
    permissions:context.iam?.permissions,
    adminSections: admin.sections,
    canManageApprovals: admin.manageOrganization,
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
        <div id="workspace-content" tabIndex={-1}>
          <WorkContextBoundary
            key={`${context.companyId}:${context.assignmentId}:${context.authority!.revision}`}
            base={base}
            companyId={context.companyId}
            assignmentId={context.assignmentId}
            revision={context.authority!.revision}
          >
            <FeatureEnvironment navigation={navigation}>{children}</FeatureEnvironment>
            <Suspense><WorkspaceAssistant base={base} crossContext={hasPermission(context,"ai.context.cross")}/></Suspense>
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
