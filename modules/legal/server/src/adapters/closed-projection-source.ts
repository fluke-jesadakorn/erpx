import "server-only";

/** One closed renderer for the fixed Legal native projection sources. Column
 * list, separator, table and wrapper are trusted constants in the sibling
 * source adapters; no SQL text or relation name is accepted from a caller. */
export const closedProjection = (
  cols: readonly string[],
  sep: string,
  table: string,
  wrap: readonly [string, string] = ['', ''],
) => `${wrap[0]}SELECT ${cols.map((c) => `"${c}"`).join(sep)} FROM ${table}${wrap[1]}`;
