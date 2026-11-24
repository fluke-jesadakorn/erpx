/** G4 / retire.crm.read-tail-r34 byte-identical owned-source row head.
 *
 * The opportunity witness, the parameterized source read, the linked-document
 * reference and the workspace-record lookup all project a row from an owned
 * source by an id/company pair. The renderer below reproduces the pre-dedupe
 * inline SQL byte-for-byte; callers pass only their projection, owned source,
 * row alias and predicate tail. Transaction locks, command-receipt reads,
 * native command kinds and the r8-r33 families are untouched.
 */
export const crmOwnedSourceRowSql = (
  projection: string,
  source: string,
  alias: string,
  tail = '',
) => `SELECT ${projection} FROM (${source}) ${alias} WHERE id=? AND company_id=?${tail}`;
