import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { commandAuditProbe, latestSupportVersion, supportAbsent } from '../../../packages/server-customer_authority/src/adapters/package-obligation-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-customer_authority/src');
const adapter = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Pre-dedupe fragments copied verbatim from HEAD; byte-identity oracles only.
const LATEST_ACK = '(SELECT max(latest.response_version) FROM customer_portal_support_responses latest WHERE latest.request_id=ack.request_id)';
const LATEST_CLOSURE = '(SELECT max(response.response_version) FROM customer_portal_support_responses response WHERE response.request_id=closure.request_id)';
const NO_CLOSURE = 'NOT EXISTS(SELECT 1 FROM customer_portal_support_closures closure WHERE closure.request_id=ack.request_id)';
const NO_RESPONSE = 'NOT EXISTS(SELECT 1 FROM customer_portal_support_responses response WHERE response.request_id=closure.request_id AND response.response_version=closure.response_version AND response.actor_id=closure.actor_id)';
const NO_ACK = 'NOT EXISTS(SELECT 1 FROM customer_portal_support_acknowledgements ack WHERE ack.request_id=closure.request_id)';
const RECEIPT_PROBES = [
  {
    negated: true,
    join: 'audit.record_id=binding.id AND audit.action=receipt.operation',
    where: "receipt.record_id=binding.id AND receipt.operation='customer.access.request' AND receipt.actor_id=binding.created_by AND receipt.authority_json::jsonb->>'companyId'=binding.company_id AND receipt.result_json::jsonb->>'id'=binding.id",
    head: "NOT EXISTS(SELECT 1 FROM command_receipts receipt JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=binding.id AND audit.action=receipt.operation WHERE receipt.record_id=binding.id AND receipt.operation='customer.access.request' AND receipt.actor_id=binding.created_by AND receipt.authority_json::jsonb->>'companyId'=binding.company_id AND receipt.result_json::jsonb->>'id'=binding.id)",
  },
  {
    negated: true,
    join: 'audit.record_id=invitation.binding_id AND audit.action=receipt.operation AND audit.actor_id=receipt.actor_id',
    where: "receipt.command_id=invitation.command_id AND receipt.record_id=invitation.binding_id AND receipt.operation IN ('customer.access.invite','customer.access.resend') AND receipt.actor_id=invitation.created_by AND receipt.authority_json::jsonb->>'companyId'=binding.company_id AND receipt.result_json::jsonb->>'id'=invitation.binding_id AND receipt.result_json::jsonb->>'invitationId'=invitation.id AND receipt.result_json::jsonb->>'bindingId'=invitation.binding_id AND receipt.result_json::jsonb->>'generation'=invitation.generation::text AND EXISTS(SELECT 1 FROM customer_portal_invitation_events issued WHERE issued.id='customer-portal-invite-event-'||receipt.command_id AND issued.invitation_id=invitation.id AND issued.action='issued' AND issued.actor_id=receipt.actor_id)",
    head: "NOT EXISTS(SELECT 1 FROM command_receipts receipt JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=invitation.binding_id AND audit.action=receipt.operation AND audit.actor_id=receipt.actor_id WHERE receipt.command_id=invitation.command_id AND receipt.record_id=invitation.binding_id AND receipt.operation IN ('customer.access.invite','customer.access.resend') AND receipt.actor_id=invitation.created_by AND receipt.authority_json::jsonb->>'companyId'=binding.company_id AND receipt.result_json::jsonb->>'id'=invitation.binding_id AND receipt.result_json::jsonb->>'invitationId'=invitation.id AND receipt.result_json::jsonb->>'bindingId'=invitation.binding_id AND receipt.result_json::jsonb->>'generation'=invitation.generation::text AND EXISTS(SELECT 1 FROM customer_portal_invitation_events issued WHERE issued.id='customer-portal-invite-event-'||receipt.command_id AND issued.invitation_id=invitation.id AND issued.action='issued' AND issued.actor_id=receipt.actor_id))",
  },
  {
    negated: false,
    join: 'audit.record_id=response.request_id AND audit.record_version=response.response_version AND audit.action=receipt.operation AND audit.actor_id=receipt.actor_id',
    where: "receipt.command_id=response.command_id AND receipt.operation='customer.portal.support.respond' AND receipt.record_id=response.request_id AND receipt.actor_id=response.actor_id AND receipt.authority_json::jsonb->>'companyId'=response.company_id AND receipt.result_json::jsonb->>'id'=response.id AND receipt.result_json::jsonb->>'requestId'=response.request_id AND receipt.result_json::jsonb->>'bindingId'=response.binding_id AND receipt.result_json::jsonb->>'version'=response.response_version::text AND receipt.result_json::jsonb->>'status'='published' AND receipt.result_json::jsonb->>'message'=response.message",
    head: "EXISTS(SELECT 1 FROM command_receipts receipt JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=response.request_id AND audit.record_version=response.response_version AND audit.action=receipt.operation AND audit.actor_id=receipt.actor_id WHERE receipt.command_id=response.command_id AND receipt.operation='customer.portal.support.respond' AND receipt.record_id=response.request_id AND receipt.actor_id=response.actor_id AND receipt.authority_json::jsonb->>'companyId'=response.company_id AND receipt.result_json::jsonb->>'id'=response.id AND receipt.result_json::jsonb->>'requestId'=response.request_id AND receipt.result_json::jsonb->>'bindingId'=response.binding_id AND receipt.result_json::jsonb->>'version'=response.response_version::text AND receipt.result_json::jsonb->>'status'='published' AND receipt.result_json::jsonb->>'message'=response.message)",
  },
];

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Customer Authority read-tail r22 byte-identical / parameterized families', () => {
  test('the three builders reproduce every pre-dedupe subselect byte-identically', () => {
    expect(latestSupportVersion('latest', 'ack.request_id')).toBe(LATEST_ACK);
    expect(latestSupportVersion('response', 'closure.request_id')).toBe(LATEST_CLOSURE);
    expect(supportAbsent('customer_portal_support_closures', 'closure', 'closure.request_id=ack.request_id')).toBe(NO_CLOSURE);
    expect(supportAbsent('customer_portal_support_responses', 'response', 'response.request_id=closure.request_id AND response.response_version=closure.response_version AND response.actor_id=closure.actor_id')).toBe(NO_RESPONSE);
    expect(supportAbsent('customer_portal_support_acknowledgements', 'ack', 'ack.request_id=closure.request_id')).toBe(NO_ACK);
    for (const p of RECEIPT_PROBES) expect(commandAuditProbe(p.negated, p.join, p.where), p.head).toBe(p.head);
    for (const sql of [LATEST_ACK, LATEST_CLOSURE, NO_CLOSURE, NO_RESPONSE, NO_ACK, ...RECEIPT_PROBES.map(p => p.head)]) readOnly(sql);
  });

  test('all eight former sites compose builders and keep their own prepared statements', () => {
    const pkg = adapter('package-obligations.ts');
    for (const sql of [LATEST_ACK, LATEST_CLOSURE, NO_CLOSURE, NO_RESPONSE, NO_ACK, ...RECEIPT_PROBES.map(p => p.head)]) expect(pkg, sql).not.toContain(sql);
    expect(pkg.match(/latestSupportVersion\(/g) ?? []).toHaveLength(2);
    expect(pkg.match(/supportAbsent\(/g) ?? []).toHaveLength(3);
    expect(pkg.match(/commandAuditProbe\(/g) ?? []).toHaveLength(3);
    // Independent consumers still prepare and bind their own statements.
    expect(pkg).toContain('database.prepare(');
    expect(pkg).toContain('.bind(input.companyId)');
  });

  test('r21 builders, source locks and portal consumers stay untouched', () => {
    const shared = adapter('customer-authority-read-sql.ts');
    expect(shared.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(shared).toContain('export const jsonbAggFact');
    expect(shared).toContain('export const bindingsExists');
    const pkg = adapter('package-obligations.ts');
    expect(pkg).toContain('export async function portalAuthorityPackageObligations');
    expect(pkg).toContain('export const portalSupportConclusions');
    expect(pkg).toContain('lockPortalAuthorityPackageObligationSources');
    expect(pkg).toContain("database.prepare('LOCK TABLE command_receipts,audit_events,outbox_events IN SHARE MODE').run()");
    expect(pkg).toContain('portalFilePackageObligationFacts');
    expect(pkg).toContain('outstanding:');
  });

  test('shared module declares each read literal exactly once', () => {
    const shared = adapter('package-obligation-sql.ts');
    expect(shared.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  test('retirement:ts-statements stays strictly below the r22 BEFORE=70 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(70);
  });
});
