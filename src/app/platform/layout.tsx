import { RememberWorkspace } from "@/components/workplace/remember-workspace";
import { WorkspaceMenu } from "@/components/workplace/workspace-menu";
import { workspaceChoices } from "@/server/workspace-choices";
import Link from "next/link";
import { cookies } from "next/headers";
import { pageError } from "@/server/page-error";
import { Suspense } from "react";
import { ShieldCheck } from "lucide-react";
import { requirePlatformContext } from "@/server/authorization";
import {
  Appearance,
  CommandSearch,
  WorkplaceNavigation,
} from "@/components/workplace/workplace-controls";
import { PlatformRoleSwitcher } from "@/components/workplace/platform-role-switcher";
import styles from "@/components/workplace/shell.module.css";

export const dynamic = "force-dynamic";

export default async function PlatformLayout({
  children,
}: LayoutProps<"/platform">) {
  let context;
  try {
    context = await requirePlatformContext();
  } catch (error) {
    return pageError(error);
  }
  const theme = (await cookies()).get("erpx_appearance")?.value ?? "light";
  return (
    <div className={styles.shell}>
      <RememberWorkspace base="/platform" />
      <a href="#workspace-content" className="skip-link">
        Skip to content
      </a>
      <aside className={styles.sidebar}>
        <Link className={styles.brand} href="/platform/home">
          <span className={styles.brandMark}>E</span>
          <strong>
            erpx<span style={{ color: "var(--brand)" }}>.</span>
          </strong>
          <small>PLATFORM</small>
        </Link>
        <Suspense>
          <WorkplaceNavigation
            workspaceBase="/platform"
            scopeKind="platform"
          />
        </Suspense>
        <div className={styles.sidebarBottom}>
          <div>
            <ShieldCheck size={17} />
            <div>
              <strong>Platform</strong>
              <small>Control plane only</small>
            </div>
          </div>
          <Appearance initialTheme={theme} />
        </div>
      </aside>
      <div className={styles.main}>
        <header className={styles.topbar}>
          <div className={styles.workspace}>
            <WorkspaceMenu
              label="Platform administration"
              choices={await workspaceChoices()}
              current={{ kind: "platform" }}
            />
            <div className={styles.workspaceDetails}>
              <strong>Platform</strong>
              <span className={styles.workspaceScope}>
                No customer-data authority
              </span>
            </div>
          </div>
          <div className={styles.actions}>
            <CommandSearch
              workspaceBase="/platform"
              scopeKind="platform"
            />
            <PlatformRoleSwitcher
              activeAssignmentId={context.assignmentId}
              assignments={context.assignments}
            />
          </div>
        </header>
        <div id="workspace-content" tabIndex={-1}>
          {children}
        </div>
      </div>
      <Suspense>
        <WorkplaceNavigation
          workspaceBase="/platform"
          scopeKind="platform"
          mobile
        />
      </Suspense>
    </div>
  );
}
