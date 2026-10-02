import {
  type NavigationApp,
  type NavigationCategory,
  type NavigationItem,
  type WorkplaceNavigationModel,
} from "./workplace-navigation";

export type NavigationLauncherResult = {
  item: NavigationItem;
  context: string;
  categoryId: NavigationCategory["id"];
};

function appItem(app: NavigationApp): NavigationItem {
  return {
    id: app.id,
    label: app.label,
    href: app.href ?? app.items[0].href,
    group: "Apps",
    icon: app.icon,
    ...(app.items[0]?.openInNewTab ? { openInNewTab: true } : {}),
    keywords: app.keywords,
  };
}

function searchableText(
  item: NavigationItem,
  app: NavigationApp | undefined,
  category: NavigationCategory,
) {
  return [
    item.label,
    item.group,
    ...(item.keywords ?? []),
    app?.label ?? "",
    ...(app?.keywords ?? []),
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
  const id = app?.id ?? item.id;
  return model.categories.find((category) =>
    category.id === id || category.apps.some((entry) => entry.id === id),
  );
}

/**
 * The launcher shows apps by default. A query expands discovery to the full,
 * permission-filtered destination catalog while retaining the owning app.
 */
export function navigationLauncher(
  model: WorkplaceNavigationModel,
  search: string,
): { categories: NavigationCategory[]; results: NavigationLauncherResult[] } {
  const query = search.trim().toLocaleLowerCase();
  if (!query) return { categories: model.categories.map((category) =>
    category.id === "pms"
      ? { ...category, apps: model.apps.filter((app) => app.id === "pms").map(appItem) }
      : category,
  ), results: [] };

  const primaryIds = new Set(model.primary.map((item) => item.id));
  const candidates = [
    ...model.apps.map(appItem),
    ...model.search.filter((item) => !primaryIds.has(item.id)),
  ];
  const seen = new Set<string>();
  const results = candidates.flatMap((item): NavigationLauncherResult[] => {
    const app = model.apps.find(
      (entry) =>
        entry.id === item.id || entry.items.some((child) => child.id === item.id),
    );
    const category = categoryFor(model, item, app);
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
        context: app && app.id !== item.id ? app.label : category.label,
        categoryId: category.id,
      },
    ];
  });
  return { categories: [], results };
}

export function navigationAccordionStorageKey(workspaceBase: string) {
  return `erpx-navigation-category:v1:${workspaceBase}`;
}

export function navigationRailStorageKey(workspaceBase: string) {
  return `erpx-navigation-rail:v1:${workspaceBase}`;
}
