import 'server-only';
/** r34 additions: kind-parameterized active-node EXISTS guard. This family is
 * new; every r17-r33 helper and its call sites are left untouched. Every call
 * renders byte-identical SQL to the literal it replaces; only the owner-supplied
 * literal kind value varies (never caller input). Text only: callers keep their
 * own binds and supplied statements. */
export const activeNodeKindExists = (kind: string) =>
  `EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='${kind}' AND status='active')`;
