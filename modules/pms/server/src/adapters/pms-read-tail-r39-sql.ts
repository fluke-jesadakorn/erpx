/** Byte-identical PMS read-tail r39 query heads shared across owner-local sources.
 * Text only: every consumer keeps its own suffix, binds, result type and
 * transaction; nothing is cached or executed here. Variants come from closed
 * unions owned by this module, never caller SQL. Locking reads, the r17-r38
 * fragments and the native read registry stay untouched. */

type PmsHomeGuestAccountSumKind = 'receivable' | 'advance' | 'deposit';

const GUEST_ACCOUNT_SUM_FIELD: Record<PmsHomeGuestAccountSumKind, 'due' | 'advance' | 'deposit'> = {
  receivable: 'due',
  advance: 'advance',
  deposit: 'deposit',
};

/** Closed union: the three home guest-account money columns share one query head. */
export const pmsHomeGuestAccountSumSql = (kind: PmsHomeGuestAccountSumKind): string =>
  `SELECT sum((r.data_json::jsonb->>'${GUEST_ACCOUNT_SUM_FIELD[kind]}')::bigint) FROM pms_records r WHERE r.property_id=p.id AND r.kind='guest_account'`;

const JOURNAL_IDENTITY_HEAD = 'SELECT p.company_id,d.journal_id id';
const IMPORT_JOURNAL_IDENTITY_HEAD = 'SELECT company_id,opening_journal_id';

/** Closed union: the Finance journal identity and its source-image projection
 * share the same two query heads; the image only adds output columns. */
export const pmsJournalIdentityUnionSql = (kind: 'identity' | 'sourceImage'): string => kind === 'identity'
  ? `${JOURNAL_IDENTITY_HEAD} FROM pms_postings d JOIN pms_properties p ON p.id=d.property_id WHERE d.journal_id IS NOT NULL UNION ${IMPORT_JOURNAL_IDENTITY_HEAD} FROM pms_import_batches WHERE state='applied' AND opening_journal_id IS NOT NULL`
  : `${JOURNAL_IDENTITY_HEAD},jsonb_build_object('property',to_jsonb(p),'posting',to_jsonb(d)) source_image FROM pms_postings d JOIN pms_properties p ON p.id=d.property_id WHERE d.journal_id IS NOT NULL UNION ALL ${IMPORT_JOURNAL_IDENTITY_HEAD},to_jsonb(d) FROM pms_import_batches d WHERE state='applied' AND opening_journal_id IS NOT NULL`;
