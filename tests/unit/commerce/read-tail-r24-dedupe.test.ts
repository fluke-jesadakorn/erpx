import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { fulfillmentParentGuardSql } from '../../../packages/server-commerce/src/adapters/attachment-parent-guard-sql';
import { commerceFulfillmentGuardSql } from '../../../packages/server-commerce/src/adapters/guarded-source-dedupe-sql';
import { COMMERCE_OWNER_WRITE_NOOP_SQL } from '../../../packages/server-commerce/src/adapters/native-owner-write';

const root = resolve(import.meta.dirname ?? process.cwd(), '../../..');
const adapters = resolve(root, 'packages/server-commerce/src/adapters');
const src = resolve(root, 'packages/server-commerce/src');
const read = (file: string) => readFileSync(resolve(adapters, file), 'utf8');
const norm = (s: string) => s.replace(/\s+/g, ' ');
const sha = (s: string) => createHash('sha256').update(s, 'utf8').digest('hex');

const PENDING = 'order_id=? AND invoice_id IS NULL LIMIT 1';
const ECOM = 'id=? AND order_id=? AND version=?';
const R30 = 'id=? AND version=? AND invoice_id IS NULL';

type Site = {
  id: string;
  file: string;
  before: string;
  after: string;
  call: string;
  digest: string;
};

/** Family 3: the three retired fulfillment guard-head literals. r23
 * (commit 918e4bc67) already routed each through the single closed renderer
 * `commerceFulfillmentGuardSql(tail)`; r24 pins that end state byte-for-byte. */
const FAMILY3: Site[] = [
  {
    id: 'pending',
    file: 'workflow-flow-routing-facts.ts',
    before: `SELECT 1 FROM commerce_fulfillments WHERE ${PENDING}`,
    after: commerceFulfillmentGuardSql(PENDING),
    call: `database.prepare(commerceFulfillmentGuardSql("${PENDING}"))`,
    digest: '2becfb6c8c999360586d01d787a87b1a20c25eed865f38d3251940ea962d5b7d',
  },
  {
    id: 'ecommerce',
    file: 'ecommerce-sources.ts',
    before: `SELECT 1 FROM commerce_fulfillments WHERE ${ECOM}`,
    after: commerceFulfillmentGuardSql(ECOM),
    call: `db.prepare(commerceFulfillmentGuardSql('${ECOM}'))`,
    digest: '43d8eaf12ccb9475d82dfcb6ec096ff30d9fc6bdc36d3fa88730749e7e7fe2bc',
  },
  {
    id: 'rule30',
    file: 'commerce.ts',
    before: `EXISTS(SELECT 1 FROM commerce_fulfillments WHERE ${R30})`,
    after: `EXISTS(${commerceFulfillmentGuardSql(R30)})`,
    call: `commerceFulfillmentGuardSql('${R30}')`,
    digest: 'a2b3dd35db41afbc2df3e39b6536de2e8eadd8ae94921ff5fdfd0931b72e7b34',
  },
];

/** r24 re-scoped family: three parent-guard literals in attachment-read-sources
 * (const line 15, valuations line 29, invoiceCarrying line 31) through one
 * closed renderer parameterized by parent/child column. Byte-identical output. */
const LINE_GUARD = 'EXISTS(SELECT 1 FROM commerce_fulfillments f WHERE f.id=child.fulfillment_id AND f.order_id=o.id)';
const INVOICE_GUARD = 'EXISTS(SELECT 1 FROM commerce_fulfillments f WHERE f.invoice_id=child.invoice_id AND f.order_id=o.id)';
const PARENT_SITES: Site[] = [
  {
    id: 'line-parent-const',
    file: 'attachment-read-sources.ts',
    before: LINE_GUARD,
    after: fulfillmentParentGuardSql('id', 'fulfillment_id'),
    call: `fulfillmentParentGuardSql('id','fulfillment_id')`,
    digest: '59afccb4354d5d577d102a3a3b91b2e808b9aa475b6b917f53ff597086385061',
  },
  {
    id: 'valuations-parent',
    file: 'attachment-read-sources.ts',
    before: LINE_GUARD,
    after: fulfillmentParentGuardSql('id', 'fulfillment_id'),
    call: `fulfillmentParentGuardSql('id','fulfillment_id')`,
    digest: '59afccb4354d5d577d102a3a3b91b2e808b9aa475b6b917f53ff597086385061',
  },
  {
    id: 'invoice-carrying-parent',
    file: 'attachment-read-sources.ts',
    before: INVOICE_GUARD,
    after: fulfillmentParentGuardSql('invoice_id', 'invoice_id'),
    call: `fulfillmentParentGuardSql('invoice_id','invoice_id')`,
    digest: '0e1d04f2bfce24e313bff4f6fed0da954c078d2b2ea5603e2b8a7b0eb0988546',
  },
];

