"use client";
import type { UiAppearance } from "@/components/ui/appearance";
import { Button } from "@/components/ui/foundation";
import {
  createContext,
  lazy,
  Suspense,
  useContext,
  useState,
  type HTMLAttributes,
  type ReactNode,
} from "react";
import Link from "next/link";
import { usePathname, useSearchParams } from "next/navigation";
import { Settings2 } from "lucide-react";
import type { NavigationContext } from "@/lib/workplace-navigation";
import { featureLocation, legacyWidgetTarget, isConfigurablePage } from "@/lib/page-layouts";
import { productionPageCut, type PageCutEntry } from "@/lib/hospitality-page-cut";
import { ComingSoonPage } from "@/components/workplace/coming-soon-page";
import { GridLocation, PageDrawer } from "./widget-context";
export { FeatureWidget } from "./feature-widget";
import styles from "./features.module.css";

const LayoutGrid = lazy(() =>
  import("./layout-editor").then((module) => ({ default: module.LayoutGrid })),
);
const Environment = createContext<NavigationContext>({
  workspaceBase: "/account",
  scopeKind: "platform",
});
export function useFeatureNavigation() {
  return useContext(Environment);
}
export function FeatureEnvironment({
  navigation,
  children,
  productionCatalogue,
}: {
  navigation: NavigationContext;
  children: ReactNode;
  productionCatalogue?: readonly PageCutEntry[];
}) {
  const pathname = usePathname(),
    search = useSearchParams();
  const cut =
    productionCatalogue &&
    productionPageCut(pathname, navigation.workspaceBase, search, productionCatalogue);
  return (
    <Environment value={navigation}>
      {cut ? <ComingSoonPage entry={cut} base={navigation.workspaceBase} /> : children}
    </Environment>
  );
}
export function FeatureTabs({ appearance = "legacy" }: { appearance?: UiAppearance } = {}) {
  const navigation = useContext(Environment),
    drawer = useContext(PageDrawer),
    pathname = usePathname(),
    search = useSearchParams();
  const feature = featureLocation(navigation, pathname, new URLSearchParams(search));
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
        <Suspense fallback={fixed}>
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
        </Suspense>
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
