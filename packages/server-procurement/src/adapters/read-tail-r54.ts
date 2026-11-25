/** G4 / retire.procurement.read-tail-r54 safe NEW read-tail SQL family.
 *
 * The columns/FROM/WHERE head is shared by the r30 order-scoped aggregate
 * and the r34 calendar scoped list read. Every export renders byte-identical
 * text to the literal it replaces. Callers keep their own tails, binds, bind
 * order, result types and transaction; nothing here is cached or executed.
 * Row locks, native registries, cross-owner consumers and the r17-r53 helpers
 * stay untouched.
 */
export const columnsFromWhereHead = (columns: string, from: string, tail: string) =>
  `SELECT ${columns} FROM ${from} WHERE ${tail}`;