/** Optional fourth head: the attachments-service-facts JOIN variant. It is
 * NOT byte-identical to the renderer (aliased `f` + JOIN + `f.version=?`), so
 * r24 deliberately leaves it inline rather than widening the closed family. */
const JOIN_VARIANT = 'EXISTS(SELECT 1 FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id WHERE f.id=? AND o.company_id=? AND f.version=?)';

describe('Commerce read-tail r24 fulfillment guard-head retirement', () => {
  test('all three family-3 sites render byte-identically to the retired literals', () => {
    expect(FAMILY3).toHaveLength(3);
    for (const site of FAMILY3) {
      expect(site.after, site.id).toBe(site.before);
      expect(Buffer.from(site.after, 'utf8'), site.id).toEqual(Buffer.from(site.before, 'utf8'));
      expect(sha(site.after), site.id).toBe(site.digest);
      expect((site.after.match(/SELECT /g) ?? []).length, site.id).toBe(1);
    }
    expect(sha(FAMILY3.map((site) => site.after).join('\n'))).toBe('7b0d328f4af598af6c65d7e3a0215036874717dc5b71822583a61254b54b45f4');
  });

  test('every family-3 call site delegates with unchanged tails and binds', () => {
    for (const site of FAMILY3) {
      const text = read(site.file);
      expect(norm(text), `${site.id} delegation`).toContain(norm(site.call));
      expect(text, `${site.id} retired literal`).not.toContain(site.before);
      expect(text, `${site.id} retired literal (quoted)`).not.toContain(`'${site.before}'`);
      expect(text, `${site.id} retired literal (quoted)`).not.toContain(`"${site.before}"`);
    }
    // The distinct source witnesses keep their own binds; no bind was hoisted.
    expect(norm(read('workflow-flow-routing-facts.ts'))).toContain(norm('.bind(id).first()'));
    expect(norm(read('ecommerce-sources.ts'))).toContain(norm('.bind(id, orderId, version)'));
    expect(norm(read('commerce.ts'))).toContain(norm('facts.values)'));
  });

  test('all three re-scoped parent-guard sites render byte-identically to the retired literals', () => {
    expect(PARENT_SITES).toHaveLength(3);
    for (const site of PARENT_SITES) {
      expect(site.after, site.id).toBe(site.before);
      expect(Buffer.from(site.after, 'utf8'), site.id).toEqual(Buffer.from(site.before, 'utf8'));
      expect(sha(site.after), site.id).toBe(site.digest);
      expect((site.after.match(/SELECT /g) ?? []).length, site.id).toBe(1);
    }
    expect(sha(PARENT_SITES.map((site) => site.after).join('\n'))).toBe('d539c9bbbb15cf4c49d2a54c4357736afdc3a0626bf42ee4b4b9f8742082a626');
    expect(fulfillmentParentGuardSql('id', 'fulfillment_id')).not.toBe(fulfillmentParentGuardSql('invoice_id', 'invoice_id'));
  });

  test('every re-scoped parent-guard call site delegates with unchanged fragments', () => {
    const text = read('attachment-read-sources.ts');
    expect(norm(text)).toContain(norm("const FULFILLMENT_LINE_PARENT_GUARD=fulfillmentParentGuardSql('id','fulfillment_id');"));
    expect(text.split("fulfillmentParentGuardSql('id','fulfillment_id')").length - 1).toBe(2);
    expect(text.split("fulfillmentParentGuardSql('invoice_id','invoice_id')").length - 1).toBe(1);
    expect(text).not.toContain(LINE_GUARD);
    expect(text).not.toContain(INVOICE_GUARD);
    // The single outer prepare/execute and its row fragments are unchanged.
    expect(norm(text)).toContain(norm("rows('commerce_fulfillment_lines',FULFILLMENT_LINE_PARENT_GUARD)"));
    expect(norm(text)).toContain(norm("rows('commerce_fulfillment_evidence',FULFILLMENT_LINE_PARENT_GUARD)"));
    expect(norm(text)).toContain(norm("rows('commerce_goods_valuations','child.company_id=o.company_id AND '+fulfillmentParentGuardSql('id','fulfillment_id'))"));
    expect(norm(text)).toContain(norm("rows('commerce_invoice_goods_carrying','child.company_id=o.company_id AND '+fulfillmentParentGuardSql('invoice_id','invoice_id'))"));
    expect(norm(text)).toContain(norm('.bind(id,companyId).first<Native>()'));
  });

  test('the optional JOIN variant is excluded: not byte-identical, kept inline', () => {
    expect(sha(JOIN_VARIANT)).toBe('bdaa0a65e9fd6bd2597ec4a558071581a8016c1adff1d96ed360ffd683e59ac8');
    expect(JOIN_VARIANT).not.toBe(commerceFulfillmentGuardSql(''));
    expect(JOIN_VARIANT).not.toBe(`EXISTS(${commerceFulfillmentGuardSql(ECOM)})`);
    expect(JOIN_VARIANT).not.toBe(fulfillmentParentGuardSql('id', 'fulfillment_id'));
    expect(read('attachments-service-facts.ts')).toContain(JOIN_VARIANT);
  });

  test('one executable head per family; no residual inline literals', () => {
    let receipts = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) receipts += (readFileSync(p, 'utf8').match(/SELECT 1 FROM commerce_fulfillments WHERE/g) ?? []).length;
      }
    };
    walk(src);
    expect(receipts).toBe(1);
    const guards = read('guarded-source-dedupe-sql.ts');
    expect(guards.split('`SELECT 1 FROM commerce_fulfillments WHERE ${tail}`').length - 1).toBe(1);
    const parent = read('attachment-parent-guard-sql.ts');
    expect(parent.split('export const fulfillmentParentGuardSql =').length - 1).toBe(1);
    expect(parent.split('`EXISTS(SELECT 1 FROM commerce_fulfillments f WHERE f.${parent}=child.${child} AND f.order_id=o.id)`').length - 1).toBe(1);
    expect((parent.match(/SELECT /g) ?? []).length).toBe(1);
  });

  test('protected r18-r23 families, locks and native kinds stay untouched', () => {
    // Row locks stay inline in their owner call sites.
    expect(read('commerce-goods-accounting.ts')).toContain('"SELECT id FROM commerce_fulfillments WHERE id=? FOR UPDATE"');
    expect(read('commerce.ts')).toContain("'SELECT id FROM commerce_returns WHERE id=? FOR UPDATE'");
    // r18-r22 native kinds, order renderer and noop constant are unchanged.
    expect(read('native_commerce_read_sql.rs')).toContain('assert_eq!(STATEMENTS.len(), 45);');
    expect(read('read-tail-r21-dedupe-sql.ts')).toContain('`SELECT ${columns} FROM commerce_orders WHERE ${tail}`');
    expect(COMMERCE_OWNER_WRITE_NOOP_SQL).toBe('SELECT 1');
    expect(sha(COMMERCE_OWNER_WRITE_NOOP_SQL)).toBe('e004ebd5b5532a4b85984a62f8ad48a81aa3460c1ca07701f386135d72cdecf5');
    expect(read('commerce.ts').split('db.prepare(COMMERCE_OWNER_WRITE_NOOP_SQL)').length - 1).toBe(17);
  });

  test('retirement:ts-statements pins the owner TypeScript SELECT KPI at 371 (before 373)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(src);
    expect(total).toBe(371);
    expect((read('commerce.ts').match(/SELECT /g) ?? []).length).toBe(22);
    expect((read('ecommerce-sources.ts').match(/SELECT /g) ?? []).length).toBe(0);
    expect((read('workflow-flow-routing-facts.ts').match(/SELECT /g) ?? []).length).toBe(8);
    expect((read('attachment-read-sources.ts').match(/SELECT /g) ?? []).length).toBe(16);
  });
});
