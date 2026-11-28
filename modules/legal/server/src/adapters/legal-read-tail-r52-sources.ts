import "server-only";
import { legalPointByKeySql } from './legal-read-tail-r36-sources';

/** R52 destruction-state guards: the exact item-keyed point reads, shared.
 * Delegates to the verified r36 point-read head; the caller supplies only a
 * fixed state scope. No request value reaches the head, no bind is added and
 * each rendered statement is byte-identical to the former inline copy. */
export const legalDestructionGuardSql = (scope: 'executing' | 'active') =>
  legalPointByKeySql('1', 'legal_destructions', 'item_id') +
  (scope === 'executing' ? " AND state='executing'" : " AND state IN ('executing','completed')");
