import 'server-only';
/** r31 additions: projection-parameterized company-node lookup. This family is
 * new; every r17-r30 helper and its call sites are left untouched. Every call
 * renders byte-identical SQL to the literal it replaces; only the owner-supplied
 * projection column varies (never caller input). Unlike r27's activeNodeByKind
 * there is no status filter here. Text only: callers keep their own binds. */
export const companyNodeByIdKind = (column: string) =>
  `SELECT ${column} FROM organization_nodes WHERE id=? AND kind='company'`;
