/** G4 / retire.crm.read-tail-r51 byte-identical lead-select head.
 *
 * The creator-scope EXISTS guard and the eligible-owner probe share one read
 * head: both select a witness row from crm_leads, then append their own alias,
 * predicates and tail. The renderer below reproduces the pre-dedupe inline SQL
 * byte-for-byte; callers pass only their own tail, so rendered SQL, whitespace
 * and bind order stay identical. Locks, no-space probes, command-receipt reads,
 * native command/read kinds, cross-owner exports and the r8-r50 families are
 * untouched.
 */
export const crmLeadSelectSql = (tail: string) => `SELECT 1 FROM crm_leads ${tail}`;
