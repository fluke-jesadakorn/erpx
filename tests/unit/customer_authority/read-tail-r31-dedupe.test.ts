import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { supportConclusionIdentity } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r31-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-customer_authority/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Exact pre-dedupe SELECT clauses copied verbatim from HEAD (r30 baseline,
// BEFORE=53). Only the closed row alias and outcome expression are
// parameterized; the caller's FROM/WHERE text and binds are untouched.
const ACK = "SELECT ack.request_id,ack.company_id,ack.id,'acknowledged' outcome";
const CLOSED = 'SELECT closure.request_id,closure.company_id,closure.id,closure.outcome';
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Customer Authority read-tail r31 support-conclusion identity family', () => {
  test('renders byte-identically to both pre-dedupe SELECT clauses', () => {
    expect(supportConclusionIdentity('ack', "'acknowledged' outcome")).toBe(ACK);
    expect(supportConclusionIdentity('closure', 'closure.outcome')).toBe(CLOSED);
    for (const sql of [ACK, CLOSED]) readOnly(sql);
  });

  test('one shared literal, pinned caller arguments and intact control plane', () => {
    const shared = src('customer-authority-read-tail-r31-sources.ts');
    expect(shared.match(/\bSELECT\b/g) ?? []).toHaveLength(0);
    expect(shared).toContain('export const supportConclusionIdentity');
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    const caller = src('package-obligations.ts');
    expect(caller).toContain("supportConclusionIdentity('ack', \"'acknowledged' outcome\")");
    expect(caller).toContain("supportConclusionIdentity('closure','closure.outcome')");
    expect(caller.match(/supportConclusionIdentity\(/g) ?? []).toHaveLength(2);
    expect(caller).not.toContain(ACK);
    expect(caller).not.toContain(CLOSED);
    expect(caller).toContain("database.prepare('LOCK TABLE command_receipts,audit_events,outbox_events IN SHARE MODE').run()");
    for (const kind of ['acknowledged', 'closed_without_customer_acknowledgement', 'portal-invitation', 'response', 'closure']) {
      expect(caller, kind).toContain(kind);
    }
  });

  test('r21-r30 builders, source locks, native kinds and manager helpers stay untouched', () => {
    const counts: Array<[string, number, string]> = [
      ['customer-authority-read-sql.ts', 2, 'export const jsonbAggFact'],
      ['package-obligation-sql.ts', 1, 'export const latestSupportVersion'],
      ['customer-authority-read-tail-r23-sources.ts', 1, 'export const jsonbImageProjection'],
      ['customer-authority-read-tail-r24-sources.ts', 1, 'export const bindingFactProjection'],
      ['customer-authority-read-tail-r25-sources.ts', 0, 'export const ownerColumnProjection'],
      ['customer-authority-read-tail-r26-sources.ts', 1, 'export const accessBindingOrderProjection'],
      ['customer-authority-read-tail-r27-sources.ts', 0, 'export const accessEventEvidenceProbe'],
      ['customer-authority-read-tail-r28-sources.ts', 0, 'export const releaseGateRead'],
      ['customer-authority-read-tail-r29-sources.ts', 1, 'export const reviewEligibilityRows'],
      ['customer-authority-read-tail-r30-sources.ts', 0, 'export const existenceProbe'],
    ];
    for (const [name, count, symbol] of counts) {
      const helper = src(name);
      expect(helper.match(/\bSELECT\b/g) ?? [], name).toHaveLength(count);
      expect(helper, name).toContain(symbol);
      expect(helper, name).not.toContain('supportConclusionIdentity');
    }
    for (const name of ['manager-eligibility.ts', 'assignment-eligibility-source.ts', 'portal-source-locks.ts', 'portal_commerce_locks.rs', 'native-owner-read.ts']) {
      expect(src(name), name).not.toContain('supportConclusionIdentity');
    }
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=53', () => {
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
    expect(total).toBeLessThan(53);
    // r39 further reduced the tail (39 -> 34); keep the current exact pin.
    expect(total).toBe(28);
  });
});
