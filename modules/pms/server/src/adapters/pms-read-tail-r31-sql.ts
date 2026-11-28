/** Byte-identical and closed-variant PMS read-tail r31 SQL shared across owner-local sources.
 * Text only: every consumer keeps its own guards, binds, result type and transaction;
 * nothing is cached or executed here. Wrapper keywords, aliases, kinds and column
 * flags are closed literal unions owned by this module, never caller SQL.
 * Locking reads, r17-r30 fragments and the native day-report registry are untouched. */

import { PMS_CHOSEN_RECORDS_SQL } from './pms-read-tail-r26-sql';

type Snapshot = 'documents' | 'properties' | 'importBatches' | 'importBalances' | 'records';
const CLOSED: Record<Snapshot, { columns: string; table: string }> = {
  documents: { columns: '"id","property_id","guest_account_id","kind","finance_id","snapshot_json","created_at"', table: 'pms_documents' },
  properties: { columns: '"id","company_id","branch_id","name","business_date","state","config_json","version","created_by","updated_at"', table: 'pms_properties' },
  importBatches: { columns: '"id","company_id","property_id","cutover_date","property_version","definition_json","report_json","state","created_by","approved_by","opening_journal_id","version","created_at","updated_at"', table: 'pms_import_batches' },
  importBalances: { columns: '"batch_id","source_key","property_id","guest_account_id","entry_id","pocket","amount_minor","finance_payment_id","finance_invoice_id"', table: 'pms_import_balances' },
  records: { columns: '"id","property_id","kind","data_json","version","updated_at"', table: 'pms_records' },
};

/** Closed Finance snapshot projection wrapper. The snapshot, wrapper keyword and
 * alias are closed unions; the owned column list and table are never caller supplied. */
export const pmsFinanceSnapshotWrapperSql = (
  snapshot: Snapshot,
  wrapper: 'FROM' | 'JOIN',
  alias: 'd' | 'p' | 'b' | 'ib' | 'r',
): string => `${wrapper} (SELECT ${CLOSED[snapshot].columns} FROM ${CLOSED[snapshot].table}) ${alias}`;

/** Closed setup/eligibility existence probe. The kind and its active flag are
 * closed unions; every probe keeps its own bind and alias at the call site. */
export const pmsSetupExistsSql = (kind: 'room_type' | 'room' | 'rate' | 'stay'): string =>
  `EXISTS(SELECT 1 FROM pms_records WHERE property_id=? AND kind='${kind}'${kind === 'room' || kind === 'rate' ? " AND data_json::jsonb->>'active'='true'" : ''})`;

/** Byte-exact point-projection prefix derived from the r26 chosen-records projection,
 * so the point readers add no second copy of the projection text. */
export const PMS_RECORD_POINT_PREFIX_SQL = PMS_CHOSEN_RECORDS_SQL.slice(0, PMS_CHOSEN_RECORDS_SQL.indexOf('id IN '));

/** Closed CRM hotel reservation source projection. The optional state column is a
 * closed flag, not caller SQL; the trailing JOIN/WHERE stays call-site owned. */
export const crmHotelReservationSourceSql = (withState: boolean): string =>
  `SELECT reservation.id,property.company_id,property.branch_id,property.id property_id,reservation.data_json::jsonb->>'guestId' guest_id${withState ? ",reservation.data_json::jsonb->>'state' state" : ''} FROM pms_records reservation JOIN pms_properties property ON property.id=reservation.property_id`;
