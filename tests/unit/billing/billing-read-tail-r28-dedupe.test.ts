import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import { billingStoredLedgerSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r28-sql';

const dir = new URL('../../../packages/server-billing/src/adapters/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');

/** Verbatim pre-r28 storage-ledger statements. Byte-identity oracle only. */
const SOURCE_BY_ID = `SELECT bytes FROM billing_evidence_storage_ledger WHERE evidence_id=$1 AND holding_id=$2 AND object_key=$3 AND state='stored'`;
const DUP_OBJECTS = `SELECT object_key FROM billing_evidence_storage_ledger WHERE state='stored' GROUP BY object_key HAVING count(*)>1`;
const STORED_TOTALS = `SELECT COALESCE(sum(bytes),0)::bigint stored_bytes,(SELECT count(*)::bigint FROM (SELECT object_key FROM billing_evidence_storage_ledger WHERE state='stored' GROUP BY object_key HAVING count(*)>1) duplicate_objects) AS duplicate_objects FROM billing_evidence_storage_ledger WHERE state='stored'`;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('billing read-tail r28 storage-ledger parameterized family dedupe', () => {
  it('authors the storage-ledger projection once and renders every variant byte-identically', async () => {
    const helper = await src('billing-read-tail-r28-sql.ts');
    expect(helper.split('SELECT').length - 1).toBe(1);
    expect(billingStoredLedgerSql('bytes', "evidence_id=$1 AND holding_id=$2 AND object_key=$3 AND state='stored'")).toBe(SOURCE_BY_ID);
    expect(billingStoredLedgerSql('object_key', "state='stored' GROUP BY object_key HAVING count(*)>1")).toBe(DUP_OBJECTS);
    expect(billingStoredLedgerSql(`COALESCE(sum(bytes),0)::bigint stored_bytes,(SELECT count(*)::bigint FROM (${billingStoredLedgerSql('object_key', "state='stored' GROUP BY object_key HAVING count(*)>1")}) duplicate_objects) AS duplicate_objects`, "state='stored'")).toBe(STORED_TOTALS);
  });

  it('routes both repositories through the shared text with their own binds', async () => {
    const text = await src('billing-attachments.ts');
    expect(text.split("import {billingStoredLedgerSql} from './billing-read-tail-r28-sql';").length - 1).toBe(1);
    expect(text.split('billingStoredLedgerSql(').length - 1).toBe(3); // sourceLedger + storedTotals outer/inner
    expect(text).not.toContain("'SELECT bytes FROM billing_evidence_storage_ledger");
    expect(text).not.toContain('SELECT COALESCE(sum(bytes),0)::bigint stored_bytes,');
    expect(text).toContain('billingStoredLedgerSql(\'bytes\',"evidence_id=$1 AND holding_id=$2 AND object_key=$3 AND state=\'stored\'")');
    expect(text).toContain('[row.id,row.holding_id,row.object_key]');
    // The non-family statements stay untouched: pending-scan prefix, reconciliation
    // point read, source listing and the duplicate-object COUNT wrapper.
    expect(text).toContain('BILLING_PENDING_SCAN_COUNT_SQL');
    expect(text).toContain('SELECT ready,created_at FROM billing_evidence_reconciliation_runs');
    expect(text).toContain('SELECT id,holding_id,object_key,size,sha256,source_state FROM billing_evidence ORDER BY id');
    expect(text).toContain('(SELECT count(*)::bigint FROM (');
  });

  it('keeps the shared helper read-only and leaves prior families untouched', async () => {
    const helper = await src('billing-read-tail-r28-sql.ts');
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    const billing = await src('billing.ts');
    expect(billing).toContain('"SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL FOR UPDATE"');
    expect(billing).toContain('BILLING_ADVISORY_XACT_LOCK_SQL');
    expect(billing).not.toContain('billingMetricCountSql');
    expect(await src('billing-read-tail-r27-sql.ts')).toContain('billingMetricCountSql');
    expect(await src('billing-read-tail-r25-sql.ts')).toContain('BILLING_ADVISORY_XACT_LOCK_SQL');
  });

  it('lowers the billing owner SELECT KPI strictly below the recorded BEFORE=59 baseline', async () => {
    const files = await tsFiles(new URL('../../../packages/server-billing/src/', import.meta.url));
    let count = 0;
    for (const file of files) count += (await readFile(file, 'utf8')).split('SELECT').length - 1;
    expect(count).toBeLessThan(59);
    expect(count).toBe(53); // r30 left 54; r31 deduped the target-month read family.
  });
});
