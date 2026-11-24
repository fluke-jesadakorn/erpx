import 'server-only';

/** Canonical parameterized fragments for the Support ticket-portal image reads.
 * `supportTicketPortalImage` and `supportTicketPortalCoreImage` share one
 * selected-bindings CTE, the binding aggregate and the same bound-row image
 * aggregate; only the table, alias, ordering column and binding predicate vary.
 * Every caller keeps its own binds and its own independent statement. The
 * native ticket-portal reader remains the authority. */
export const portalImageSelectedCte = (ids: string[]) =>
  `WITH selected AS(SELECT * FROM support_ticket_portal_bindings WHERE id IN (${ids.map(() => '?').join(',')}))`;

export const portalBindingImageSql = () =>
  `(SELECT jsonb_agg(to_jsonb(binding) ORDER BY binding.id) FROM selected binding)`;

export const portalBoundImageSql = (alias: string, table: string, orderBy: string, predicate: string) =>
  `(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.${orderBy}) FROM ${table} ${alias} WHERE EXISTS(SELECT 1 FROM selected binding WHERE ${predicate}))`;

/** One byte-identical outer image aggregate for the full and core portal image
 * readers. They differ only by the bound-row image list; both keep their own
 * selected-binding CTE, binds and independent statement. */
export const portalImageAggregateSql = (images: string[]) =>
  `SELECT jsonb_build_array(${images.join(',')})::text image`;
