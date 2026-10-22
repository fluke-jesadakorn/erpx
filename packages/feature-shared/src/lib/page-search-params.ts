/** App Router page search params use arrays for repeated query keys. These
 * workspace filters are single-valued, so use URLSearchParams.get semantics. */
export type PageSearchParams = Readonly<Record<string, string | string[] | undefined>>;

export function firstPageSearchValues(params: PageSearchParams): Record<string, string | undefined> {
  const values: Record<string, string | undefined> = {};
  for (const [key, value] of Object.entries(params)) {
    const first = Array.isArray(value) ? value[0] : value;
    if (typeof first === "string") values[key] = first;
  }
  return values;
}
