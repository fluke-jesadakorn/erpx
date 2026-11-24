/** One canonical customer-access event evidence probe shared by the Customer
 *  Authority package-obligation facts read (G4 read-tail r27). It wraps the two
 *  `customer-access-event-` id probes of the binding event chain. The table,
 *  alias, id column and trailing predicate are closed, owner-local strings;
 *  both callers render byte-identically to their pre-dedupe SQL. Callers keep
 *  their own prepared statement and binds. This family is a pure read: no
 *  writes, no row/table locks and no advisory locks. The r21-r26 builders, all
 *  source locks and native-owned statement kinds stay untouched. */
export const accessEventEvidenceProbe = (table: string, alias: string, key: string, predicate: string) =>
  `EXISTS(SELECT 1 FROM ${table} ${alias} WHERE event.id='customer-access-event-'||${alias}.${key} AND ${predicate})`;
