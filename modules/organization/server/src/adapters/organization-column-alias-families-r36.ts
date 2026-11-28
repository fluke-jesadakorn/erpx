import 'server-only';
/** r36 additions: column/kind-parameterized active-node lookup with a company
 * scope. This family is new; every r17-r35 helper and its call sites are left
 * untouched. Every call renders byte-identical SQL to the literal it replaces;
 * only the owner-supplied projection column and literal kind value vary (never
 * caller input). Unlike r34's activeNodeKindExists this is a standalone SELECT
 * with no EXISTS wrapper; unlike r27's activeNodeByKind it carries company_id
 * and a literal kind. Text only: callers keep their own binds, locks and
 * supplied statements. */
export const activeNodeByIdCompanyKind = (column: string, kind: string) =>
  `SELECT ${column} FROM organization_nodes WHERE id=? AND company_id=? AND kind='${kind}' AND status='active'`;
