import 'server-only';
/** r35 additions: relation/column-list-parameterized bare owner projection.
 * This family is new; every r17-r34 helper and its call sites are left untouched.
 * Every call renders byte-identical SQL to the literal it replaces; only the
 * owner-supplied quoted column list and relation vary (never caller input).
 * Text only: callers keep their own binds, locks and supplied statements. */
export const ownerProjection = (columns: string, relation: string) =>
  `SELECT ${columns} FROM ${relation}`;
