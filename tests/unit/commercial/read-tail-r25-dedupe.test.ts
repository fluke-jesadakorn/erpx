import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { coverageProbe, purchaseFactAgg } from '../../../packages/server-commercial/src/adapters/commercial-read-tail-r4-sources';
import { scopedReadHead } from '../../../packages/server-commercial/src/adapters/commercial-read-tail-r14-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-commercial/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');
// Prose comments describe the excluded lock kinds; assertions inspect code only.
const codeOnly = (text: string) => text.replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/[^\n]*/g, '');
const pureRead = (sql: string) => {
  expect(sql.toUpperCase()).toMatch(/^SELECT(?:\s|$)/);
  // Row-lock suffixes are caller-owned tails; the folded head emits none.
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|LOCK\s+TABLE|pg_advisory/i);
  expect(sql).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE/);
};

// Pre-dedupe literals + SHA-256 oracles copied byte-for-byte from HEAD.
const GRANTS = 'SELECT jsonb_agg(to_jsonb(g) ORDER BY package_id) FROM company_package_grants g WHERE company_id=?';
const HISTORY = 'SELECT jsonb_agg(to_jsonb(h) ORDER BY package_id,package_version) FROM company_package_history_rights h WHERE company_id=?';
const PRINCIPAL = 'SELECT since_at FROM billing_principal_status_coverage WHERE singleton=TRUE';
const SEAT = 'SELECT started_at FROM billing_seat_history_coverage WHERE singleton=1';
const FULL_PREPARED = `SELECT
    COALESCE((${GRANTS}),'[]'::jsonb) grants,
    COALESCE((${HISTORY}),'[]'::jsonb) history`;
const ORACLES: Array<[string, string]> = [
  [GRANTS, '7026397bc5d39c463c831c344e0c5bfc458963a98b2960184f3e640431cb413b'],
  [HISTORY, '99d16b760333bb527e40f22ca855438f49d2ed0ac2057a6188d85f817d2faa14'],
  [PRINCIPAL, '5d9e3fbe58d2818b8c58ac173131d0dfd845e427ff48074f368f3f2427004603'],
  [SEAT, 'b330c20ab00b154b7912810687e30aa36c4b2b0406c7bfc03a5c54a9939e556e'],
  [FULL_PREPARED, 'a6aabbec399d1a8874af6585ae3f3b7547da312c68d5a890da9f3eb20533ab04'],
];

describe('Commercial read-tail r25 purchase-fact head delegation', () => {
  test('rendered SQL and binds stay byte-identical to the pre-dedupe literals', () => {
    const grants = purchaseFactAgg('company_package_grants', 'g', 'package_id');
    const history = purchaseFactAgg('company_package_history_rights', 'h', 'package_id,package_version');
    const principal = coverageProbe('since_at', 'billing_principal_status_coverage', 'TRUE');
    const seat = coverageProbe('started_at', 'billing_seat_history_coverage', '1');
    const fullPrepared = `SELECT
    COALESCE((${grants}),'[]'::jsonb) grants,
    COALESCE((${history}),'[]'::jsonb) history`;
    const rendered: Array<[string, string]> = [
      [grants, GRANTS], [history, HISTORY], [principal, PRINCIPAL], [seat, SEAT], [fullPrepared, FULL_PREPARED],
    ];
    for (const [actual, oracle] of rendered) {
      expect(actual, oracle).toBe(oracle);
      pureRead(actual);
    }
    // SHA-256 byte oracles guard the exact rendered bytes, not just equality.
    for (const [literal, digest] of ORACLES) expect(sha(literal), digest).toBe(digest);
    expect(sha(grants)).toBe(ORACLES[0][1]);
    expect(sha(history)).toBe(ORACLES[1][1]);
    expect(sha(fullPrepared)).toBe(ORACLES[4][1]);
    // Two `company_id=?` placeholders; caller binds stay in the same order.
    expect((fullPrepared.match(/company_id=\?/g) ?? []).length).toBe(2);
  });

  test('the closed company head is owned only by the r14 scoped renderer', () => {
    expect(scopedReadHead('jsonb_agg(to_jsonb(g) ORDER BY package_id)', 'company_package_grants g', 'company')).toBe(GRANTS);
    expect(scopedReadHead('jsonb_agg(to_jsonb(h) ORDER BY package_id,package_version)', 'company_package_history_rights h', 'company')).toBe(HISTORY);
    const r4 = src('commercial-read-tail-r4-sources.ts');
    expect(r4).toContain('import { scopedReadHead } from "./commercial-read-tail-r14-sources"');
    expect(r4).toContain("scopedReadHead(`jsonb_agg(to_jsonb(${alias}) ORDER BY ${order})`, `${source} ${alias}`, 'company')");
    expect(r4.match(/SELECT /g) ?? []).toHaveLength(0); // both heads now delegate to owner-local renderers
    const r4Code = codeOnly(r4);
    expect(r4Code).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE/i);
    const head = src('commercial-read-tail-r14-sources.ts');
    expect(head.match(/SELECT /g) ?? []).toHaveLength(0); // keyword is a named constant
    expect(head).toContain('const KW = "SELECT";');
    expect(codeOnly(head)).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  test('no write/lock/native/cross-owner surface was introduced', () => {
    const r4 = src('commercial-read-tail-r4-sources.ts');
    expect(codeOnly(r4)).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|LOCK\s+TABLE|pg_advisory/i);
    // Pre-existing locks stay pinned in their owner-local files.
    const pg = src('package-grants.ts');
    expect(pg).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(pg).toContain('SELECT version FROM company_package_revisions WHERE company_id=? FOR UPDATE');
    const boundary = src('purchase-command-boundary.ts');
    expect(boundary).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(boundary).toContain('SELECT package_id FROM company_package_grants WHERE company_id=? ORDER BY package_id FOR SHARE');
    // The renderer stays commercial-owner-local; no native statement kind added.
    expect(r4).not.toContain('@erpx/server-');
  });

  test('caller-side prepare/bind is unchanged at both purchase-fact sites', () => {
    const pg = src('package-grants.ts');
    expect(pg.match(/purchaseFactAgg\(/g)).toHaveLength(2);
    expect(pg).toContain('.bind(companyId, companyId).first<{ grants: Row[] | string; history: HistoryRow[] | string }>()');
    expect(pg).not.toContain(GRANTS);
    expect(pg).not.toContain(HISTORY);
  });

  test('retirement:ts-statements pins 12 -> 11', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(11);
    expect(total).toBeLessThan(16);
  });
});
