import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  COMMERCE_MFG_INITIALIZED_GUARD_SQL,
  COMMERCE_POSTING_RECEIPT_RECORD_GUARD_SQL,
  COMMERCE_RECEIPT_GUARD_SQL,
  commandReceiptsGuardSql,
  commerceFulfillmentGuardSql,
  commerceReturnsPointSql,
} from '../../../packages/server-commerce/src/adapters/guarded-source-dedupe-sql';
import { COMMERCE_OWNER_WRITE_NOOP_SQL } from '../../../packages/server-commerce/src/adapters/native-owner-write';

const root = resolve(import.meta.dirname ?? process.cwd(), '../../..');
const adapters = resolve(root, 'packages/server-commerce/src/adapters');
const src = resolve(root, 'packages/server-commerce/src');
const read = (file: string) => readFileSync(resolve(adapters, file), 'utf8');
const norm = (s: string) => s.replace(/\s+/g, ' ');
const sha = (s: string) => createHash('sha256').update(s, 'utf8').digest('hex');

const RECEIPT = "command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?";
const MFG = "operation='commerce.manufacturing-posting-setup.initialize' AND record_id=? AND authority_json::jsonb->>'companyId'=?";
const POSTING = "command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?";
const FLOW = "command_id=? AND operation='flow.assign' AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?";
const PENDING = 'order_id=? AND invoice_id IS NULL LIMIT 1';
const ECOM = 'id=? AND order_id=? AND version=?';
const R30 = 'id=? AND version=? AND invoice_id IS NULL';
const TAX_READ = 'id=? AND company_id=?';
const TAX_GUARD = 'id=? AND company_id=? AND version=?';
const CLOSE = "company_id=? AND status='approved' AND return_date<=?";
const REFUND = "invoice_id=? AND status IN ('requested','approved','posted')";

type Site = {
  id: string;
  file: string;
  before: string;
  after: string;
  call: string;
};

/** Exactly the 12 retired literals at the parent SHA, with their renderers. */
const SITES: Site[] = [
  {
    id: 'f1-receipt', file: 'guarded-source-dedupe-sql.ts',
    before: `SELECT 1 FROM command_receipts WHERE ${RECEIPT}`,
    after: COMMERCE_RECEIPT_GUARD_SQL,
    call: `export const COMMERCE_RECEIPT_GUARD_SQL = commandReceiptsGuardSql("${RECEIPT}")`,
  },
  {
    id: 'f1-mfg', file: 'guarded-source-dedupe-sql.ts',
    before: `SELECT 1 FROM command_receipts WHERE ${MFG}`,
    after: COMMERCE_MFG_INITIALIZED_GUARD_SQL,
    call: `export const COMMERCE_MFG_INITIALIZED_GUARD_SQL = commandReceiptsGuardSql("${MFG}")`,
  },
  {
    id: 'f1-posting-record', file: 'guarded-source-dedupe-sql.ts',
    before: `SELECT 1 FROM command_receipts WHERE ${POSTING}`,
    after: COMMERCE_POSTING_RECEIPT_RECORD_GUARD_SQL,
    call: `export const COMMERCE_POSTING_RECEIPT_RECORD_GUARD_SQL = commandReceiptsGuardSql("${POSTING}")`,
  },
  {
    id: 'f1-flow-assign', file: 'workflow-purchases.ts',
    before: `SELECT 1 FROM command_receipts WHERE ${FLOW}`,
    after: commandReceiptsGuardSql(FLOW),
    call: `database.prepare(commandReceiptsGuardSql("${FLOW}"))`,
  },
  {
    id: 'f3-pending', file: 'workflow-flow-routing-facts.ts',
    before: `SELECT 1 FROM commerce_fulfillments WHERE ${PENDING}`,
    after: commerceFulfillmentGuardSql(PENDING),
    call: `database.prepare(commerceFulfillmentGuardSql("${PENDING}"))`,
  },
  {
    id: 'f3-ecommerce', file: 'ecommerce-sources.ts',
    before: `SELECT 1 FROM commerce_fulfillments WHERE ${ECOM}`,
    after: commerceFulfillmentGuardSql(ECOM),
    call: `db.prepare(commerceFulfillmentGuardSql('${ECOM}'))`,
  },
  {
    id: 'f3-commerce-rule30', file: 'commerce.ts',
    before: `EXISTS(SELECT 1 FROM commerce_fulfillments WHERE ${R30})`,
    after: `EXISTS(${commerceFulfillmentGuardSql(R30)})`,
    call: `assertion(db, facts.key, facts.guardId, \`EXISTS(\${commerceFulfillmentGuardSql('${R30}')})\`, facts.values)`,
  },
  {
    id: 'f2-tax-read', file: 'finance-tax-source-version.ts',
    before: `SELECT version FROM commerce_returns WHERE ${TAX_READ}`,
    after: commerceReturnsPointSql('version', TAX_READ),
    call: `db.prepare(commerceReturnsPointSql('version','${TAX_READ}'))`,
  },
  {
    id: 'f2-tax-guard', file: 'finance-tax-source-version.ts',
    before: `EXISTS(SELECT 1 FROM commerce_returns WHERE ${TAX_GUARD})`,
    after: `EXISTS(${commerceReturnsPointSql('1', TAX_GUARD)})`,
    call: `\`EXISTS(\${commerceReturnsPointSql('1','${TAX_GUARD}')})\``,
  },
  {
    id: 'f2-close-guard', file: 'finance-close-source.ts',
    before: `NOT EXISTS(SELECT 1 FROM commerce_returns WHERE ${CLOSE})`,
    after: `NOT EXISTS(${commerceReturnsPointSql('1', CLOSE)})`,
    call: `\`NOT EXISTS(\${commerceReturnsPointSql('1',"${CLOSE}")})\``,
  },
  {
    id: 'f2-close-count', file: 'finance-close-source.ts',
    before: `SELECT COUNT(*) FROM commerce_returns WHERE ${CLOSE}`,
    after: commerceReturnsPointSql('COUNT(*)', CLOSE),
    call: `pendingRefundCount:commerceReturnsPointSql('COUNT(*)',"${CLOSE}")`,
  },
  {
    id: 'f2-refund', file: 'finance-refund-source.ts',
    before: `SELECT 1 FROM commerce_returns WHERE ${REFUND}`,
    after: commerceReturnsPointSql('1', REFUND),
    call: `db.prepare(commerceReturnsPointSql('1',"${REFUND}"))`,
  },
];

