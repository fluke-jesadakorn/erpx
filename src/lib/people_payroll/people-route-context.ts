import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
export type PeopleRouteSearch = Record<string, string | string[] | undefined>;
import { routeContextHref } from "@/lib/shared/route-context";

export function peopleSettingsHref(base: string, search: PeopleRouteSearch, context: Record<string, string | null> = {}) {
  return routeContextHref(`${base}/people/settings`, search, { panel: null, ...context });
}

/** Keep the full legacy context, including repeated values, when changing its UI. */
export function peopleContextHref(
  base: string,
  search: PeopleRouteSearch,
  context: Record<string, string | null> = {},
) {
  const query = new URLSearchParams();
  for (const [key, value] of Object.entries(search)) {
    if (typeof value === "string") query.append(key, value);
    else if (value) for (const item of value) query.append(key, item);
  }
  for (const [key, value] of Object.entries(context)) {
    if (value === null) query.delete(key);
    else query.set(key, value);
  }
  return canonicalUiHref(`${base}/people${query.size ? `?${query}` : ""}`);
}
