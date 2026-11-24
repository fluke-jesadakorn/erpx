import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import {
  crmHotelReservationSourceSql,
  PMS_RECORD_POINT_PREFIX_SQL,
  pmsFinanceSnapshotWrapperSql,
  pmsSetupExistsSql,
} from '../../../packages/server-pms/src/adapters/pms-read-tail-r31-sql';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SHARED = 'adapters/pms-read-tail-r31-sql.ts';
const FINANCE = 'adapters/finance-owned-snapshot-sources.ts';
const REPO = 'adapters/pms/read-repository.ts';
const CRM_ANALYTICS = 'adapters/crm-analytics-sources.ts';
const CRM_PURCHASE = 'adapters/crm-purchase-sources.ts';

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r31 closed-family dedupe', () => {
  it('declares each shared fragment exactly once', async () => {
    const shared = await src(SHARED);
    for (const name of ['pmsFinanceSnapshotWrapperSql', 'pmsSetupExistsSql', 'PMS_RECORD_POINT_PREFIX_SQL', 'crmHotelReservationSourceSql']) {
      expect(shared.split(name).length - 1).toBe(1);
    }
  });

  it('generates byte-identical SQL for every closed variant', () => {
    expect(pmsFinanceSnapshotWrapperSql('documents', 'JOIN', 'd'))
      .toBe('JOIN (SELECT "id","property_id","guest_account_id","kind","finance_id","snapshot_json","created_at" FROM pms_documents) d');
    expect(pmsFinanceSnapshotWrapperSql('properties', 'FROM', 'p'))
      .toBe('FROM (SELECT "id","company_id","branch_id","name","business_date","state","config_json","version","created_by","updated_at" FROM pms_properties) p');
    expect(pmsFinanceSnapshotWrapperSql('importBatches', 'JOIN', 'b'))
      .toBe('JOIN (SELECT "id","company_id","property_id","cutover_date","property_version","definition_json","report_json","state","created_by","approved_by","opening_journal_id","version","created_at","updated_at" FROM pms_import_batches) b');
    expect(pmsFinanceSnapshotWrapperSql('importBalances', 'FROM', 'ib'))
      .toBe('FROM (SELECT "batch_id","source_key","property_id","guest_account_id","entry_id","pocket","amount_minor","finance_payment_id","finance_invoice_id" FROM pms_import_balances) ib');
    expect(pmsFinanceSnapshotWrapperSql('records', 'FROM', 'r'))
      .toBe('FROM (SELECT "id","property_id","kind","data_json","version","updated_at" FROM pms_records) r');
    expect(pmsFinanceSnapshotWrapperSql('documents', 'FROM', 'd'))
      .toBe('FROM (SELECT "id","property_id","guest_account_id","kind","finance_id","snapshot_json","created_at" FROM pms_documents) d');

    expect(pmsSetupExistsSql('room_type')).toBe("EXISTS(SELECT 1 FROM pms_records WHERE property_id=? AND kind='room_type')");
    expect(pmsSetupExistsSql('room')).toBe("EXISTS(SELECT 1 FROM pms_records WHERE property_id=? AND kind='room' AND data_json::jsonb->>'active'='true')");
    expect(pmsSetupExistsSql('rate')).toBe("EXISTS(SELECT 1 FROM pms_records WHERE property_id=? AND kind='rate' AND data_json::jsonb->>'active'='true')");
    expect(pmsSetupExistsSql('stay')).toBe("EXISTS(SELECT 1 FROM pms_records WHERE property_id=? AND kind='stay')");

    expect(PMS_RECORD_POINT_PREFIX_SQL).toBe('SELECT id,kind,data_json::jsonb data FROM pms_records WHERE property_id=? AND ');

    const crmPrefix = "SELECT reservation.id,property.company_id,property.branch_id,property.id property_id,reservation.data_json::jsonb->>'guestId' guest_id";
    expect(crmHotelReservationSourceSql(true)).toBe(`${crmPrefix},reservation.data_json::jsonb->>'state' state FROM pms_records reservation JOIN pms_properties property ON property.id=reservation.property_id`);
    expect(crmHotelReservationSourceSql(false)).toBe(`${crmPrefix} FROM pms_records reservation JOIN pms_properties property ON property.id=reservation.property_id`);
  });

  it('routes every finance snapshot wrapper call site through the shared helper', async () => {
    const text = await src(FINANCE);
    expect(text.split('pmsFinanceSnapshotWrapperSql').length - 1).toBe(7); // import + 6 uses
    for (const literal of [
      '"id","property_id","guest_account_id","kind","finance_id","snapshot_json","created_at" FROM pms_documents) d',
      '"id","company_id","branch_id","name","business_date","state","config_json","version","created_by","updated_at" FROM pms_properties) p',
      '"batch_id","source_key","property_id","guest_account_id","entry_id","pocket","amount_minor","finance_payment_id","finance_invoice_id" FROM pms_import_balances) ib',
      '"id","property_id","kind","data_json","version","updated_at" FROM pms_records) r',
      'opening_journal_id","version","created_at","updated_at" FROM pms_import_batches) b',
    ]) expect(text).not.toContain(literal);
  });

  it('routes setup/eligibility probes and point projections through shared SQL', async () => {
    const text = await src(REPO);
    expect(text.split('pmsSetupExistsSql').length - 1).toBe(5); // import + 4 uses
    expect(text.split('PMS_RECORD_POINT_PREFIX_SQL').length - 1).toBe(3); // import + 2 uses
    expect(text).not.toContain("EXISTS(SELECT 1 FROM pms_records WHERE property_id=? AND kind='room_type')");
    expect(text).not.toContain("EXISTS(SELECT 1 FROM pms_records WHERE property_id=? AND kind='stay')");
    expect(text).not.toContain('SELECT id,kind,data_json::jsonb data FROM pms_records WHERE property_id=? AND');
  });

  it('routes the CRM reservation source through the shared projection', async () => {
    const analytics = await src(CRM_ANALYTICS), purchase = await src(CRM_PURCHASE);
    expect(analytics.split('crmHotelReservationSourceSql').length - 1).toBe(2); // import + use
    expect(purchase.split('crmHotelReservationSourceSql').length - 1).toBe(2); // import + use
    for (const text of [analytics, purchase]) {
      expect(text).not.toContain("SELECT reservation.id,property.company_id,property.branch_id,property.id property_id,reservation.data_json::jsonb->>'guestId' guest_id FROM");
    }
  });

  it('keeps the fragment read-only, leaves locks inline and does not re-dedupe r17-r30', async () => {
    expect(await src(SHARED)).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|FOR\s+KEY\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    expect(await src('adapters/pms-stripe.ts')).toContain('FOR UPDATE');
    expect(await src('adapters/feature-availability.ts')).toContain('FOR SHARE');
    expect(await src('adapters/property-purchase-boundary.ts')).toContain('FOR KEY SHARE');
    const prior = [
      ['adapters/pms-read-tail-r17-sql.ts', 'PMS_WRITE_CONTEXT_SQL'],
      ['adapters/pms-read-tail-r18-sql.ts', 'PMS_BOOKING_PAYMENT_BY_ID_SQL'],
      ['adapters/pms-read-tail-r19-sql.ts', 'PMS_IMPORT_BATCHES_FROM_SQL'],
      ['adapters/pms-read-tail-r20-sql.ts', 'PMS_PROPERTIES_PROJECTION_SQL'],
      ['adapters/pms-read-tail-r21-sql.ts', 'PMS_RECIPIENT_CROSS_JOIN_SQL'],
      ['adapters/pms-read-tail-r22-sql.ts', 'PMS_STRIPE_REFUND_COLUMNS_SQL'],
      ['adapters/pms-read-tail-r23-sql.ts', 'PMS_SCOPED_RECORDS_SQL'],
      ['adapters/pms-read-tail-r24-sql.ts', 'PMS_RECORDS_PROJECTION_PREFIX_SQL'],
      ['adapters/pms-read-tail-r25-sql.ts', 'PMS_GUEST_STAYS_CHOSEN_MEMBER_SQL'],
      ['adapters/pms-read-tail-r26-sql.ts', 'PMS_CHOSEN_RECORDS_SQL'],
      ['adapters/pms-read-tail-r27-sql.ts', 'PMS_PURCHASE_CLOCK_CTE_SQL'],
      ['adapters/pms-read-tail-r29-sql.ts', 'PMS_POSTINGS_PROJECTION_SQL'],
    ];
    for (const [file, constant] of prior) expect((await src(file)).split(constant).length - 1).toBe(1);
    expect((await src('adapters/finance-record-sources.ts')).split('pmsPaymentIdentityUnionSql').length - 1).toBe(2); // decl + 1 use
  });

  it('lowers the pms owner SELECT KPI strictly below the r30 after=358 baseline', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(313);
    expect(bare).toBe(315);
    expect(spaced).toBeLessThan(358);
    expect(bare).toBeLessThan(360);
  });
});
