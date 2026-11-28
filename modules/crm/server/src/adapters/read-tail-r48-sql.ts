/** G4 / retire.crm.read-tail-r48 byte-identical company-scope head.
 *
 * The company bind prefix is the owner select-body plan applied to the body
 * `?::text company_id${tail}`, so this module no longer carries a read token.
 * The actor scope row and the obligation scope CTE keep their own tails,
 * multiline whitespace and bind order byte-for-byte. Locks, command-receipt
 * reads, native command/read kinds, cross-owner exports and the r8-r47 families
 * are untouched.
 */
import { crmSelectBodySql } from './read-tail-r46-sql';

export const crmCompanyScopeSql = (tail: string) => crmSelectBodySql(`?::text company_id${tail}`);
