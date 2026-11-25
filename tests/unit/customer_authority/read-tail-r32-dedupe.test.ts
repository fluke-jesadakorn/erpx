import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { factExistenceWitness } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r32-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-customer_authority/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Exact pre-dedupe fact-witness clauses copied verbatim from the r31 baseline
// (BEFORE=52). Only the closed polarity, source clause and predicate are
// parameterized; the caller's prepared statement, binds and source leases stay
// untouched. The nested access-event probes are rendered by their unchanged
// r27 helper, so their bytes are preserved exactly.
const ACK = "EXISTS(SELECT 1 FROM audit_events audit JOIN outbox_events event ON event.command_id=audit.command_id AND event.event_type=audit.action AND event.aggregate_id=audit.record_id AND event.aggregate_version=audit.record_version WHERE audit.command_id=ack.request_key AND audit.actor_id=ack.user_id AND audit.assignment_id='external-customer' AND audit.action='customer.portal.support.acknowledge' AND audit.record_id=ack.request_id AND audit.record_version=ack.response_version AND audit.authority_json::jsonb->>'bindingId'=ack.binding_id AND event.payload_json::jsonb->>'responseId'=ack.response_id AND event.payload_json::jsonb->>'bindingId'=ack.binding_id AND event.payload_json::jsonb->>'userId'=ack.user_id)";
const BINDING = "NOT EXISTS(SELECT 1 FROM customer_access_events event WHERE event.binding_id=binding.id AND event.version=binding.version AND (event.action='portal-evidence-bind' AND EXISTS(SELECT 1 FROM customer_portal_actions request WHERE event.id='customer-access-event-'||request.request_key AND request.binding_id=binding.id AND request.action='request_support') OR event.action IN ('request','approve','reject','revoke') AND EXISTS(SELECT 1 FROM command_receipts receipt WHERE event.id='customer-access-event-'||receipt.command_id AND receipt.record_id=binding.id AND receipt.operation='customer.access.'||event.action AND receipt.actor_id=event.actor_id AND receipt.authority_json::jsonb->>'companyId'=binding.company_id AND receipt.result_json::jsonb->>'status'=binding.status AND receipt.result_json::jsonb->>'version'=binding.version::text)))";
const INVITATION = "EXISTS(SELECT 1 FROM customer_portal_invitation_events issued WHERE issued.id='customer-portal-invite-event-'||receipt.command_id AND issued.invitation_id=invitation.id AND issued.action='issued' AND issued.actor_id=receipt.actor_id)";
const CLOSURE = 'NOT EXISTS(SELECT 1 FROM (SUBQUERY) conclusion WHERE conclusion.id=closure.id AND conclusion.company_id=scope.company_id)';

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Customer Authority read-tail r32 fact-existence witness family', () => {
  test('renders byte-identically to all four pre-dedupe fact witnesses', () => {
    const eventProbe = (table: string, alias: string, key: string, predicate: string) =>
      `EXISTS(SELECT 1 FROM ${table} ${alias} WHERE event.id='customer-access-event-'||${alias}.${key} AND ${predicate})`;
    expect(factExistenceWitness(false,
      `audit_events audit JOIN outbox_events event ON event.command_id=audit.command_id AND event.event_type=audit.action AND event.aggregate_id=audit.record_id AND event.aggregate_version=audit.record_version`,
      `audit.command_id=ack.request_key AND audit.actor_id=ack.user_id AND audit.assignment_id='external-customer' AND audit.action='customer.portal.support.acknowledge' AND audit.record_id=ack.request_id AND audit.record_version=ack.response_version AND audit.authority_json::jsonb->>'bindingId'=ack.binding_id AND event.payload_json::jsonb->>'responseId'=ack.response_id AND event.payload_json::jsonb->>'bindingId'=ack.binding_id AND event.payload_json::jsonb->>'userId'=ack.user_id`)).toBe(ACK);
    expect(factExistenceWitness(true, 'customer_access_events event',
      `event.binding_id=binding.id AND event.version=binding.version AND (event.action='portal-evidence-bind' AND ${eventProbe('customer_portal_actions','request','request_key',"request.binding_id=binding.id AND request.action='request_support'")} OR event.action IN ('request','approve','reject','revoke') AND ${eventProbe('command_receipts','receipt','command_id',"receipt.record_id=binding.id AND receipt.operation='customer.access.'||event.action AND receipt.actor_id=event.actor_id AND receipt.authority_json::jsonb->>'companyId'=binding.company_id AND receipt.result_json::jsonb->>'status'=binding.status AND receipt.result_json::jsonb->>'version'=binding.version::text")})`)).toBe(BINDING);
    expect(factExistenceWitness(false, 'customer_portal_invitation_events issued',
      "issued.id='customer-portal-invite-event-'||receipt.command_id AND issued.invitation_id=invitation.id AND issued.action='issued' AND issued.actor_id=receipt.actor_id")).toBe(INVITATION);
    expect(factExistenceWitness(true, '(SUBQUERY) conclusion', 'conclusion.id=closure.id AND conclusion.company_id=scope.company_id')).toBe(CLOSURE);
    for (const sql of [ACK, BINDING, INVITATION, CLOSURE]) readOnly(sql);
  });

  test('one shared literal, four pinned callers and intact control plane', () => {
    const shared = src('customer-authority-read-tail-r32-sources.ts');
    expect(shared.match(/\bSELECT\b/g) ?? []).toHaveLength(0);
    expect(shared).toContain('export const factExistenceWitness');
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    const caller = src('package-obligations.ts');
    expect(caller.match(/factExistenceWitness\(/g) ?? []).toHaveLength(4);
    expect(caller).not.toContain('EXISTS(SELECT 1 FROM audit_events audit');
    expect(caller).not.toContain('NOT EXISTS(SELECT 1 FROM customer_access_events event');
    expect(caller).not.toContain('EXISTS(SELECT 1 FROM customer_portal_invitation_events issued');
    expect(caller).not.toContain('FROM (${portalSupportConclusions}) conclusion WHERE');
    expect(caller).toContain("database.prepare('LOCK TABLE command_receipts,audit_events,outbox_events IN SHARE MODE').run()");
    for (const kind of ['acknowledged', 'closed_without_customer_acknowledgement', 'portal-invitation', 'response', 'closure']) {
      expect(caller, kind).toContain(kind);
    }
  });

  test('r21-r31 builders, source locks, native kinds and manager helpers stay untouched', () => {
    const counts: Array<[string, number, string]> = [
      ['customer-authority-read-sql.ts', 1, 'export const jsonbAggFact'],
      ['package-obligation-sql.ts', 1, 'export const latestSupportVersion'],
      ['customer-authority-read-tail-r23-sources.ts', 0, 'export const jsonbImageProjection'],
      ['customer-authority-read-tail-r24-sources.ts', 1, 'export const bindingFactProjection'],
      ['customer-authority-read-tail-r25-sources.ts', 0, 'export const ownerColumnProjection'],
      ['customer-authority-read-tail-r26-sources.ts', 1, 'export const accessBindingOrderProjection'],
      ['customer-authority-read-tail-r27-sources.ts', 0, 'export const accessEventEvidenceProbe'],
      ['customer-authority-read-tail-r28-sources.ts', 0, 'export const releaseGateRead'],
      ['customer-authority-read-tail-r29-sources.ts', 1, 'export const reviewEligibilityRows'],
      ['customer-authority-read-tail-r30-sources.ts', 0, 'export const existenceProbe'],
      ['customer-authority-read-tail-r31-sources.ts', 0, 'export const supportConclusionIdentity'],
    ];
    for (const [name, count, symbol] of counts) {
      const helper = src(name);
      expect(helper.match(/\bSELECT\b/g) ?? [], name).toHaveLength(count);
      expect(helper, name).toContain(symbol);
      expect(helper, name).not.toContain('factExistenceWitness');
    }
    for (const name of ['manager-eligibility.ts', 'assignment-eligibility-source.ts', 'portal-source-locks.ts', 'portal_commerce_locks.rs', 'native-owner-read.ts', 'portal-access.ts', 'portal-support-sources.ts']) {
      expect(src(name), name).not.toContain('factExistenceWitness');
    }
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=52', () => {
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
    expect(total).toBeLessThan(52);
    // r43 further reduced the tail (26 -> 22); keep the current exact pin.
    expect(total).toBe(17);
  });
});
