import { responsibilityRoles } from "@/lib/responsibilities";
import Link from "next/link";
import { cookies } from "next/headers";
import { notFound, redirect } from "next/navigation";
import { Suspense } from "react";
import { Building2 } from "lucide-react";
import {
  publicScope,
  listWorkspaceCompanies,
  requireWorkContext,
} from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { AccountSwitcher } from "./account-switcher";
import { WorkspaceMenu } from "@/components/workplace/workspace-menu";
import { administrationAccess } from "@/server/administration";
import {
  WorkspaceBreadcrumb,
  type WorkspaceCrumb,
} from "@/components/workplace/workspace-breadcrumb";
import {
  CommandSearch,
  WorkplaceNavigation,
} from "@/components/workplace/workplace-controls";
import styles from "@/components/workplace/shell.module.css";
export const dynamic = "force-dynamic";
export default async function WorkplaceLayout({
  children,
  params,
}: LayoutProps<"/holdings/[holdingSlug]">) {
  const { holdingSlug } = await params;
  let context;
  try {
    context = await requireWorkContext(holdingSlug);
  } catch (error) {
    if (error instanceof Error && error.message === "UNAUTHENTICATED")
      redirect("/login");
    notFound();
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
  const companies =
    context.scope.kind === "holding"
      ? await listWorkspaceCompanies(context.holdingId)
      : [];
  const admin = await administrationAccess(context);
  const navigation = {
    workspaceBase: base,
    beta: context.beta,
    role: context.role,
    roles: responsibilityRoles(context),
    allowedModules: context.modules,
    adminSections: admin.sections,
  };
  const companyName =
    companies.find((company) => company.id === context.companyId)?.name ??
    holdingName;
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
    <div className={styles.shell}>
      <a href="#workspace-content" className="skip-link">
        Skip to content
      </a>
      <aside className={styles.sidebar}>
        <Link className={styles.brand} href={`${base}/home`}>
          <span className={styles.brandMark}>E</span>
          <strong>
            erpx<span style={{ color: "var(--brand)" }}>.</span>
          </strong>
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
      <div className={styles.main}>
        <header className={styles.topbar}>
          <div className={styles.workspace}>
            <WorkspaceMenu
              base={base}
              label={workspaceLabel}
              companyId={context.companyId}
              companies={companies}
              assignments={context.assignments}
              activeAssignmentId={context.assignmentId}
              workspaces={context.workspaces}
            />

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
              />
            </Suspense>
          </div>
          <div className={styles.actions}>
            <CommandSearch {...navigation} />
            <AccountSwitcher
              workspaceBase={base}
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
          {children}
        </div>
      </div>
      <Suspense>
        <WorkplaceNavigation {...navigation} mobile />
      </Suspense>
    </div>
  );
}
