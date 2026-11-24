/** Byte-identical PMS read-tail r36 point-read heads shared across owner-local sources.
 * Text only: every consumer keeps its own WHERE/ORDER/LIMIT, binds, result type
 * and transaction; nothing is cached or executed here. Projection variants come
 * from a closed union owned by this module, never caller SQL. Locking reads,
 * r17-r35 fragments and the native read registry are untouched. */

/** Rank 1: byte-identical property point-read head shared by three sites. */
export const PMS_PROPERTY_POINT_HEAD_SQL = 'SELECT * FROM pms_properties';

type PmsRecordPropertyPointHead = 'exists' | 'ownerFact' | 'guestEvidence';

const HEADS: Record<PmsRecordPropertyPointHead, { cols: string; sep: string }> = {
  exists: { cols: '1', sep: ' ' },
  ownerFact: { cols: 'p.company_id,p.branch_id,NULL::text department_id,r.version,r.kind', sep: '\n       ' },
  guestEvidence: { cols: `r.id,r.property_id,r.data_json,p.name,p.business_date,p.config_json::jsonb->>'currency' currency`, sep: '\n    ' },
};

/** Rank 2: closed-union record/property point-read head. WHERE and binds stay at call sites. */
export const pmsRecordPropertyPointHeadSql = (head: PmsRecordPropertyPointHead): string => {
  const { cols, sep } = HEADS[head];
  return `SELECT ${cols}${sep}FROM pms_records r JOIN pms_properties p ON p.id=r.property_id`;
};
