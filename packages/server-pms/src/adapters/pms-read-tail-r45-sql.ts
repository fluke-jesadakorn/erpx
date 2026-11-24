/** Byte-identical PMS read-tail r45 query heads shared across owner-local sources.
 * Text only: every consumer keeps its own suffix, binds, result type and
 * transaction; nothing is cached or executed here. Variants come from closed
 * unions owned by this module, never caller SQL. Locking reads, feature/native
 * reads and the r17-r44 fragments stay untouched. */

/** Rank 1: reservation relation arms. CTEs and fields come from a closed arm
 * union; call sites keep the UNION order and their own binds (zero per arm). */
type RelationArm =
  | 'chosenReservationId'
  | 'reservationIdChosen'
  | 'selectedGuestAccountId'
  | 'chosenId'
  | 'staysId'
  | 'guestAccountsId'
  | 'reservationIdStays'
  | 'guestIdStays'
  | 'guestIdChosen';
const RELATION_ARM: Record<RelationArm, string> = {
  chosenReservationId: "SELECT id FROM chosen WHERE kind='reservation'",
  reservationIdChosen: "SELECT data->>'reservationId' FROM chosen",
  selectedGuestAccountId: "SELECT id FROM selected WHERE kind='guest_account'",
  chosenId: 'SELECT id FROM chosen',
  staysId: 'SELECT id FROM stays',
  guestAccountsId: 'SELECT id FROM guestAccounts',
  reservationIdStays: "SELECT data->>'reservationId' FROM stays",
  guestIdStays: "SELECT data->>'guestId' FROM stays",
  guestIdChosen: "SELECT data->>'guestId' FROM chosen",
};
export const pmsReservationRelationArmSql = (arm: RelationArm): string => RELATION_ARM[arm];

/** Rank 2: chosen-kind point heads. The optional `id` alias and column arity
 * follow the closed kind; sources are chosen/stays/selected only. */
type PointKind = 'guestAccountStayAliased' | 'typeIdStay' | 'rateIdStay' | 'guestAccountSelected';
const POINT_HEAD: Record<PointKind, { field: string; source: string }> = {
  guestAccountStayAliased: { field: 'guestAccountId', source: 'stays' },
  typeIdStay: { field: 'typeId', source: 'stays' },
  rateIdStay: { field: 'rateId', source: 'stays' },
  guestAccountSelected: { field: 'guestAccountId', source: 'selected' },
};
export const pmsChosenKindPointSql = (kind: PointKind): string => {
  const { field, source } = POINT_HEAD[kind];
  return kind === 'guestAccountStayAliased'
    ? `SELECT data->>'${field}' id FROM ${source}`
    : `SELECT data->>'${field}' FROM ${source}`;
};

/** Rank 3: record×property join anchor. Owner heads are closed strings; the
 * two CRM source heads share one template over closed aliases. Projections and
 * branch tails stay at the call site. */
type JoinHead = 'occupancyEvidence' | 'roomCount' | 'sourceIdentity' | 'transferList' | 'crmAttachmentImage' | 'crmGuestSource';
type OwnerJoinHead = Exclude<JoinHead, 'crmAttachmentImage' | 'crmGuestSource'>;
type CrmJoinHead = Exclude<JoinHead, OwnerJoinHead>;
const OWNER_JOIN_HEAD: Record<OwnerJoinHead, string> = {
  occupancyEvidence: "SELECT r.id,r.property_id,p.name property_name,p.config_json::jsonb->>'currency' property_currency,r.data_json::jsonb->>'date' business_date,\n      r.data_json::jsonb->>'state' state,r.data_json::jsonb->'roomMetrics'->>'capacity' capacity,\n      r.data_json::jsonb->'roomMetrics'->>'sold' sold,r.version\n    FROM pms_records r JOIN pms_properties p ON p.id=r.property_id",
  roomCount: 'SELECT p.name label,count(*)::text value FROM pms_records r JOIN pms_properties p ON p.id=r.property_id',
  sourceIdentity: "SELECT r.id,r.property_id,r.kind,r.data_json::jsonb data,r.version,r.updated_at,\n    p.company_id,p.branch_id,p.name property_name,p.business_date\n  FROM pms_records r JOIN pms_properties p ON p.id=r.property_id",
  transferList: "SELECT t.id,t.property_id,p.version property_version,p.config_json::jsonb->'accounts'->>'bank' bank_account,t.guest_account_id,t.state,t.amount_minor,t.currency,t.version,t.reference,t.recorded_by,\n      t.bank_transaction_id,t.entry_id,t.finance_payment_id,t.updated_at\n    FROM pms_transfer_requests t JOIN pms_properties p ON p.id=t.property_id AND p.company_id=t.company_id",
};
const CRM_JOIN_HEAD: Record<CrmJoinHead, { projection: string; record: string; property: string }> = {
  crmAttachmentImage: {
    projection: "jsonb_build_object('record',to_jsonb(record),'property',to_jsonb(property))::text image",
    record: 'record',
    property: 'property',
  },
  crmGuestSource: {
    projection: 'guest.id,property.company_id,property.branch_id,property.id property_id',
    record: 'guest',
    property: 'property',
  },
};
export const pmsRecordPropertyJoinSql = (kind: JoinHead): string => {
  if (kind === 'crmAttachmentImage' || kind === 'crmGuestSource') {
    const { projection, record, property } = CRM_JOIN_HEAD[kind];
    return `SELECT ${projection} FROM pms_records ${record} JOIN pms_properties ${property} ON ${property}.id=${record}.property_id`;
  }
  return OWNER_JOIN_HEAD[kind];
};

/** Anchor-only form for call sites whose projection is rendered locally. */
export const pmsRecordPropertyAnchorSql = (record: 'r', property: 'p'): string =>
  `FROM pms_records ${record} JOIN pms_properties ${property} ON ${property}.id=${record}.property_id`;
