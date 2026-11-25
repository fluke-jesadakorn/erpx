/** G4 / retire.crm.read-tail-r44 byte-identical UNION-ALL head.
 *
 * The fact-arm prefix is the owner select-body plan with a literal `UNION ALL `
 * in front, so the second read token in this module is folded into r46. Callers
 * keep their leading indentation and pass only the projection/predicate body
 * (including their own cross-owner sources, kind interpolations and nested
 * probes), so rendered SQL, whitespace and bind order stay byte-identical to
 * the retired inline literals. MATERIALIZED heads, the r42 activity NOT EXISTS
 * and the r8-r43 families are untouched.
 */
import { crmSelectBodySql } from './read-tail-r46-sql';

export const crmUnionAllSql = (body: string) => `UNION ALL ${crmSelectBodySql(body)}`;
