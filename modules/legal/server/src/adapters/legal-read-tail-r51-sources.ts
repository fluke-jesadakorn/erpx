import "server-only";
import { legalPointByIdSql } from './legal-read-tail-r32-sources';

/** R51 law-corpus state guards: the exact source/revision point reads, shared.
 * Delegates to the verified r32 point-read head; the caller supplies only the
 * fixed fact table and fixed state literal. No request value reaches the head,
 * no bind is added and each rendered statement is byte-identical to the former
 * inline copy. */
export const legalLawStateGuardSql = (table: string, state: 'active' | 'approved') =>
  legalPointByIdSql('1', table) + ` AND rule_id=? AND company_id=? AND state='${state}'`;