describe('Commerce read-tail r23 head-dedupe retirement', () => {
  test('all 12 sites render byte-identically to the retired literals', () => {
    expect(SITES).toHaveLength(12);
    const digests: Record<string, string> = {
      'f1-receipt': '7fd7a3cfb5c80b2423873745a84487b3021f64f4651a879334a49b1135561c7d',
      'f1-mfg': '5185c2b3ffac68b8013924bd66aed19dd5378e55ade9ae5fb552e28272534249',
      'f1-posting-record': '58c5e07caec8ad8d4cd9f965735c4d6a0e425dd60f57532d5984cc5362e8f27f',
      'f1-flow-assign': 'af97c090cf92056ef2558dcc78fa69b89e429dbe3fec9947d1f0c5ae23a1a625',
      'f3-pending': '2becfb6c8c999360586d01d787a87b1a20c25eed865f38d3251940ea962d5b7d',
      'f3-ecommerce': '43d8eaf12ccb9475d82dfcb6ec096ff30d9fc6bdc36d3fa88730749e7e7fe2bc',
      'f3-commerce-rule30': 'a2b3dd35db41afbc2df3e39b6536de2e8eadd8ae94921ff5fdfd0931b72e7b34',
      'f2-tax-read': 'c7aae4f263ab5f830ab3a5245c65a10fe845a9e07c78432d80acaf0441ad235e',
      'f2-tax-guard': '8f23d0a23f70a1f739a1087169462a88b2dadcab956b82f8acba29592e884382',
      'f2-close-guard': '8276ee5847d6d8aff4576181a313af30d5b31cc22e88dfc5a6c4649e96004ba6',
      'f2-close-count': '4e4f2f7adfe6c45df23c6b68197af016e729bd6ce49bd68d81ed9c4d717842c0',
      'f2-refund': '6e3f18461e146b26ce799cda18e8e0050a71c8f82d24d3d7e8e17801e7374ff2',
    };
    expect(Object.keys(digests)).toHaveLength(SITES.length);
    for (const site of SITES) {
      expect(site.after, site.id).toBe(site.before);
      expect(Buffer.from(site.after, 'utf8'), site.id).toEqual(Buffer.from(site.before, 'utf8'));
      expect(sha(site.after), site.id).toBe(digests[site.id]);
      expect((site.after.match(/SELECT /g) ?? []).length, site.id).toBe(1);
    }
    expect(sha(SITES.map((site) => site.after).join('\n'))).toBe('275cd4336fc67b36e4715273cd8da739efd99d12d189d90ac70267718f3eac14');
    expect(commandReceiptsGuardSql('x')).not.toBe(commerceFulfillmentGuardSql('x'));
    expect(commerceReturnsPointSql('1', 'x')).not.toBe(commerceReturnsPointSql('version', 'x'));
  });

  test('every call site delegates with unchanged tails, columns and binds', () => {
    const byFile = new Map<string, Site[]>();
    for (const site of SITES) byFile.set(site.file, [...(byFile.get(site.file) ?? []), site]);
    for (const [file, sites] of byFile) {
      const text = read(file);
      for (const site of sites) {
        expect(norm(text), `${site.id} delegation`).toContain(norm(site.call));
        expect(text, `${site.id} retired literal`).not.toContain(site.before);
        expect(text, `${site.id} retired literal (quoted)`).not.toContain(`'${site.before}'`);
        expect(text, `${site.id} retired literal (quoted)`).not.toContain(`"${site.before}"`);
      }
    }
    // The distinct source witnesses keep their own binds; no bind was hoisted.
    expect(norm(read('finance-tax-source-version.ts'))).toContain(norm('.bind(id,companyId)'));
    expect(norm(read('finance-close-source.ts'))).toContain(norm('[companyId,endDate]'));
    expect(norm(read('finance-refund-source.ts'))).toContain(norm('.bind(invoiceId)'));
    expect(norm(read('workflow-purchases.ts'))).toContain(norm('.bind(command.key,id,actor.userId,actor.companyId)'));
    expect(norm(read('workflow-flow-routing-facts.ts'))).toContain(norm('.bind(id).first()'));
    expect(norm(read('commerce.ts'))).toContain(norm('facts.values)'));
  });

  test('one executable head per family; no residual inline literals', () => {
    let receipts = 0;
    let fulfillments = 0;
    let returns = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) {
          const text = readFileSync(p, 'utf8');
          receipts += (text.match(/SELECT 1 FROM command_receipts WHERE/g) ?? []).length;
          fulfillments += (text.match(/SELECT 1 FROM commerce_fulfillments WHERE/g) ?? []).length;
          returns += (text.match(/SELECT [^ ]* FROM commerce_returns WHERE/g) ?? []).length;
        }
      }
    };
    walk(src);
    expect(receipts).toBe(1);
    expect(fulfillments).toBe(1);
    expect(returns).toBe(8);
    const guards = read('guarded-source-dedupe-sql.ts');
    expect(guards.split('`SELECT 1 FROM command_receipts WHERE ${tail}`').length - 1).toBe(1);
    expect(guards.split('`SELECT 1 FROM commerce_fulfillments WHERE ${tail}`').length - 1).toBe(1);
    expect(guards.split('`SELECT ${columns} FROM commerce_returns WHERE ${tail}`').length - 1).toBe(1);
  });

  test('protected r18-r22 families, locks and native kinds stay untouched', () => {
    // Row locks stay inline in their owner call sites.
    expect(read('commerce.ts')).toContain("'SELECT id FROM commerce_returns WHERE id=? FOR UPDATE'");
    expect(read('commerce-goods-accounting.ts')).toContain('"SELECT id FROM commerce_fulfillments WHERE id=? FOR UPDATE"');
    // r18-r22 native kinds, order renderer and noop constant are unchanged.
    expect(read('native_commerce_read_sql.rs')).toContain('assert_eq!(STATEMENTS.len(), 20);');
    expect(read('read-tail-r21-dedupe-sql.ts')).toContain('`SELECT ${columns} FROM commerce_orders WHERE ${tail}`');
    expect(COMMERCE_OWNER_WRITE_NOOP_SQL).toBe('SELECT 1');
    expect(sha(COMMERCE_OWNER_WRITE_NOOP_SQL)).toBe('e004ebd5b5532a4b85984a62f8ad48a81aa3460c1ca07701f386135d72cdecf5');
    expect(read('commerce.ts').split('db.prepare(COMMERCE_OWNER_WRITE_NOOP_SQL)').length - 1).toBe(17);
  });

  test('retirement:ts-statements pins the owner TypeScript SELECT KPI at 409 (before 413)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(src);
    expect(total).toBe(409);
  });
});
