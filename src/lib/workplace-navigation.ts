import {
  moduleRegistry,
  liveDestination,
  moduleHref,
} from "@/data/module-registry";
import type { AdminSection } from "./administration";
export type NavigationContext = {
  workspaceBase: string;
  beta?: boolean;
  allowedModules: string[];
  role?: string;
  roles?: string[];
  adminSections?: AdminSection[];
};
export type NavigationItem = {
  id: string;
  label: string;
  href: string;
  group: string;
  icon: string;
};
export function workplaceNavigation(context: NavigationContext) {
  const {
    workspaceBase: base,
    beta,
    allowedModules,
    role = "employee",
    adminSections = [],
  } = context;
  const roles = context.roles ?? [role];
  const platform = base === "/platform";
  const item = (
    id: string,
    label: string,
    route = id,
    group = "Workspace",
    icon = id,
  ): NavigationItem => ({ id, label, href: `${base}/${route}`, group, icon });
  const primary = [
    item("home", "Home"),
    item("work", "Work"),
    item("inbox", "Inbox"),
  ];
  const more: NavigationItem[] = [];
  if (!platform) { more.push(item("people", "People")); more.push(item("handbook", "Company handbook", "handbook", "Workspace", "learn")); }
  if (!beta) {
    if (!platform) {
      if (
        [
          "sales_operator",
          "sales_manager",
          "purchasing_operator",
          "purchasing_manager",
          "warehouse_manager",
          "operations_manager",
          "holding_owner",
          "finance_preparer",
          "finance_approver",
          "auditor",
        ].some((r) => roles.includes(r))
      )
        more.push(item("commerce", "Operations"));
      if (
        [
          "finance_preparer",
          "finance_approver",
          "treasury",
          "holding_owner",
          "auditor",
        ].some((r) => roles.includes(r))
      )
        more.push(item("finance", "Finance"));
      more.push(item("projects", "Projects"));
    }
    if (allowedModules.length) more.push(item("modules", "ERP tools"));
    more.push(item("learn", "Learn"));
    if (platform) more.push(item("feed", "Platform updates"));
  }
  if (!platform) {
    const sections = adminSections.filter((s) => s !== "feedback");
    if (sections.length)
      more.push(
        item("settings", "Administration", `settings?section=${sections[0]}`),
      );
    more.push(item("feedback", "Help & feedback", "settings?section=feedback"));
  }
  const known = [...primary, ...more];
  const capabilities: NavigationItem[] = beta
    ? []
    : moduleRegistry
        .filter((module) => allowedModules.includes(module.id))
        .flatMap((module) =>
          module.sections
            .filter((s) => s.capability)
            .map((section) => {
              const destination = platform
                ? undefined
                : liveDestination(module.id, section.id);
              return {
                id: `${module.id}/${section.id}`,
                label: `${module.name} · ${section.label}`,
                href: destination
                  ? `${base}/${destination === "feed" ? "home" : destination}`
                  : moduleHref(base, module, section.id),
                group: module.group,
                icon: module.id,
              };
            }),
        );
  const seen = new Set(known.map((x) => x.href));
  const search = [
    ...known,
    ...capabilities.filter((x) => {
      if (seen.has(x.href)) return false;
      seen.add(x.href);
      return true;
    }),
  ];
  return { primary, more, search };
}
export function activeDestination(base: string, pathname: string) {
  const route = pathname.slice(base.length + 1).split("/")[0];
  if (base === "/platform" && route === "feed") return "more";
  if (["home", "feed", "posts"].includes(route)) return "home";
  if (
    [
      "work",
      "requests",
      "staff-tasks",
      "tasks",
      "expenses",
      "stock-counts",
      "stock-transfers",
    ].includes(route)
  )
    return "work";
  if (route === "inbox") return "inbox";
  return "more";
}
