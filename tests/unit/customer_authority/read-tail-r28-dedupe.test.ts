import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { releaseGateRead } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r28-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-customer_authority/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Closed sample locals reconstructed verbatim from the pre-dedupe
// portal-release-gate.ts (r27 baseline, BEFORE=58). Only predicate/source/tail
// are parameterized; emitted text is pinned byte-for-byte.
const PRED = "scheduled_feature_enabled('portal_release_gate.company_id','customer_authority')";
const SRC = '(SELECT ?::text company_id) portal_release_gate';
const GATE = `SELECT ${PRED} allowed FROM ${SRC}`;
const ASSERTION = `(SELECT ${PRED} FROM ${SRC})`;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Customer Authority read-tail r28 release-gate read family', () => {
  test('renders byte-identically to both pre-dedupe literals', () => {
    expect(releaseGateRead(PRED, SRC, ' allowed')).toBe(GATE);
    const wrapped = `(${releaseGateRead(PRED, SRC, '')})`;
    expect(wrapped).toBe(ASSERTION);
    readOnly(GATE);
    readOnly(ASSERTION);
  });

  test('one shared literal, no inline copies, and pinned caller arguments', () => {
    const shared = src('customer-authority-read-tail-r28-sources.ts');
    expect(shared.match(/\bSELECT\b/g) ?? []).toHaveLength(0);
    expect(shared).toContain('export const releaseGateRead');
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    const gate = src('portal-release-gate.ts');
    expect(gate).not.toContain(GATE);
    expect(gate).not.toContain(ASSERTION);
    expect(gate.match(/releaseGateRead\(/g) ?? []).toHaveLength(2);
    expect(gate).toContain(`releaseGateRead(predicate(),gateSource,' allowed')`);
    expect(gate).toContain(`releaseGateRead(predicate(),gateSource,'')`);
    // The gate-source company scope now comes from the r33 shared probe; no inline SELECT.
    expect(gate.match(/\bSELECT\b/g) ?? []).toHaveLength(0);
    expect(gate).toContain('`(${companyScopeSelect()}) ${gateAlias}`');
    expect(gate).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  test('r21-r27 builders, source locks and native kinds stay untouched', () => {
    const counts: Array<[string, number, string]> = [
      ['customer-authority-read-sql.ts', 2, 'export const jsonbAggFact'],
      ['package-obligation-sql.ts', 1, 'export const latestSupportVersion'],
      ['customer-authority-read-tail-r23-sources.ts', 1, 'export const jsonbImageProjection'],
      ['customer-authority-read-tail-r24-sources.ts', 1, 'export const bindingFactProjection'],
      ['customer-authority-read-tail-r25-sources.ts', 0, 'export const ownerColumnProjection'],
      ['customer-authority-read-tail-r26-sources.ts', 1, 'export const accessBindingOrderProjection'],
      ['customer-authority-read-tail-r27-sources.ts', 0, 'export const accessEventEvidenceProbe'],
    ];
    for (const [name, count, symbol] of counts) {
      const helper = src(name);
      expect(helper.match(/\bSELECT\b/g) ?? [], name).toHaveLength(count);
      expect(helper, name).toContain(symbol);
    }

    // Both consumers still prepare/bind and the assertion stays a command guard.
    const purchases = src('portal-purchases.ts');
    expect(purchases).toContain('portalReleaseGateSql()');
    expect(purchases).toContain(".first<boolean>('allowed')");
    const attachments = src('attachment-purchases.ts');
    expect(attachments).toContain('assertion(database,key,\'portal-file-release\',portalReleaseAssertionSql()');
    expect(attachments).toContain('portalReleaseGateSql()');
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=58', () => {
    const files: string[] = [];
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) files.push(p);
      }
    };
    walk(srcRoot);
    const total = files.reduce((n, p) => n + (readFileSync(p, 'utf8').match(/SELECT/g) ?? []).length, 0);
    expect(total).toBeLessThan(58);
    // r29 further reduced the tail (57 -> 56); keep a bound, not a stale exact pin.
    expect(total).toBeLessThan(57);
  });
});
