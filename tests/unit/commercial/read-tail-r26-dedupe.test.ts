import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { coverageProbe } from '../../../packages/server-commercial/src/adapters/commercial-read-tail-r4-sources';
import { singletonProbe } from '../../../packages/server-commercial/src/adapters/commercial-read-tail-r26-sources';

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
const PRINCIPAL = 'SELECT since_at FROM billing_principal_status_coverage WHERE singleton=TRUE';
const SEAT = 'SELECT started_at FROM billing_seat_history_coverage WHERE singleton=1';
const ORACLES: Array<[string, string]> = [
  [PRINCIPAL, '5d9e3fbe58d2818b8c58ac173131d0dfd845e427ff48074f368f3f2427004603'],
  [SEAT, 'b330c20ab00b154b7912810687e30aa36c4b2b0406c7bfc03a5c54a9939e556e'],
];

describe('Commercial read-tail r26 singleton head delegation', () => {
  test('both rendered probes stay byte-identical to the pre-dedupe oracles', () => {
    const principal = coverageProbe('since_at', 'billing_principal_status_coverage', 'TRUE');
    const seat = coverageProbe('started_at', 'billing_seat_history_coverage', '1');
    const rendered: Array<[string, string]> = [[principal, PRINCIPAL], [seat, SEAT]];
    for (const [actual, oracle] of rendered) {
      expect(actual, oracle).toBe(oracle);
      pureRead(actual);
    }
    // SHA-256 byte oracles guard the exact rendered bytes, not just equality.
    for (const [literal, digest] of ORACLES) expect(sha(literal), digest).toBe(digest);
    expect(sha(principal)).toBe(ORACLES[0][1]);
    expect(sha(seat)).toBe(ORACLES[1][1]);
    // The renderer itself renders both originals byte-identically.
    expect(singletonProbe('since_at', 'billing_principal_status_coverage', 'TRUE')).toBe(PRINCIPAL);
    expect(singletonProbe('started_at', 'billing_seat_history_coverage', '1')).toBe(SEAT);
  });

  test('the byte-identical head is owned once by the owner-local r26 renderer', () => {
    const r4 = src('commercial-read-tail-r4-sources.ts');
    const r26 = src('commercial-read-tail-r26-sources.ts');
    expect(r4).toContain('import { singletonProbe } from "./commercial-read-tail-r26-sources"');
    expect(r4).toContain('singletonProbe(column, source, singleton)');
    expect(r4.match(/SELECT /g) ?? []).toHaveLength(0); // no local keyword literal remains
    expect(r26.match(/SELECT /g) ?? []).toHaveLength(0); // keyword is a named constant
    expect(r26).toContain('const KW = "SELECT";');
    expect((r26.match(/WHERE singleton=/g) ?? []).length).toBe(1); // one byte-identical head
    expect(codeOnly(r26)).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  test('no write/lock/native/cross-owner surface was introduced', () => {
    const r26 = src('commercial-read-tail-r26-sources.ts');
    expect(r26).not.toContain('@erpx/server-');
    // Pre-existing locks stay pinned in their owner-local files.
    const pg = src('package-grants.ts');
    expect(pg).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(pg).toContain('SELECT version FROM company_package_revisions WHERE company_id=? FOR UPDATE');
    const boundary = src('purchase-command-boundary.ts');
    expect(boundary).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(boundary).toContain('SELECT package_id FROM company_package_grants WHERE company_id=? ORDER BY package_id FOR SHARE');
    expect(src('../public/native_read.rs')).toContain('caller: "commercial-read-tail-r5-sources.receiptProbe"');
  });

  test('caller-side prepare/bind is unchanged at both singleton probe sites', () => {
    const pj = src('projection.ts');
    const st = src('seats.ts');
    expect(pj).toContain('coverageProbe("since_at","billing_principal_status_coverage","TRUE")');
    expect(pj).toContain('.first<{since_at:number}>()');
    expect(st).toContain('coverageProbe("started_at","billing_seat_history_coverage","1")');
    expect(st).toContain('.first<{started_at:number}>()');
    expect(pj).not.toContain(PRINCIPAL);
    expect(st).not.toContain(SEAT);
  });

  test('retirement:ts-statements pins 14 -> 13', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(13);
    expect(total).toBeLessThan(15);
  });
});
