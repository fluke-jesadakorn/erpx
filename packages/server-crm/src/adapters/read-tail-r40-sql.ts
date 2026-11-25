/** G4 / retire.crm.read-tail-r40 byte-identical read-tail heads.
 *
 * Three owner-local read families share one renderer each: the scoped
 * descending option lists, the distinct owner directory reads and the lead
 * row sources. Callers pass their own body (and fixed ORDER BY / lead source
 * tail) so rendered SQL, multiline whitespace and bind order stay byte-identical
 * to the retired inline literals. Locks, command-receipt reads, native command
 * kinds and the r8-r39 families are untouched.
 */

/** Scoped descending option list. The body carries the projection, joins,
 * predicates and interpolations (native read filters and scope fragments);
 * the caller passes the fixed ORDER BY / LIMIT tail. The leading read head and
 * its " ORDER BY " separator live here so every former literal renders unchanged. */
import { crmSelectBodySql } from './read-tail-r46-sql';

export const crmScopedDescListSql = (body: string, order: string) => crmSelectBodySql(`${body} ORDER BY ${order}`);

/** Distinct owner directory head. DISTINCT stays in the head; callers pass the
 * full projection/join/predicate/ORDER BY body (including inner source calls),
 * so both former literals render byte-for-byte identically. */
export const crmDistinctOwnerSql = (body: string) => crmSelectBodySql(`DISTINCT ${body}`);

/** Lead row source head. Both callers pass the full FROM/WHERE body, including
 * any interpolation boundaries and NOT EXISTS / EXISTS tails; the shared lead
 * projection lives only here. */
export const crmLeadRowSourceSql = (body: string) => crmSelectBodySql(`lead.* ${body}`);
