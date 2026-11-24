import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { bindingsExists, selectOneProbe } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-sql';
import { accessEventEvidenceProbe } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r27-sources';
import { existenceProbe } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r30-sources';
import { factExistenceWitness } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r32-sources';
import { commandAuditProbe, supportAbsent } from '../../../packages/server-customer_authority/src/adapters/package-obligation-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-customer_authority/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Exact pre-dedupe call-site text copied verbatim from HEAD (r38 baseline, BEFORE=39).
// Only closed FROM/WHERE tails are parameterized; emitted SQL is pinned byte-for-byte.
const BINDINGS_PRED = 'b.order_id=o.id AND b.company_id=o.company_id';
const EVENT_FROM = 'customer_portal_actions request';
const EVENT_PRED = "request.binding_id=binding.id AND request.action='request_support'";
const IDENTITY_PRED = 'binding_id=? AND user_id=? AND email=?';
const AUDIT_SOURCE = 'audit_events audit JOIN outbox_events event ON event.command_id=audit.command_id';
const AUDIT_PRED = 'audit.command_id=ack.request_key';
const CLOSURES_PRED = 'closure.request_id=ack.request_id';
const RECEIPT_JOIN = 'audit.record_id=binding.id AND audit.action=receipt.operation';
const RECEIPT_WHERE = "receipt.record_id=binding.id AND receipt.operation='customer.access.request'";

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT 1');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Customer Authority read-tail r39 canonical select-one probe family', () => {
  test('renders byte-identically to every pre-dedupe existence literal', () => {
    const originals = [
      `EXISTS(SELECT 1 FROM bindings b WHERE ${BINDINGS_PRED})`,
      `EXISTS(SELECT 1 FROM ${EVENT_FROM} WHERE event.id='customer-access-event-'||request.request_key AND ${EVENT_PRED})`,
      `SELECT 1 FROM customer_access_identities WHERE ${IDENTITY_PRED}`,
      `EXISTS(SELECT 1 FROM ${AUDIT_SOURCE} WHERE ${AUDIT_PRED})`,
      `NOT EXISTS(SELECT 1 FROM customer_portal_support_closures closure WHERE ${CLOSURES_PRED})`,
      `NOT EXISTS(SELECT 1 FROM command_receipts receipt JOIN audit_events audit ON audit.command_id=receipt.command_id AND ${RECEIPT_JOIN} WHERE ${RECEIPT_WHERE})`,
    ];
    expect(bindingsExists(BINDINGS_PRED)).toBe(originals[0]);
    expect(accessEventEvidenceProbe('customer_portal_actions','request','request_key',EVENT_PRED)).toBe(originals[1]);
    expect(existenceProbe('FROM customer_access_identities',IDENTITY_PRED)).toBe(originals[2]);
    expect(factExistenceWitness(false,AUDIT_SOURCE,AUDIT_PRED)).toBe(originals[3]);
    expect(supportAbsent('customer_portal_support_closures','closure',CLOSURES_PRED)).toBe(originals[4]);
    expect(commandAuditProbe(true,RECEIPT_JOIN,RECEIPT_WHERE)).toBe(originals[5]);
    // Negation polarity is preserved for the two parameterized witnesses.
    expect(factExistenceWitness(true,AUDIT_SOURCE,AUDIT_PRED)).toBe(`NOT EXISTS(SELECT 1 FROM ${AUDIT_SOURCE} WHERE ${AUDIT_PRED})`);
    expect(commandAuditProbe(false,RECEIPT_JOIN,RECEIPT_WHERE)).toBe(`EXISTS(SELECT 1 FROM command_receipts receipt JOIN audit_events audit ON audit.command_id=receipt.command_id AND ${RECEIPT_JOIN} WHERE ${RECEIPT_WHERE})`);
    for (const sql of originals) readOnly(sql);
  });

  test('one canonical probe literal and six thin named wrappers', () => {
    const shared = src('customer-authority-read-sql.ts');
    // jsonbAggFact + selectOneProbe; bindingsExists now delegates.
    expect(shared.match(/\bSELECT\b/g) ?? []).toHaveLength(2);
    expect(shared).toContain('export const selectOneProbe');
    expect(shared).toContain('export const bindingsExists');
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    expect(selectOneProbe('FROM bindings b WHERE 1=1')).toBe('SELECT 1 FROM bindings b WHERE 1=1');

    for (const [name, symbol] of [
      ['customer-authority-read-tail-r27-sources.ts', 'accessEventEvidenceProbe'],
      ['customer-authority-read-tail-r30-sources.ts', 'existenceProbe'],
      ['customer-authority-read-tail-r32-sources.ts', 'factExistenceWitness'],
    ] as Array<[string, string]>) {
      const helper = src(name);
      expect(helper.match(/\bSELECT\b/g) ?? [], name).toHaveLength(0);
      expect(helper, name).toContain(`export const ${symbol}`);
      expect(helper, name).toContain('selectOneProbe(');
      expect(helper, name).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    const obligations = src('package-obligation-sql.ts');
    // latestSupportVersion keeps its distinct max() subselect head, untouched.
    expect(obligations.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(obligations).toContain('export const latestSupportVersion');
  });

  test('locks, source locks, native kinds and eligibility helpers stay untouched', () => {
    const access = src('portal-access.ts');
    expect(access).toContain('FOR UPDATE');
    expect(access).not.toContain('selectOneProbe');

    const locks = src('portal-source-locks.ts');
    expect(locks).toContain('LOCK TABLE');
    expect(locks).not.toContain('selectOneProbe');

    const kinds = src('portal-purchase-binding-source.ts');
    expect(kinds).toContain('o.kind IN (${kinds})');
    expect(kinds).not.toContain('selectOneProbe');

    for (const name of ['assignment-eligibility-source.ts', 'customer-authority-read-tail-r29-sources.ts', 'manager-eligibility.ts']) {
      expect(src(name), name).not.toContain('selectOneProbe');
    }
    const facts = src('package-obligations.ts');
    expect(facts).toContain("database.prepare('LOCK TABLE command_receipts,audit_events,outbox_events IN SHARE MODE').run()");
  });

  test('retirement:ts-statements is strictly below the recorded BEFORE=39', () => {
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
    expect(total).toBeLessThan(39);
    expect(total).toBe(27);
  });
});
