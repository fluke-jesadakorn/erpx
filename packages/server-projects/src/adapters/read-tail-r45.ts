/** G4 / retire.projects.read-tail-r10 closed bare-projection head.
 *
 * The remaining owner-local select-head helpers each begin with the same bare
 * projection prefix before their caller-owned body: the assignment viewer head
 * (r28), the ordered JSONB image aggregate head (r29), the clock projection head
 * (r41) and the evaluation source-identity columns. Each caller keeps its own
 * closed column list, alias, clock expression and trailing SQL; no request SQL
 * enters this helper and no write or lock text is emitted.
 */
export const selectBodySql = (body: string) => `SELECT ${body}`;
