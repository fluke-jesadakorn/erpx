import 'server-only';
/** r27 additions: column/kind-parameterized owner read SQL. These families are
 * new in r27; every r17-r26 helper is left untouched. Every call renders
 * byte-identical SQL to the literal it replaces; only owner-supplied columns,
 * relations and literal kind values vary (never caller input). Text only:
 * callers keep their own binds and supplied statements. */

/** Active node lookup by id and literal kind, projecting an owner column.
 * Family: finance intercompany active company and legacy company brand scope. */
export const activeNodeByKind = (column: string, kind: string) =>
  `SELECT ${column} FROM organization_nodes WHERE id=? AND kind='${kind}' AND status='active'`;

/** Two-column lookup parameterized by projection, relation and predicate order.
 * Family: level_number by id-then-company and roster_required by company-then-id. */
export const twoColumnLookup = (columns: string, table: string, column1: string, column2: string) =>
  `SELECT ${columns} FROM ${table} WHERE ${column1}=? AND ${column2}=?`;

/** Node lookup by id+company and literal kind, projecting owner columns.
 * Family: POS branch id and operations department-head branch_id reads. */
export const nodeKindByIdCompany = (columns: string, kind: string) =>
  `SELECT ${columns} FROM organization_nodes WHERE id=? AND company_id=? AND kind='${kind}'`;

/** Versioned attachment node active guard with a parameterized second predicate
 * column and literal kind. Family: branch_brand and company_storefront_brand
 * attachment owner guards. */
export const versionedNodeActiveGuard = (column: string, kind: string) =>
  `?=1 AND EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND ${column}=? AND kind='${kind}' AND status='active')`;
