/** Byte-identical PMS read-tail r44 query heads shared across owner-local sources.
 * Text only: every consumer keeps its own CTE aliases, suffix, binds, result type
 * and transaction; nothing is cached or executed here. Variants come from closed
 * unions owned by this module, never caller SQL. Locking reads, feature/native
 * reads and the r17-r43 fragments stay untouched. */

/** Rank 1: chosen-kind id union. CTE aliases stay at the call site; binds are
 * zero and the UNION order is preserved. */
type ChosenKind = 'guest_account' | 'room';
const CHOSEN_KIND_ID_FIELD: Record<ChosenKind, string> = {
  guest_account: 'guestAccountId',
  room: 'roomId',
};
export const pmsChosenKindUnionSql = (kind: ChosenKind): string => {
  const idField = CHOSEN_KIND_ID_FIELD[kind];
  return `SELECT id FROM chosen WHERE kind='${kind}' UNION SELECT data->>'${idField}' FROM stays UNION SELECT data->>'${idField}' FROM chosen`;
};

/** Rank 2: payment identity/source-image union. Identity keeps UNION and the
 * optional document-id alias; the image keeps UNION ALL and the document,
 * transfer and balance extras. */
type PaymentUnionKind = 'identity' | 'sourceImage';
const PAYMENT_UNION: Record<PaymentUnionKind, string> = {
  identity: 'UNION',
  sourceImage: 'UNION ALL',
};
const PAYMENT_TRANSFER_ALIAS: Record<PaymentUnionKind, string> = {
  identity: '',
  sourceImage: ' d',
};
export const pmsPaymentUnionSql = (kind: PaymentUnionKind, aliasDocumentId = kind === 'sourceImage'): string => {
  const document = kind === 'identity'
    ? `p.company_id,d.finance_id${aliasDocumentId ? ' id' : ''}`
    : "p.company_id,d.finance_id id,jsonb_build_object('property',to_jsonb(p),'document',to_jsonb(d)) source_image";
  const transfer = kind === 'identity'
    ? 'company_id,finance_payment_id'
    : 'company_id,finance_payment_id,to_jsonb(d)';
  const balance = kind === 'identity'
    ? 'p.company_id,b.finance_payment_id'
    : "p.company_id,b.finance_payment_id,jsonb_build_object('property',to_jsonb(p),'balance',to_jsonb(b))";
  const union = PAYMENT_UNION[kind];
  return `SELECT ${document} FROM pms_documents d JOIN pms_properties p ON p.id=d.property_id WHERE d.kind IN ('receipt','refund') AND d.finance_id IS NOT NULL ${union} SELECT ${transfer} FROM pms_transfer_requests${PAYMENT_TRANSFER_ALIAS[kind]} WHERE finance_payment_id IS NOT NULL ${union} SELECT ${balance} FROM pms_import_balances b JOIN pms_properties p ON p.id=b.property_id WHERE b.finance_payment_id IS NOT NULL`;
};

/** Rank 3: recursive night-lineage anchor head. The CTE kind is a closed union;
 * business_date and indentation follow the kind, arity matches the r43 step. */
type LineageAnchorKind = 'entry_lineage' | 'lineage';
const NIGHT_LINEAGE_ANCHOR: Record<LineageAnchorKind, { indent: string; columns: string }> = {
  entry_lineage: {
    indent: '        ',
    columns: 'id,property_id,business_date,net_raw,id,source_key,original_id,ARRAY[id]::text[]',
  },
  lineage: {
    indent: '      ',
    columns: 'id,property_id,net_raw,id,source_key,original_id,ARRAY[id]::text[]',
  },
};
export const pmsNightLineageAnchorSql = (kind: LineageAnchorKind): string => {
  const { indent, columns } = NIGHT_LINEAGE_ANCHOR[kind];
  return `${indent}SELECT ${columns} FROM candidate_entries`;
};
