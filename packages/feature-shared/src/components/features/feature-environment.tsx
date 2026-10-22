"use client";

import { createContext, useContext, type ReactNode } from "react";
import { usePathname } from "next/navigation";
import { useSearchParams } from "@erpx/feature-shared/lib/destination-search";
import type { NavigationContext } from "@erpx/feature-work_projection/lib/workplace-navigation";
import { productionPageCut, type PageCutEntry } from "@erpx/feature-shared/lib/hospitality-page-cut";
import { ComingSoonPage } from "@erpx/workplace/components/coming-soon-page";

export const FeatureNavigationContext = createContext<NavigationContext>({
  workspaceBase: "/account",
  scopeKind: "platform",
});
export function useFeatureNavigation() {
  return useContext(FeatureNavigationContext);
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
    <FeatureNavigationContext value={navigation}>
      {cut ? <ComingSoonPage entry={cut} base={navigation.workspaceBase} /> : children}
    </FeatureNavigationContext>
  );
}
