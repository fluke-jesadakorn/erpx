import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { ownerColumnProjection } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r25-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-customer_authority/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Exact pre-dedupe literals copied verbatim from HEAD (r24 baseline, BEFORE=61).
const BINDINGS_COLS = `"id","company_id","order_id","customer_id","email","expires_at","status","reason","created_by","approved_by","reviewed_at","version","created_at","updated_at"`;
const IDENTITIES_COLS = `"binding_id","user_id","email","bound_at"`;
const BINDINGS = `SELECT ${BINDINGS_COLS} FROM customer_access_bindings`;
const IDENTITIES = `SELECT ${IDENTITIES_COLS} FROM customer_access_identities`;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Customer Authority read-tail r25 column/table projection family', () => {
  test('renders byte-identically to both pre-dedupe literals', () => {
    expect(ownerColumnProjection(BINDINGS_COLS, 'customer_access_bindings')).toBe(BINDINGS);
    expect(ownerColumnProjection(IDENTITIES_COLS, 'customer_access_identities')).toBe(IDENTITIES);
    readOnly(BINDINGS);
    readOnly(IDENTITIES);
  });

  test('one shared literal, no inline copies, and pinned caller arguments', () => {
    const shared = src('customer-authority-read-tail-r25-sources.ts');
    expect(shared.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(shared).toContain('export const ownerColumnProjection');
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    const caller = src('last-audit-owner-projection-sources.ts');
    expect(caller).not.toContain(BINDINGS);
    expect(caller).not.toContain(IDENTITIES);
    expect(caller.match(/ownerColumnProjection\(/g) ?? []).toHaveLength(2);
    expect(caller).toContain(`ownerColumnProjection('${BINDINGS_COLS}', 'customer_access_bindings')`);
    expect(caller).toContain(`ownerColumnProjection('${IDENTITIES_COLS}', 'customer_access_identities')`);
    expect(caller.match(/\bSELECT\b/g) ?? []).toHaveLength(0);
    expect(caller).not.toMatch(/FOR\s+UPDATE|LOCK\s+TABLE|pg_advisory/i);
  });

  test('r21-r24 builders, source locks and native kinds stay untouched', () => {
    const r21 = src('customer-authority-read-sql.ts');
    expect(r21.match(/\bSELECT\b/g) ?? []).toHaveLength(2);
    expect(r21).toContain('export const jsonbAggFact');
    expect(r21).toContain('export const bindingsExists');

    const r22 = src('package-obligation-sql.ts');
    expect(r22.match(/\bSELECT\b/g) ?? []).toHaveLength(3);
    expect(r22).toContain('export const latestSupportVersion');
    expect(r22).toContain('export const supportAbsent');
    expect(r22).toContain('export const commandAuditProbe');

    const r23 = src('customer-authority-read-tail-r23-sources.ts');
    expect(r23.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(r23).toContain('export const jsonbImageProjection');

    const r24 = src('customer-authority-read-tail-r24-sources.ts');
    expect(r24.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(r24).toContain('export const bindingFactProjection');

    const r26 = src('customer-authority-read-tail-r26-sources.ts');
    expect(r26.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(r26).toContain('export const accessBindingOrderProjection');

    // Both closed projection keys are preserved; no fact kind or lock changed.
    const caller = src('last-audit-owner-projection-sources.ts');
    expect(caller).toContain('customer_access_bindings: ownerColumnProjection(');
    expect(caller).toContain('customer_access_identities: ownerColumnProjection(');
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=61', () => {
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
    expect(total).toBeLessThan(61);
    // r26 further reduced the tail (60 -> 59); keep a bound, not a stale exact pin.
    expect(total).toBeLessThan(60);
  });
});
