import {
  type NavigationApp,
  type NavigationCategory,
  type NavigationItem,
  type WorkplaceNavigationModel,
} from "@erpx/feature-work_projection/lib/workplace-navigation";

export type NavigationLauncherResult = {
  item: NavigationItem;
  context: string;
  categoryId: NavigationCategory["id"] | "daily";
};

function searchableText(
  item: NavigationItem,
  app: NavigationApp | undefined,
  category: Pick<NavigationCategory,"label">,
) {
  return [
    item.label,
    item.group,
    ...(item.keywords ?? []),
    app?.label ?? "",
    category.label,
  ]
    .join(" ")
    .toLocaleLowerCase();
}

function categoryFor(
  model: WorkplaceNavigationModel,
  item: NavigationItem,
  app?: NavigationApp,
) {
  if (item.categoryId) return model.categories.find(category => category.id === item.categoryId);
  const id = app?.id ?? item.id;
  return model.categories.find((category) =>
    category.id === item.categoryId || category.apps.some((entry) => entry.id === item.id || entry.id === id || app?.items.some(child => child.id === entry.id)),
  );
}

/**
 * The launcher shows apps by default. A query expands discovery to the full,
 * permission-filtered destination catalog and its discovery categories.
 */
export function navigationSearch(
  model: WorkplaceNavigationModel,
  search: string,
  includePrimary = true,
): NavigationLauncherResult[] {
  const query = search.trim().toLocaleLowerCase();
  const primaryIds = new Set(model.primary.map((item) => item.id));
  const candidates = model.search.filter(item => includePrimary || !primaryIds.has(item.id));
  const seen = new Set<string>();
  const results = candidates.flatMap((item): NavigationLauncherResult[] => {
    const app = model.apps.find(
      (entry) =>
        entry.id === item.id || entry.items.some((child) => child.id === item.id),
    );
    const category = item.categoryId === "daily" || !model.categories.length
      ? { id: "daily" as const, label: item.group }
      : categoryFor(model, item, app);
    if (
      !category ||
      seen.has(item.href) ||
      !searchableText(item, app, category).includes(query)
    )
      return [];
    seen.add(item.href);
    return [
      {
        item,
        context: category.label,
        categoryId: category.id,
      },
    ];
  });
  return results;
}

export function navigationLauncher(
  model: WorkplaceNavigationModel,
  search: string,
): { categories: NavigationCategory[]; results: NavigationLauncherResult[] } {
  return search.trim()
    ? { categories: [], results: navigationSearch(model, search, false) }
    : { categories: model.categories, results: [] };
}

export function navigationAccordionStorageKey(workspaceBase: string) {
  return `erpx-navigation-category:v1:${workspaceBase}`;
}

export function navigationRailStorageKey(workspaceBase: string) {
  return `erpx-navigation-rail:v1:${workspaceBase}`;
}
