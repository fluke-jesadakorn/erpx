import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { bindingFactProjection } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r24-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-customer_authority/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Exact pre-dedupe fact subselects copied verbatim from HEAD (r23 baseline,
// BEFORE=64). Only closed kind/id literals, the source table/alias, the pending
// predicate and the optional join tail are parameterized; the emitted text is
// pinned byte-for-byte.
const BINDING_EVENT = "SELECT 'binding-event',event.id,to_jsonb(event),false FROM customer_access_events event JOIN bindings binding ON binding.id=event.binding_id";
const IDENTITY = "SELECT 'identity',identity.binding_id,to_jsonb(identity),identity.email<>binding.email FROM customer_access_identities identity JOIN bindings binding ON binding.id=identity.binding_id";
const READ = "SELECT 'read',read.id,to_jsonb(read),false FROM customer_access_reads read JOIN bindings binding ON binding.id=read.binding_id";
const INVITATION_EVENT = "SELECT 'portal-invitation-event',event.id,to_jsonb(event),false FROM customer_portal_invitation_events event JOIN customer_portal_invitations invitation ON invitation.id=event.invitation_id JOIN bindings binding ON binding.id=invitation.binding_id";

const cases = [
  {
    label: 'binding-event',
    sql: BINDING_EVENT,
    call: "'binding-event','event.id','customer_access_events','event','false'",
    render: () => bindingFactProjection('binding-event', 'event.id', 'customer_access_events', 'event', 'false'),
  },
  {
    label: 'identity',
    sql: IDENTITY,
    call: "'identity','identity.binding_id','customer_access_identities','identity','identity.email<>binding.email'",
    render: () => bindingFactProjection('identity', 'identity.binding_id', 'customer_access_identities', 'identity', 'identity.email<>binding.email'),
  },
  {
    label: 'read',
    sql: READ,
    call: "'read','read.id','customer_access_reads','read','false'",
    render: () => bindingFactProjection('read', 'read.id', 'customer_access_reads', 'read', 'false'),
  },
  {
    label: 'portal-invitation-event',
    sql: INVITATION_EVENT,
    call: "'portal-invitation-event','event.id','customer_portal_invitation_events','event','false','JOIN customer_portal_invitations invitation ON invitation.id=event.invitation_id JOIN bindings binding ON binding.id=invitation.binding_id'",
    render: () => bindingFactProjection('portal-invitation-event', 'event.id', 'customer_portal_invitation_events', 'event', 'false', 'JOIN customer_portal_invitations invitation ON invitation.id=event.invitation_id JOIN bindings binding ON binding.id=invitation.binding_id'),
  },
] as const;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Customer Authority read-tail r24 binding-joined fact family', () => {
  test('renders byte-identically to every pre-dedupe literal', () => {
    for (const c of cases) expect(c.render(), c.label).toBe(c.sql);
    for (const c of cases) readOnly(c.sql);
  });

  test('one shared literal, no inline copies, and the caller keeps its binds', () => {
    const shared = src('customer-authority-read-tail-r24-sources.ts');
    expect(shared.match(/\bSELECT\b/g) ?? []).toHaveLength(1);

    const caller = src('package-obligations.ts');
    for (const c of cases) {
      expect(caller, `${c.label} literal is gone`).not.toContain(c.sql);
      // Closed arguments are pinned so caller drift cannot silently change the
      // emitted SQL while the byte-identity oracle stays green.
      expect(caller, c.label).toContain(`bindingFactProjection(${c.call})`);
    }
    expect(caller.match(/bindingFactProjection\(/g) ?? []).toHaveLength(4);
    expect(caller).toContain('.bind(input.companyId)');
    expect(caller).toContain("database.prepare('LOCK TABLE command_receipts,audit_events,outbox_events IN SHARE MODE').run()");
  });

  test('r21/r22/r23 builders, source locks and native kinds stay untouched', () => {
    const r21 = src('customer-authority-read-sql.ts');
    expect(r21.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(r21).toContain('export const jsonbAggFact');
    expect(r21).toContain('export const bindingsExists');

    const r22 = src('package-obligation-sql.ts');
    expect(r22.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(r22).toContain('export const latestSupportVersion');
    expect(r22).toContain('export const supportAbsent');
    expect(r22).toContain('export const commandAuditProbe');

    const r23 = src('customer-authority-read-tail-r23-sources.ts');
    expect(r23.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(r23).toContain('export const jsonbImageProjection');

    const caller = src('package-obligations.ts');
    for (const ref of [
      'export async function portalAuthorityPackageObligations',
      'lockPortalAuthorityPackageObligationSources',
      'assertPortalAuthorityPackageObligationSources',
      'export const portalSupportConclusions',
      'portalFilePackageObligationFacts',
      'commandAuditProbe(',
      'latestSupportVersion(',
      'supportAbsent(',
      'LOCK TABLE command_receipts,audit_events,outbox_events IN SHARE MODE',
    ]) expect(caller, ref).toContain(ref);
    // Every fact kind is preserved; no lock or native-owned kind was altered.
    for (const kind of ['binding-event', 'identity', 'read', 'portal-invitation-event', 'response', 'closure', 'portal-invitation']) {
      expect(caller, kind).toContain(`'${kind}'`);
    }
    expect(caller).not.toMatch(/FOR\s+UPDATE|pg_advisory/i);
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=64', () => {
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
    expect(total).toBeLessThan(64);
    // r25 further reduced the tail (61 -> 60); keep a bound, not a stale exact pin.
    expect(total).toBeLessThan(61);
  });
});
