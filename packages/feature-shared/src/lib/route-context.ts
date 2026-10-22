import { canonicalUiHref } from '@erpx/feature-work_projection/lib/destination-registry';
export type RouteSearch = Record<string, string | string[] | undefined>;

export function routeSearchFromParams(params: Pick<URLSearchParams, "keys" | "getAll">): RouteSearch {
  return Object.fromEntries([...new Set(params.keys())].map(key => {
    const values = params.getAll(key);
    return [key, values.length > 1 ? values : values[0]];
  }));
}

/** Carry context through UI moves without losing repeated filters. */
export function routeContextHref(path: string, search: RouteSearch, patch: Record<string, string | null> = {}) {
  const query = new URLSearchParams();
  for (const [key, value] of Object.entries(search)) {
    if (typeof value === "string") query.append(key, value);
    else if (value) for (const item of value) query.append(key, item);
  }
  for (const [key, value] of Object.entries(patch)) {
    if (value === null) query.delete(key);
    else query.set(key, value);
  }
  return canonicalUiHref(`${path}${query.size ? `?${query}` : ""}`);
}
