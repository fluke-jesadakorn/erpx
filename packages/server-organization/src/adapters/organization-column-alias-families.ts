import 'server-only';
/** Column/alias-parameterized owner read SQL shared by the directory and roster
 * fact readers. Every call renders byte-identical SQL to the literal it replaces;
 * only owner-supplied column expressions, table aliases, CTE names and join
 * predicates vary (never caller input). Text only: callers keep their own binds
 * and supplied statements. r17-r24 constants are untouched. */

/** Company-scope lookup projecting <column> as "companyId" from <tableAlias>.
 * Family: the four directory companyId projections (organization_nodes by id,
 * organization_positions/staff_levels/organization_nodes by company_id). */
export const companyIdProjection = (column: string, tableAlias: string) =>
  `SELECT ${column} AS "companyId" FROM ${tableAlias}`;

/** Recursive organization_nodes tree CTE. Family: the roster subtree (walk
 * children) and ancestors (walk parents) CTEs; column lists, CTE name, parent
 * alias and join predicate are all owner-supplied literals. */
export const nodeTreeCte = (
  cte: string,
  anchorColumns: string,
  unionColumns: string,
  parentAlias: string,
  joinOn: string,
) =>
  `WITH RECURSIVE ${cte} AS (\n    SELECT ${anchorColumns} FROM organization_nodes WHERE id=? AND company_id=?\n    UNION ALL SELECT ${unionColumns} FROM organization_nodes n JOIN ${cte} ${parentAlias} ON ${joinOn} WHERE n.company_id=? AND n.status='active')`;
