import { ProductBrand } from "@/components/brand/product-brand";
import { FeatureEnvironment } from "@/components/features/feature-page";
import { PlatformRoleSwitcher } from "@/components/workplace/platform-role-switcher";
import { RememberWorkspace } from "@/components/workplace/remember-workspace";
import styles from "@/components/workplace/shell.module.css";
import {
Appearance,
CommandSearch,
WorkplaceNavigation,
} from "@/components/workplace/workplace-controls";
import { WorkspaceMenu } from "@/components/workplace/workspace-menu";
import { requirePlatformContext } from "@/server/authorization";
import { pageError } from "@/server/page-error";
import { workspaceChoices } from "@/server/workspace-choices";
import { ShieldCheck } from "lucide-react";
import { cookies } from "next/headers";
import Link from "next/link";
import { Suspense } from "react";

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
      <aside className={styles.sidebar} data-app-chrome>
        <Link className={styles.brand} href="/platform/home">
          <ProductBrand className={styles.productBrand} />
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
      <div className={styles.main} data-app-main>
        <header className={styles.topbar} data-app-chrome>
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
          <FeatureEnvironment navigation={{workspaceBase:"/platform",scopeKind:"platform",role:context.role}}>{children}</FeatureEnvironment>
        </div>
      </div>
      <div data-app-chrome><Suspense>
        <WorkplaceNavigation
          workspaceBase="/platform"
          scopeKind="platform"
          mobile
        />
      </Suspense></div>
    </div>
  );
}
