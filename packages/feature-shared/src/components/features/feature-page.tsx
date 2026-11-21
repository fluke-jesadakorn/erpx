"use client";
import type { UiAppearance } from "@erpx/ui/components/appearance";
import { Button } from "@erpx/ui/components/foundation";
import {
  useContext,
  useState,
  type HTMLAttributes,
} from "react";
import Link from "@erpx/workplace/components/workspace-link";
import { usePathname } from "next/navigation";
import { useSearchParams } from "@erpx/feature-shared/lib/destination-search";
import { destinationForPath, workspacePathParts } from "@erpx/feature-work_projection/lib/destination-registry";
import { workEntryRoots } from "@erpx/feature-work_projection/lib/navigation-presentation";
import { Settings2 } from "lucide-react";
import { featureLocation, legacyWidgetTarget, isConfigurablePage } from "@erpx/feature-work_projection/lib/page-layouts-view";
import { GridLocation, PageDrawer } from "@erpx/feature-shared/components/features/widget-context";
import { LayoutGrid } from "@erpx/feature-shared/components/features/layout-editor";
export { FeatureWidget } from "@erpx/feature-shared/components/features/feature-widget";
import styles from "@erpx/feature-shared/components/features/features.module.css";

export { FeatureEnvironment, useFeatureNavigation } from "@erpx/feature-shared/components/features/feature-environment";
import { FeatureNavigationContext as Environment } from "@erpx/feature-shared/components/features/feature-environment";

export function FeatureTabs({ appearance = "legacy" }: { appearance?: UiAppearance } = {}) {
  const navigation = useContext(Environment),
    drawer = useContext(PageDrawer),
    pathname = usePathname(),
    search = useSearchParams();
  const feature = featureLocation(navigation, pathname, new URLSearchParams(search));
  const collection = workspacePathParts(pathname)?.path;
  if (collection && (destinationForPath(collection) || Object.hasOwn(workEntryRoots, collection))) return null;
  if (
    feature.id === "work" ||
    feature.id === "people" ||
    (feature.id === "finance" && navigation.financeWorkspaceV2)
  )
    return null;
  const hasPeopleTools = feature.secondaryTabs.length > 0;
  if (feature.tabs.length < 2 && !hasPeopleTools) return null;
  return (
    <nav className={appearance === "standard" ? "ui-tab-nav" : styles.tabs} data-feature-tabs aria-label={`${feature.title} views`}>
      {feature.tabs.map((tab) => (
        <Link
          key={tab.id}
          href={
            feature.id === "pms" && search.get("property")
              ? `${tab.href}?property=${encodeURIComponent(search.get("property")!)}`
              : tab.href
          }
          className={appearance === "standard" ? "ui-tab-link" : undefined}
          aria-current={tab.id === feature.activeId ? "page" : undefined}
        >
          {tab.label}
        </Link>
      ))}
      {hasPeopleTools && (
        <Button
          variant="plain"
          type="button"
          className={styles.peopleToolsTrigger}
          aria-expanded={drawer?.open ?? false}
          aria-controls="people-tools-drawer"
          onClick={() => drawer?.setOpen(true)}
        >
          <Settings2 size={16} /> People tools
        </Button>
      )}
    </nav>
  );
}
export function FeatureTabBar({ children, className = "", appearance = "legacy", ...props }: HTMLAttributes<HTMLElement> & { appearance?: UiAppearance }) {
  return (
    <nav {...props} className={`${appearance === "standard" ? "ui-tab-nav ui-feature-tab-nav" : styles.tabs} ${className}`} data-feature-tabs>
      {children}
    </nav>
  );
}
export function WidgetGrid({
  children,
  className = "",
  pageKey: explicit,
  ...props
}: HTMLAttributes<HTMLDivElement> & { pageKey?: string }) {
  const navigation = useContext(Environment),
    pathname = usePathname(),
    search = useSearchParams();
  const parent = useContext(GridLocation);
  const feature = featureLocation(navigation, pathname, new URLSearchParams(search));
  const pageKey = explicit ?? feature.pageKey;
  const target = legacyWidgetTarget(new URLSearchParams(search));
  if (parent)
    return (
      <div {...props} className={`${styles.nestedGrid} ${className}`}>
        {children}
      </div>
    );
  const fixed = (
    <div {...props} className={`${styles.grid} ${className}`} data-widget-grid data-page-key={pageKey}>
      {children}
    </div>
  );
  return (
    <GridLocation value={{ target }}>
      {isConfigurablePage(pageKey) ? (
        <LayoutGrid
          key={`${navigation.workspaceBase}:${pageKey}`}
          {...props}
          className={className}
          pageKey={pageKey}
          navigation={navigation}
          feature={feature}
          target={target}
        >
          {children}
        </LayoutGrid>
      ) : fixed}
    </GridLocation>
  );
}

export function FeaturePage({ children, ...props }: HTMLAttributes<HTMLDivElement>) {
  const [open, setOpen] = useState(false);
  return (
    <PageDrawer value={{ open, setOpen }}>
      <div {...props} data-feature-page>
        {children}
      </div>
    </PageDrawer>
  );
}
