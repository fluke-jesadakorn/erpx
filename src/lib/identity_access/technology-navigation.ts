import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { routeContextHref, type RouteSearch } from "@/lib/shared/route-context";

export const technologySections = {
  resources: [
    { id: "packages", title: "Packages" }, { id: "accounts", title: "Accounts" },
    { id: "groups", title: "Groups" }, { id: "policies", title: "ERPX policies" }, { id: "tasks", title: "Tasks" },
  ],
  directory: [
    { id: "objects", title: "Objects & OUs" }, { id: "connections", title: "Connections" },
    { id: "mappings", title: "Field mappings" }, { id: "work", title: "Work & verification" },
  ],
  settings: [
    { id: "attributes", title: "Custom attributes" }, { id: "roles", title: "Roles & permissions" },
    { id: "matrix", title: "Access matrix" }, { id: "groups", title: "Group mappings" }, { id: "reviews", title: "Reviews & audit" },
  ],
} as const;

export function technologyHref(base: string, search: RouteSearch, view: string, section?: string) {
  return routeContextHref(`${base}/technology`, search, { view, section: section ?? null, panel: null });
}

export function technologySettingsDestination(base: string, search: RouteSearch, section: string) {
  return technologyHref(base, search, section === "connections" ? "directory" : "settings", section);
}
