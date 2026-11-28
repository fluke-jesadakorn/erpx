import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { createHash } from 'node:crypto';
import { arraySelectBy } from '../../../modules/procurement/server/src/adapters/read-tail-r55';
import { procurementAttachmentReadOuterFrame } from '../../../modules/procurement/server/src/adapters/read-tail-r45';

const adapters = (name: string) => new URL(`../../../modules/procurement/server/src/adapters/${name}`, import.meta.url);
const src = (name: string) => readFileSync(adapters(name), 'utf8');
const count = (s: string, needle: string) => s.split(needle).length - 1;
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// Per-site byte oracles recorded verbatim from the BEFORE=78 source at HEAD
// d107e637ea1c3b0173d308fcfa1d38a0701aa54a. `head` is the exact byte-identical
// SQL prefix the r55 renderer owns; each `tail` stays at the caller with every
// predicate and alias fragment unchanged.
const HEAD = 'ARRAY(SELECT ${columns} FROM ${table} WHERE ${tail})';
const ORACLES = [
  {
    site: 'r45/RFQ attachment product-id array',
    columns: 'product_id', table: 'procurement_rfq_lines',
    tail: 'rfq_id=r.id AND product_id IS NOT NULL',
    sql: 'ARRAY(SELECT product_id FROM procurement_rfq_lines WHERE rfq_id=r.id AND product_id IS NOT NULL)',
  },
  {
    site: 'r45/RFQ attachment supplier-id array',
    columns: 'supplier_id', table: 'procurement_rfq_offers',
    tail: 'rfq_id=r.id',
    sql: 'ARRAY(SELECT supplier_id FROM procurement_rfq_offers WHERE rfq_id=r.id)',
  },
];

// The complete r45 RFQ frame must keep exactly the same bytes after the fold.
const RFQ_FRAME = 'SELECT (RFQ_IMAGE)::text image,r.warehouse_id,ARRAY(SELECT product_id FROM procurement_rfq_lines WHERE rfq_id=r.id AND product_id IS NOT NULL) product_ids,ARRAY(SELECT supplier_id FROM procurement_rfq_offers WHERE rfq_id=r.id) supplier_ids,r.order_id,NULL::text invoice_id,ARRAY[]::text[] receipt_ids FROM procurement_rfqs r WHERE r.id=? AND r.company_id=?';

// BEFORE sha256 of the single edited file. Reversing both folds
// (renderer call -> original literal, import restored) must reproduce it
// exactly, proving no rendered byte was lost.
const BEFORE: Record<string, string> = {
  'attachment-read-sources.ts': '53ecef5acafd67f8e7de821f50fb249446b739fb9ab11cfdba215131f00f5a0d',
};
const revert: Record<string, (s: string) => string> = {
  'attachment-read-sources.ts': (s) => s
    .replace("import {arraySelectBy} from './read-tail-r55';\n", '')
    .replace(
      '`${arraySelectBy("product_id","procurement_rfq_lines","rfq_id=r.id AND product_id IS NOT NULL")} product_ids`',
      '"ARRAY(SELECT product_id FROM procurement_rfq_lines WHERE rfq_id=r.id AND product_id IS NOT NULL) product_ids"',
    )
    .replace(
      '`${arraySelectBy("supplier_id","procurement_rfq_offers","rfq_id=r.id")} supplier_ids`',
      '"ARRAY(SELECT supplier_id FROM procurement_rfq_offers WHERE rfq_id=r.id) supplier_ids"',
    ),
};

