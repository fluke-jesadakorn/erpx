/** Canonical witness-array renderer for the Social upload cleanup graph.
 * Every call reproduces the original inlined COALESCE/jsonb_agg SQL bytes exactly;
 * alias, source, order and optional filter are the only varying parts. */
export const jsonbRowsSql = (alias: string, from: string, orderBy: string, where = '') =>
  `COALESCE((SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${orderBy}) FROM ${from} ${alias}${where ? ` WHERE ${where}` : ''}),'[]'::jsonb)`;
