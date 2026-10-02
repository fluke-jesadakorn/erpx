export type PeopleRouteSearch = Record<string, string | string[] | undefined>;

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
  return `${base}/people${query.size ? `?${query}` : ""}`;
}
