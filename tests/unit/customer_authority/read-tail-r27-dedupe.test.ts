import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { accessEventEvidenceProbe } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r27-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-customer_authority/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Closed locals reconstructed verbatim from the pre-dedupe package-obligations.ts
// (r26 baseline, BEFORE=59). Only table/alias/key/predicate are parameterized;
// emitted text is pinned byte-for-byte.
const REQUEST = "EXISTS(SELECT 1 FROM customer_portal_actions request WHERE event.id='customer-access-event-'||request.request_key AND request.binding_id=binding.id AND request.action='request_support')";
const RECEIPT = "EXISTS(SELECT 1 FROM command_receipts receipt WHERE event.id='customer-access-event-'||receipt.command_id AND receipt.record_id=binding.id AND receipt.operation='customer.access.'||event.action AND receipt.actor_id=event.actor_id AND receipt.authority_json::jsonb->>'companyId'=binding.company_id AND receipt.result_json::jsonb->>'status'=binding.status AND receipt.result_json::jsonb->>'version'=binding.version::text)";

const requestArgs = ['customer_portal_actions', 'request', 'request_key', "request.binding_id=binding.id AND request.action='request_support'"] as const;
const receiptArgs = ['command_receipts', 'receipt', 'command_id', "receipt.record_id=binding.id AND receipt.operation='customer.access.'||event.action AND receipt.actor_id=event.actor_id AND receipt.authority_json::jsonb->>'companyId'=binding.company_id AND receipt.result_json::jsonb->>'status'=binding.status AND receipt.result_json::jsonb->>'version'=binding.version::text"] as const;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Customer Authority read-tail r27 customer-access event evidence probe family', () => {
  test('renders byte-identically to both pre-dedupe literals', () => {
    expect(accessEventEvidenceProbe(...requestArgs)).toBe(REQUEST);
    expect(accessEventEvidenceProbe(...receiptArgs)).toBe(RECEIPT);
    readOnly(REQUEST);
    readOnly(RECEIPT);
  });

  test('one shared literal, no inline copies, and pinned caller arguments', () => {
    const shared = src('customer-authority-read-tail-r27-sources.ts');
    expect(shared.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(shared).toContain('export const accessEventEvidenceProbe');
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    const caller = src('package-obligations.ts');
    expect(caller).not.toContain(REQUEST);
    expect(caller).not.toContain(RECEIPT);
    expect(caller.match(/accessEventEvidenceProbe\(/g) ?? []).toHaveLength(2);
    expect(caller).toContain(`accessEventEvidenceProbe('customer_portal_actions','request','request_key',`);
    expect(caller).toContain(`accessEventEvidenceProbe('command_receipts','receipt','command_id',`);
    // The enclosing binding event chain and every bind stay intact.
    for (const ref of ["event.action='portal-evidence-bind'", "event.action IN ('request','approve','reject','revoke')", 'NOT EXISTS(SELECT 1 FROM customer_access_events event', '.bind(input.companyId)']) {
      expect(caller, ref).toContain(ref);
    }
  });

  test('r21-r26 builders, source locks and native kinds stay untouched', () => {
    const counts: Array<[string, number, string]> = [
      ['customer-authority-read-sql.ts', 2, 'export const jsonbAggFact'],
      ['package-obligation-sql.ts', 3, 'export const latestSupportVersion'],
      ['customer-authority-read-tail-r23-sources.ts', 1, 'export const jsonbImageProjection'],
      ['customer-authority-read-tail-r24-sources.ts', 1, 'export const bindingFactProjection'],
      ['customer-authority-read-tail-r25-sources.ts', 1, 'export const ownerColumnProjection'],
      ['customer-authority-read-tail-r26-sources.ts', 1, 'export const accessBindingOrderProjection'],
    ];
    for (const [name, count, symbol] of counts) {
      const helper = src(name);
      expect(helper.match(/\bSELECT\b/g) ?? [], name).toHaveLength(count);
      expect(helper, name).toContain(symbol);
    }

    // The existing package-obligation source lock and native fact kinds are unchanged.
    const caller = src('package-obligations.ts');
    expect(caller).toContain("LOCK TABLE command_receipts,audit_events,outbox_events IN SHARE MODE");
    for (const kind of ['binding-event', 'identity', 'read', 'portal-invitation-event']) {
      expect(caller, kind).toContain(`bindingFactProjection('${kind}'`);
    }
    expect(caller).toContain('export const portalSupportConclusions');
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=59', () => {
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
    expect(total).toBeLessThan(59);
    // r28 further reduced the tail (58 -> 57); keep a bound, not a stale exact pin.
    expect(total).toBeLessThan(58);
  });
});
