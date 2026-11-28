/** G4 / retire.procurement.read-tail-r52 safe NEW read-tail SQL family.
 *
 * The by-id source-existence head is shared by the r25 company/scope visibility
 * witness and the r30 version/status command fence. Every export renders
 * byte-identical text to the literal it replaces. Callers keep their own tails,
 * binds, bind order, result types and transaction; nothing here is cached or
 * executed. Row locks, native writes, cross-owner consumers and the r17-r51
 * helpers stay untouched.
 */
export const byIdExistsHead = (table: string, alias: string) =>
  `EXISTS(SELECT 1 FROM ${table} ${alias} WHERE ${alias}.id=`;
