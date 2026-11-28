import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { jsonbAggHead, jsonbAggregateHead } from '../../../modules/customer-authority/server/src/adapters/customer-authority-read-tail-r45-sources';
import { jsonbImageProjection } from '../../../modules/customer-authority/server/src/adapters/customer-authority-read-tail-r23-sources';
import { jsonbAggFact } from '../../../modules/customer-authority/server/src/adapters/customer-authority-read-sql';

const srcRoot = resolve(__dirname, '../../../modules/customer-authority/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const publicSrc = (name: string) => readFileSync(resolve(srcRoot, 'public', name), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');

// Verbatim pre-dedupe rendered bytes copied from HEAD (BEFORE=19). The r45
// aggregate head and the r23 image projection opened the same byte-identical
// aggregate prefix; only the caller-owned aggregate expression, ordering,
// source tail and closing bytes vary. Neither owns writes, locks, native
// statement kinds or authority evaluation.
const HEAD = '(SELECT jsonb_agg(';
const AGG_ORIGINAL = (alias: string) => `(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id)`;
const IDENTITY_BODY = 'i) ORDER BY i.user_id) FROM customer_access_identities i WHERE i.binding_id=b.id)';
const IDENTITY_AGG = '(SELECT jsonb_agg(to_jsonb(i) ORDER BY i.user_id) FROM customer_access_identities i WHERE i.binding_id=b.id)';
const IDENTITY_ARGS = [
  "'id',i.binding_id,'image',to_jsonb(i)::text,'userId',i.user_id,'email',i.email",
  'i.binding_id',
  'FROM customer_access_identities i JOIN bindings b ON b.id=i.binding_id',
] as const;
const READS = "(SELECT jsonb_agg(jsonb_build_object('id',r.id,'image',to_jsonb(r)::text,'bindingId',r.binding_id,'userId',r.user_id,'bindingVersion',r.binding_version,'projectionVersion',r.projection_version) ORDER BY r.id) FROM customer_access_reads r JOIN bindings b ON b.id=r.binding_id WHERE r.user_id=?)";
const READ_ARGS = [
  "'id',r.id,'image',to_jsonb(r)::text,'bindingId',r.binding_id,'userId',r.user_id,'bindingVersion',r.binding_version,'projectionVersion',r.projection_version",
  'r.id',
  'FROM customer_access_reads r JOIN bindings b ON b.id=r.binding_id WHERE r.user_id=?',
] as const;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Customer Authority read-tail r47 byte-identical aggregate head family', () => {
  test('renders byte-identically to every pre-dedupe literal', () => {
    const cases: Array<[string, string, string, number, string]> = [
      ['fact', jsonbAggregateHead('b) ORDER BY b.id)'), '(SELECT jsonb_agg(to_jsonb(b) ORDER BY b.id)', 44, '8d7cff1d82baba32'],
      ['identity', jsonbAggregateHead(IDENTITY_BODY), IDENTITY_AGG, 108, 'c42de1b9ea467837'],
      ['fact caller', jsonbAggFact('b'), AGG_ORIGINAL('b'), 44, '8d7cff1d82baba32'],
      ['image identities', jsonbImageProjection(...IDENTITY_ARGS), "(SELECT jsonb_agg(jsonb_build_object('id',i.binding_id,'image',to_jsonb(i)::text,'userId',i.user_id,'email',i.email) ORDER BY i.binding_id) FROM customer_access_identities i JOIN bindings b ON b.id=i.binding_id)", 211, '403dbc0dde3e43d1'],
      ['image reads', jsonbImageProjection(...READ_ARGS), READS, 293, 'f97c2a247b225a74'],
    ];
    for (const [name, rendered, oracle, len, hash] of cases) {
      expect(rendered, name).toBe(oracle);
      expect(Buffer.byteLength(rendered), name).toBe(len);
      expect(sha(rendered).slice(0, 16), name).toBe(hash);
      expect(rendered.startsWith(HEAD), name).toBe(true);
      readOnly(rendered);
    }
    // The new head owns the shared prefix once; both renderers delegate.
    expect(jsonbAggHead('to_jsonb(b) ORDER BY b.id)')).toBe(jsonbAggregateHead('b) ORDER BY b.id)'));
    expect(jsonbAggHead('jsonb_build_object(x) ORDER BY x) tail)')).toBe(jsonbImageProjection('x', 'x', 'tail'));
  });

  test('one owner-local head, two delegating renderers and no inline copies', () => {
    const head = src('customer-authority-read-tail-r45-sources.ts');
    expect(head.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(head.match(/export const jsonbAggHead/g) ?? []).toHaveLength(1);
    expect(head.match(/export const jsonbAggregateHead/g) ?? []).toHaveLength(1);
    expect(head).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    const image = src('customer-authority-read-tail-r23-sources.ts');
    expect(image.match(/\bSELECT\b/g) ?? []).toHaveLength(0);
    expect(image).toContain('jsonbAggHead(');
    expect(image).toContain("from './customer-authority-read-tail-r45-sources'");
    expect(image.match(/export const jsonbImageProjection/g) ?? []).toHaveLength(1);
    expect(image).not.toContain('jsonb_agg(');
    expect(image).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    const shared = src('customer-authority-read-sql.ts');
    expect(shared.match(/\bSELECT\b/g) ?? []).toHaveLength(1); // selectOneProbe only
    expect(shared).toContain('jsonbAggregateHead(');
    expect(shared).toContain("from './customer-authority-read-tail-r45-sources'");

    const read = src('attachment-read.ts');
    expect(read.match(/\bSELECT\b/g) ?? []).toHaveLength(0);
    expect(read).toContain('jsonbAggregateHead(');
    expect(read).toContain("from './customer-authority-read-tail-r45-sources'");
    expect(read).toContain('${portalPurchaseBindingSource(');
  });

  test('authority evaluation, locks, native kinds and prior read families stay pinned', () => {
    // The eligibility head, the row-locked portal reader and the control-plane
    // locks are deliberately excluded from this read-only head fold.
    const eligibility = src('assignment-eligibility-source.ts');
    expect(eligibility).toContain('export const assignmentSource');
    expect(eligibility).toContain('SELECT ${projection}');
    expect(eligibility).not.toContain('jsonbAggHead(');
    expect(src('portal-access.ts')).toContain('CA_RD_PORTAL_BINDING_LOCK');
    expect(src('portal-access.ts')).toContain('nativePortalReadStatement');
    expect(src('portal-source-locks.ts')).toContain('LOCK TABLE');
    expect(publicSrc('portal_commerce_locks.rs')).toBeTruthy();
    expect(src('native-owner-read.ts')).toBeTruthy();

    // Caller binds, lock references and the two image subselect call sites stay intact.
    const caller = src('portal-projection-unit.ts');
    expect(caller.match(/jsonbImageProjection\(/g) ?? []).toHaveLength(2);
    expect(caller).toContain('COALESCE(${jsonbImageProjection(');
    expect(caller).toContain('.bind(actor.email.toLowerCase(),actor.userId)');
    for (const ref of ['customerPortalSourceReadLocks(database)', 'portalSupportSourceReadLocks(database)']) {
      expect(caller, ref).toContain(ref);
    }

    // r40/r44/r46 heads and the r23/r45 exports remain owned and delegating.
    expect(src('customer-authority-read-tail-r40-sources.ts')).toContain('export const selectProjection');
    const core = src('customer-authority-read-tail-r44-sources.ts');
    expect(core).toContain('export const portalBindingOrderCore');
    expect(core).toContain('selectProjection(');
  });

  test('retirement:ts-statements is strictly below the BEFORE=19 baseline', () => {
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
    expect(total).toBeLessThan(19);
    expect(total).toBe(14);
  });
});
