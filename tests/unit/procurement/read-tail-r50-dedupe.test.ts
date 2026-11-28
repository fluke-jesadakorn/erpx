import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { createHash } from 'node:crypto';
import { billMatchReceiptHead } from '../../../modules/procurement/server/src/adapters/read-tail-r50';
import { billReceiptBinding, returnedWithoutReplacement } from '../../../modules/procurement/server/src/adapters/read-tail-r23';
import { foreignBillReceiptWitness } from '../../../modules/procurement/server/src/adapters/read-tail-r25';

const adapters = (name: string) => new URL(`../../../modules/procurement/server/src/adapters/${name}`, import.meta.url);
const src = (name: string) => readFileSync(adapters(name), 'utf8');
const count = (s: string, needle: string) => s.split(needle).length - 1;
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// Per-site byte oracles recorded verbatim from the BEFORE=85 source at HEAD
// 26182273a6de584fd5527fbecd726a8a20d003af. `head` is the exact `SELECT `
// substring that the renderer now owns; `tail` stays at the caller with every
// bind, JOIN and ordering fragment unchanged.
const HEAD = 'SELECT 1 FROM procurement_bill_match_receipts ';
const ORACLES = [
  {
    site: 'read-tail-r23/billReceiptBinding',
    alias: '${inner}',
    tail: ' WHERE ${inner}.match_id=${outer}.id AND ${inner}.fulfillment_id=${outer}.fulfillment_id)',
  },
  {
    site: 'read-tail-r25/foreignBillReceiptWitness',
    alias: 'br',
    tail: " LEFT JOIN commerce_fulfillments f ON f.id=br.fulfillment_id LEFT JOIN commerce_orders o ON o.id=f.order_id WHERE br.match_id=${alias}.id AND (o.id IS NULL OR o.kind<>'purchase' OR o.company_id<>${alias}.company_id OR o.party_id<>${alias}.supplier_id OR o.currency<>${alias}.currency))",
  },
  {
    site: 'supplier-bills/captureOptions',
    alias: 'br',
    tail: '\n        WHERE br.fulfillment_id=f.id AND br.active=TRUE',
  },
  {
    site: 'supplier-bills/stageInvoice',
    alias: 'br',
    tail: ' JOIN (${commercePlatformSources.commerce_fulfillments}) f ON f.id=br.fulfillment_id WHERE br.id=? AND br.match_id=? AND br.active=TRUE AND f.version=br.captured_version AND f.invoice_id IS NULL',
  },
];

// BEFORE sha256 of the three edited source files. Reversing the four folds
// (helper calls -> original literals, import removed) must reproduce them.
const BEFORE: Record<string, string> = {
  'read-tail-r23.ts': '81d376cdbbdce60b0703738bb627c732ebcc8347a9a9da79c0cce143701d5d96',
  'read-tail-r25.ts': 'd7c34051d679e09ed2389b10641a5456a7724f6bf0e3fe41487c5ce4523510a8',
  'supplier-bills.ts': '0a48283879f9e13c2bea95e0062233bd8930ac2e99fc61e2e962223c4c98b6aa',
};
const revert = (s: string) => s
  .replace(/^import \{billMatchReceiptHead\} from '\.\/read-tail-r50';\n/m, '')
  // r52 stacks a later by-id fold on the r25 witness; undo it too so the
  // pre-r50 bytes (and its recorded SHA) stay the exact floor anchor.
  .replace(/^import \{byIdExistsHead\} from '\.\/read-tail-r52';\n/m, '')
  .replace(
    "/** Visible actual source guarded by the owner's company/scope predicate. The\n * byte-identical SQL head now comes from r52; every tail and bind stays here. */",
    "/** Visible actual source guarded by the owner's company/scope predicate. */",
  )
  .replace(
    '`${byIdExistsHead(table, alias)}${id} AND ${alias}.company_id=${company} AND ${where})`',
    '`EXISTS(SELECT 1 FROM ${table} ${alias} WHERE ${alias}.id=${id} AND ${alias}.company_id=${company} AND ${where})`',
  )
  .replace(/\$\{billMatchReceiptHead\(inner\)\}/g, HEAD + '${inner}')
  .replace(/\$\{billMatchReceiptHead\("br"\)\}/g, HEAD + 'br');

