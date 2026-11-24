/** Shared parameterized ticket guard witness for the Support read tail.
 * The source-version assertion and the evidence-stage guard use the exact same
 * ticket identity prefix; each caller passes its own trailing predicate and
 * binds. Rendering stays byte-identical and the native readers remain the
 * authority. */
export const ticketGuardExistsSql = (predicate: string) =>
    `EXISTS(SELECT 1 FROM support_tickets t WHERE t.id=? AND ${predicate})`;
