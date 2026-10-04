import { workViews } from "./work-item";
import { z } from "zod";
import {
  resolveNavigation,
  workplaceNavigation,
  type NavigationContext,
  type NavigationItem,
} from "./workplace-navigation";
import { hasPermission } from "./permissions";
import catalog from "@/data/feature-widgets.json";

export type FeatureWidgetDefinition = {
  id: string;
  title: string;
  page: string;
  permission?: string;
  columns?: number;
};
export type FeatureDefinition = {
  id: string;
  title: string;
  pageKey: string;
  activeId: string;
  tabs: NavigationItem[];
  secondaryTabs: NavigationItem[];
};
export type WidgetLayoutDifference = {
  id: string;
  badges: string[];
};
export type LayoutDifferenceView = {
  hasPersonalOverride: boolean;
  changeCount: number;
  widgets: WidgetLayoutDifference[];
};
export const layoutItemSchema = z
  .object({
    id: z.string().min(1).max(160),
    columns: z.number().int().min(3).max(12),
    height: z.number().int().min(160).max(1600).nullable(),
    hidden: z.boolean(),
  })
  .strict();
export const pageLayoutSchema = z
  .object({
    schemaVersion: z.literal(1),
    widgets: z
      .array(layoutItemSchema)
      .max(120)
      .refine(
        (items) => new Set(items.map((w) => w.id)).size === items.length,
        "Choose each widget once.",
      ),
  })
  .strict();
export type PageLayout = z.infer<typeof pageLayoutSchema>;
export type LayoutItem = PageLayout["widgets"][number];
export type LayoutSnapshot = {
  personal: PageLayout | null;
  organization: PageLayout | null;
  version: number;
  organizationVersion: number;
  canPublish: boolean;
};
export const configurablePageKeys = [
  "people-directory",
  "people-directory-detail",
  "people-organization",
  "people-roster",
  "people-settings",
  "people-payroll",
  "people-payroll-detail",
  "people-clearance-detail",
  "overtime",
  "overtime-detail",
  "executive",
] as const;
export function isConfigurablePage(page: string): boolean {
  return (configurablePageKeys as readonly string[]).includes(page);
}
export const widgetDefinitions = catalog as FeatureWidgetDefinition[];
export function pageNavigationId(page: string) {
  const aliases: Record<string, string> = {
    executive: "executive-canvas",
    "people-organization": "people-directory",
    "people-settings": "people-directory",
    "people-directory-detail": "people-directory",
    "people-clearance-detail": "people-directory",
    "people-payroll-detail": "people-payroll",
    overtime: "people-roster",
    "overtime-detail": "people-roster",
  };
  return aliases[page] ?? page;
}
export function allowedWidgets(page: string, context: NavigationContext) {
  if (!isConfigurablePage(page)) return [];
  const actor = {
    role: context.role ?? "employee",
    permissions: context.permissions,
    assignmentScope: context.scopeKind,
  };
  if (
    context.workspaceBase !== "/account" &&
    context.scopeKind !== "platform" &&
    !workplaceNavigation(context).search.some(
      (item) => item.id === pageNavigationId(page),
    )
  )
    return [];
  return widgetDefinitions.filter(
    (w) =>
      w.page === page && (!w.permission || hasPermission(actor, w.permission)),
  );
}
export function defaultLayout(widgets: FeatureWidgetDefinition[]): PageLayout {
  return {
    schemaVersion: 1,
    widgets: widgets.map((w) => ({
      id: w.id,
      columns: w.columns ?? 12,
      height: null,
      hidden: false,
    })),
  };
}
export function reconcileLayout(
  layout: PageLayout | null,
  widgets: FeatureWidgetDefinition[],
): PageLayout {
  const defaults = defaultLayout(widgets),
    ids = new Set(widgets.map((w) => w.id));
  const existing = (layout?.widgets ?? []).filter((w) => ids.has(w.id));
  return {
    schemaVersion: 1,
    widgets: [
      ...existing,
      ...defaults.widgets.filter((w) => !existing.some((e) => e.id === w.id)),
    ],
  };
}
export function layoutDifferences(
  personal: PageLayout | null,
  organization: PageLayout | null,
  widgets: FeatureWidgetDefinition[],
): LayoutDifferenceView {
  if (!personal)
    return { hasPersonalOverride: false, changeCount: 0, widgets: [] };
  const current = reconcileLayout(personal, widgets);
  const baseline = reconcileLayout(organization, widgets);
  const baselineOrder = new Map(
    baseline.widgets.map((widget, index) => [widget.id, index]),
  );
  const differences = current.widgets.flatMap((widget, index) => {
    const organizationWidget = baseline.widgets.find(
      (candidate) => candidate.id === widget.id,
    );
    if (!organizationWidget) return [];
    const badges: string[] = [];
    if (widget.hidden !== organizationWidget.hidden)
      badges.push(widget.hidden ? "Hidden" : "Shown");
    if (baselineOrder.get(widget.id) !== index) badges.push("Moved");
    if (widget.columns !== organizationWidget.columns)
      badges.push(
        `Width ${organizationWidget.columns}/12 → ${widget.columns}/12`,
      );
    if (widget.height !== organizationWidget.height)
      badges.push(
        organizationWidget.height === null
          ? `Auto → Fixed height`
          : widget.height === null
            ? "Fixed height → Auto"
            : `Height ${organizationWidget.height}px → ${widget.height}px`,
      );
    return badges.length ? [{ id: widget.id, badges }] : [];
  });
  return {
    hasPersonalOverride: true,
    changeCount: differences.length,
    widgets: differences,
  };
}

