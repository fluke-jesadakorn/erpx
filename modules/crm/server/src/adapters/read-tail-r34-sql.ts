import { crmOwnedListSql } from './read-tail-r47-sql';

/** G4 / retire.crm.read-tail-r34 byte-identical owned-source row head.
 *
 * The opportunity witness, the parameterized source read, the linked-document
 * reference and the workspace-record lookup all project a row from an owned
 * source by an id/company pair. The renderer below reproduces the pre-dedupe
 * inline SQL byte-for-byte; callers pass only their projection, owned source,
 * row alias and predicate tail. Transaction locks, command-receipt reads,
 * native command kinds and the r8-r33 families are untouched. The r47
 * owned-source list renderer owns the shared SELECT/FROM/WHERE head.
 */
export const crmOwnedSourceRowSql = (
  projection: string,
  source: string,
  alias: string,
  tail = '',
) => crmOwnedListSql(projection, source, alias, `id=? AND company_id=?${tail}`);
