import 'server-only';
import { readTailR43ColumnsFromSql } from './read-tail-r43-sql';

/** Canonical parameterized selected-bindings CTE for the Support ticket-portal
 * image reads. The image aggregate itself is planned by the native owner; this
 * CTE owns only the variable binding-id list, and every caller keeps its own
 * binds and its own independent statement. */
export const portalImageSelectedCte = (ids: string[]) =>
  `WITH selected AS(${readTailR43ColumnsFromSql('*', `support_ticket_portal_bindings WHERE id IN (${ids.map(() => '?').join(',')})`)})`;
