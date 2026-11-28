import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { jsonbImageProjection } from '../../../modules/customer-authority/server/src/adapters/customer-authority-read-tail-r23-sources';

const srcRoot = resolve(__dirname, '../../../modules/customer-authority/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Exact pre-dedupe subselects copied verbatim from HEAD (r22 baseline).
// Only the closed object list, ORDER BY expression and source tail are
// parameterized; the emitted text is pinned byte-for-byte.
const IDENTITIES = "(SELECT jsonb_agg(jsonb_build_object('id',i.binding_id,'image',to_jsonb(i)::text,'userId',i.user_id,'email',i.email) ORDER BY i.binding_id) FROM customer_access_identities i JOIN bindings b ON b.id=i.binding_id)";
const READS = "(SELECT jsonb_agg(jsonb_build_object('id',r.id,'image',to_jsonb(r)::text,'bindingId',r.binding_id,'userId',r.user_id,'bindingVersion',r.binding_version,'projectionVersion',r.projection_version) ORDER BY r.id) FROM customer_access_reads r JOIN bindings b ON b.id=r.binding_id WHERE r.user_id=?)";

const identityArgs = [
  "'id',i.binding_id,'image',to_jsonb(i)::text,'userId',i.user_id,'email',i.email",
  'i.binding_id',
  'FROM customer_access_identities i JOIN bindings b ON b.id=i.binding_id',
] as const;
const readArgs = [
  "'id',r.id,'image',to_jsonb(r)::text,'bindingId',r.binding_id,'userId',r.user_id,'bindingVersion',r.binding_version,'projectionVersion',r.projection_version",
  'r.id',
  'FROM customer_access_reads r JOIN bindings b ON b.id=r.binding_id WHERE r.user_id=?',
] as const;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Customer Authority read-tail r23 JSONB image projection family', () => {
  test('emits byte-identical SQL for both pre-dedupe literals', () => {
    expect(jsonbImageProjection(...identityArgs)).toBe(IDENTITIES);
    expect(jsonbImageProjection(...readArgs)).toBe(READS);
    readOnly(jsonbImageProjection(...identityArgs));
    readOnly(jsonbImageProjection(...readArgs));
  });

  test('one delegated head, no inline copies, and the caller keeps its binds', () => {
    const shared = src('customer-authority-read-tail-r23-sources.ts');
    expect(shared.match(/\bSELECT\b/g) ?? []).toHaveLength(0);
    expect(shared).toContain('jsonbAggHead(');
    expect(shared).toContain("from './customer-authority-read-tail-r45-sources'");
    expect(shared.split('jsonb_build_object(')).toHaveLength(2);
    expect(shared).not.toContain('jsonb_agg(');

    const caller = src('portal-projection-unit.ts');
    for (const literal of [IDENTITIES, READS]) expect(caller).not.toContain(literal);
    expect(caller.match(/jsonbImageProjection\(/g) ?? []).toHaveLength(2);
    expect(caller, 'COALESCE wrapper preserved').toContain('COALESCE(${jsonbImageProjection(');
    // The closed arguments themselves are pinned so caller drift cannot
    // silently change the emitted SQL while the builder oracle stays green.
    for (const arg of [...identityArgs, ...readArgs]) expect(caller, arg).toContain(arg);
    expect(caller).toContain(".bind(actor.email.toLowerCase(),actor.userId)");
    expect(caller).toContain("first<{core:string;identities:string;reads:string}>()");
  });

  test('r21/r22 builders, source locks and portal consumers stay untouched', () => {
    // r21 builder module is unchanged.
    const r21 = src('customer-authority-read-sql.ts');
    expect(r21.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(r21).toContain('export const jsonbAggFact');
    expect(r21).toContain('export const bindingsExists');
    // r22 builder module is unchanged.
    const r22 = src('package-obligation-sql.ts');
    expect(r22.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(r22).toContain('export const latestSupportVersion');
    expect(r22).toContain('export const supportAbsent');
    expect(r22).toContain('export const commandAuditProbe');

    const caller = src('portal-projection-unit.ts');
    const facts = src('package-obligations.ts');
    // Control-plane locks and the owned portal projection unit are retained.
    for (const ref of [
      'lockPackageSettlementSources(database)',
      'commerceCompleteOwnerSourceReadLocks(database)',
      'customerPortalSourceReadLocks(database)',
      'portalSupportSourceReadLocks(database)',
      'portalPreviewSupportSourceReadLocks(database)',
      'attachmentPrivateReadSourceLocks(database)',
      'lockScheduledFeatureSources(database)',
      'flushFinanceDeferredEffects(database)',
      'export async function withCustomerPortalRequestProjection',
      'customerPortalProjectionSupportInstalled',
      'customerPortalPurchaseScope(database,\'b\')',
    ]) expect(caller, ref).toContain(ref);
    for (const ref of [
      'export async function portalAuthorityPackageObligations',
      'lockPortalAuthorityPackageObligationSources',
      "database.prepare('LOCK TABLE command_receipts,audit_events,outbox_events IN SHARE MODE').run()",
      'portalFilePackageObligationFacts',
    ]) expect(facts, ref).toContain(ref);
  });

  test('retirement:ts-statements is strictly lower than the recorded BEFORE=65', () => {
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
    expect(total).toBeLessThan(65);
    // r24 lowers the tail further; only the strict-lower bound stays stable.
    expect(total).toBeLessThan(64);
  });
});
