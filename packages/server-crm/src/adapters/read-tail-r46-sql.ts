/** G4 / retire.crm.read-tail-r46 byte-identical projection/FROM head.
 *
 * Four owner-local read heads share one byte-identical prefix: the projection
 * and FROM body vary while the read scaffold stays fixed. Callers pass both
 * parts explicitly, so rendered SQL, multiline whitespace and bind order stay
 * byte-identical to the retired inline literals. The r42 existence renderer,
 * the r44 UNION ALL renderer, locks, command-receipt reads and cross-owner
 * exports are untouched.
 */
export const crmProjectionFromSql = (projection: string, from: string) =>
  `SELECT ${projection} FROM ${from}`;