describe('Procurement read-tail r50 bill-receipt head dedupe', () => {
  test('the renderer owns exactly one SELECT and emits the byte-identical head', () => {
    const helper = src('read-tail-r50.ts');
    expect(count(helper, 'export const billMatchReceiptHead =')).toBe(1);
    expect(count(helper, 'SELECT ')).toBe(1);
    expect(billMatchReceiptHead('br')).toBe(HEAD + 'br');
    expect(billMatchReceiptHead('${inner}')).toBe(HEAD + '${inner}');
    expect(billMatchReceiptHead('binding')).toBe(HEAD + 'binding');
  });

  test('all four heads render byte-identically with their original tails', () => {
    for (const { site, alias, tail } of ORACLES) {
      expect([site, billMatchReceiptHead(alias) + tail]).toEqual([site, HEAD + alias + tail]);
    }
  });

  test('the two helper renders keep their original EXISTS/NOT EXISTS bytes', () => {
    expect(billReceiptBinding(false, 'br', 'm')).toBe(
      'EXISTS(SELECT 1 FROM procurement_bill_match_receipts br WHERE br.match_id=m.id AND br.fulfillment_id=m.fulfillment_id)',
    );
    expect(billReceiptBinding(true, 'binding', 'bill')).toBe(
      'NOT EXISTS(SELECT 1 FROM procurement_bill_match_receipts binding WHERE binding.match_id=bill.id AND binding.fulfillment_id=bill.fulfillment_id)',
    );
    expect(foreignBillReceiptWitness('m')).toBe(
      "NOT EXISTS(SELECT 1 FROM procurement_bill_match_receipts br LEFT JOIN commerce_fulfillments f ON f.id=br.fulfillment_id LEFT JOIN commerce_orders o ON o.id=f.order_id WHERE br.match_id=m.id AND (o.id IS NULL OR o.kind<>'purchase' OR o.company_id<>m.company_id OR o.party_id<>m.supplier_id OR o.currency<>m.currency))",
    );
    // The untouched r23 sibling stays a floor anchor.
    expect(returnedWithoutReplacement('latest')).toContain(
      'NOT EXISTS( SELECT 1 FROM procurement_bill_match_receipts returned_receipt JOIN procurement_bill_match_receipts replacement_receipt',
    );
  });

  test('all four call sites delegate and the retired inline heads are gone', () => {
    const r23 = src('read-tail-r23.ts');
    const r25 = src('read-tail-r25.ts');
    const sb = src('supplier-bills.ts');
    for (const s of [r23, r25, sb]) expect(s).toContain("import {billMatchReceiptHead} from './read-tail-r50';");
    expect(count(r23, 'billMatchReceiptHead(')).toBe(1);
    expect(count(r25, 'billMatchReceiptHead(')).toBe(1);
    expect(count(sb, 'billMatchReceiptHead(')).toBe(2);
    expect(r23).not.toContain('EXISTS(SELECT 1 FROM procurement_bill_match_receipts ${inner} WHERE');
    expect(r25).not.toContain('NOT EXISTS(SELECT 1 FROM procurement_bill_match_receipts br LEFT JOIN');
    expect(sb).not.toContain('SELECT 1 FROM procurement_bill_match_receipts br');
    // Binds and bind order are unaffected.
    expect(sb).toContain('.bind(actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId).all<Row>()).results;');
    expect(sb).toContain('[receipt.id,bill.id]));');
  });

  test('reversing the four folds reproduces the original source bytes', () => {
    for (const [file, before] of Object.entries(BEFORE)) {
      expect([file, sha(revert(src(file)))]).toEqual([file, before]);
    }
  });

  test('witnesses, locks, r47/r48/r49 helpers and LOCK TABLE stay untouched', () => {
    expect(sha(src('read-tail-r28.ts'))).toBe('d9a93d385847a379c5e470da3add3fd5e504558ec2a1e9c9fb94043efeab94d0');
    expect(sha(src('read-tail-r31.ts'))).toBe('d6d1fa0a6b3499731d60d8cb9aad71f2f9bfde5f19b77ea354d4016aae432ce1');
    expect(sha(src('read-tail-r44.ts'))).toBe('f5dc17c41403aded63b04458839a97a5db2ad0025139f88390478638f45239be');
    expect(sha(src('read-tail-r45.ts'))).toBe('7e85a1f2555ab7d26ef18662380522a8db6d4f78cac5f5be0a27a9d1f3b120ec');
    expect(sha(src('read-tail-r47.ts'))).toBe('d4908a00c22d2da7aebc30a2b14f0f514ae602872b15e464399fc6768b21a67e');
    expect(sha(src('read-tail-r48.ts'))).toBe('3c49967c017a03df44136e588df5b9ca71abf04d49465be79c2c572a3cadc816');
    expect(sha(src('package-obligations.ts'))).toBe('2c344b0529a049c1b03ded03a3a5df74ce5bf0d30d76eb7b8769b1210196b102');
    expect(src('read-tail-r28.ts')).toContain('export const attachmentVersionWitness =');
    expect(src('read-tail-r44.ts')).toContain('export const commandReceiptEventWitness =');
    expect(src('read-tail-r45.ts')).toContain('export const procurementAttachmentReadOuterFrame =');
    expect(src('read-tail-r47.ts')).toContain("export { quotedProjection as procurementSettlementFactSelect } from './read-tail-r33';");
    expect(src('read-tail-r48.ts')).toContain('export const supplierDetailSelect =');
    expect(src('read-tail-sql.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(src('package-obligations.ts')).toContain('LOCK TABLE procurement_rfqs,');
    expect(src('package-obligations.ts')).toContain('command_receipts IN SHARE MODE');
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
  });

  test('worktree SELECT count is 85 - 4 + 1 = 82, then r51 82 - 2 + 1 = 81', () => {
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
    expect(total).toBeLessThan(85);
    expect(total).toBe(75); // r51: two jsonb_agg heads (-2 +1 helper) 82 -> 81; r52: two by-id EXISTS heads (-2 +1 helper) 81 -> 80; r53: two EXISTS/FROM/WHERE heads (-2 +1 helper) 80 -> 79; r54: two columns/FROM/WHERE heads (-2 +1 helper) 79 -> 78; r55: two ARRAY heads (-2 +1 helper) 78 -> 77.
  });
});
