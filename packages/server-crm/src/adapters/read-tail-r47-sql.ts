/** G4 / retire.crm.read-tail-r47 byte-identical read heads.
 *
 * Two owner-local head families share one renderer each: the MATERIALIZED
 * scope-row CTE heads and the owned-source id/company list heads. Callers pass
 * their projection, source, alias and predicate bodies, so rendered SQL,
 * multiline whitespace and bind order stay byte-identical to the retired
 * inline literals. The r42 existence renderer, the r44 UNION ALL renderer,
 * the r46 projection/FROM renderer, locks, command-receipt reads and
 * cross-owner exports are untouched.
 */
import { crmProjectionFromSql } from './read-tail-r46-sql';

/** MATERIALIZED scope-row CTE head. The three owner-local scope facts share
 * this exact MATERIALIZED CTE prefix; callers pass only the row body. */
export const crmMaterializedScopeSql = (body: string) => `AS MATERIALIZED(SELECT ${body})`;

/** Owned-source id/company list head. Both callers pass the projection, owned
 * source, row alias and the full predicate body after WHERE, so the rendered
 * head, ORDER tail and bind order stay byte-identical to the former literals. */
export const crmOwnedListSql = (
  projection: string,
  source: string,
  alias: string,
  predicates: string,
) => crmProjectionFromSql(projection, `(${source}) ${alias} WHERE ${predicates}`);
