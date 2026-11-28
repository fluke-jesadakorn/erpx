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

/** r26 additions: further column/table/alias-parameterized owner read SQL. The
 * r17-r25 constants above are untouched. Every call renders byte-identical SQL
 * to the literal it replaces; only owner-supplied columns, relations, aliases
 * and clause order vary (never caller input). Text only: callers keep binds. */

/** Active-node scope check with a parameterized scope column. Family: legal
 * unit kind/branch_id scopes and the collaboration audience guard. */
export const nodeActiveScope = (column: string) =>
  `SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND ${column}=? AND status='active'`;

/** Company-scoped max-version lookup parameterized by relation and id column.
 * Family: position-pay and level-leave version reads. */
export const maxVersionSource = (table: string, idColumn: string) =>
  `SELECT coalesce(max(version),0) version FROM ${table} WHERE company_id=? AND ${idColumn}=?`;

/** Denial witness aggregate clause parameterized by relation and alias.
 * Family: node and position private-file denial facts. */
export const denialAggregateClause = (table: string, alias: string) =>
  `(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id) FROM ${table} ${alias} WHERE ${alias}.company_id=denial_scope.company_id)`;

/** Staff-level number lookup parameterized by the two predicate columns.
 * Family: id-then-company and company-then-id reads. */
export const staffLevelNumberSource = (column1: string, column2: string) =>
  `SELECT level_number FROM staff_levels WHERE ${column1}=? AND ${column2}=? AND active=1`;

/** Two-column projection with parameterized relation and predicate columns.
 * Family: department-head snapshot and node target reads. */
export const snapshotProjection = (columns: string, table: string, column1: string, column2: string) =>
  `SELECT ${columns} FROM ${table} WHERE ${column1}=? AND ${column2}=?`;
