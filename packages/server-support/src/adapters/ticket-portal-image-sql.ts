import 'server-only';
import { readTailR43ColumnsFromSql } from './read-tail-r43-sql';
import { readTailR44ImageAggregateSql } from './read-tail-r44-sql';

/** Canonical parameterized fragments for the Support ticket-portal image reads.
 * `supportTicketPortalImage` and `supportTicketPortalCoreImage` share one
 * selected-bindings CTE, the binding aggregate and the same bound-row image
 * aggregate; only the table, alias, ordering column and binding predicate vary.
 * Every caller keeps its own binds and its own independent statement. The
 * native ticket-portal reader remains the authority. */
export const portalImageSelectedCte = (ids: string[]) =>
  `WITH selected AS(${readTailR43ColumnsFromSql('*', `support_ticket_portal_bindings WHERE id IN (${ids.map(() => '?').join(',')})`)})`;

export const portalBindingImageSql = () =>
  readTailR44ImageAggregateSql('binding', 'binding.id', 'selected binding');

export const portalBoundImageSql = (alias: string, table: string, orderBy: string, predicate: string) =>
  readTailR44ImageAggregateSql(alias, `${alias}.${orderBy}`, `${table} ${alias} WHERE EXISTS(SELECT 1 FROM selected binding WHERE ${predicate})`);

/** One byte-identical outer image aggregate for the full and core portal image
 * readers. They differ only by the bound-row image list; both keep their own
 * selected-binding CTE, binds and independent statement. */
export const portalImageAggregateSql = (images: string[]) =>
  `SELECT jsonb_build_array(${images.join(',')})::text image`;
