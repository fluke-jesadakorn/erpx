import { productPageAllowed } from "@/lib/runtime/product-route-policy";
import type { ProductHostKey } from "@/server/modules/commercial/public/rules/product-registry";
import type { NavigationItem, WorkplaceNavigationModel } from "./workplace-navigation";

/** Presentation only: retain the original permission/readiness decision, then
 * keep destinations owned by this host. Parent groups may retain an eligible
 * child without linking to another product's collection. */
export function productOfficeNavigation(model: WorkplaceNavigationModel, productId: ProductHostKey): WorkplaceNavigationModel {
  const allowed = (href: string) => {
    try { return productPageAllowed(productId, new URL(href, "https://navigation.invalid").pathname); }
    catch { return false; }
  };
  const item = (entry: NavigationItem): NavigationItem | null => {
    const children = (entry.submodules ?? []).flatMap(child => { const kept = item(child); return kept ? [kept] : []; });
    if (!allowed(entry.href) && !children.length) return null;
    return { ...entry, href: allowed(entry.href) ? entry.href : children[0].href,
      ...(entry.submodules ? { submodules: children } : {}) };
  };
  const items = (entries: NavigationItem[]) => entries.flatMap(entry => { const kept = item(entry); return kept ? [kept] : []; });
  return { ...model, primary: items(model.primary), standalone: items(model.standalone), search: items(model.search),
    apps: model.apps.flatMap(app => {
      const kept = items(app.items);
      return kept.length ? [{ ...app, items: kept, href: app.href && allowed(app.href) ? app.href : kept[0].href }] : [];
    }),
    categories: model.categories.flatMap(category => {
      const apps = items(category.apps), secondaryItems = items(category.secondaryItems ?? []);
      return apps.length || secondaryItems.length ? [{ ...category, apps, secondaryItems }] : [];
    }),
  };
}
