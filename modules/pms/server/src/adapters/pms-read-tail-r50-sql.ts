/** Byte-identical PMS read-tail r50 settlement projection head shared by the
 * two `payments` CTE projections in the addon-package-obligations adapter.
 * Text only: every consumer keeps its own projection tail, FROM/JOIN/WHERE,
 * binds, locks and result type; nothing is cached or executed here. The kind
 * is a closed union owned by this module, never caller SQL. Locking reads,
 * the native registry and the r17-r49 fragments stay untouched. */

type Head = 'expected' | 'receipts';
const KINDS: Record<Head, true> = { expected: true, receipts: true };
const HEAD_SQL = 'SELECT e.package_id,e.company_id,e.property_id,';
export const pmsSettledProjectionHeadSql = (kind: Head): string => {
  if (!KINDS[kind]) throw new Error('PMS_RD_R50_HEAD_REQUIRED');
  return HEAD_SQL;
};
