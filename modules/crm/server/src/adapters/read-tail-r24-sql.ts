/** G4 / retire.crm.read-tail-r24 byte-identical read family.
 *
 * Every fragment below renders byte-for-byte as the pre-dedupe inline SQL.
 * All transaction lock and row-lock statements stay inline in the calling
 * adapters. r8-r23 families are untouched.
 */
import {crmLeadSelectSql} from './read-tail-r51-sql';

/** One parameterized renderer for the creator (created_by) lead-scope EXISTS
 * guard shared by the CRM identity, attachment and command-authority reads.
 * Callers pass only the creator alias, lead alias, field/company predicates and
 * an optional tail; rendered SQL stays byte-identical to the retired text. */
export const crmCreatorLeadScopeSql = (
  creator: string,
  leadAlias: string,
  fieldExpr: string,
  companyExpr: string,
  tail = '',
) => `(?::text IS NULL OR ${creator}.created_by=? OR EXISTS(${crmLeadSelectSql(`${leadAlias} WHERE ${fieldExpr}${companyExpr ? ` AND ${companyExpr}` : ''} AND ${leadAlias}.branch_id=? AND (?::text IS NULL OR ${leadAlias}.department_id=?)${tail}`)}))`;
