/** G4 / retire.procurement.read-tail-r53 safe NEW read-tail SQL family.
 *
 * The by-source EXISTS/FROM/WHERE head is shared by the r19 point-version
 * guard and the r34 approved-request guard. Every export renders
 * byte-identical text to the literal it replaces. Callers keep their own tails,
 * binds, bind order, result types and transaction; nothing here is cached or
 * executed. Row locks, native registries, cross-owner consumers and the
 * r17-r52 helpers stay untouched.
 */
export const existsSourceWhereHead = (source: string) =>
  `EXISTS(SELECT 1 FROM ${source} WHERE `;
