/** One parameterized ticket-identity read for the Support read tail. The
 * source-version guard witness and the evidence eligibility witness differ only
 * by projection and trailing predicate; both render from this single read so
 * the ticket identity predicate cannot drift. Native readers remain the
 * authority. */
export const ticketByIdSql = (projection: string, predicate: string) =>
    `SELECT ${projection} FROM support_tickets t WHERE t.id=? AND ${predicate}`;

/** Shared parameterized ticket guard witness for the Support read tail.
 * The source-version assertion and the evidence-stage guard use the exact same
 * ticket identity prefix; each caller passes its own trailing predicate and
 * binds. Rendering stays byte-identical and the native readers remain the
 * authority. */
export const ticketGuardExistsSql = (predicate: string) =>
    `EXISTS(${ticketByIdSql('1', predicate)})`;