export function resetWidgetToOrganization(
  layout: PageLayout,
  widgetId: string,
  organization: PageLayout | null,
  widgets: FeatureWidgetDefinition[],
): PageLayout {
  const current = reconcileLayout(layout, widgets);
  const baseline = reconcileLayout(organization, widgets);
  const organizationWidget = baseline.widgets.find(
    (widget) => widget.id === widgetId,
  );
  if (!organizationWidget) return current;
  const remaining = current.widgets.filter((widget) => widget.id !== widgetId);
  const organizationIndex = baseline.widgets.findIndex(
    (widget) => widget.id === widgetId,
  );
  const insertionIndex = Math.min(organizationIndex, remaining.length);
  remaining.splice(insertionIndex, 0, { ...organizationWidget });
  return { schemaVersion: 1, widgets: remaining };
}
export function featureLocation(
  context: NavigationContext,
  pathname: string | null,
  query: URLSearchParams,
): FeatureDefinition {
  pathname ??= `${context.workspaceBase}/home`;
  const model = workplaceNavigation(context),
    resolved = resolveNavigation(model, context.workspaceBase, pathname, query);
  const relative = pathname
    .slice(context.workspaceBase.length)
    .replace(/^\//, "");
  const app = model.apps.find((entry) => entry.id === resolved.groupId);
  const active = model.search.find((item) => item.id === resolved.activeId);
  let tabs =
    app?.items.filter((item) => !["projects-tasks"].includes(item.id)) ?? [];
  if (app?.id === "people" && context.beta && !tabs.some((item) => item.id === "people-organization")) {
    const organization: NavigationItem = { id: "people-organization", label: "Organization", href: `${context.workspaceBase}/people/organization`, group: "People", icon: "admin-people" };
    const rosterIndex = tabs.findIndex((item) => item.id === "people-roster");
    tabs = [...tabs];
    tabs.splice(rosterIndex >= 0 ? rosterIndex + 1 : 1, 0, organization);
  }
  let secondaryTabs: NavigationItem[] = [];
  let pageKey = resolved.activeId || relative.split("/")[0] || "home";
  if (relative === "people/organization") pageKey = "people-organization";
  if (relative === "commerce" && query.get("view") === "parties") pageKey = "operations-parties";
  if (
    relative.startsWith("people/") &&
    /^people\/[^/]+$/.test(relative) &&
    !["roster", "access", "organization", "settings"].includes(
      relative.split("/")[1],
    )
  )
    pageKey = "people-directory-detail";
  else if (
    /\/(?:[^/]+)$/.test(relative) &&
    /^(finance\/(invoices|journals|payments)|people\/payroll|people\/clearance|commerce\/orders|stock-counts|stock-transfers|expenses|projects|routine-templates|staff-tasks|requests|leave|overtime|posts|handbook|executive\/reports)\//.test(
      relative,
    )
  )
    pageKey = `${relative.replace(/\/[^/]+$/, "").replaceAll("/", "-")}-detail`;
  if (
    relative === "people/settings" &&
    ["templates", "attributes", "leave"].includes(
      query.get("section") ?? "templates",
    )
  )
    pageKey = "people-settings";
  if (
    relative === "people/access" &&
    ["groups", "policies"].includes(query.get("view") ?? "")
  )
    pageKey = "technology-organization";
  if (
    relative === "people/settings" &&
    ["roles", "matrix"].includes(query.get("section") ?? "")
  )
    pageKey = "technology-roles";
  if (resolved.activeId === "technology-organization") tabs = [];
  if(resolved.activeId === "work") tabs=workViews.map(([id,label])=>({id:`work-${id}`,label,href:`${context.workspaceBase}/work?view=${id}`,group:"Work",icon:"work"}));
  if (app?.id === "people" && !context.beta) {
    secondaryTabs = [];
    tabs = [];
  }
  if (relative === "calendar") {
    pageKey = `calendar-${query.get("preset") === "coverage" ? "coverage" : query.get("mode") === "mine" ? "mine" : "team"}`;
    tabs = ["mine", "team", "coverage"].map((id) => ({
      id: `calendar-${id}`,
      label: {
        mine: "My calendar",
        team: "Team calendar",
        coverage: "Coverage",
      }[id]!,
      href: `${context.workspaceBase}/calendar?${new URLSearchParams({ ...Object.fromEntries(query), mode: id === "mine" ? "mine" : "team", preset: id })}`,
      group: "Calendar",
      icon: "calendar",
    }));
  }
  if (relative.startsWith("pms/documents/")) pageKey = "pms-documents-detail";
  if (relative === "pos") pageKey = `pos-${query.get("tab") ?? "sale"}`;
  if (context.scopeKind === "platform") {
    pageKey = `platform-${relative.replaceAll("/", "-") || "home"}`;
  }
  return {
    id: app?.id ?? pageKey,
    title: app?.label ?? active?.label ?? relative.split("/")[0],
    pageKey,
    activeId: relative === "people/organization" ? "people-organization" : relative === "calendar" ? pageKey : resolved.activeId === "work" ? `work-${query.get("view")??"actions"}` : resolved.activeId,
    tabs,
    secondaryTabs,
  };
}

export function legacyWidgetTarget(query: URLSearchParams) {
  return (
    query.get("widget") ??
    (query.get("status") === "archived"
      ? "archived-profiles"
      : query.get("display") === "explorer"
        ? "people-explorer"
        : (query.get("section") ??
          (query.get("edit") === "1"
            ? "handbook-drafts"
            : query.get("view") === "policies"
              ? "policies"
              : query.get("tab")) ??
          null))
  );
}
