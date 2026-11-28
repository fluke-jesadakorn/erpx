/** G4 / retire.crm.read-tail-r42 byte-identical existence-probe head.
 *
 * The owner-local existence probes share one renderer. Callers keep their own
 * EXISTS / NOT EXISTS wrapper (or standalone probe) and pass the full source
 * and predicate body, so rendered SQL, multiline whitespace and bind order stay
 * byte-identical to the retired inline literals. Locks, command-receipt reads,
 * native command/read kinds, the no-space attachment-read probe and the r8-r41
 * families are untouched.
 */
export const crmSelectOneSql = (body: string) => `SELECT 1 FROM ${body}`;
