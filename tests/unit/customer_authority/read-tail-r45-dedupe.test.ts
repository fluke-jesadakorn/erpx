import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { finalRootOwnerProjectionSources as commerce } from '@erpx/server-commerce/public/final-root-owner-projection-sources';
import { jsonbAggregateHead } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r45-sources';
import { jsonbAggFact } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-sql';
import { portalPurchaseBindingSource } from '../../../packages/server-customer_authority/src/adapters/portal-purchase-binding-source';

const srcRoot = resolve(__dirname, '../../../packages/server-customer_authority/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Verbatim pre-dedupe literals from HEAD (BEFORE=21). The r21 customer-document
// aggregate and the Portal attachment identity witness open the same closed
// JSONB aggregate head; only the alias, order, source and closing bytes vary.
const AGG_HEAD = '(SELECT jsonb_agg(to_jsonb(';
const aggOriginal = (alias: string) => `(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id)`;
const aggBody = (alias: string) => `${alias}) ORDER BY ${alias}.id)`;
const IDENTITY_AGG = '(SELECT jsonb_agg(to_jsonb(i) ORDER BY i.user_id) FROM customer_access_identities i WHERE i.binding_id=b.id)';
const IDENTITY_BODY = 'i) ORDER BY i.user_id) FROM customer_access_identities i WHERE i.binding_id=b.id)';

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Customer Authority read-tail r45 JSONB aggregate head family', () => {
  test('renders byte-identically to both pre-dedupe aggregate literals', () => {
    for (const alias of ['b', 'o', 'f', 'a', 'r']) {
      expect(jsonbAggregateHead(aggBody(alias)), alias).toBe(aggOriginal(alias));
      expect(jsonbAggFact(alias), alias).toBe(aggOriginal(alias));
      readOnly(jsonbAggFact(alias));
    }
    expect(jsonbAggregateHead(IDENTITY_BODY)).toBe(IDENTITY_AGG);
    expect(IDENTITY_AGG.startsWith(AGG_HEAD)).toBe(true);
    expect(aggOriginal('b').startsWith(AGG_HEAD)).toBe(true);
    readOnly(IDENTITY_AGG);
  });

  test('the full attachment witness keeps its exact pre-dedupe SQL bytes', () => {
    const projection = `jsonb_build_object('binding',to_jsonb(b),'order',to_jsonb(o),
 'identities',${jsonbAggregateHead(IDENTITY_BODY)})::text image`;
    const after = `${portalPurchaseBindingSource(projection, "'sales','quote'")}
 WHERE b.id=? AND b.company_id=?`;
    const before = `SELECT jsonb_build_object('binding',to_jsonb(b),'order',to_jsonb(o),
 'identities',${IDENTITY_AGG})::text image
 FROM customer_access_bindings b JOIN (${commerce.commerce_orders}) o ON o.id=b.order_id AND o.company_id=b.company_id AND o.party_id=b.customer_id AND o.kind IN ('sales','quote')
 WHERE b.id=? AND b.company_id=?`;
    expect(after).toBe(before);
    readOnly(after);
  });

  test('one owner-local renderer and two delegating call sites', () => {
    const head = src('customer-authority-read-tail-r45-sources.ts');
    expect(head.match(/SELECT/g) ?? []).toHaveLength(1);
    expect(head).toContain('export const jsonbAggregateHead');
    expect(head).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    const shared = src('customer-authority-read-sql.ts');
    expect(shared.match(/SELECT/g) ?? []).toHaveLength(1); // selectOneProbe only
    expect(shared).not.toContain(AGG_HEAD);
    expect(shared).toContain('jsonbAggregateHead(');
    expect(shared).toContain("from './customer-authority-read-tail-r45-sources'");
    expect(shared).toContain('export const selectOneProbe');

    const read = src('attachment-read.ts');
    expect(read.match(/SELECT/g) ?? []).toHaveLength(0);
    expect(read).not.toContain(AGG_HEAD);
    expect(read).toContain('jsonbAggregateHead(');
    expect(read).toContain("from './customer-authority-read-tail-r45-sources'");
    expect(read).toContain('${portalPurchaseBindingSource(');
  });

  test('prepared statements, binds, locks, native kinds and eligibility stay pinned', () => {
    const read = src('attachment-read.ts');
    expect(read).toContain('.bind(id,actor.companyId)');
    expect(read).toContain('PORTAL_ATTACHMENT_TARGET_NOT_FOUND');
    expect(read).toContain('LOCK TABLE company_package_grants IN SHARE ROW EXCLUSIVE MODE');
    expect(read).toContain('customerPortalSourceReadLocks(database)');
    expect(read).toContain('privateFileDenialAuthorityWitness(database,actor)');
    expect(read).toContain("import {bindingIdentityCurrentSql} from '@erpx/server-customer_authority/adapters/attachment-withdraw-proof'");

    const projection = src('portal-projection-unit.ts');
    expect(projection.match(/jsonbAggFact\(/g) ?? []).toHaveLength(7);
    expect(projection).toContain('.bind(actor.email.toLowerCase(),actor.userId)');

    expect(src('portal-access.ts')).toContain('FOR UPDATE');
    expect(src('portal-source-locks.ts')).toContain('LOCK TABLE');
    expect(src('assignment-eligibility-source.ts')).toContain('export const assignmentSource');
    expect(src('manager-eligibility.ts')).toContain('export const');
    expect(src('native-owner-read.ts')).toContain('export');
    for (const name of [
      'customer-authority-read-tail-r40-sources.ts',
      'customer-authority-read-tail-r41-sources.ts',
      'customer-authority-read-tail-r44-sources.ts',
    ]) expect(src(name), name).toBeTruthy();
  });

  test('retirement:ts-statements is strictly below the BEFORE=21 baseline', () => {
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
    expect(total).toBeLessThan(21);
    expect(total).toBe(19);
  });
});
