import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  SALES_RD_CLOCK_NOW_MS,
  SALES_RD_FOLLOWUP_EVIDENCE_PREFIX,
  SALES_RD_FOLLOWUP_METRIC_PREFIX,
} from '../../../packages/server-sales/src/adapters/native-sales-read';

const srcRoot = resolve(__dirname, '../../../packages/server-sales/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const registry = read('native_sales_read_sql.rs');
const shared = read('followup-source-sql.ts');
const followups = read('followups.ts');
const purchases = read('followup-purchases.ts');
const evidence = read('executive-followup-evidence-sources.ts');

function rustSql(constant: string): string {
  const marker = `const ${constant}: &str = "`;
  const at = registry.indexOf(marker);
  expect(at).toBeGreaterThanOrEqual(0);
  const end = registry.indexOf('";', at + marker.length);
  expect(end).toBeGreaterThan(at);
  return registry.slice(at + marker.length, end).replace(/\\"/g, '"');
}

const families = [
  ['clock', 'SQL_CLOCK_NOW_MS', SALES_RD_CLOCK_NOW_MS, 71, 'c73656211aef39055a3da29f7d5e9f76d2aac05eb690b205fad82a6759ba60d9'],
  ['evidence', 'SQL_FOLLOWUP_EVIDENCE_PREFIX', SALES_RD_FOLLOWUP_EVIDENCE_PREFIX, 94, '925fd363cdbc62dccc93cc03999ff598f6eb202f1ee26de719b9bb3a00a7ae59'],
  ['metric', 'SQL_FOLLOWUP_METRIC_PREFIX', SALES_RD_FOLLOWUP_METRIC_PREFIX, 78, '6b7e54fef4405dfbf9ac40effae086a674e26d5be9bc2845bda473a26e4696a1'],
] as const;

describe('native Sales SQL fragments', () => {
  test.each(families)('%s fragment is byte-identical and allow-listed', (_name, constant, kind, length, hash) => {
    const sql = rustSql(constant);
    expect(sql).toHaveLength(length);
    expect(createHash('sha256').update(sql).digest('hex')).toBe(hash);
    expect((sql.match(/\?/g) ?? []).length).toBe(0);
    expect(registry).toContain(`"${kind}"`);
    expect(registry).toContain('bind_count: 0,');
    for (const word of ['INSERT', 'UPDATE ', 'DELETE', 'LOCK TABLE']) expect(sql.toUpperCase()).not.toContain(word);
  });

  test('TS composers use the planned fragments with no SQL mirror', () => {
    expect(shared).not.toContain('export const clockNowMs=');
    expect(shared).not.toContain('export const followupEvidencePrefix=');
    expect(shared).not.toContain('export const followupMetricPrefix=');
    expect(followups).toContain('nativeSalesPlan(SALES_RD_FOLLOWUP_EVIDENCE_PREFIX,[])');
    expect(evidence).toContain('nativeSalesPlan(SALES_RD_FOLLOWUP_EVIDENCE_PREFIX,[])');
    expect(purchases).toContain('nativeSalesPlan(SALES_RD_FOLLOWUP_METRIC_PREFIX,[])');
    expect(followups).toContain('${evidencePrefix}');
    expect(evidence).toContain('${evidencePrefix}');
    expect(purchases).toContain('${(await nativeSalesPlan(SALES_RD_FOLLOWUP_METRIC_PREFIX,[])).sql}');
  });
});
