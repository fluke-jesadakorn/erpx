/** G4 / retire.crm.read-tail-r39 byte-identical clocked read head.
 *
 * The analytics option, base-row and cost reads share one clocked read head.
 * Callers pass their own clock CTE and full body so rendered SQL, multiline
 * whitespace and bind order stay byte-identical to the retired inline literals.
 * The revenue read keeps its inline `WITH ${clock},source_journals AS(` head
 * because it adds a second CTE. Locks, command-receipt reads, native command
 * kinds and the r8-r38 families are untouched.
 */
export const crmClockedReadSql = (clock: string, body: string) => `WITH ${clock} SELECT ${body}`;
