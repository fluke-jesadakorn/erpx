import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import { PMS_POSTINGS_FROM_SQL } from '../../../packages/server-pms/src/adapters/pms-read-tail-r19-sql';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SHARED = 'adapters/pms-read-tail-r19-sql.ts';
const CALL_SITE = 'adapters/finance-owned-snapshot-sources.ts';
// File-text forms: inner column quotes are backslash-escaped in the TS source.
const IMPORT_BATCHES = String.raw`FROM (SELECT \"id\",\"company_id\",\"property_id\",\"cutover_date\",\"property_version\",\"definition_json\",\"report_json\",\"state\",\"created_by\",\"approved_by\",\"opening_journal_id\",\"version\",\"created_at\",\"updated_at\" FROM pms_import_batches) pms_import_batches`;
const POSTINGS = String.raw`FROM (SELECT \"id\",\"property_id\",\"source_key\",\"record_id\",\"journal_id\",\"business_date\" FROM pms_postings) pms_postings`;
const POSTINGS_RUNTIME = POSTINGS.replaceAll('\\"', '"');

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r19 byte-identical projection-fragment dedupe', () => {
  it('declares each shared SQL text exactly once', async () => {
    const shared = await src(SHARED);
    expect(shared.split(IMPORT_BATCHES).length - 1).toBe(1);
    // r53 folds the byte-identical postings head into the owner-local renderer;
    // the exported fragment keeps its exact HEAD value.
    expect(shared.split("pmsOwnerProjectionHeadSql('postings')").length - 1).toBe(1);
    expect(PMS_POSTINGS_FROM_SQL).toBe(POSTINGS_RUNTIME);
  });

  it('routes every projection source through the shared fragments with no inline literal', async () => {
    const text = await src(CALL_SITE);
    // one import + four call sites for each fragment
    expect(text.split('PMS_IMPORT_BATCHES_FROM_SQL').length - 1).toBe(5);
    expect(text.split('PMS_POSTINGS_FROM_SQL').length - 1).toBe(5);
    expect(text).not.toContain(IMPORT_BATCHES);
    expect(text).not.toContain(POSTINGS);
  });

  it('keeps the fragments read-only and leaves distinct aliases inline', async () => {
    for (const sql of [IMPORT_BATCHES, POSTINGS]) {
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    const text = await src(CALL_SITE);
    // the pp wrapper stays inline; r31 routes the distinct-alias pms_import_batches
    // wrapper through a closed helper with byte-identical generated SQL
    expect(text).toContain('FROM (${PMS_POSTINGS_PROJECTION_SQL}) pp');
    expect(text).toContain("pmsFinanceSnapshotWrapperSql('importBatches','JOIN','b')");
  });

  it('lowers the pms owner SELECT KPI strictly below the r18 after=487 baseline', async () => {
    let count = 0;
    for (const file of await tsFiles(dir)) count += (await readFile(file, 'utf8')).split('SELECT ').length - 1;
    // r20 deduped the 3x pms_properties and 3x pms_records projections below r19 after=481.
    // r21 deduped the 4x parameterized recipient cross-join fragment below r20 after=477.
    // r30 deduped the 3-SELECT parameterized payment-identity union below r29 after=361.
    expect(count).toBe(232);
    expect(count).toBeLessThan(487);
  });
});

describe('pms read-tail r30 parameterized payment-identity union dedupe', () => {
  const UNION = String.raw`FROM pms_documents d JOIN pms_properties p ON p.id=d.property_id WHERE d.kind IN ('receipt','refund') AND d.finance_id IS NOT NULL UNION SELECT company_id,finance_payment_id FROM pms_transfer_requests WHERE finance_payment_id IS NOT NULL UNION SELECT p.company_id,b.finance_payment_id FROM pms_import_balances b JOIN pms_properties p ON p.id=b.property_id WHERE b.finance_payment_id IS NOT NULL`;

  it('declares the union once and routes both callers through the shared function', async () => {
    // r44 folds the identity and source-image unions into one closed builder.
    const shared = await src('adapters/pms-read-tail-r44-sql.ts');
    const fin = await src('adapters/finance-record-sources.ts');
    const pkg = await src('adapters/package-obligations.ts');
    expect(fin.split(UNION).length - 1).toBe(0);
    expect(pkg.split(UNION).length - 1).toBe(0);
    expect(shared).toContain("type PaymentUnionKind = 'identity' | 'sourceImage'");
    expect(shared).toContain('const PAYMENT_UNION');
    expect(fin).toContain('export function pmsPaymentIdentityUnionSql(aliasDocumentId: boolean): string');
    expect(fin).toContain("return pmsPaymentUnionSql('identity', aliasDocumentId);");
    expect(fin).toContain('payment: pmsPaymentIdentityUnionSql(true)');
    expect(pkg).toContain('import { pmsPaymentIdentityUnionSql } from "./finance-record-sources";');
    expect(pkg).toContain('payments: pmsPaymentIdentityUnionSql(false)');
  });

  it('keeps the union read-only with the only variant being the document id alias', async () => {
    const shared = await src('adapters/pms-read-tail-r44-sql.ts');
    expect(UNION).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    expect(shared).toContain("d.finance_id${aliasDocumentId ? ' id' : ''}");
  });
});
