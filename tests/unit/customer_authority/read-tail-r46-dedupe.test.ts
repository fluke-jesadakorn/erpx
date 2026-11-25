import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { finalRootOwnerProjectionSources as commerce } from '@erpx/server-commerce/public/final-root-owner-projection-sources';
import { selectProjection } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r40-sources';
import { portalBindingOrderCore } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r44-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-customer_authority/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const publicSrc = (name: string) => readFileSync(resolve(srcRoot, 'public', name), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');

// Verbatim pre-dedupe rendered bytes copied from HEAD (BEFORE=20). The r40
// universal projection head and the r44 binding/order core opened the same
// closed literal; only the caller-owned projection and separators vary.
const ORDERS = commerce.commerce_orders;
const BINDING_ORIGINAL = (projection: string, fromSep: string, joinSep: string) =>
  `SELECT ${projection}${fromSep}FROM customer_access_bindings b${joinSep}JOIN (${ORDERS}) o ON o.id=b.order_id AND o.company_id=b.company_id AND o.party_id=b.customer_id`;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Customer Authority read-tail r46 projection head family', () => {
  test('renders byte-identically to every pre-dedupe literal', () => {
    const simple = BINDING_ORIGINAL('x', ' ', ' ');
    const cols = BINDING_ORIGINAL(
      'b.id,b.company_id,o.branch_id,o.department_id,b.version',
      '\n    ',
      '\n    ',
    );
    const cases: Array<[string, string, string, number, string]> = [
      ['head', selectProjection('id'), 'SELECT id', 9, 'c59c370157577798'],
      ['binding core', portalBindingOrderCore('x', ' ', ' ', ORDERS, ''), simple, 527, '6227aeb4aa2d08e5'],
      ['binding core columns', portalBindingOrderCore('b.id,b.company_id,o.branch_id,o.department_id,b.version', '\n    ', '\n    ', ORDERS, ''), cols, 589, '8bb67e3f6f35e143'],
    ];
    for (const [name, rendered, oracle, len, hash] of cases) {
      expect(rendered, name).toBe(oracle);
      expect(Buffer.byteLength(rendered), name).toBe(len);
      expect(sha(rendered).slice(0, 16), name).toBe(hash);
      expect(selectProjection('id'), name).toBe('SELECT id');
      readOnly(rendered);
    }
  });

  test('one owner-local head and one delegating renderer', () => {
    const head = src('customer-authority-read-tail-r40-sources.ts');
    expect(head.match(/SELECT/g) ?? []).toHaveLength(1);
    expect(head.match(/export const selectProjection/g) ?? []).toHaveLength(1);
    expect(head).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    const core = src('customer-authority-read-tail-r44-sources.ts');
    expect(core.match(/SELECT/g) ?? []).toHaveLength(0);
    expect(core).toContain('selectProjection(');
    expect(core).toContain("from './customer-authority-read-tail-r40-sources'");
    expect(core.match(/export const portalBindingOrderCore/g) ?? []).toHaveLength(1);
    expect(core).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    const binding = src('portal-binding-source.ts');
    const purchase = src('portal-purchase-binding-source.ts');
    for (const [name, caller] of [['binding', binding], ['purchase', purchase]] as Array<[string, string]>) {
      expect(caller, name).toContain('portalBindingOrderCore(');
      expect(caller, name).not.toContain('SELECT ${projection}');
    }
  });

  test('authority evaluation, locks, native kinds and prior read families stay pinned', () => {
    // The third head family member (eligibility) and the privileged readers are
    // deliberately excluded from this read-only head fold.
    expect(src('assignment-eligibility-source.ts')).toContain('export const assignmentSource');
    expect(src('assignment-eligibility-source.ts')).not.toContain('selectProjection(');
    expect(src('customer-authority-read-tail-r29-sources.ts')).not.toContain('selectProjection(');
    expect(src('portal-access.ts')).toContain('CA_RD_PORTAL_BINDING_LOCK');
    expect(src('portal-access.ts')).toContain('nativePortalReadStatement');
    expect(src('portal-source-locks.ts')).toContain('LOCK TABLE');
    expect(publicSrc('portal_commerce_locks.rs')).toBeTruthy();
    expect(src('native-owner-read.ts')).toBeTruthy();

    // r37/r41/r42 helpers keep delegating to r40; r23/r45 aggregate families intact.
    for (const [name, symbol] of [
      ['customer-authority-read-tail-r37-sources.ts', 'allColumnsProjection'],
      ['customer-authority-read-tail-r41-sources.ts', 'evidenceSource'],
      ['customer-authority-read-tail-r42-sources.ts', 'supportRequestRead'],
    ] as Array<[string, string]>) {
      expect(src(name), name).toContain(`export const ${symbol}`);
      expect(src(name), name).toContain('selectProjection(');
    }
    expect(src('customer-authority-read-tail-r23-sources.ts')).toContain('export const jsonbImageProjection');
    expect(src('customer-authority-read-tail-r45-sources.ts')).toContain('export const jsonbAggregateHead');
  });

  test('retirement:ts-statements is strictly below the BEFORE=20 baseline', () => {
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
    expect(total).toBeLessThan(20);
    expect(total).toBe(15);
  });
});