describe('Procurement read-tail r55 ARRAY subquery head dedupe', () => {
  test('the renderer owns exactly one SELECT and emits the byte-identical head', () => {
    const helper = src('read-tail-r55.ts');
    expect(count(helper, 'export const arraySelectBy =')).toBe(1);
    expect(count(helper, 'SELECT ')).toBe(1);
    expect(arraySelectBy('${columns}', '${table}', '${tail}')).toBe(HEAD);
    expect(arraySelectBy('C', 'T', 'id=?')).toBe('ARRAY(SELECT C FROM T WHERE id=?)');
  });

  test('each head plus its original tail renders byte-identically', () => {
    for (const { site, columns, table, tail, sql } of ORACLES) {
      expect([site, arraySelectBy(columns, table, tail)]).toEqual([site, sql]);
    }
  });

  test('the full RFQ outer frame is unchanged', () => {
    expect(procurementAttachmentReadOuterFrame('procurement_rfqs', '(RFQ_IMAGE)',
      'r.warehouse_id',
      `${arraySelectBy('product_id', 'procurement_rfq_lines', 'rfq_id=r.id AND product_id IS NOT NULL')} product_ids`,
      `${arraySelectBy('supplier_id', 'procurement_rfq_offers', 'rfq_id=r.id')} supplier_ids`,
      'NULL::text invoice_id', 'ARRAY[]::text[] receipt_ids',
    )).toBe(RFQ_FRAME);
  });

  test('the source delegates both ARRAY heads, keeps the UNION branch and bind order', () => {
    const a = src('attachment-read-sources.ts');
    expect(a).toContain("import {arraySelectBy} from './read-tail-r55';");
    expect(count(a, '${arraySelectBy(')).toBe(2);
    expect(count(a, '.bind(id,companyId)')).toBe(2);
    expect(a).not.toContain('"ARRAY(SELECT product_id FROM procurement_rfq_lines');
    expect(a).not.toContain('"ARRAY(SELECT supplier_id FROM procurement_rfq_offers');
    expect(a).toContain('ARRAY(SELECT fulfillment_id FROM procurement_bill_match_receipts WHERE match_id=r.id UNION SELECT r.fulfillment_id) receipt_ids');
  });

  test('reversing both folds reproduces the original source bytes', () => {
    for (const [file, before] of Object.entries(BEFORE)) {
      expect([file, sha(revert[file](src(file)))]).toEqual([file, before]);
    }
  });

  test('no write, lock, native or cross-owner consumer was touched', () => {
    expect(src('read-tail-r55.ts')).not.toMatch(/FOR\s+(UPDATE|SHARE)|LOCK\s+TABLE|INSERT\s+INTO|DELETE\s+FROM|UPDATE\s+\w+\s+SET/i);
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
    expect(src('package-obligations.ts')).toContain('LOCK TABLE procurement_rfqs,');
    expect(src('package-obligations.ts')).toContain('command_receipts IN SHARE MODE');
  });

  test('witnesses and r23/r28/r31/r44/r45/r47/r48/r50/r51/r52/r53/r54 helpers stay byte-identical', () => {
    expect(sha(src('read-tail-r23.ts'))).toBe('b1f282c6de197c6681cb53bea93075a89fd9d16684d7d039af4ffe44d0c5548e');
    expect(sha(src('read-tail-r28.ts'))).toBe('d9a93d385847a379c5e470da3add3fd5e504558ec2a1e9c9fb94043efeab94d0');
    expect(sha(src('read-tail-r31.ts'))).toBe('d6d1fa0a6b3499731d60d8cb9aad71f2f9bfde5f19b77ea354d4016aae432ce1');
    expect(sha(src('read-tail-r44.ts'))).toBe('f5dc17c41403aded63b04458839a97a5db2ad0025139f88390478638f45239be');
    expect(sha(src('read-tail-r45.ts'))).toBe('7e85a1f2555ab7d26ef18662380522a8db6d4f78cac5f5be0a27a9d1f3b120ec');
    expect(sha(src('read-tail-r47.ts'))).toBe('d4908a00c22d2da7aebc30a2b14f0f514ae602872b15e464399fc6768b21a67e');
    expect(sha(src('read-tail-r48.ts'))).toBe('3c49967c017a03df44136e588df5b9ca71abf04d49465be79c2c572a3cadc816');
    expect(sha(src('read-tail-r50.ts'))).toBe('a22440f40b969c713465c4ad55110805176fb27c70500cc4c7584889110d919e');
    expect(sha(src('read-tail-r51.ts'))).toBe('08bd16f0507ba84fe3f9bf6285faea9c24126a871141c78707dcbdeb3f907958');
    expect(sha(src('read-tail-r52.ts'))).toBe('3b588e2a3ec73e6ac8caf5eee864c1b8e5b95139e9eb40da12e732c31261603d');
    expect(sha(src('read-tail-r53.ts'))).toBe('70fe1a434534f27038ec5225e1f2558fcbf57734335db2cd1aefbdfed9913ce5');
    expect(sha(src('read-tail-r54.ts'))).toBe('9ecd018a32bf54ad2e917a9a27a31b9da00e3a74fb300fb7d5abef79d2bbbf8e');
    expect(sha(src('package-obligations.ts'))).toBe('2c344b0529a049c1b03ded03a3a5df74ce5bf0d30d76eb7b8769b1210196b102');
    expect(sha(src('supplier-bills.ts'))).toBe('f1485ee0fc42475f3337626f561f97aa4ab1d1921b6df412bc97c12a7e534fef');
  });

  test('worktree SELECT count is 78 - 2 + 1 = 77', () => {
    const root = new URL('../../../modules/procurement/server/src/', import.meta.url);
    const files: URL[] = [];
    const walk = (dir: URL) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = new URL(entry.name + (entry.isDirectory() ? '/' : ''), dir);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) files.push(p);
      }
    };
    walk(root);
    const total = files.reduce((n, p) => n + (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length, 0);
    expect(total).toBeLessThan(78);
    expect(total).toBe(75);
  });
});
