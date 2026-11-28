/** G4 / retire.procurement.read-tail-r55 safe NEW read-tail SQL family.
 *
 * The owner-local ARRAY subquery head (`ARRAY(` + a SELECT/FROM/WHERE body + `)`)
 * is shared by the RFQ attachment source's product-id and supplier-id
 * projections. Every export renders byte-identical text to the literal it
 * replaces. Callers keep their own tails, binds, bind order, result types and
 * transaction; nothing here is cached or executed. Row locks, native
 * registries, cross-owner consumers and the r17-r54 helpers stay untouched.
 */
export const arraySelectBy = (columns: string, table: string, tail: string) =>
  `ARRAY(SELECT ${columns} FROM ${table} WHERE ${tail})`;
