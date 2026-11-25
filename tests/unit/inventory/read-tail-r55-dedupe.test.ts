import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { commandReceiptRecordIdGuardSql } from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const src = (name: string) => readFileSync(resolve(srcRoot, name), 'utf8');
const adapter = (name: string) => src(`adapters/${name}`);
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;
const sha = (text: string) => createHash('sha256').update(text).digest('hex');

// Verbatim byte oracles captured from the pre-r55 source. They are identity
// checks only; the canonical renderer owns the head text and every caller keeps
// its own guard/ORDER tail, binds and independent statement.
const HEAD = 'SELECT 1 FROM command_receipts receipt WHERE receipt.record_id=';
const HEAD_SHA = 'b3256f420d134727b4141026d080474166c6204ab568aa4bffd32567ddba0b01';
const IDS = ['hold.id', 't.id'] as const;
// SHA of each full head (literal head + record id) as it appeared before the fold.
const HEAD_SHAS: Record<string, string> = {
  'hold.id': '6de491e2125b375501e70bc17d31c05d0db6818df260e993f36fa1e982010009',
  't.id': '4866b05a75a6f17d075dbb3a3a020ebbe3c75e85bd8e0c04457ad09e5c8c52b5',
};
// SHA of the 160-char guard tail that followed each literal head in the pre-r55
// parent blob. Pinning the tail keeps the oracle on caller-owned text rather
// than on the shared fold.
const TAIL_SHAS: Record<string, string> = {
  'hold.id': 'e8f0a2cbad89af2ca56da70e2aa2152f1687fe073076aedc3d2074775b200ec6',
  't.id': '7494f07ec6dae78170617c202e817dbab1e804e79577e05a5859bf240eae5a67',
};
const CASES: [string, string][] = [
  ['sales-package-obligations.ts', 'hold.id'],
  ['supply-package-obligations.ts', 't.id'],
];

describe('Inventory read-tail r55 command-receipt guard dedupe', () => {
  test('f1: the parameterized renderer reproduces both heads byte-identically', () => {
    expect(sha(HEAD)).toBe(HEAD_SHA);
    for (const id of IDS) {
      const rendered = commandReceiptRecordIdGuardSql(id);
      expect(rendered, id).toBe(HEAD + id);
      expect(sha(rendered), id).toBe(HEAD_SHAS[id]);
      expect(selectCount(rendered), id).toBe(1);
      expect(rendered, id).not.toContain('?');
      expect(rendered, id).not.toMatch(lockRe);
    }
  });

  test('f2: one owner-local helper owns the head and both former sites delegate', () => {
    const shared = adapter('read-tail-sql.ts');
    expect(shared.split('export const commandReceiptRecordIdGuardSql').length - 1).toBe(1);
    for (const [file, id] of CASES) {
      const text = adapter(file);
      expect(text.split(`\${commandReceiptRecordIdGuardSql('${id}')}`).length - 1, id).toBe(1);
      expect(text, id).not.toContain(HEAD + id);
    }
    expect(adapter('sales-package-obligations.ts')).toContain('commandReceiptRecordIdGuardSql,disposalCountsSql');
    expect(adapter('supply-package-obligations.ts')).toContain('commandReceiptRecordIdGuardSql,packageObligationStockHeadSql');
  });

  test('f3: guard tails and bind order stay byte-identical in their callers', () => {
    for (const [file, id] of CASES) {
      const text = adapter(file);
      const marker = `\${commandReceiptRecordIdGuardSql('${id}')}`;
      const at = text.indexOf(marker);
      expect(at, id).toBeGreaterThanOrEqual(0);
      const tail = text.slice(at + marker.length, at + marker.length + 160);
      expect(sha(tail), `tail:${id}`).toBe(TAIL_SHAS[id]);
    }
    expect(adapter('sales-package-obligations.ts')).toContain("receipt.result_json::jsonb->>'version'=hold.version::text))");
    expect(adapter('supply-package-obligations.ts')).toContain("receipt.result_json::jsonb->>'version'=t.version::text)");
    expect(adapter('read-tail-sql.ts')).not.toMatch(lockRe);
  });

  test('f4: locks, native kinds and r45-r54 helpers stay untouched', () => {
    const shared = adapter('read-tail-sql.ts');
    for (const helper of [
      'transitEventQuantitySumSql', 'schemaReadySql', 'inList', 'reservationsCteHeadSql',
      'markedCountExistsSql', 'replayProbePrefixSql', 'countSelectHeadSql',
      'initialPhysicalCountJournalSql', 'evidencePositionRowsSql', 'executivePositionPointSql',
      'countEvidenceRowHeadSql', 'sourceImagesHeadSql', 'lotBalanceRowsHeadSql',
      'countAuthorityScopeSql', 'packageObligationStockHeadSql',
    ]) expect(shared, helper).toContain(`export const ${helper}`);
    // The FOR SHARE lock literals are intentionally not folded.
    expect(adapter('count-attachment-purchases.ts')).toContain('SELECT id FROM assignments WHERE id=? AND company_id=? FOR SHARE');
    expect(adapter('transfer-attachment-purchases.ts')).toContain('SELECT id FROM assignments WHERE id=? AND company_id=? FOR SHARE');
    expect(src('adapters/native_command_sql.rs').length).toBeGreaterThan(0);
    expect(src('adapters/native-inventory-read.ts').length).toBeGreaterThan(0);
    expect(adapter('sales-package-obligations.ts')).toContain('LOCK TABLE');
    expect(adapter('supply-package-obligations.ts')).toContain('LOCK TABLE');
  });

  test('retirement:ts-statements KPI is 297 after the r55 command-receipt guard fold (strictly below r54 298)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r54 ended at 298. r55 folds the byte-identical command-receipt record-id
    // guard head shared by the sales quality-hold and supply transit-cancel
    // reads into one kind-parameterized owner-local renderer: 2 source tokens
    // -> 1, i.e. 298 -> 297.
    expect(total).toBe(292);
    expect(total).toBeLessThan(297);
  });
});
